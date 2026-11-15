use sha2::{Digest, Sha256};
use windows::Win32::{Devices::Display::*, Foundation::*, Graphics::Gdi::*};
#[derive(Clone, Debug)]
pub struct Monitor {
    pub id: String,
    pub name: String,
    pub device: String,
    pub rect: RECT,
}
fn text(words: &[u16]) -> String {
    String::from_utf16_lossy(&words[..words.iter().position(|v| *v == 0).unwrap_or(words.len())])
}
unsafe extern "system" fn enum_monitor(
    handle: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    param: LPARAM,
) -> windows::core::BOOL {
    let monitors = &mut *(param.0 as *mut Vec<(String, RECT)>);
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if GetMonitorInfoW(handle, (&mut info as *mut MONITORINFOEXW).cast()).as_bool() {
        monitors.push((text(&info.szDevice), info.monitorInfo.rcMonitor));
    }
    true.into()
}
pub fn enumerate() -> Vec<Monitor> {
    unsafe {
        let mut geometry: Vec<(String, RECT)> = Vec::new();
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(enum_monitor),
            LPARAM((&mut geometry as *mut Vec<(String, RECT)>) as isize),
        );
        let mut result = Vec::new();
        for _ in 0..3 {
            let (mut n, mut m) = (0, 0);
            if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut n, &mut m).0 != 0 {
                return result;
            }
            if n > 64 || m > 512 {
                return result;
            }
            let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); n as usize];
            let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); m as usize];
            let status = QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut n,
                paths.as_mut_ptr(),
                &mut m,
                modes.as_mut_ptr(),
                None,
            );
            if status.0 == 122 {
                continue;
            }
            if status.0 != 0 {
                return result;
            }
            for p in paths.iter().take(n as usize) {
                let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
                    header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                        r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                        size: std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                        adapterId: p.targetInfo.adapterId,
                        id: p.targetInfo.id,
                    },
                    ..Default::default()
                };
                let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
                    header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                        r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                        size: std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                        adapterId: p.sourceInfo.adapterId,
                        id: p.sourceInfo.id,
                    },
                    ..Default::default()
                };
                if DisplayConfigGetDeviceInfo(&mut target.header) != 0
                    || DisplayConfigGetDeviceInfo(&mut source.header) != 0
                {
                    continue;
                }
                let device = text(&source.viewGdiDeviceName);
                let Some((_, rect)) = geometry.iter().find(|(name, _)| *name == device) else {
                    continue;
                };
                let path = text(&target.monitorDevicePath);
                let id = format!("{:x}", Sha256::digest(path.to_lowercase().as_bytes()));
                let name = text(&target.monitorFriendlyDeviceName);
                result.push(Monitor {
                    id,
                    name,
                    device,
                    rect: *rect,
                });
            }
            break;
        }
        result
    }
}
pub fn select<'a>(monitors: &'a [Monitor], id: &str, name: &str) -> Option<&'a Monitor> {
    if !id.is_empty()
        && let Some(m) = monitors.iter().find(|m| m.id == id)
    {
        return Some(m);
    }
    let matches: Vec<_> = monitors
        .iter()
        .filter(|m| m.name.eq_ignore_ascii_case(name))
        .collect();
    if matches.len() == 1 {
        Some(matches[0])
    } else {
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ambiguous_monitor_does_not_move_to_primary() {
        let m = Monitor {
            id: "a".into(),
            name: "same".into(),
            device: String::new(),
            rect: RECT::default(),
        };
        assert!(select(&[m.clone(), m], "", "same").is_none());
    }
}
