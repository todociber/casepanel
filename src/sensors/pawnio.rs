// Reading algorithms adapted from LibreHardwareMonitor (MPL-2.0).
// Copyright LibreHardwareMonitor and Contributors. See THIRD-PARTY-NOTICES.md.
use crate::{
    model::Sensor,
    platform::{self, Affinity, BusLock, Dll, Inventory, RawHandle},
};
use std::{path::PathBuf, time::Instant};
type Open = unsafe extern "system" fn(*mut RawHandle) -> i32;
type Load = unsafe extern "system" fn(RawHandle, *const u8, usize) -> i32;
type Execute = unsafe extern "system" fn(
    RawHandle,
    *const u8,
    *const u64,
    usize,
    *mut u64,
    usize,
    *mut usize,
) -> i32;
struct Module {
    _dll: Dll,
    handle: RawHandle,
    execute: Execute,
}
impl Module {
    fn new(blob: &[u8]) -> Result<Self, String> {
        unsafe {
            let path = platform::registry_string(
                true,
                "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\PawnIO",
                "InstallLocation",
            )
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("C:\\Program Files\\PawnIO"));
            let dll = Dll::load(&path.join("PawnIOLib.dll"))?;
            let open: Open = dll
                .function(b"pawnio_open\0")
                .ok_or("PawnIO is missing pawnio_open")?;
            let load: Load = dll
                .function(b"pawnio_load\0")
                .ok_or("PawnIO is missing pawnio_load")?;
            let execute: Execute = dll
                .function(b"pawnio_execute\0")
                .ok_or("PawnIO is missing pawnio_execute")?;
            let mut handle = 0;
            let status = open(&mut handle);
            if status < 0 {
                return Err(format!(
                    "PawnIO access denied or not installed (HRESULT {status:08X}); run the collector elevated"
                ));
            }
            let status = load(handle, blob.as_ptr(), blob.len());
            if status < 0 {
                platform::CloseHandle(handle);
                return Err(format!("Signed PawnIO module rejected ({status:08X})"));
            }
            Ok(Self {
                _dll: dll,
                handle,
                execute,
            })
        }
    }
    fn call(&self, function: &[u8], input: &[u64], output: usize) -> Result<Vec<u64>, String> {
        unsafe {
            let mut result = vec![0; output];
            let mut returned = 0;
            let status = (self.execute)(
                self.handle,
                function.as_ptr(),
                input.as_ptr(),
                input.len(),
                result.as_mut_ptr(),
                output,
                &mut returned,
            );
            if status < 0 {
                return Err(format!("PawnIO HRESULT {status:08X}"));
            }
            if returned != output {
                return Err("Incomplete PawnIO response".into());
            }
            Ok(result)
        }
    }
    fn read_msr(&self, register: u32) -> Result<u64, String> {
        Ok(self.call(b"ioctl_read_msr\0", &[register as u64], 1)?[0])
    }
}
impl Drop for Module {
    fn drop(&mut self) {
        unsafe {
            platform::CloseHandle(self.handle);
        }
    }
}
struct Chip {
    module: Module,
    id: u16,
    base: u16,
    slot: u8,
}
impl Chip {
    fn pio_read(module: &Module, port: u16) -> Result<u8, String> {
        Ok(module.call(b"ioctl_pio_inb\0", &[port as u64], 1)?[0] as u8)
    }
    fn pio_write(module: &Module, port: u16, value: u8) -> Result<(), String> {
        module.call(b"ioctl_pio_outb\0", &[port as u64, value as u64], 0)?;
        Ok(())
    }
    fn discover(slot: u8) -> Result<Self, String> {
        let _lock =
            BusLock::acquire("Global\\Access_ISABUS.HTP.Method").ok_or("ISA bus is busy")?;
        let module = Module::new(include_bytes!("../../vendor/pawnio/modules/LpcIO.bin"))?;
        module.call(b"ioctl_select_slot\0", &[slot as u64], 0)?;
        let port = if slot == 0 { 0x2e } else { 0x4e };
        let read_word = |r: u8| -> Result<u16, String> {
            Ok(module.call(b"ioctl_superio_inw\0", &[r as u64], 1)?[0] as u16)
        };
        let mut id = read_word(0x20)?;
        if slot == 0 || id == 0xffff {
            for b in [0x87, 0x01, 0x55, if slot == 0 { 0x55 } else { 0xaa }] {
                Self::pio_write(&module, port, b)?;
            }
            id = read_word(0x20)?;
        }
        let found = (|| {
            if !matches!(id, 0x8696 | 0x8695) {
                return Err(format!("Unsupported ITE chip in slot {slot}: {id:04x}"));
            }
            module.call(b"ioctl_find_bars\0", &[], 0)?;
            module.call(b"ioctl_superio_outb\0", &[0x07, 0x04], 0)?;
            let base = read_word(0x60)?;
            std::thread::sleep(std::time::Duration::from_millis(1));
            if base != read_word(0x60)? || base < 0x100 || (base & 0xf007) != 0 {
                return Err("Invalid ITE address".into());
            }
            Ok(base)
        })();
        if slot == 0 {
            Self::pio_write(&module, port, 0x02)?;
            Self::pio_write(&module, port + 1, 0x02)?;
        }
        Ok(Self {
            module,
            id,
            base: found?,
            slot,
        })
    }
    fn read_register(&self, register: u8) -> Result<u8, String> {
        // Only the index latch is written, never fan/PWM/voltage registers.
        Self::pio_write(&self.module, self.base + 5, register)?;
        let value = Self::pio_read(&self.module, self.base + 6)?;
        let verified = Self::pio_read(&self.module, self.base + 5)?;
        if register != verified {
            return Err("ITE index changed during the read".into());
        }
        Ok(value)
    }
    fn fan(&self, index: usize) -> Result<Option<f64>, String> {
        const LO: [u8; 6] = [0x0d, 0x0e, 0x0f, 0x80, 0x82, 0x4c];
        const HI: [u8; 6] = [0x18, 0x19, 0x1a, 0x81, 0x83, 0x4d];
        if index >= if self.id == 0x8696 { 6 } else { 3 } {
            return Ok(None);
        }
        let vendor = self.read_register(0x58)?;
        if vendor != 0x90 && vendor != 0x7f {
            return Err(format!("Invalid ITE vendor: {vendor:02x}"));
        }
        let modes = self.read_register(0x0c)?;
        if index >= 3 && modes & (1 << (index + 1)) == 0 {
            return Ok(None);
        }
        let count =
            self.read_register(LO[index])? as u16 | ((self.read_register(HI[index])? as u16) << 8);
        Ok(fan_rpm(count))
    }
}
pub fn fan_rpm(counter: u16) -> Option<f64> {
    if counter == 0xffff {
        Some(0.)
    } else if counter > 0x3f {
        Some(1_350_000. / (counter as f64 * 2.))
    } else {
        None
    }
}
pub fn decode_temperature(raw: u32) -> f64 {
    ((raw >> 21) as f64) * 0.125 - if raw & (1 << 19) != 0 { 49. } else { 0. }
}
pub fn package_watts(previous: u32, current: u32, unit: u32, seconds: f64) -> Option<f64> {
    if !(0.1..=5.).contains(&seconds) || unit > 31 {
        return None;
    }
    let value = current.wrapping_sub(previous) as f64 * 2f64.powi(-(unit as i32)) / seconds;
    if (0. ..=1000.).contains(&value) {
        Some(value)
    } else {
        None
    }
}
pub struct Hardware {
    amd: Module,
    chips: Vec<Chip>,
    sensors: Vec<Sensor>,
    energy: Option<(u32, Instant)>,
    cpu_count: u32,
    pub warnings: Vec<String>,
}
impl Hardware {
    pub fn new(inventory: Inventory) -> Result<Self, String> {
        if inventory.cpu_family != 0x1a
            || inventory.cpu_model != 0x44
            || !inventory.cpu_name.contains("9900X")
        {
            return Err("CPU is outside the validated list; skipping low-level AMD access".into());
        }
        let amd = Module::new(include_bytes!(
            "../../vendor/pawnio/modules/AMDFamily17.bin"
        ))?;
        let cpu = format!("cpu-hw:amd:{}:{}", inventory.cpu_id, inventory.cpu_name);
        let board = format!("board:{}:{}", inventory.board_vendor, inventory.board);
        let mut sensors = vec![
            Sensor::new(&cpu, "cpu.temperature", "CPU Tctl/Tdie", "°C", 1000),
            Sensor::new(&cpu, "cpu.clock", "CPU maximum core clock", "GHz", 1000),
            Sensor::new(&cpu, "cpu.power", "CPU Package", "W", 1000),
            Sensor::new(&board, "cpu.fan", "CPU fan / primary ITE #1", "RPM", 2000),
            Sensor::new(
                &board,
                "board.case_fan",
                "System fan #4 / secondary ITE #3",
                "RPM",
                2000,
            ),
        ];
        let mut chips = Vec::new();
        let mut warnings = Vec::new();
        if inventory.board == "X870 AORUS ELITE WIFI7"
            && inventory.board_vendor.to_lowercase().contains("gigabyte")
        {
            for slot in 0..2 {
                match Chip::discover(slot) {
                    Ok(chip) => {
                        warnings.push(format!(
                            "ITE slot {} chip {:04x} base {:04x}",
                            chip.slot, chip.id, chip.base
                        ));
                        chips.push(chip);
                    }
                    Err(e) => warnings.push(e),
                }
            }
        } else {
            for s in &mut sensors[3..] {
                s.fail("Motherboard is outside the validated list");
            }
        }
        Ok(Self {
            amd,
            chips,
            sensors,
            energy: None,
            cpu_count: unsafe { platform::GetActiveProcessorCount(0xffff) }.min(64),
            warnings,
        })
    }
    pub fn poll(&mut self, tick: u64) -> Vec<Sensor> {
        if let Some(_lock) = BusLock::acquire("Global\\Access_PCI") {
            match self.amd.call(b"ioctl_read_smn\0", &[0x00059800], 1) {
                Ok(v) => {
                    let t = decode_temperature(v[0] as u32);
                    if (-20. ..=120.).contains(&t) {
                        self.sensors[0].set(t);
                    } else {
                        self.sensors[0].fail("CPU temperature is out of range");
                    }
                }
                Err(e) => self.sensors[0].fail(e),
            }
        } else {
            self.sensors[0].fail("PCI bus is busy");
        }
        if let Some(_affinity) = Affinity::set(0) {
            match (self.amd.read_msr(0xc0010299), self.amd.read_msr(0xc001029b)) {
                (Ok(units), Ok(energy)) => {
                    let now = Instant::now();
                    if let Some((old, t)) = self.energy {
                        if let Some(w) = package_watts(
                            old,
                            energy as u32,
                            ((units >> 8) & 31) as u32,
                            now.duration_since(t).as_secs_f64(),
                        ) {
                            self.sensors[2].set(w);
                        } else {
                            self.sensors[2]
                                .fail("Energy counter reset or invalid sampling interval");
                        }
                    }
                    self.energy = Some((energy as u32, now));
                }
                _ => self.sensors[2].fail("Could not read package energy"),
            }
        }
        let mut max_mhz = 0u64;
        for cpu in (0..self.cpu_count).step_by(2) {
            if let Some(_affinity) = Affinity::set(cpu)
                && let Ok(raw) = self.amd.read_msr(0xc0010293)
            {
                max_mhz = max_mhz.max((raw & 0xfff) * 5);
            }
        }
        if (100..=7000).contains(&max_mhz) {
            self.sensors[1].set(max_mhz as f64 / 1000.);
        } else {
            self.sensors[1].fail("CPU clock is unavailable");
        }
        if tick.is_multiple_of(2) {
            if let Some(_lock) = BusLock::acquire("Global\\Access_ISABUS.HTP.Method") {
                for (s, slot, index) in [(3, 0, 0), (4, 1, 2)] {
                    match self
                        .chips
                        .iter()
                        .find(|c| c.slot == slot)
                        .map(|c| c.fan(index))
                    {
                        Some(Ok(Some(rpm))) => self.sensors[s].set(rpm),
                        Some(Err(e)) => self.sensors[s].fail(e),
                        _ => self.sensors[s].fail("Fan channel is unavailable"),
                    }
                }
            } else {
                for s in &mut self.sensors[3..] {
                    s.fail("ISA bus is busy");
                }
            }
        }
        self.sensors.clone()
    }
}
pub struct NvidiaMemory {
    module: Module,
    bus: u32,
    device: u32,
    sensor: Sensor,
}
impl NvidiaMemory {
    pub fn new(pci: super::nvml::Pci, identity: &str) -> Result<Self, String> {
        if (pci.id & 0xffff) != 0x10de || ((pci.id >> 16) & 0xff00) != 0x2c00 {
            return Err("GPU is outside the validated NVIDIA list".into());
        }
        Ok(Self {
            module: Module::new(include_bytes!("../../vendor/pawnio/modules/Nvidia.bin"))?,
            bus: pci.bus,
            device: pci.device,
            sensor: Sensor::new(
                identity,
                "gpu.memory_temperature",
                "Maximum GPU memory temperature",
                "°C",
                1000,
            ),
        })
    }
    pub fn poll(&mut self) -> Sensor {
        match self.module.call(
            b"ioctl_read_memory_temperatures\0",
            &[self.bus as u64, self.device as u64, 0],
            50,
        ) {
            Ok(raw) if raw[1] == 48 => {
                let value = raw[2..]
                    .iter()
                    .map(|v| *v as i32)
                    .filter(|v| (-20..=150).contains(v))
                    .max();
                if let Some(v) = value {
                    self.sensor.set(v as f64);
                } else {
                    self.sensor.fail("No valid memory temperatures");
                }
            }
            Ok(_) => self.sensor.fail("Unexpected NVIDIA response format"),
            Err(e) => self.sensor.fail(e),
        }
        self.sensor.clone()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fan_stopped_is_not_missing() {
        assert_eq!(fan_rpm(0xffff), Some(0.));
        assert_eq!(fan_rpm(0), None);
        assert!((fan_rpm(450).unwrap() - 1500.).abs() < 0.01);
    }
    #[test]
    fn energy_wrap_is_modular() {
        assert_eq!(package_watts(u32::MAX - 4, 5, 0, 1.).unwrap(), 10.);
    }
    #[test]
    fn sleep_discards_power_interval() {
        assert!(package_watts(0, 10, 0, 60.).is_none());
    }
    #[test]
    fn amd_temperature_offset() {
        assert_eq!(decode_temperature(600 << 21), 75.);
        assert_eq!(decode_temperature((600 << 21) | (1 << 19)), 26.);
    }
}
