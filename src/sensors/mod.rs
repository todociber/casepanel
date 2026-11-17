mod basic;
mod fps;
mod nvml;
mod pawnio;
pub mod system;
use crate::{model::Snapshot, platform};
pub use basic::Basic;
pub use fps::Fps;
pub struct Collector {
    basic: Basic,
    gpu: Option<nvml::Nvml>,
    hardware: Option<pawnio::Hardware>,
    memory: Option<pawnio::NvidiaMemory>,
    diagnostics: Vec<String>,
    tick: u64,
    fps: fps::Fps,
}
impl Collector {
    pub fn new() -> Self {
        let inventory = platform::inventory();
        let mut diagnostics = vec![format!(
            "CPU: {}; motherboard: {}",
            inventory.cpu_name, inventory.board
        )];
        let gpu = match nvml::Nvml::new() {
            Ok(g) => Some(g),
            Err(e) => {
                diagnostics.push(e);
                None
            }
        };
        let hardware = match pawnio::Hardware::new(inventory) {
            Ok(h) => Some(h),
            Err(e) => {
                diagnostics.push(e);
                None
            }
        };
        if let Some(h) = &hardware {
            diagnostics.extend(h.warnings.clone());
        }
        let memory = gpu.as_ref().and_then(|g| {
            g.pci.and_then(
                |p| match pawnio::NvidiaMemory::new(p, &g.memory_identity()) {
                    Ok(m) => Some(m),
                    Err(e) => {
                        diagnostics.push(e);
                        None
                    }
                },
            )
        });
        Self {
            basic: Basic::new(),
            gpu,
            hardware,
            memory,
            diagnostics,
            tick: 0,
            fps: fps::Fps::new(),
        }
    }
    pub fn poll(&mut self) -> Snapshot {
        let mut sensors = self.basic.poll();
        sensors.extend(self.fps.poll());
        if let Some(g) = &mut self.gpu {
            sensors.extend(g.poll(self.tick));
        }
        if let Some(m) = &mut self.memory
            && sensors
                .iter()
                .find(|s| s.key == "gpu.memory_temperature")
                .is_none_or(|s| s.quality != crate::model::Quality::Valid)
        {
            let sample = m.poll();
            sensors.retain(|s| s.key != "gpu.memory_temperature");
            sensors.push(sample);
        }
        if let Some(h) = &mut self.hardware {
            sensors.extend(h.poll(self.tick));
        }
        self.tick += 1;
        Snapshot::new(sensors, self.diagnostics.clone())
    }
    pub fn pause(&mut self) {
        self.fps.pause();
    }
}
