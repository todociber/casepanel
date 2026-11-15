use crate::{model::Sensor, platform::Dll};
use std::{
    ffi::{CStr, c_void},
    path::Path,
};
type Device = *mut c_void;
type Init = unsafe extern "C" fn() -> u32;
type Count = unsafe extern "C" fn(*mut u32) -> u32;
type Handle = unsafe extern "C" fn(u32, *mut Device) -> u32;
type Text = unsafe extern "C" fn(Device, *mut i8, u32) -> u32;
type GetUint = unsafe extern "C" fn(Device, *mut u32) -> u32;
type GetIndexed = unsafe extern "C" fn(Device, u32, *mut u32) -> u32;
#[repr(C)]
#[derive(Default)]
struct Utilization {
    gpu: u32,
    memory: u32,
}
#[repr(C)]
#[derive(Default)]
struct Memory {
    total: u64,
    free: u64,
    used: u64,
}
#[repr(C)]
#[derive(Default)]
struct Fan {
    version: u32,
    index: u32,
    speed: u32,
}
#[repr(C)]
#[derive(Default)]
struct Field {
    field_id: u32,
    scope: u32,
    timestamp: i64,
    latency: i64,
    kind: u32,
    status: u32,
    value: u64,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Pci {
    legacy: [i8; 16],
    pub domain: u32,
    pub bus: u32,
    pub device: u32,
    pub id: u32,
    subsystem: u32,
    bus_id: [i8; 32],
}
pub struct Nvml {
    _dll: Dll,
    device: Device,
    shutdown: Init,
    temp: Option<GetIndexed>,
    clock: Option<GetIndexed>,
    power: Option<GetUint>,
    util: Option<unsafe extern "C" fn(Device, *mut Utilization) -> u32>,
    memory: Option<unsafe extern "C" fn(Device, *mut Memory) -> u32>,
    fan: Option<unsafe extern "C" fn(Device, *mut Fan) -> u32>,
    fields: Option<unsafe extern "C" fn(Device, i32, *mut Field) -> u32>,
    sensors: Vec<Sensor>,
    pub pci: Option<Pci>,
}
impl Nvml {
    pub fn new() -> Result<Self, String> {
        unsafe {
            let dll = Dll::load(Path::new("C:\\Windows\\System32\\nvml.dll"))?;
            let init: Init = dll
                .function(b"nvmlInit_v2\0")
                .ok_or("NVML no ofrece inicialización")?;
            let shutdown: Init = dll.function(b"nvmlShutdown\0").ok_or("NVML incompleta")?;
            let count: Count = dll
                .function(b"nvmlDeviceGetCount_v2\0")
                .ok_or("NVML sin enumeración")?;
            let handle: Handle = dll
                .function(b"nvmlDeviceGetHandleByIndex_v2\0")
                .ok_or("NVML sin dispositivos")?;
            let uuid: Text = dll
                .function(b"nvmlDeviceGetUUID\0")
                .ok_or("NVML sin identidad persistente")?;
            let name: Text = dll
                .function(b"nvmlDeviceGetName\0")
                .ok_or("NVML sin nombre")?;
            let status = init();
            if status != 0 {
                return Err(format!("NVML inicialización: {status}"));
            }
            let mut n = 0;
            if count(&mut n) != 0 || n == 0 {
                shutdown();
                return Err("No hay GPU NVIDIA disponible".into());
            }
            let mut chosen = None;
            for i in 0..n.min(16) {
                let mut d = std::ptr::null_mut();
                if handle(i, &mut d) != 0 {
                    continue;
                }
                let mut buf = [0i8; 256];
                if name(d, buf.as_mut_ptr(), 256) != 0 {
                    continue;
                }
                let label = CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned();
                if chosen.is_none() || label.contains("RTX 5080") {
                    chosen = Some((d, label));
                    if chosen.as_ref().unwrap().1.contains("RTX 5080") {
                        break;
                    }
                }
            }
            let Some((device, label)) = chosen else {
                shutdown();
                return Err("No se pudo abrir GPU NVIDIA".into());
            };
            let mut id = [0i8; 128];
            if uuid(device, id.as_mut_ptr(), 128) != 0 {
                shutdown();
                return Err("GPU sin UUID; no se asociará por índice".into());
            }
            let identity = format!("nvidia:{}", CStr::from_ptr(id.as_ptr()).to_string_lossy());
            let sensors = [
                ("gpu.temperature", "Temperatura GPU", "°C", 1000),
                ("gpu.load", "Carga GPU", "%", 1000),
                ("gpu.clock", "Frecuencia GPU", "MHz", 1000),
                ("gpu.power", "Consumo GPU", "W", 1000),
                ("gpu.fan", "Ventilador GPU 1 (reportado)", "RPM", 2000),
                ("gpu.vram.used", "VRAM usada", "GiB", 1000),
                ("gpu.vram.free", "VRAM libre", "GiB", 1000),
                ("gpu.vram.load", "Utilización VRAM", "%", 1000),
                (
                    "gpu.memory_temperature",
                    "Temperatura memoria GPU",
                    "°C",
                    1000,
                ),
            ]
            .into_iter()
            .map(|(key, l, u, interval)| {
                Sensor::new(&identity, key, &format!("{label}: {l}"), u, interval)
            })
            .collect();
            let mut pci = Pci::default();
            let pci_fn: Option<unsafe extern "C" fn(Device, *mut Pci) -> u32> =
                dll.function(b"nvmlDeviceGetPciInfo_v3\0");
            let pci = if pci_fn.is_some_and(|f| f(device, &mut pci) == 0) {
                Some(pci)
            } else {
                None
            };
            Ok(Self {
                device,
                shutdown,
                temp: dll.function(b"nvmlDeviceGetTemperature\0"),
                clock: dll.function(b"nvmlDeviceGetClockInfo\0"),
                power: dll.function(b"nvmlDeviceGetPowerUsage\0"),
                util: dll.function(b"nvmlDeviceGetUtilizationRates\0"),
                memory: dll.function(b"nvmlDeviceGetMemoryInfo\0"),
                fan: dll.function(b"nvmlDeviceGetFanSpeedRPM\0"),
                fields: dll.function(b"nvmlDeviceGetFieldValues\0"),
                _dll: dll,
                sensors,
                pci,
            })
        }
    }
    pub fn poll(&mut self, tick: u64) -> Vec<Sensor> {
        unsafe {
            let mut value = 0;
            if self
                .temp
                .is_some_and(|f| f(self.device, 0, &mut value) == 0)
            {
                self.sensors[0].set(value as f64);
            } else {
                self.sensors[0].fail("Temperatura no soportada por NVML");
            }
            let mut util = Utilization::default();
            if self.util.is_some_and(|f| f(self.device, &mut util) == 0) {
                self.sensors[1].set(util.gpu as f64);
            } else {
                self.sensors[1].fail("Carga no disponible");
            }
            if self
                .clock
                .is_some_and(|f| f(self.device, 0, &mut value) == 0)
            {
                self.sensors[2].set(value as f64);
            } else {
                self.sensors[2].fail("Frecuencia no disponible");
            }
            if self.power.is_some_and(|f| f(self.device, &mut value) == 0) {
                self.sensors[3].set(value as f64 / 1000.);
            } else {
                self.sensors[3].fail("Consumo no disponible");
            }
            if tick.is_multiple_of(2) {
                let mut fan = Fan {
                    version: std::mem::size_of::<Fan>() as u32 | (1 << 24),
                    ..Default::default()
                };
                if self.fan.is_some_and(|f| f(self.device, &mut fan) == 0) {
                    self.sensors[4].set(fan.speed as f64);
                } else {
                    self.sensors[4].fail("RPM no disponibles; no se convierten porcentajes a RPM");
                }
            }
            let mut mem = Memory::default();
            if self.memory.is_some_and(|f| f(self.device, &mut mem) == 0) && mem.total > 0 {
                self.sensors[5].set(mem.used as f64 / 1073741824.);
                self.sensors[6].set(mem.free as f64 / 1073741824.);
                self.sensors[7].set(mem.used as f64 / mem.total as f64 * 100.);
            } else {
                for s in &mut self.sensors[5..8] {
                    s.fail("Memoria global no disponible por NVML/WDDM");
                }
            }
            let mut field = Field {
                field_id: 82,
                ..Default::default()
            };
            if self
                .fields
                .is_some_and(|f| f(self.device, 1, &mut field) == 0)
                && field.status == 0
            {
                let v = match field.kind {
                    0 => f64::from_bits(field.value),
                    1 | 2 => (field.value as u32) as f64,
                    3 => field.value as f64,
                    4 => (field.value as i64) as f64,
                    _ => f64::NAN,
                };
                if (-20. ..=150.).contains(&v) {
                    self.sensors[8].set(v);
                } else {
                    self.sensors[8].fail("Temperatura memoria inválida");
                }
            } else {
                self.sensors[8].fail("NVML no expone temperatura de memoria");
            }
            self.sensors.clone()
        }
    }
    pub fn memory_identity(&self) -> String {
        self.sensors[8].device.clone()
    }
}
impl Drop for Nvml {
    fn drop(&mut self) {
        unsafe {
            (self.shutdown)();
        }
    }
}
