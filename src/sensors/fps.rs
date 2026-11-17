//! Rendered frame rate from a pinned, isolated PresentMon console process.
//! Only presentation timing is collected; input and GPU tracking are disabled.
use crate::model::Sensor;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    io::{BufRead, BufReader},
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::*,
    UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
};

const HASH: &str = "364e5d98d4d134bd54dd25c22ed2ca2f4883f8bc3ed6502bee0c151e3436d30c";
const EXCLUDED: &[&str] = &[
    "dwm.exe",
    "explorer.exe",
    "sensorpanel.exe",
    "presentmon.exe",
    "codex.exe",
    "chrome.exe",
    "msedge.exe",
    "firefox.exe",
    "brave.exe",
    "opera.exe",
    "discord.exe",
    "steam.exe",
    "steamwebhelper.exe",
    "epicgameslauncher.exe",
    "epicwebhelper.exe",
    "searchhost.exe",
    "startmenuexperiencehost.exe",
    "applicationframehost.exe",
    "shellexperiencehost.exe",
    "taskmgr.exe",
];

#[derive(Default)]
struct Stream {
    name: String,
    last: Option<Instant>,
    frames: VecDeque<(f64, f64)>,
}
#[derive(Default)]
struct Frames {
    streams: BTreeMap<(u32, String), Stream>,
    selected: Option<u32>,
}
struct Frame {
    name: String,
    pid: u32,
    swapchain: String,
    seconds: f64,
    milliseconds: f64,
}
struct Columns {
    application: usize,
    pid: usize,
    swapchain: usize,
    seconds: usize,
    milliseconds: usize,
}
impl Columns {
    fn parse(line: &str) -> Option<Self> {
        let fields = csv_fields(line)?;
        let column = |name: &str| fields.iter().position(|v| v.eq_ignore_ascii_case(name));
        Some(Self {
            application: column("Application")?,
            pid: column("ProcessID")?,
            swapchain: column("SwapChainAddress")?,
            seconds: column("TimeInSeconds")?,
            milliseconds: column("MsBetweenPresents")?,
        })
    }
    fn frame(&self, line: &str) -> Option<Frame> {
        let fields = csv_fields(line)?;
        let name = fields.get(self.application)?.to_string();
        if name.len() > 256 || EXCLUDED.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
            return None;
        }
        let seconds: f64 = fields.get(self.seconds)?.parse().ok()?;
        let milliseconds: f64 = fields.get(self.milliseconds)?.parse().ok()?;
        if !seconds.is_finite()
            || seconds < 0.
            || !milliseconds.is_finite()
            || !(0.1..=1000.).contains(&milliseconds)
        {
            return None;
        }
        Some(Frame {
            name,
            pid: fields.get(self.pid)?.parse().ok()?,
            swapchain: fields.get(self.swapchain)?.to_string(),
            seconds,
            milliseconds,
        })
    }
}
// PresentMon quotes application names; a comma in a file name must not shift columns.
fn csv_fields(line: &str) -> Option<Vec<String>> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line
        .trim()
        .trim_start_matches('\u{feff}')
        .chars()
        .peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut field)),
            _ => field.push(ch),
        }
    }
    if quoted {
        return None;
    }
    fields.push(field);
    Some(fields)
}
impl Frames {
    fn push(&mut self, frame: Frame, now: Instant) {
        // Retain at most 64 swap chains and 4096 frame intervals per chain.
        self.streams.retain(|_, s| {
            s.last
                .is_some_and(|t| now.duration_since(t) < Duration::from_secs(3))
        });
        let key = (frame.pid, frame.swapchain);
        if self.streams.len() >= 64 && !self.streams.contains_key(&key) {
            return;
        }
        let stream = self.streams.entry(key).or_default();
        stream.name = frame.name;
        stream.last = Some(now);
        if stream
            .frames
            .back()
            .is_some_and(|(time, _)| *time >= frame.seconds)
        {
            stream.frames.clear();
        }
        stream.frames.push_back((frame.seconds, frame.milliseconds));
        while stream.frames.len() > 4096
            || stream
                .frames
                .front()
                .is_some_and(|(t, _)| frame.seconds - *t > 1.)
        {
            stream.frames.pop_front();
        }
    }
    fn reading(&mut self, foreground: u32, now: Instant) -> Option<(String, f64, f64)> {
        let eligible = |s: &&Stream| {
            s.frames.len() >= 3
                && s.last
                    .is_some_and(|t| now.duration_since(t) < Duration::from_secs(2))
        };
        let for_pid = |pid| {
            self.streams
                .iter()
                .filter(|((p, _), s)| *p == pid && eligible(s))
                .max_by_key(|(_, s)| s.frames.len())
                .map(|(_, s)| s)
        };
        let stream = for_pid(foreground).or_else(|| self.selected.and_then(for_pid))?;
        // Never sum multiple swap chains: select the busiest chain of the target.
        let ms = stream.frames.iter().map(|(_, ms)| ms).sum::<f64>() / stream.frames.len() as f64;
        let name = stream.name.clone();
        if for_pid(foreground).is_some() {
            self.selected = Some(foreground);
        }
        Some((name, 1000. / ms, ms))
    }
}

struct Session {
    child: Child,
    job: HANDLE,
    reader: Option<JoinHandle<()>>,
    frames: Arc<Mutex<Frames>>,
    path: PathBuf,
    name: String,
    _instance: crate::platform::Instance,
}
impl Drop for Session {
    fn drop(&mut self) {
        // Stop only our own ETW session, including when pausing collection.
        if let Ok(mut stop) = Command::new(&self.path)
            .args(["--session_name", &self.name, "--terminate_existing_session"])
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            let deadline = Instant::now() + Duration::from_secs(2);
            while matches!(stop.try_wait(), Ok(None)) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(20));
            }
            let _ = stop.kill();
            let _ = stop.wait();
        }
        // Closing this job also kills PresentMon if the collector crashes/exits.
        unsafe {
            let _ = CloseHandle(self.job);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
impl Session {
    fn start() -> Result<Self, String> {
        let instance =
            crate::platform::Instance::claim("fps")?.ok_or("FPS capture is already running")?;
        let name = format!("CasePanel-FPS-{}", crate::platform::user_sid()?);
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let parent = exe.parent().ok_or("No application directory")?;
        let candidates = [
            parent.join("PresentMon.exe"),
            parent.join("../../vendor/presentmon/PresentMon.exe"),
        ];
        let path: PathBuf = candidates
            .into_iter()
            .find(|p| p.is_file())
            .ok_or("PresentMon is not installed")?;
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        if format!("{:x}", Sha256::digest(bytes)) != HASH {
            return Err("PresentMon checksum mismatch".into());
        }
        let mut command = Command::new(&path);
        command
            .args([
                "--output_stdout",
                "--no_console_stats",
                "--v1_metrics",
                "--no_track_display",
                "--no_track_input",
                "--no_track_gpu",
                "--session_name",
            ])
            .arg(&name)
            .arg("--stop_existing_session")
            // No overlay, hotkeys, input capture, or automatic UAC request.
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for name in EXCLUDED {
            command.args(["--exclude", name]);
        }
        let job = unsafe {
            let job = CreateJobObjectW(None, None).map_err(|e| e.to_string())?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if let Err(e) = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&info) as u32,
            ) {
                let _ = CloseHandle(job);
                return Err(e.to_string());
            }
            job
        };
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(e) => {
                unsafe {
                    let _ = CloseHandle(job);
                }
                return Err(e.to_string());
            }
        };
        if let Err(e) = unsafe { AssignProcessToJobObject(job, HANDLE(child.as_raw_handle())) } {
            let _ = child.kill();
            let _ = child.wait();
            unsafe {
                let _ = CloseHandle(job);
            }
            return Err(e.to_string());
        }
        let stdout = child
            .stdout
            .take()
            .ok_or("PresentMon stdout is unavailable")?;
        let frames = Arc::new(Mutex::new(Frames::default()));
        let shared = frames.clone();
        let reader = std::thread::spawn(move || {
            let mut columns = None;
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                if line.len() > 16_384 {
                    break;
                }
                if columns.is_none() || line.starts_with("Application,") {
                    if let Some(header) = Columns::parse(&line) {
                        columns = Some(header);
                    }
                    continue;
                }
                if let Some(frame) = columns.as_ref().and_then(|c| c.frame(&line))
                    && let Ok(mut frames) = shared.lock()
                {
                    frames.push(frame, Instant::now());
                }
            }
        });
        Ok(Self {
            child,
            job,
            reader: Some(reader),
            frames,
            path,
            name,
            _instance: instance,
        })
    }
}
pub struct Fps {
    session: Option<Session>,
    retry: Instant,
    attempts: u32,
    error: String,
}
impl Fps {
    pub fn new() -> Self {
        Self {
            session: None,
            retry: Instant::now(),
            attempts: 0,
            error: String::new(),
        }
    }
    pub fn pause(&mut self) {
        self.session = None;
        self.attempts = 0;
        self.retry = Instant::now();
    }
    pub fn poll(&mut self) -> Vec<Sensor> {
        if self
            .session
            .as_mut()
            .is_some_and(|s| !matches!(s.child.try_wait(), Ok(None)))
        {
            self.session = None;
            self.error = "FPS capture is unavailable; run the installed collector".into();
            self.retry = Instant::now() + Duration::from_secs(60);
        }
        if self.session.is_none() && self.attempts < 3 && Instant::now() >= self.retry {
            self.attempts += 1;
            match Session::start() {
                Ok(session) => {
                    self.session = Some(session);
                    self.error.clear();
                }
                Err(e) => {
                    self.error = e;
                    self.retry = Instant::now() + Duration::from_secs(60);
                }
            }
        }
        let mut sensors = vec![
            Sensor::new("presentmon", "game.fps", "Game rendered FPS", "FPS", 1000),
            Sensor::new(
                "presentmon",
                "game.frame_time",
                "Game frame time",
                "ms",
                1000,
            ),
        ];
        let mut foreground = 0;
        unsafe {
            GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut foreground));
        }
        let reading = self
            .session
            .as_ref()
            .and_then(|s| s.frames.lock().ok()?.reading(foreground, Instant::now()));
        if let Some((application, fps, ms)) = reading {
            sensors[0].set(fps);
            sensors[1].set(ms);
            for sensor in &mut sensors {
                sensor.message = application.clone();
            }
        } else {
            for sensor in &mut sensors {
                sensor.message = if self.error.is_empty() {
                    "No game detected".into()
                } else {
                    self.error.clone()
                };
            }
        }
        sensors
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(pid: u32, chain: &str, seconds: f64, ms: f64) -> Frame {
        Frame {
            name: format!("game{pid}.exe"),
            pid,
            swapchain: chain.into(),
            seconds,
            milliseconds: ms,
        }
    }
    #[test]
    fn parses_quoted_csv_and_rejects_invalid_intervals() {
        let columns = Columns::parse(
            "Application,ProcessID,SwapChainAddress,TimeInSeconds,MsBetweenPresents",
        )
        .unwrap();
        assert_eq!(
            columns
                .frame("\"game, deluxe.exe\",42,0x1,1.2,6.944")
                .unwrap()
                .name,
            "game, deluxe.exe"
        );
        let real_header = Columns::parse("Application,ProcessID,SwapChainAddress,Runtime,SyncInterval,PresentFlags,Dropped,TimeInSeconds,msInPresentAPI,msBetweenPresents").unwrap();
        assert!(
            (real_header
                .frame("game.exe,42,0x1,DXGI,0,0,0,1.0,0.1,6.944")
                .unwrap()
                .milliseconds
                - 6.944)
                .abs()
                < 0.001
        );
        for row in [
            "game.exe,42,0x1,1.2,NaN",
            "game.exe,42,0x1,1.2,0",
            "chrome.exe,42,0x1,1.2,7",
            "game.exe,42,0x1,NA,7",
        ] {
            assert!(columns.frame(row).is_none());
        }
    }
    #[test]
    fn chooses_foreground_and_dominant_chain_without_double_counting() {
        let mut frames = Frames::default();
        let now = Instant::now();
        for i in 1..=120 {
            frames.push(frame(42, "main", i as f64 / 120., 1000. / 120.), now);
        }
        for i in 1..=60 {
            frames.push(frame(42, "menu", i as f64 / 60., 1000. / 60.), now);
        }
        assert!(frames.reading(99, now).is_none());
        let (_, fps, ms) = frames.reading(42, now).unwrap();
        assert!((fps - 120.).abs() < 0.01);
        assert!((ms - 1000. / 120.).abs() < 0.01);
        assert!(frames.reading(99, now + Duration::from_secs(1)).is_some());
        assert!(frames.reading(42, now + Duration::from_secs(3)).is_none());
    }
    #[test]
    fn rolling_window_drops_old_performance_and_bounds_streams() {
        let mut frames = Frames::default();
        let now = Instant::now();
        for i in 1..=60 {
            frames.push(frame(42, "main", i as f64 / 60., 1000. / 60.), now);
        }
        for i in 1..=120 {
            frames.push(frame(42, "main", 2. + i as f64 / 120., 1000. / 120.), now);
        }
        assert!((frames.reading(42, now).unwrap().1 - 120.).abs() < 0.01);
        for i in 0..100 {
            frames.push(frame(i, "other", 3., 10.), now);
        }
        assert!(frames.streams.len() <= 64);
    }
}
