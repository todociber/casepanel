use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WidgetKind {
    Text,
    Value,
    Bar,
    Gauge,
    Image,
    Graph,
    Rectangle,
    Clock,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Widget {
    pub id: String,
    pub kind: WidgetKind,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub sensor: String,
    pub text: String,
    pub unit: String,
    pub decimals: u32,
    pub min: f64,
    pub max: f64,
    pub color: String,
    pub background: String,
    pub font: String,
    pub font_size: f32,
    pub align: String,
    pub image_path: String,
}
impl Default for Widget {
    fn default() -> Self {
        Self {
            id: "widget".into(),
            kind: WidgetKind::Value,
            x: 40.,
            y: 40.,
            width: 240.,
            height: 60.,
            sensor: String::new(),
            text: String::new(),
            unit: String::new(),
            decimals: 0,
            min: 0.,
            max: 100.,
            color: "#FFFFFF".into(),
            background: "#333333".into(),
            font: "Segoe UI".into(),
            font_size: 36.,
            align: "left".into(),
            image_path: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub schema_version: u32,
    pub canvas_width: f32,
    pub canvas_height: f32,
    pub monitor_name: String,
    pub monitor_id: String,
    pub startup_delay_seconds: u32,
    pub bindings: BTreeMap<String, String>,
    pub widgets: Vec<Widget>,
}
impl Default for Config {
    fn default() -> Self {
        let mut c = Self {
            schema_version: 1,
            canvas_width: 1920.,
            canvas_height: 1200.,
            monitor_name: "PM1562".into(),
            monitor_id: String::new(),
            startup_delay_seconds: 5,
            bindings: BTreeMap::new(),
            widgets: vec![],
        };
        let mut add = |id: &str,
                       kind: WidgetKind,
                       x: f32,
                       y: f32,
                       w: f32,
                       h: f32,
                       text: &str,
                       sensor: &str,
                       size: f32,
                       unit: &str,
                       max: f64| {
            c.widgets.push(Widget {
                id: id.into(),
                kind,
                x,
                y,
                width: w,
                height: h,
                text: text.into(),
                sensor: sensor.into(),
                font_size: size,
                unit: unit.into(),
                max,
                ..Widget::default()
            });
        };
        use WidgetKind::*;
        for (id, x, y, w, h) in [
            ("cpu_tile", 20., 210., 915., 510.),
            ("gpu_tile", 967., 210., 915., 510.),
            ("ram_tile", 20., 750., 600., 300.),
            ("power_tile", 650., 750., 600., 300.),
            ("vram_tile", 1282., 750., 600., 300.),
        ] {
            add(id, Rectangle, x, y, w, h, "", "", 1., "", 100.);
        }
        add(
            "ai_title", Text, 50., 23., 500., 50., "AI USAGE", "", 38., "", 100.,
        );
        for (name, label, key, y) in [
            ("claude", "CLAUDE 5H", "ai.claude.5h", 85.),
            ("codex", "CODEX 5H", "ai.codex.5h", 141.),
        ] {
            add(
                &format!("{name}_label"),
                Text,
                50.,
                y,
                155.,
                35.,
                label,
                "",
                28.,
                "",
                100.,
            );
            add(
                &format!("{name}_bar"),
                Bar,
                210.,
                y + 8.,
                435.,
                18.,
                "",
                key,
                1.,
                "",
                100.,
            );
            add(
                &format!("{name}_value"),
                Value,
                665.,
                y - 2.,
                75.,
                40.,
                "",
                key,
                28.,
                "%",
                100.,
            );
        }
        add(
            "case_label",
            Text,
            987.,
            90.,
            220.,
            48.,
            "Case fans",
            "",
            40.,
            "",
            100.,
        );
        add(
            "case_value",
            Value,
            1210.,
            90.,
            220.,
            48.,
            "",
            "board.case_fan",
            40.,
            " RPM",
            2500.,
        );
        add(
            "case_bar",
            Bar,
            987.,
            150.,
            420.,
            10.,
            "",
            "board.case_fan",
            1.,
            "",
            2500.,
        );
        add(
            "clock", Clock, 1480., 32., 420., 110., "", "", 100., "", 100.,
        );
        add(
            "ai_source",
            Text,
            50.,
            182.,
            680.,
            22.,
            "AI usage reported by the external script",
            "",
            16.,
            "",
            100.,
        );
        add(
            "cpu_title",
            Text,
            65.,
            242.,
            820.,
            65.,
            "CPU {cpu_name}",
            "",
            46.,
            "",
            100.,
        );
        add(
            "gpu_title",
            Text,
            1012.,
            242.,
            830.,
            65.,
            "GPU {gpu_name}",
            "",
            44.,
            "",
            100.,
        );
        for (prefix, x, base, key, temp_max) in [
            ("cpu", 70., 490., "cpu", 100.),
            ("gpu", 1017., 1430., "gpu", 100.),
        ] {
            add(
                &format!("{prefix}_gauge"),
                Gauge,
                x,
                360.,
                320.,
                320.,
                "",
                &format!("{key}.temperature"),
                1.,
                "",
                temp_max,
            );
            add(
                &format!("{prefix}_temp"),
                Value,
                x + 20.,
                458.,
                280.,
                100.,
                "",
                &format!("{key}.temperature"),
                80.,
                "°C",
                temp_max,
            );
            for (suffix, metric, y, max, unit, fs) in [
                ("load", "load", 360., 100., "%", 40.),
                (
                    "clock",
                    "clock",
                    475.,
                    if key == "cpu" { 6.0 } else { 3200. },
                    if key == "cpu" { " GHz" } else { " MHz" },
                    40.,
                ),
                ("fan", "fan", 595., 2500., " RPM", 40.),
            ] {
                add(
                    &format!("{prefix}_{suffix}_bar"),
                    Bar,
                    base,
                    y,
                    400.,
                    10.,
                    "",
                    &format!("{key}.{metric}"),
                    1.,
                    "",
                    max,
                );
                add(
                    &format!("{prefix}_{suffix}_value"),
                    Value,
                    base,
                    y + 17.,
                    400.,
                    60.,
                    "",
                    &format!("{key}.{metric}"),
                    fs,
                    unit,
                    max,
                );
            }
        }
        add(
            "ram_title",
            Text,
            65.,
            785.,
            530.,
            65.,
            "RAM {memory_name}",
            "",
            46.,
            "",
            100.,
        );
        add(
            "vram_title",
            Text,
            1327.,
            785.,
            500.,
            65.,
            "GPU VRAM",
            "",
            46.,
            "",
            100.,
        );
        for (prefix, x, key) in [("ram", 65., "ram"), ("vram", 1327., "gpu.vram")] {
            add(
                &format!("{prefix}_bar"),
                Bar,
                x,
                862.,
                505.,
                25.,
                "",
                &format!("{key}.load"),
                1.,
                "",
                100.,
            );
            add(
                &format!("{prefix}_used"),
                Value,
                x,
                912.,
                300.,
                55.,
                "U: ",
                &format!("{key}.used"),
                38.,
                " GiB",
                100.,
            );
            add(
                &format!("{prefix}_free"),
                Value,
                x,
                967.,
                300.,
                55.,
                "F: ",
                &format!("{key}.free"),
                38.,
                " GiB",
                100.,
            );
            add(
                &format!("{prefix}_percent"),
                Value,
                x + 312.,
                897.,
                220.,
                120.,
                "",
                &format!("{key}.load"),
                94.,
                "%",
                100.,
            );
        }
        add(
            "power_title",
            Text,
            665.,
            785.,
            580.,
            55.,
            "POWER / TEMPERATURE",
            "",
            37.,
            "",
            100.,
        );
        for (id, key, label, y, unit) in [
            ("cpu_power", "cpu.power", "CPU POWER", 850., " W"),
            ("gpu_power", "gpu.power", "GPU POWER", 910., " W"),
            (
                "gpu_memory_temp",
                "gpu.memory_temperature",
                "GPU MEMORY TEMP",
                970.,
                "°C",
            ),
        ] {
            add(
                &format!("{id}_label"),
                Text,
                697.,
                y,
                310.,
                50.,
                label,
                "",
                35.,
                "",
                100.,
            );
            add(id, Value, 1010., y, 200., 50., "", key, 38., unit, 100.);
        }
        for w in &mut c.widgets {
            if w.kind == Rectangle {
                w.color = "#101010".into();
            }
            if w.id == "ai_source" {
                w.color = "#777777".into();
            }
            if w.kind == Value {
                w.align = if w.id.ends_with("_temp") {
                    "center"
                } else {
                    "right"
                }
                .into();
            }
            if w.sensor == "cpu.clock" {
                w.decimals = 2;
            }
            if w.id.ends_with("_used") || w.id.ends_with("_free") {
                w.decimals = 1;
                w.align = "left".into();
            }
            if w.sensor.ends_with(".power") {
                w.decimals = 2;
            }
        }
        c
    }
}

impl Config {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("Unsupported profile version".into());
        }
        if !(320. ..=16384.).contains(&self.canvas_width)
            || !(200. ..=16384.).contains(&self.canvas_height)
            || self.widgets.len() > 256
            || self.startup_delay_seconds > 60
        {
            return Err("Invalid canvas size, widget count, or startup delay".into());
        }
        let mut ids = BTreeSet::new();
        for w in &self.widgets {
            if w.id.is_empty()
                || !ids.insert(&w.id)
                || ![w.x, w.y, w.width, w.height, w.font_size]
                    .iter()
                    .all(|v| v.is_finite())
                || w.width < 1.
                || w.height < 1.
                || w.width > 32768.
                || w.height > 32768.
                || !(1. ..=500.).contains(&w.font_size)
                || !w.min.is_finite()
                || !w.max.is_finite()
                || w.max <= w.min
                || w.decimals > 4
                || w.text.len() > 2048
                || w.font.len() > 128
                || w.image_path.len() > 32768
                || parse_color(&w.color).is_none()
                || parse_color(&w.background).is_none()
            {
                return Err(format!("Invalid widget: {}", w.id));
            }
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
        if metadata.len() > 2 * 1024 * 1024 {
            return Err("Profile is too large".into());
        }
        let c: Self = serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        c.validate()?;
        Ok(c)
    }
    pub fn load_or_recover(path: &Path) -> (Self, Option<String>) {
        if !path.exists() {
            return (Self::default(), None);
        }
        match Self::load(path) {
            Ok(c) => (c, None),
            Err(e) => {
                let backup = backup_path(path);
                match Self::load(&backup) {
                    Ok(c) => (
                        c,
                        Some(format!(
                            "Profile is damaged; recovered the valid backup: {e}"
                        )),
                    ),
                    Err(_) => (
                        Self::default(),
                        Some(format!(
                            "Profile could not be loaded; using a temporary default: {e}"
                        )),
                    ),
                }
            }
        }
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("json.tmp");
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
            f.write_all(&serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
        }
        if path.exists() && Self::load(path).is_ok() {
            std::fs::copy(path, backup_path(path)).map_err(|e| e.to_string())?;
        }
        crate::platform::atomic_replace(&tmp, path)
    }
}
pub fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}
pub fn default_path() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("SensorPanel")
        .join("profile.json")
}
pub fn parse_color(s: &str) -> Option<(f32, f32, f32)> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(s, 16).ok()?;
    Some((
        ((n >> 16) & 255) as f32 / 255.,
        ((n >> 8) & 255) as f32 / 255.,
        (n & 255) as f32 / 255.,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shipped_layout_is_valid() {
        Config::default().validate().unwrap();
    }
    #[test]
    fn invalid_scale_is_rejected() {
        let mut c = Config::default();
        c.widgets[0].max = c.widgets[0].min;
        assert!(c.validate().is_err());
    }
    #[test]
    fn duplicate_widget_is_rejected() {
        let mut c = Config::default();
        c.widgets.push(c.widgets[0].clone());
        assert!(c.validate().is_err());
    }
    #[test]
    fn unsupported_schema_is_rejected() {
        let c = Config {
            schema_version: 2,
            ..Default::default()
        };
        assert!(c.validate().is_err());
    }
    #[test]
    fn damaged_profile_recovers_last_valid_backup() {
        let root = std::env::temp_dir().join(format!(
            "sensorpanel-test-{}-{}",
            std::process::id(),
            crate::model::now_ms()
        ));
        let path = root.join("profile.json");
        let first = Config::default();
        first.save(&path).unwrap();
        let mut second = first.clone();
        second.monitor_name = "changed".into();
        second.save(&path).unwrap();
        std::fs::write(&path, b"{bad json").unwrap();
        let (recovered, warning) = Config::load_or_recover(&path);
        assert!(warning.is_some());
        assert_eq!(recovered.monitor_name, first.monitor_name);
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(backup_path(&path)).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
