use crate::{
    model::{Quality, Sensor},
    platform::*,
};
pub struct Basic {
    previous: Option<(u64, u64)>,
    sensors: Vec<Sensor>,
    ticks: u64,
}
impl Basic {
    pub fn new() -> Self {
        let identity = format!("windows-cpu:{}", inventory().cpu_id);
        Self {
            previous: None,
            sensors: vec![
                Sensor::new(&identity, "cpu.load", "CPU usage", "%", 1000),
                Sensor::new("windows-memory", "ram.used", "RAM used", "GiB", 1000),
                Sensor::new("windows-memory", "ram.free", "RAM available", "GiB", 1000),
                Sensor::new("windows-memory", "ram.load", "RAM usage", "%", 1000),
                Sensor::new("registry-ai", "ai.claude.5h", "Claude 5h", "%", 5000),
                Sensor::new("registry-ai", "ai.codex.5h", "Codex 5h", "%", 5000),
            ],
            ticks: 0,
        }
    }
    pub fn poll(&mut self) -> Vec<Sensor> {
        unsafe {
            let mut idle = FileTime::default();
            let mut kernel = idle;
            let mut user = idle;
            if GetSystemTimes(&mut idle, &mut kernel, &mut user) != 0 {
                let total = kernel.ticks() + user.ticks();
                if let Some((old_idle, old_total)) = self.previous {
                    let dt = total.saturating_sub(old_total);
                    let di = idle.ticks().saturating_sub(old_idle);
                    if dt > 0 {
                        self.sensors[0].set(100. * (dt.saturating_sub(di)) as f64 / dt as f64);
                    }
                }
                self.previous = Some((idle.ticks(), total));
            } else {
                self.sensors[0].fail("GetSystemTimes failed");
            }
            let mut mem = MemoryStatus {
                length: std::mem::size_of::<MemoryStatus>() as u32,
                ..Default::default()
            };
            if GlobalMemoryStatusEx(&mut mem) != 0 && mem.total_phys > 0 {
                self.sensors[1].set((mem.total_phys - mem.avail_phys) as f64 / 1073741824.);
                self.sensors[2].set(mem.avail_phys as f64 / 1073741824.);
                self.sensors[3]
                    .set(100. * (mem.total_phys - mem.avail_phys) as f64 / mem.total_phys as f64);
            } else {
                for s in &mut self.sensors[1..4] {
                    s.fail("GlobalMemoryStatusEx failed");
                }
            }
            if self.ticks.is_multiple_of(5) {
                let path = "Software\\FinalWire\\AIDA64\\ImportValues";
                let state = registry_string(false, path, "Str9")
                    .unwrap_or_else(|| "No external status provided".into());
                let stamp = registry_string(false, path, "Str10")
                    .unwrap_or_else(|| "No update timestamp".into());
                for (i, name, description, reset) in [
                    (4, "DW1", "Str1", "Claude5HResetAt"),
                    (5, "DW3", "Str3", "Codex5HResetAt"),
                ] {
                    self.sensors[i].reset_at_ms = registry_string(false, path, reset)
                        .and_then(|v| v.trim().parse::<u64>().ok())
                        .and_then(|v| v.checked_mul(1000));
                    let note = registry_string(false, path, description).unwrap_or_default();
                    match registry_dword(path, name) {
                        Some(value) if value <= 100 => {
                            self.sensors[i].set(value as f64);
                            self.sensors[i].quality = Quality::Reported;
                            self.sensors[i].message = format!(
                                "Reported by external script; per-provider freshness is unverified. {note} | {state} | {stamp}"
                            );
                        }
                        _ => self.sensors[i].fail("No valid external percentage"),
                    }
                }
            }
            self.ticks += 1;
            self.sensors.clone()
        }
    }
}
