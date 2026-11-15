use crate::{
    model::{ClientCommand, MAX_FRAME, PROTOCOL_VERSION, Snapshot},
    platform::{self, RawHandle, SecurityAttributes, wide},
};
use std::{
    ffi::c_void,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::Duration,
};
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateNamedPipeW(
        name: *const u16,
        open: u32,
        mode: u32,
        instances: u32,
        out: u32,
        input: u32,
        timeout: u32,
        sa: *const SecurityAttributes,
    ) -> RawHandle;
    fn ConnectNamedPipe(pipe: RawHandle, overlapped: *mut c_void) -> i32;
    fn DisconnectNamedPipe(pipe: RawHandle) -> i32;
    fn PeekNamedPipe(
        pipe: RawHandle,
        buffer: *mut c_void,
        size: u32,
        read: *mut u32,
        available: *mut u32,
        left: *mut u32,
    ) -> i32;
    fn ReadFile(
        file: RawHandle,
        buffer: *mut c_void,
        size: u32,
        read: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn WriteFile(
        file: RawHandle,
        buffer: *const c_void,
        size: u32,
        written: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        sa: *const SecurityAttributes,
        creation: u32,
        flags: u32,
        template: RawHandle,
    ) -> RawHandle;
    fn SetNamedPipeHandleState(
        pipe: RawHandle,
        mode: *const u32,
        max: *const u32,
        timeout: *const u32,
    ) -> i32;
}
struct Pipe(RawHandle);
impl Drop for Pipe {
    fn drop(&mut self) {
        unsafe {
            platform::CloseHandle(self.0);
        }
    }
}
fn pipe_name() -> Result<String, String> {
    Ok(format!(
        "\\\\.\\pipe\\SensorPanel-v1-{}",
        platform::user_sid()?
    ))
}
fn write_frame<T: serde::Serialize>(pipe: &Pipe, value: &T) -> Result<(), String> {
    let mut bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() >= MAX_FRAME {
        return Err("IPC frame is too large".into());
    }
    bytes.push(b'\n');
    let mut sent = 0;
    while sent < bytes.len() {
        let mut n = 0;
        let ok = unsafe {
            WriteFile(
                pipe.0,
                bytes[sent..].as_ptr().cast(),
                (bytes.len() - sent) as u32,
                &mut n,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 || n == 0 {
            return Err("IPC channel is closed or full".into());
        }
        sent += n as usize;
    }
    Ok(())
}
fn read_available(pipe: &Pipe, buffer: &mut Vec<u8>) -> Result<Vec<Vec<u8>>, String> {
    unsafe {
        let mut available = 0;
        if PeekNamedPipe(
            pipe.0,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut available,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err("Channel disconnected".into());
        }
        if buffer.len() + available as usize > MAX_FRAME {
            return Err("IPC frame exceeds the size limit".into());
        }
        if available > 0 {
            let start = buffer.len();
            buffer.resize(start + available as usize, 0);
            let mut read = 0;
            if ReadFile(
                pipe.0,
                buffer[start..].as_mut_ptr().cast(),
                available,
                &mut read,
                std::ptr::null_mut(),
            ) == 0
            {
                return Err("IPC read failed".into());
            }
            buffer.truncate(start + read as usize);
        }
        let mut frames = Vec::new();
        while let Some(pos) = buffer.iter().position(|b| *b == b'\n') {
            let mut frame: Vec<_> = buffer.drain(..=pos).collect();
            frame.pop();
            frames.push(frame);
        }
        Ok(frames)
    }
}
fn connect() -> Result<Pipe, String> {
    unsafe {
        let h = CreateFileW(
            wide(pipe_name()?).as_ptr(),
            0x80000000 | 0x40000000,
            0,
            std::ptr::null(),
            3,
            0,
            0,
        );
        if h == -1 {
            return Err("Collector is not connected".into());
        }
        let mode = 1u32; // PIPE_NOWAIT: UI worker never blocks inside ReadFile.
        if SetNamedPipeHandleState(h, &mode, std::ptr::null(), std::ptr::null()) == 0 {
            platform::CloseHandle(h);
            return Err("Could not configure the named pipe".into());
        }
        Ok(Pipe(h))
    }
}
pub fn shutdown() -> Result<(), String> {
    let pipe = connect()?;
    write_frame(
        &pipe,
        &ClientCommand::Shutdown {
            version: PROTOCOL_VERSION,
        },
    )
}

pub struct Client {
    pub active: Arc<AtomicBool>,
    pub quit: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    pub receiver: Receiver<Snapshot>,
}
impl Client {
    pub fn start() -> Self {
        let active = Arc::new(AtomicBool::new(false));
        let quit = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, receiver) = mpsc::sync_channel(2);
        let (a, q, s) = (active.clone(), quit.clone(), stop.clone());
        let thread = std::thread::spawn(move || client_loop(a, q, s, tx));
        Self {
            active,
            quit,
            stop,
            thread: Some(thread),
            receiver,
        }
    }
    pub fn close(&mut self) {
        self.quit.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.stop.store(true, Ordering::Relaxed);
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}
fn client_loop(
    active: Arc<AtomicBool>,
    quit: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    tx: SyncSender<Snapshot>,
) {
    let clock = std::time::Instant::now();
    let mut recovery = RetryBudget::default();
    while !stop.load(Ordering::Relaxed) {
        if quit.load(Ordering::Relaxed) {
            let _ = shutdown();
            return;
        }
        if let Ok(pipe) = connect() {
            let mut buffer = Vec::new();
            let mut last = None;
            loop {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                if quit.load(Ordering::Relaxed) {
                    let _ = write_frame(
                        &pipe,
                        &ClientCommand::Shutdown {
                            version: PROTOCOL_VERSION,
                        },
                    );
                    return;
                }
                let state = active.load(Ordering::Relaxed);
                if last != Some(state) {
                    if write_frame(
                        &pipe,
                        &ClientCommand::Subscribe {
                            version: PROTOCOL_VERSION,
                            active: state,
                        },
                    )
                    .is_err()
                    {
                        break;
                    }
                    last = Some(state);
                }
                match read_available(&pipe, &mut buffer) {
                    Ok(frames) => {
                        for frame in frames {
                            match serde_json::from_slice::<Snapshot>(&frame) {
                                Ok(snapshot) if snapshot.validate().is_ok() => {
                                    let _ = tx.try_send(snapshot);
                                }
                                _ => return,
                            }
                        }
                    }
                    Err(_) => break,
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            recovery.last_ms = clock.elapsed().as_millis() as u64;
        } else if recovery.request(
            clock.elapsed().as_millis() as u64,
            active.load(Ordering::Relaxed),
        ) {
            use std::os::windows::process::CommandExt;
            let result = std::process::Command::new("C:\\Windows\\System32\\schtasks.exe")
                .args(["/Run", "/TN", "\\SensorPanel\\Collector"])
                .creation_flags(0x08000000)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
            crate::logging::write(&format!(
                "Collector restart attempt {}/3: {}",
                recovery.attempts,
                if result.is_ok_and(|s| s.success()) {
                    "requested"
                } else {
                    "task is unavailable"
                }
            ));
        }
        for _ in 0..10 {
            if stop.load(Ordering::Relaxed) || quit.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

#[derive(Default)]
struct RetryBudget {
    attempts: u8,
    last_ms: u64,
}
impl RetryBudget {
    fn request(&mut self, now: u64, active: bool) -> bool {
        if !active {
            self.last_ms = now;
            return false;
        }
        if self.attempts >= 3 || now.saturating_sub(self.last_ms) < 60_000 {
            return false;
        }
        self.attempts += 1;
        self.last_ms = now;
        true
    }
}

pub fn serve() -> Result<(), String> {
    unsafe {
        let identity = platform::user_sid()?;
        let sddl = wide(format!("D:P(A;;GA;;;SY)(A;;GA;;;{identity})"));
        let mut descriptor = std::ptr::null_mut();
        if platform::ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            1,
            &mut descriptor,
            std::ptr::null_mut(),
        ) == 0
        {
            return Err("Could not secure IPC".into());
        }
        let sa = SecurityAttributes {
            length: std::mem::size_of::<SecurityAttributes>() as u32,
            descriptor,
            inherit: 0,
        };
        let raw = CreateNamedPipeW(
            wide(pipe_name()?).as_ptr(),
            3 | 0x80000,
            1 | 8,
            1,
            65536,
            65536,
            0,
            &sa,
        );
        platform::LocalFree(descriptor);
        if raw == -1 {
            return Err(format!(
                "Could not create IPC; another instance may be running ({})",
                platform::GetLastError()
            ));
        }
        let pipe = Pipe(raw);
        let heartbeat = Arc::new(AtomicU64::new(crate::model::now_ms()));
        let armed = Arc::new(AtomicBool::new(false));
        let (hb, arm) = (heartbeat.clone(), armed.clone());
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(1));
                if arm.load(Ordering::Relaxed)
                    && crate::model::now_ms().saturating_sub(hb.load(Ordering::Relaxed)) > 10_000
                {
                    platform::TerminateProcess(platform::GetCurrentProcess(), 70);
                }
            }
        });
        let mut collector = None;
        loop {
            let _ = ConnectNamedPipe(pipe.0, std::ptr::null_mut());
            let mut available = 0;
            if PeekNamedPipe(
                pipe.0,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            ) == 0
            {
                if matches!(platform::GetLastError(), 109 | 232) {
                    DisconnectNamedPipe(pipe.0);
                }
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            let mut active = false;
            let mut buffer = Vec::new();
            let mut last_poll = std::time::Instant::now() - Duration::from_secs(2);
            while let Ok(frames) = read_available(&pipe, &mut buffer) {
                for frame in frames {
                    match serde_json::from_slice::<ClientCommand>(&frame) {
                        Ok(ClientCommand::Subscribe { version, active: a })
                            if version == PROTOCOL_VERSION =>
                        {
                            active = a
                        }
                        Ok(ClientCommand::Shutdown { version }) if version == PROTOCOL_VERSION => {
                            return Ok(());
                        }
                        _ => {
                            active = false;
                        }
                    }
                }
                if active && last_poll.elapsed() >= Duration::from_secs(1) {
                    heartbeat.store(crate::model::now_ms(), Ordering::Relaxed);
                    armed.store(true, Ordering::Relaxed);
                    let c = collector.get_or_insert_with(crate::sensors::Collector::new);
                    let snapshot = c.poll();
                    heartbeat.store(crate::model::now_ms(), Ordering::Relaxed);
                    armed.store(false, Ordering::Relaxed);
                    if write_frame(&pipe, &snapshot).is_err() {
                        break;
                    }
                    last_poll = std::time::Instant::now();
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            armed.store(false, Ordering::Relaxed);
            DisconnectNamedPipe(pipe.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovery_is_bounded_and_spaced() {
        let mut r = RetryBudget::default();
        assert!(!r.request(59_999, true));
        assert!(r.request(60_000, true));
        assert!(!r.request(60_001, true));
        assert!(r.request(120_000, true));
        assert!(r.request(180_000, true));
        assert!(!r.request(240_000, true));
    }
    #[test]
    fn hidden_monitor_does_not_restart_hardware() {
        let mut r = RetryBudget::default();
        assert!(!r.request(100_000, false));
        assert!(!r.request(100_001, true));
        assert_eq!(r.attempts, 0);
    }
}
