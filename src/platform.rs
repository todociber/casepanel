use serde::Serialize;
use std::{
    ffi::{OsStr, c_void},
    os::windows::ffi::OsStrExt,
    path::Path,
};

pub type RawHandle = isize;
pub fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(Some(0)).collect()
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct FileTime {
    pub low: u32,
    pub high: u32,
}
impl FileTime {
    pub fn ticks(self) -> u64 {
        ((self.high as u64) << 32) | self.low as u64
    }
}
#[repr(C)]
#[derive(Default)]
pub struct MemoryStatus {
    pub length: u32,
    pub load: u32,
    pub total_phys: u64,
    pub avail_phys: u64,
    pub total_page: u64,
    pub avail_page: u64,
    pub total_virtual: u64,
    pub avail_virtual: u64,
    pub avail_extended: u64,
}
#[repr(C)]
#[derive(Default)]
pub struct LocalTime {
    pub year: u16,
    pub month: u16,
    pub weekday: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    pub milliseconds: u16,
}
#[repr(C)]
pub struct SecurityAttributes {
    pub length: u32,
    pub descriptor: *mut c_void,
    pub inherit: i32,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    pub fn GetSystemTimes(idle: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
    pub fn GlobalMemoryStatusEx(status: *mut MemoryStatus) -> i32;
    pub fn GetLocalTime(time: *mut LocalTime);
    pub fn GetSystemFirmwareTable(provider: u32, id: u32, buffer: *mut c_void, size: u32) -> u32;
    pub fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    pub fn GetCurrentProcess() -> RawHandle;
    pub fn GetCurrentThread() -> RawHandle;
    pub fn SetThreadAffinityMask(thread: RawHandle, mask: usize) -> usize;
    pub fn CloseHandle(handle: RawHandle) -> i32;
    pub fn GetLastError() -> u32;
    pub fn LoadLibraryExW(path: *const u16, file: RawHandle, flags: u32) -> RawHandle;
    pub fn GetProcAddress(module: RawHandle, name: *const u8) -> *mut c_void;
    pub fn FreeLibrary(module: RawHandle) -> i32;
    pub fn LocalFree(memory: *mut c_void) -> *mut c_void;
    pub fn CreateMutexW(sa: *const SecurityAttributes, owner: i32, name: *const u16) -> RawHandle;
    pub fn WaitForSingleObject(handle: RawHandle, ms: u32) -> u32;
    pub fn ReleaseMutex(handle: RawHandle) -> i32;
    pub fn TerminateProcess(process: RawHandle, code: u32) -> i32;
    pub fn GetActiveProcessorCount(group: u16) -> u32;
}
#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegOpenKeyExW(
        key: RawHandle,
        path: *const u16,
        options: u32,
        access: u32,
        out: *mut RawHandle,
    ) -> u32;
    fn RegQueryValueExW(
        key: RawHandle,
        name: *const u16,
        reserved: *const u32,
        kind: *mut u32,
        data: *mut u8,
        size: *mut u32,
    ) -> u32;
    fn RegCloseKey(key: RawHandle) -> u32;
    fn OpenProcessToken(process: RawHandle, access: u32, token: *mut RawHandle) -> i32;
    fn GetTokenInformation(
        token: RawHandle,
        class: u32,
        info: *mut c_void,
        len: u32,
        needed: *mut u32,
    ) -> i32;
    fn ConvertSidToStringSidW(sid: *const c_void, out: *mut *mut u16) -> i32;
    pub fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        sddl: *const u16,
        revision: u32,
        out: *mut *mut c_void,
        size: *mut u32,
    ) -> i32;
}

pub fn atomic_replace(from: &Path, to: &Path) -> Result<(), String> {
    unsafe {
        if MoveFileExW(wide(from).as_ptr(), wide(to).as_ptr(), 1 | 8) != 0 {
            Ok(())
        } else {
            Err(format!("No se pudo guardar perfil: {}", GetLastError()))
        }
    }
}
pub fn registry_bytes(machine: bool, path: &str, name: &str) -> Option<(u32, Vec<u8>)> {
    unsafe {
        let root = if machine {
            0x80000002u32
        } else {
            0x80000001u32
        } as i32 as isize;
        let mut key = 0;
        if RegOpenKeyExW(root, wide(path).as_ptr(), 0, 0x20019 | 0x100, &mut key) != 0 {
            return None;
        }
        let mut kind = 0;
        let mut size = 0;
        if RegQueryValueExW(
            key,
            wide(name).as_ptr(),
            std::ptr::null(),
            &mut kind,
            std::ptr::null_mut(),
            &mut size,
        ) != 0
            || size > 65536
        {
            RegCloseKey(key);
            return None;
        }
        let mut bytes = vec![0; size as usize];
        let result = RegQueryValueExW(
            key,
            wide(name).as_ptr(),
            std::ptr::null(),
            &mut kind,
            bytes.as_mut_ptr(),
            &mut size,
        );
        RegCloseKey(key);
        if result == 0 {
            bytes.truncate(size as usize);
            Some((kind, bytes))
        } else {
            None
        }
    }
}
pub fn registry_string(machine: bool, path: &str, name: &str) -> Option<String> {
    let (kind, b) = registry_bytes(machine, path, name)?;
    if kind != 1 && kind != 2 {
        return None;
    }
    let words: Vec<u16> = b
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .take_while(|v| *v != 0)
        .collect();
    Some(String::from_utf16_lossy(&words))
}
pub fn registry_dword(path: &str, name: &str) -> Option<u32> {
    let (k, b) = registry_bytes(false, path, name)?;
    if k == 4 && b.len() == 4 {
        Some(u32::from_le_bytes(b.try_into().ok()?))
    } else {
        None
    }
}
pub fn user_sid() -> Result<String, String> {
    unsafe {
        let mut token = 0;
        if OpenProcessToken(GetCurrentProcess(), 8, &mut token) == 0 {
            return Err("No se pudo consultar usuario".into());
        }
        let mut n = 0;
        GetTokenInformation(token, 1, std::ptr::null_mut(), 0, &mut n);
        // Word-aligned storage: TOKEN_USER contains a native pointer.
        let mut data = vec![0usize; (n as usize).div_ceil(std::mem::size_of::<usize>())];
        let ok = GetTokenInformation(token, 1, data.as_mut_ptr().cast(), n, &mut n);
        CloseHandle(token);
        if ok == 0 {
            return Err("No se pudo leer SID".into());
        }
        let sid = *(data.as_ptr() as *const *const c_void);
        let mut text = std::ptr::null_mut();
        if ConvertSidToStringSidW(sid, &mut text) == 0 {
            return Err("SID inválido".into());
        }
        let mut len = 0;
        while *text.add(len) != 0 && len < 256 {
            len += 1;
        }
        let result = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
        LocalFree(text.cast());
        Ok(result)
    }
}

pub struct Dll(pub RawHandle);
impl Dll {
    pub fn load(path: &Path) -> Result<Self, String> {
        unsafe {
            let h = LoadLibraryExW(wide(path).as_ptr(), 0, 0x100 | 0x800);
            if h == 0 {
                Err(format!(
                    "No se pudo cargar {} ({})",
                    path.display(),
                    GetLastError()
                ))
            } else {
                Ok(Self(h))
            }
        }
    }
    pub unsafe fn function<T: Copy>(&self, name: &[u8]) -> Option<T> {
        let p = GetProcAddress(self.0, name.as_ptr());
        if p.is_null() {
            None
        } else {
            Some(std::mem::transmute_copy(&p))
        }
    }
}
impl Drop for Dll {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.0);
        }
    }
}

pub struct BusLock(RawHandle);
impl BusLock {
    pub fn acquire(name: &str) -> Option<Self> {
        unsafe {
            let h = CreateMutexW(std::ptr::null(), 0, wide(name).as_ptr());
            if h == 0 {
                return None;
            }
            let result = WaitForSingleObject(h, 10);
            if result == 0 || result == 0x80 {
                Some(Self(h))
            } else {
                CloseHandle(h);
                None
            }
        }
    }
}
impl Drop for BusLock {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}
pub struct Affinity(usize);
impl Affinity {
    pub fn set(cpu: u32) -> Option<Self> {
        if cpu >= usize::BITS {
            return None;
        }
        let old = unsafe { SetThreadAffinityMask(GetCurrentThread(), 1usize << cpu) };
        if old == 0 { None } else { Some(Self(old)) }
    }
}
impl Drop for Affinity {
    fn drop(&mut self) {
        unsafe {
            SetThreadAffinityMask(GetCurrentThread(), self.0);
        }
    }
}

pub struct Instance(RawHandle);
impl Instance {
    pub fn claim(tag: &str) -> Result<Option<Self>, String> {
        unsafe {
            let name = wide(format!("Local\\SensorPanel-{tag}-{}", user_sid()?));
            let h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
            if h == 0 {
                return Err("No se pudo crear bloqueo de instancia".into());
            }
            if GetLastError() == 183 {
                CloseHandle(h);
                Ok(None)
            } else {
                Ok(Some(Self(h)))
            }
        }
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Inventory {
    pub cpu_name: String,
    pub cpu_family: u32,
    pub cpu_model: u32,
    pub cpu_id: String,
    pub board: String,
    pub board_vendor: String,
    pub memory_speed_mt: u32,
    pub memory_type: String,
}
pub fn inventory() -> Inventory {
    let mut result = Inventory {
        cpu_name: String::new(),
        cpu_family: 0,
        cpu_model: 0,
        cpu_id: String::new(),
        board: registry_string(
            true,
            "HARDWARE\\DESCRIPTION\\System\\BIOS",
            "BaseBoardProduct",
        )
        .unwrap_or_default(),
        board_vendor: registry_string(
            true,
            "HARDWARE\\DESCRIPTION\\System\\BIOS",
            "BaseBoardManufacturer",
        )
        .unwrap_or_default(),
        memory_speed_mt: 0,
        memory_type: String::new(),
    };
    {
        use std::arch::x86_64::__cpuid;
        let id = __cpuid(1).eax;
        let base_family = (id >> 8) & 15;
        result.cpu_family = base_family
            + if base_family == 15 {
                (id >> 20) & 255
            } else {
                0
            };
        result.cpu_model = ((id >> 4) & 15)
            | if base_family == 6 || base_family == 15 {
                ((id >> 16) & 15) << 4
            } else {
                0
            };
        result.cpu_id = format!("{:x}:{:x}", result.cpu_family, result.cpu_model);
        let mut bytes = Vec::new();
        for n in 0x80000002..=0x80000004 {
            let r = __cpuid(n);
            for x in [r.eax, r.ebx, r.ecx, r.edx] {
                bytes.extend(x.to_le_bytes());
            }
        }
        result.cpu_name = String::from_utf8_lossy(&bytes)
            .trim_matches('\0')
            .trim()
            .to_string();
    }
    if let Some((rate, kind)) = memory_info() {
        result.memory_speed_mt = rate;
        result.memory_type = kind;
    }
    result
}
fn memory_info() -> Option<(u32, String)> {
    unsafe {
        let provider = u32::from_be_bytes(*b"RSMB");
        let n = GetSystemFirmwareTable(provider, 0, std::ptr::null_mut(), 0);
        if !(8..=1_048_576).contains(&n) {
            return None;
        }
        let mut bytes = vec![0; n as usize];
        if GetSystemFirmwareTable(provider, 0, bytes.as_mut_ptr().cast(), n) != n {
            return None;
        }
        let mut offset = 8;
        let mut rate = 0;
        let mut kind = String::new();
        while offset + 4 <= bytes.len() {
            let ty = bytes[offset];
            let len = bytes[offset + 1] as usize;
            if len < 4 || offset + len > bytes.len() {
                break;
            }
            if ty == 17 && len >= 0x22 {
                let size = u16::from_le_bytes([bytes[offset + 12], bytes[offset + 13]]);
                if size != 0 {
                    rate = rate.max(
                        u16::from_le_bytes([bytes[offset + 0x20], bytes[offset + 0x21]]) as u32,
                    );
                    kind = match bytes[offset + 0x12] {
                        0x22 => "DDR5",
                        0x1a => "DDR4",
                        _ => "RAM",
                    }
                    .into();
                }
            }
            offset += len;
            while offset + 1 < bytes.len() && !(bytes[offset] == 0 && bytes[offset + 1] == 0) {
                offset += 1;
            }
            offset += 2;
            if ty == 127 {
                break;
            }
        }
        Some((rate, kind))
    }
}
