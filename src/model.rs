use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME: usize = 256 * 1024;
pub const HISTORY_LIMIT: usize = 600;

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    Valid,
    Reported,
    Unavailable,
    Error,
    Stale,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sensor {
    pub id: String,
    pub key: String,
    pub device: String,
    pub label: String,
    pub unit: String,
    pub interval_ms: u64,
    pub value: Option<f64>,
    pub observed_ms: u64,
    pub quality: Quality,
    pub message: String,
    /// Optional external producer's five-hour reset time, in Unix milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_at_ms: Option<u64>,
}

impl Sensor {
    pub fn new(device: &str, key: &str, label: &str, unit: &str, interval_ms: u64) -> Self {
        Self {
            id: format!("{device}/{key}"),
            key: key.into(),
            device: device.into(),
            label: label.into(),
            unit: unit.into(),
            interval_ms,
            value: None,
            observed_ms: 0,
            quality: Quality::Unavailable,
            message: "No reading".into(),
            reset_at_ms: None,
        }
    }
    pub fn set(&mut self, value: f64) {
        if value.is_finite() {
            self.value = Some(value);
            self.observed_ms = now_ms();
            self.quality = Quality::Valid;
            self.message.clear();
        } else {
            self.fail("Reading is not finite");
        }
    }
    pub fn fail(&mut self, message: impl Into<String>) {
        self.message = message.into();
        self.quality = if self.value.is_some() {
            Quality::Stale
        } else {
            Quality::Unavailable
        };
    }
    pub fn current_quality(&self, at: u64) -> Quality {
        if self.quality == Quality::Valid
            && at.saturating_sub(self.observed_ms) > self.interval_ms.saturating_mul(3)
        {
            Quality::Stale
        } else {
            self.quality
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub version: u32,
    pub sampled_ms: u64,
    pub sensors: Vec<Sensor>,
    pub diagnostics: Vec<String>,
}

impl Snapshot {
    pub fn new(sensors: Vec<Sensor>, diagnostics: Vec<String>) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            sampled_ms: now_ms(),
            sensors,
            diagnostics,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version != PROTOCOL_VERSION {
            return Err("Incompatible IPC version".into());
        }
        if self.sensors.len() > 256 || self.diagnostics.len() > 64 {
            return Err("Sensor catalog is too large".into());
        }
        for s in &self.sensors {
            if s.id.len() > 256
                || s.label.len() > 256
                || s.message.len() > 1024
                || s.interval_ms < 100
                || s.value.is_some_and(|v| !v.is_finite())
            {
                return Err("Invalid sample".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ClientCommand {
    Subscribe { version: u32, active: bool },
    Shutdown { version: u32 },
}

#[derive(Default)]
pub struct Store {
    pub sensors: BTreeMap<String, Sensor>,
    pub history: BTreeMap<String, VecDeque<(u64, f64)>>,
}
impl Store {
    pub fn update(&mut self, mut sensor: Sensor) {
        if sensor.value.is_none()
            && let Some(previous) = self.sensors.get(&sensor.id)
            && previous.value.is_some()
        {
            sensor.value = previous.value;
            sensor.observed_ms = previous.observed_ms;
            sensor.quality = Quality::Stale;
        }
        if sensor.quality == Quality::Valid
            && let Some(v) = sensor.value
        {
            let q = self.history.entry(sensor.id.clone()).or_default();
            if q.back().is_none_or(|(t, _)| *t != sensor.observed_ms) {
                q.push_back((sensor.observed_ms, v));
            }
            while q.len() > HISTORY_LIMIT
                || q.front()
                    .is_some_and(|(t, _)| sensor.observed_ms.saturating_sub(*t) > 600_000)
            {
                q.pop_front();
            }
        }
        self.sensors.insert(sensor.id.clone(), sensor);
    }
    pub fn apply(&mut self, snapshot: Snapshot) -> Result<(), String> {
        snapshot.validate()?;
        // Remove disappeared devices instead of silently keeping live-looking values.
        let ids: std::collections::BTreeSet<_> =
            snapshot.sensors.iter().map(|s| s.id.clone()).collect();
        for s in self.sensors.values_mut().filter(|s| {
            s.device.starts_with("cpu-hw:")
                || s.device.starts_with("board:")
                || s.device.starts_with("nvidia:")
        }) {
            if !ids.contains(&s.id) {
                s.fail("Device is not present");
            }
        }
        for s in snapshot.sensors {
            self.update(s);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_snapshots_accept_optional_ai_reset_metadata() {
        let sensor = Sensor::new("registry-ai", "ai.claude.5h", "Claude", "%", 5000);
        let mut json = serde_json::to_value(&sensor).unwrap();
        assert!(json.get("reset_at_ms").is_none());
        assert!(
            serde_json::from_value::<Sensor>(json.clone())
                .unwrap()
                .reset_at_ms
                .is_none()
        );
        json["reset_at_ms"] = serde_json::json!(1_800_000_000_000u64);
        assert_eq!(
            serde_json::from_value::<Sensor>(json).unwrap().reset_at_ms,
            Some(1_800_000_000_000)
        );
    }
    #[test]
    fn zero_is_a_valid_fan_reading() {
        let mut s = Sensor::new("test", "fan", "fan", "RPM", 2000);
        s.set(0.);
        assert_eq!(s.quality, Quality::Valid);
        assert_eq!(s.value, Some(0.));
    }
    #[test]
    fn stale_retains_last_value() {
        let mut s = Sensor::new("test", "temp", "temp", "°C", 1000);
        s.set(50.);
        assert_eq!(s.current_quality(s.observed_ms + 3001), Quality::Stale);
        s.fail("disconnect");
        assert_eq!(s.value, Some(50.));
    }
    #[test]
    fn reconnect_without_first_sample_keeps_last_reading() {
        let mut store = Store::default();
        let mut good = Sensor::new("cpu-hw:test", "cpu.power", "Package", "W", 1000);
        good.set(85.);
        let missing = Sensor::new("cpu-hw:test", "cpu.power", "Package", "W", 1000);
        store.update(good);
        store.update(missing);
        let current = store.sensors.values().next().unwrap();
        assert_eq!(current.value, Some(85.));
        assert_eq!(current.quality, Quality::Stale);
    }
    #[test]
    fn rejects_protocol_mismatch() {
        let mut s = Snapshot::new(vec![], vec![]);
        s.version = 2;
        assert!(s.validate().is_err());
    }
    #[test]
    fn history_is_bounded() {
        let mut store = Store::default();
        for i in 0..1000 {
            let mut s = Sensor::new("test", "temp", "temp", "C", 1000);
            s.set(i as f64);
            s.observed_ms = i * 1000;
            store.update(s);
        }
        assert_eq!(store.history.values().next().unwrap().len(), 600);
    }
}
