mod dashboard;
mod footer;
mod monitors;
mod render;
mod settings;
use crate::{
    config::{Config, Widget, WidgetKind},
    ipc,
    model::Store,
    platform,
    sensors::Basic,
};
use std::{cell::RefCell, path::PathBuf, sync::atomic::Ordering, time::Instant};
use windows::Win32::System::Power::{
    POWERBROADCAST_SETTING, RegisterPowerSettingNotification, UnregisterPowerSettingNotification,
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{Com::*, LibraryLoader::GetModuleHandleW},
        UI::{Controls::Dialogs::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};
const DISPLAY_STATE: windows::core::GUID =
    windows::core::GUID::from_u128(0x6fe69556_704a_47a0_8f24_c28d936fda47);
const TRAY: u32 = WM_APP + 7;
const ID_EDIT: usize = 101;
const ID_EXIT: usize = 102;
const ID_SHOW: usize = 103;
const ID_WIDGET: usize = 200;
const ID_KIND: usize = 201;
const ID_SENSOR: usize = 202;
const ID_ADD: usize = 210;
const ID_DELETE: usize = 211;
const ID_UP: usize = 212;
const ID_DOWN: usize = 213;
const ID_APPLY: usize = 214;
const ID_SAVE: usize = 215;
const ID_IMPORT: usize = 216;
const ID_EXPORT: usize = 217;
const ID_MONITOR: usize = 218;
const ID_DELAY: usize = 219;
const ID_SETTINGS: usize = 220;
const ID_IMAGE: usize = 221;
const ID_STATUS: usize = 222;
const FIELD_NAMES: [&str; 18] = [
    "X",
    "Y",
    "Width",
    "Height",
    "Text / label",
    "Unit",
    "Decimals",
    "Font size",
    "Minimum",
    "Maximum",
    "Color #RRGGBB",
    "Background #RRGGBB",
    "Font",
    "Alignment (left/center/right)",
    "Image path",
    "",
    "",
    " ",
];
const KINDS: [(WidgetKind, &str); 8] = [
    (WidgetKind::Text, "Text"),
    (WidgetKind::Value, "Value"),
    (WidgetKind::Bar, "Bar"),
    (WidgetKind::Gauge, "Circular gauge"),
    (WidgetKind::Image, "Image"),
    (WidgetKind::Graph, "Line chart"),
    (WidgetKind::Rectangle, "Rectangle"),
    (WidgetKind::Clock, "Clock"),
];
struct Drag {
    index: usize,
    x: f32,
    y: f32,
    original: Widget,
    resize: bool,
}
struct App {
    owner: *const RefCell<App>,
    config: Config,
    profile: PathBuf,
    store: Store,
    basic: Basic,
    system: crate::sensors::system::SystemTelemetry,
    dashboard: dashboard::DashboardState,
    dashboard_controls: settings::Controls,
    client: ipc::Client,
    panel: HWND,
    editor: HWND,
    renderer: render::Renderer,
    editor_renderer: Option<render::Renderer>,
    selected: Option<usize>,
    fields: Vec<HWND>,
    widget_combo: HWND,
    kind_combo: HWND,
    sensor_combo: HWND,
    sensor_options: Vec<(String, String)>,
    monitor_combo: HWND,
    delay: HWND,
    status: HWND,
    monitors: Vec<monitors::Monitor>,
    message: String,
    preview: bool,
    screen_on: bool,
    present: bool,
    dirty: bool,
    autosave: bool,
    drag: Option<Drag>,
    started: Instant,
    exit_after: Option<u64>,
    first_paint: bool,
    edit_backup: Option<Config>,
}

pub fn run(profile: PathBuf, preview: bool, exit_after: Option<u64>) -> Result<(), String> {
    unsafe {
        let started = Instant::now();
        let Some(_instance) = platform::Instance::claim(if preview { "preview" } else { "panel" })?
        else {
            return Ok(());
        };
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let (config, warning) = Config::load_or_recover(&profile);
        let autosave = warning.is_none();
        let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        for (class, proc) in [
            (
                w!("SensorPanel.Native"),
                Some(panel_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT),
            ),
            (
                w!("SensorPanel.Editor"),
                Some(
                    editor_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
                ),
            ),
        ] {
            let wc = WNDCLASSW {
                hInstance: instance.into(),
                lpszClassName: class,
                lpfnWndProc: proc,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                ..Default::default()
            };
            RegisterClassW(&wc);
        }
        let state = Box::new(RefCell::new(App {
            owner: std::ptr::null(),
            config,
            profile,
            store: Store::default(),
            basic: Basic::new(),
            system: crate::sensors::system::SystemTelemetry::default(),
            dashboard: dashboard::DashboardState::new(),
            dashboard_controls: settings::Controls::default(),
            client: ipc::Client::start(),
            panel: HWND::default(),
            editor: HWND::default(),
            renderer: render::Renderer::new()?,
            editor_renderer: None,
            selected: None,
            fields: Vec::new(),
            widget_combo: HWND::default(),
            kind_combo: HWND::default(),
            sensor_combo: HWND::default(),
            sensor_options: Vec::new(),
            monitor_combo: HWND::default(),
            delay: HWND::default(),
            status: HWND::default(),
            monitors: monitors::enumerate(),
            message: warning.unwrap_or_default(),
            preview,
            screen_on: true,
            present: false,
            dirty: false,
            autosave,
            drag: None,
            started,
            exit_after,
            first_paint: false,
            edit_backup: None,
        }));
        let pointer = (&*state as *const RefCell<App>).cast();
        let mut app = state.borrow_mut();
        app.owner = &*state;
        let style = if preview {
            WS_OVERLAPPEDWINDOW
        } else {
            WS_POPUP
        };
        let ex = if preview {
            WINDOW_EX_STYLE(0)
        } else {
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE
        };
        app.panel = CreateWindowExW(
            ex,
            w!("SensorPanel.Native"),
            w!("CasePanel"),
            style,
            100,
            100,
            960,
            640,
            None,
            None,
            Some(instance.into()),
            Some(pointer),
        )
        .map_err(|e| e.to_string())?;
        let power_notification = RegisterPowerSettingNotification(
            HANDLE(app.panel.0),
            &DISPLAY_STATE,
            DEVICE_NOTIFY_WINDOW_HANDLE,
        )
        .ok();
        tray(app.panel, true);
        app.position_panel();
        SetTimer(Some(app.panel), 1, 1000, None);
        app.tick();
        drop(app);
        let mut message = MSG::default();
        loop {
            let result = GetMessageW(&mut message, None, 0, 0);
            if result.0 <= 0 {
                break;
            }
            let editor = state.borrow().editor;
            if !editor.is_invalid() && IsDialogMessageW(editor, &message).as_bool() {
                continue;
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        state.borrow_mut().client.close();
        let editor = state.borrow().editor;
        if !editor.is_invalid() {
            let _ = DestroyWindow(editor);
        }
        if let Some(token) = power_notification {
            let _ = UnregisterPowerSettingNotification(token);
        }
        drop(state);
        CoUninitialize();
        Ok(())
    }
}
unsafe fn app<'a>(hwnd: HWND) -> Option<&'a RefCell<App>> {
    (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<App>).as_ref()
}
unsafe fn register_state(hwnd: HWND, lp: LPARAM) {
    let create = &*(lp.0 as *const CREATESTRUCTW);
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
}
unsafe fn tray(hwnd: HWND, add: bool) {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: TRAY,
        hIcon: LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
        ..Default::default()
    };
    let words = platform::wide("CasePanel — right-click to configure");
    data.szTip[..words.len()].copy_from_slice(&words);
    let _ = Shell_NotifyIconW(if add { NIM_ADD } else { NIM_DELETE }, &data);
}
unsafe extern "system" fn panel_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        register_state(hwnd, lp);
    }
    if msg == WM_DESTROY {
        tray(hwnd, false);
        PostQuitMessage(0);
        return LRESULT(0);
    }
    if let Some(cell) = app(hwnd)
        && let Ok(mut state) = cell.try_borrow_mut()
    {
        let a = &mut *state;
        match msg {
            WM_TIMER => {
                if a.exit_after
                    .is_some_and(|s| a.started.elapsed().as_secs() >= s)
                {
                    let _ = DestroyWindow(hwnd);
                } else {
                    a.tick();
                }
                return LRESULT(0);
            }
            WM_PAINT => {
                paint(a, hwnd, false);
                return LRESULT(0);
            }
            WM_ERASEBKGND => return LRESULT(1),
            WM_MOUSEACTIVATE if !a.preview => return LRESULT(MA_NOACTIVATE as isize),
            WM_LBUTTONUP => {
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                let viewport =
                    render::Viewport::new(rect.right as f32, rect.bottom as f32, &a.config);
                let x = (lp.0 as u16 as i16 as f32 - viewport.x) / viewport.scale;
                let y = ((lp.0 >> 16) as u16 as i16 as f32 - viewport.y) / viewport.scale;
                if footer::settings_hit(&a.config, x, y) {
                    a.open_editor();
                }
                return LRESULT(0);
            }
            WM_SIZE | WM_DPICHANGED => {
                a.renderer.reset();
                let _ = InvalidateRect(Some(hwnd), None, false);
                return LRESULT(0);
            }
            WM_DISPLAYCHANGE => {
                a.monitors = monitors::enumerate();
                a.position_panel();
                return LRESULT(0);
            }
            WM_POWERBROADCAST => {
                if wp.0 == 4 {
                    a.screen_on = false;
                }
                if wp.0 == 7 || wp.0 == 18 {
                    a.screen_on = true;
                    a.basic = Basic::new();
                }
                if wp.0 == 0x8013 && lp.0 != 0 {
                    let setting = &*(lp.0 as *const POWERBROADCAST_SETTING);
                    if setting.PowerSetting == DISPLAY_STATE && setting.DataLength >= 4 {
                        a.screen_on =
                            std::ptr::read_unaligned(setting.Data.as_ptr().cast::<u32>()) != 0;
                    }
                }
                a.client
                    .active
                    .store(a.screen_on && a.present, Ordering::Relaxed);
                return LRESULT(1);
            }
            TRAY => {
                if lp.0 as u32 == WM_RBUTTONUP {
                    popup(hwnd);
                }
                if lp.0 as u32 == WM_LBUTTONDBLCLK {
                    a.open_editor();
                }
                return LRESULT(0);
            }
            WM_CONTEXTMENU => {
                popup(hwnd);
                return LRESULT(0);
            }
            WM_COMMAND => {
                match wp.0 & 0xffff {
                    ID_EDIT => a.open_editor(),
                    ID_SHOW => {
                        a.position_panel();
                    }
                    ID_EXIT => {
                        let _ = DestroyWindow(hwnd);
                    }
                    _ => (),
                }
                return LRESULT(0);
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
            WM_DESTROY => {
                tray(hwnd, false);
                PostQuitMessage(0);
                return LRESULT(0);
            }
            _ => (),
        }
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}
unsafe fn popup(hwnd: HWND) {
    if let Ok(menu) = CreatePopupMenu() {
        let _ = AppendMenuW(menu, MF_STRING, ID_EDIT, w!("Edit panel and sensors"));
        let _ = AppendMenuW(menu, MF_STRING, ID_SHOW, w!("Position panel"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, ID_EXIT, w!("Exit"));
        let mut p = POINT::default();
        let _ = GetCursorPos(&mut p);
        let _ = SetForegroundWindow(hwnd);
        // Dispatch after the menu returns: App is borrowed by the outer window
        // callback, so a synchronous WM_COMMAND would be ignored as reentrant.
        let command = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
            p.x,
            p.y,
            None,
            hwnd,
            None,
        );
        if command.0 != 0 {
            let _ = PostMessageW(
                Some(hwnd),
                WM_COMMAND,
                WPARAM(command.0 as usize),
                LPARAM(0),
            );
        }
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
    }
}
unsafe fn paint(a: &mut App, hwnd: HWND, editor: bool) {
    let mut ps = PAINTSTRUCT::default();
    BeginPaint(hwnd, &mut ps);
    let mut r = RECT::default();
    let _ = GetClientRect(hwnd, &mut r);
    let result = if editor {
        if let Some(renderer) = a.editor_renderer.as_mut() {
            renderer.paint(
                hwnd,
                (r.right.max(1) as u32, r.bottom.max(1) as u32),
                &a.config,
                &a.store,
                &a.dashboard,
                render::PaintMode::Editor {
                    selected: if a.dashboard_controls.dashboard_page {
                        None
                    } else {
                        a.selected
                    },
                },
            )
        } else {
            Ok(())
        }
    } else {
        a.renderer.paint(
            hwnd,
            (r.right.max(1) as u32, r.bottom.max(1) as u32),
            &a.config,
            &a.store,
            &a.dashboard,
            render::PaintMode::Panel,
        )
    };
    let _ = EndPaint(hwnd, &ps);
    if !editor && result.is_ok() && !a.first_paint {
        a.first_paint = true;
        crate::logging::write(&format!(
            "First panel paint: {} ms",
            a.started.elapsed().as_millis()
        ));
    }
    if let Err(e) = result
        && a.message != e
    {
        crate::logging::write(&e);
        a.message = e;
    }
}
impl App {
    unsafe fn position_panel(&mut self) {
        if self.preview {
            self.present = true;
            let _ = ShowWindow(self.panel, SW_SHOWNORMAL);
            return;
        }
        if let Some(m) = monitors::select(
            &self.monitors,
            &self.config.monitor_id,
            &self.config.monitor_name,
        ) {
            let rect = m.rect;
            let id = m.id.clone();
            self.present = true;
            if self.config.monitor_id != id {
                self.config.monitor_id = id;
                self.persist_automatically();
            }
            let _ = SetWindowPos(
                self.panel,
                // Start/reposition behind other applications without taking focus.
                Some(HWND_BOTTOM),
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        } else {
            self.present = false;
            let _ = ShowWindow(self.panel, SW_HIDE);
        }
    }
    fn persist_automatically(&mut self) {
        if self.autosave
            && !self.dirty
            && let Err(e) = self.config.save(&self.profile)
        {
            self.message = e;
        }
    }
    unsafe fn tick(&mut self) {
        let active = self.screen_on && (self.present || !self.editor.is_invalid());
        self.client.active.store(active, Ordering::Relaxed);
        let mut received = false;
        while let Ok(snapshot) = self.client.receiver.try_recv() {
            if !snapshot.diagnostics.is_empty() {
                self.message = snapshot.diagnostics.join("\r\n");
            }
            let _ = self.store.apply(snapshot);
            received = true;
        }
        // Local Windows/Registry input is authoritative even with an older collector.
        if active {
            for s in self.basic.poll() {
                self.store.update(s);
            }
        }
        let mut changed = false;
        for w in &self.config.widgets {
            if !w.sensor.is_empty()
                && !self.config.bindings.contains_key(&w.sensor)
                && !self.store.sensors.contains_key(&w.sensor)
            {
                let candidates: Vec<_> = self
                    .store
                    .sensors
                    .values()
                    .filter(|s| s.key == w.sensor)
                    .collect();
                if candidates.len() == 1 {
                    self.config
                        .bindings
                        .insert(w.sensor.clone(), candidates[0].id.clone());
                    changed = true;
                }
            }
        }
        if changed {
            self.persist_automatically();
        }
        if active {
            let system = self.system.poll(
                self.config.dashboard.show_network_in_header,
                self.config.footer.enabled
                    && self.config.dashboard.bottom_bar_mode
                        != crate::config::BottomBarMode::GameOnly,
            );
            self.dashboard
                .update(&self.config, &self.store, &system, crate::model::now_ms());
            let _ = InvalidateRect(Some(self.panel), None, false);
            if !self.editor.is_invalid() {
                let _ = InvalidateRect(Some(self.editor), None, false);
            }
        } else {
            self.system = crate::sensors::system::SystemTelemetry::default();
        }
        if received
            && !self.editor.is_invalid()
            && self
                .sensor_options
                .iter()
                .skip(1)
                .map(|(id, _)| id)
                .ne(self.store.sensors.keys())
        {
            self.populate_sensors();
        }
        if !self.status.is_invalid() {
            let missing = self
                .config
                .widgets
                .iter()
                .filter(|w| {
                    !w.sensor.is_empty()
                        && render::resolve(&self.config, &self.store, &w.sensor).is_none()
                })
                .count();
            let status = format!(
                "{}\r\nSensors: {} | unresolved bindings: {}\r\n{}",
                if self.dirty {
                    "Unsaved changes"
                } else {
                    "Profile saved"
                },
                self.store.sensors.len(),
                missing,
                self.message
            );
            set_text(self.status, &status);
        }
    }
    unsafe fn open_editor(&mut self) {
        if !self.editor.is_invalid() {
            let _ = ShowWindow(self.editor, SW_RESTORE);
            let _ = SetForegroundWindow(self.editor);
            return;
        }
        self.edit_backup = Some(self.config.clone());
        if self.editor_renderer.is_none() {
            match render::Renderer::new() {
                Ok(r) => self.editor_renderer = Some(r),
                Err(e) => {
                    self.message = e;
                    return;
                }
            }
        }
        let instance = GetModuleHandleW(None).unwrap();
        let width = GetSystemMetrics(SM_CXSCREEN).min(1500);
        let height = (GetSystemMetrics(SM_CYSCREEN) - 80).min(1060);
        if let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("SensorPanel.Editor"),
            w!("CasePanel — Ajustes"),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            40,
            30,
            width,
            height,
            None,
            None,
            Some(instance.into()),
            Some(self.owner.cast()),
        ) {
            self.editor = hwnd;
            self.create_controls();
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
    }
    unsafe fn control(&self, class: PCWSTR, text: &str, id: usize, style: WINDOW_STYLE) -> HWND {
        let words = platform::wide(text);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            PCWSTR(words.as_ptr()),
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            100,
            24,
            Some(self.editor),
            Some(HMENU(id as *mut _)),
            None,
            None,
        )
        .unwrap_or_default();
        let font = GetStockObject(DEFAULT_GUI_FONT);
        let _ = SendMessageW(
            hwnd,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
        hwnd
    }
    unsafe fn create_controls(&mut self) {
        self.fields.clear();
        self.widget_combo = self.control(
            w!("COMBOBOX"),
            "",
            ID_WIDGET,
            WINDOW_STYLE(CBS_DROPDOWNLIST as u32) | WS_VSCROLL | WS_TABSTOP,
        );
        self.kind_combo = self.control(
            w!("COMBOBOX"),
            "",
            ID_KIND,
            WINDOW_STYLE(CBS_DROPDOWNLIST as u32) | WS_VSCROLL | WS_TABSTOP,
        );
        for (_, label) in KINDS {
            combo_add(self.kind_combo, label);
        }
        self.sensor_combo = self.control(
            w!("COMBOBOX"),
            "",
            ID_SENSOR,
            WINDOW_STYLE(CBS_DROPDOWNLIST as u32) | WS_VSCROLL | WS_TABSTOP,
        );
        for (i, name) in FIELD_NAMES.iter().enumerate() {
            let _ = self.control(w!("STATIC"), name, 400 + i, WINDOW_STYLE(0));
            let edit = self.control(
                w!("EDIT"),
                "",
                300 + i,
                WS_BORDER | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            );
            self.fields.push(edit);
        }
        for (id, text) in [
            (ID_ADD, "Add"),
            (ID_DELETE, "Delete"),
            (ID_UP, "Raise layer"),
            (ID_DOWN, "Lower layer"),
            (ID_APPLY, "Apply properties"),
            (ID_SAVE, "Save"),
            (ID_IMPORT, "Import"),
            (ID_EXPORT, "Export"),
            (ID_SETTINGS, "Apply display and startup"),
            (ID_IMAGE, "Choose image"),
        ] {
            self.control(w!("BUTTON"), text, id, WS_TABSTOP);
        }
        self.monitor_combo = self.control(
            w!("COMBOBOX"),
            "",
            ID_MONITOR,
            WINDOW_STYLE(CBS_DROPDOWNLIST as u32) | WS_VSCROLL | WS_TABSTOP,
        );
        self.delay = self.control(
            w!("EDIT"),
            &self.config.startup_delay_seconds.to_string(),
            ID_DELAY,
            WS_BORDER | WS_TABSTOP,
        );
        self.status = self.control(
            w!("EDIT"),
            "",
            ID_STATUS,
            WS_BORDER | WS_VSCROLL | WINDOW_STYLE((ES_MULTILINE | ES_READONLY) as u32),
        );
        self.control(w!("STATIC"), "Widget", 500, WINDOW_STYLE(0));
        self.control(w!("STATIC"), "Tipo", 501, WINDOW_STYLE(0));
        self.control(
            w!("STATIC"),
            "Sensor (AI usage is reported by an external script)",
            502,
            WINDOW_STYLE(0),
        );
        self.control(w!("STATIC"), "Target display", 503, WINDOW_STYLE(0));
        self.control(
            w!("STATIC"),
            "Logon startup delay (0–60 s)",
            504,
            WINDOW_STYLE(0),
        );
        self.populate_widgets();
        self.populate_sensors();
        self.populate_monitors();
        self.create_dashboard_controls();
        self.layout_controls();
        self.selected = (!self.config.widgets.is_empty()).then_some(0);
        self.load_fields();
    }
    unsafe fn layout_controls(&self) {
        let mut rect = RECT::default();
        let _ = GetClientRect(self.editor, &mut rect);
        let left = (rect.right - 470).max(0);
        let col = 220;
        let place = |id: usize, x: i32, y: i32, width: i32, height: i32| {
            if let Ok(h) = GetDlgItem(Some(self.editor), id as i32) {
                let _ = MoveWindow(h, left + x, y + 40, width, height, true);
            }
        };
        place(500, 0, 7, 440, 18);
        place(ID_WIDGET, 0, 27, 450, 300);
        place(501, 0, 57, 440, 18);
        place(ID_KIND, 0, 77, 450, 260);
        place(502, 0, 107, 450, 18);
        place(ID_SENSOR, 0, 127, 450, 400);
        for i in 0..18 {
            let x = (i % 2) as i32 * col;
            let y = 163 + (i / 2) as i32 * 43;
            place(400 + i, x, y, 215, 18);
            place(300 + i, x, y + 18, 215, 22);
            if i >= 15 {
                if let Ok(h) = GetDlgItem(Some(self.editor), (300 + i) as i32) {
                    let _ = ShowWindow(h, SW_HIDE);
                }
                if let Ok(h) = GetDlgItem(Some(self.editor), (400 + i) as i32) {
                    let _ = ShowWindow(h, SW_HIDE);
                }
            }
        }
        for (id, x, y, w) in [
            (ID_IMAGE, 0, 508, 215),
            (ID_ADD, 0, 547, 105),
            (ID_DELETE, 110, 547, 105),
            (ID_UP, 220, 547, 105),
            (ID_DOWN, 330, 547, 105),
            (ID_APPLY, 0, 582, 450),
            (ID_SAVE, 0, 617, 140),
            (ID_IMPORT, 150, 617, 140),
            (ID_EXPORT, 300, 617, 140),
        ] {
            place(id, x, y, w, 28);
        }
        place(503, 0, 657, 450, 18);
        place(ID_MONITOR, 0, 677, 450, 220);
        place(504, 0, 710, 330, 18);
        place(ID_DELAY, 340, 707, 105, 24);
        place(ID_SETTINGS, 0, 740, 450, 28);
        place(ID_STATUS, 0, 780, 450, (rect.bottom - 830).max(60));
        self.layout_dashboard_controls(&rect, left);
    }
    unsafe fn populate_widgets(&mut self) {
        let _ = SendMessageW(self.widget_combo, CB_RESETCONTENT, None, None);
        for w in &self.config.widgets {
            combo_add(self.widget_combo, &format!("{} · {:?}", w.id, w.kind));
        }
        if let Some(i) = self.selected {
            let _ = SendMessageW(self.widget_combo, CB_SETCURSEL, Some(WPARAM(i)), None);
        }
    }
    unsafe fn populate_sensors(&mut self) {
        let old = text(self.sensor_combo);
        let _ = SendMessageW(self.sensor_combo, CB_RESETCONTENT, None, None);
        self.sensor_options.clear();
        combo_add(self.sensor_combo, "No sensor");
        self.sensor_options.push((String::new(), String::new()));
        for s in self.store.sensors.values() {
            combo_add(self.sensor_combo, &format!("{} [{}]", s.label, s.unit));
            self.sensor_options.push((s.id.clone(), s.key.clone()));
        }
        if let Some(i) = self.selected {
            let key = &self.config.widgets[i].sensor;
            let id = self.config.bindings.get(key).unwrap_or(key);
            if let Some(index) = self.sensor_options.iter().position(|(v, _)| v == id) {
                let _ = SendMessageW(self.sensor_combo, CB_SETCURSEL, Some(WPARAM(index)), None);
            }
        }
        let _ = old;
    }
    unsafe fn populate_monitors(&self) {
        let _ = SendMessageW(self.monitor_combo, CB_RESETCONTENT, None, None);
        for (i, m) in self.monitors.iter().enumerate() {
            combo_add(
                self.monitor_combo,
                &format!(
                    "{} · {} × {} · {}",
                    m.name,
                    m.rect.right - m.rect.left,
                    m.rect.bottom - m.rect.top,
                    m.device
                ),
            );
            if m.id == self.config.monitor_id
                || self.config.monitor_id.is_empty() && m.name == self.config.monitor_name
            {
                let _ = SendMessageW(self.monitor_combo, CB_SETCURSEL, Some(WPARAM(i)), None);
            }
        }
    }
    unsafe fn load_fields(&mut self) {
        let Some(i) = self.selected else {
            return;
        };
        if i >= self.config.widgets.len() {
            return;
        }
        let w = &self.config.widgets[i];
        let values = [
            w.x.to_string(),
            w.y.to_string(),
            w.width.to_string(),
            w.height.to_string(),
            w.text.clone(),
            w.unit.clone(),
            w.decimals.to_string(),
            w.font_size.to_string(),
            w.min.to_string(),
            w.max.to_string(),
            w.color.clone(),
            w.background.clone(),
            w.font.clone(),
            w.align.clone(),
            w.image_path.clone(),
            String::new(),
            String::new(),
            String::new(),
        ];
        for (h, v) in self.fields.iter().zip(values) {
            set_text(*h, &v);
        }
        let kind = KINDS
            .iter()
            .position(|(kind, _)| *kind == w.kind)
            .unwrap_or(0);
        let _ = SendMessageW(self.kind_combo, CB_SETCURSEL, Some(WPARAM(kind)), None);
        let _ = SendMessageW(self.widget_combo, CB_SETCURSEL, Some(WPARAM(i)), None);
        self.populate_sensors();
    }
    unsafe fn apply_fields(&mut self) -> Result<(), String> {
        let Some(i) = self.selected else {
            return Ok(());
        };
        let mut w = self.config.widgets[i].clone();
        let f: Vec<_> = self.fields.iter().map(|h| text(*h)).collect();
        macro_rules! number {
            ($n:expr,$ty:ty) => {
                f[$n]
                    .parse::<$ty>()
                    .map_err(|_| format!("Invalid {}", FIELD_NAMES[$n]))?
            };
        }
        w.x = number!(0, f32);
        w.y = number!(1, f32);
        w.width = number!(2, f32);
        w.height = number!(3, f32);
        w.text = f[4].clone();
        w.unit = f[5].clone();
        w.decimals = number!(6, u32);
        w.font_size = number!(7, f32);
        w.min = number!(8, f64);
        w.max = number!(9, f64);
        w.color = f[10].clone();
        w.background = f[11].clone();
        w.font = f[12].clone();
        w.align = f[13].clone();
        w.image_path = f[14].clone();
        let kind = SendMessageW(self.kind_combo, CB_GETCURSEL, None, None).0;
        if kind >= 0 {
            w.kind = KINDS[kind as usize].0;
        }
        let sensor = SendMessageW(self.sensor_combo, CB_GETCURSEL, None, None).0;
        if sensor >= 0 {
            w.sensor = self.sensor_options[sensor as usize].0.clone();
        }
        let mut proposed = self.config.clone();
        if let Some(sensor) = render::resolve(&self.config, &self.store, &w.sensor)
            && render::convert_unit(1., &sensor.unit, &w.unit).is_none()
        {
            return Err(format!(
                "Incompatible unit; this sensor uses {}",
                sensor.unit
            ));
        }
        proposed.widgets[i] = w;
        proposed.validate()?;
        self.config = proposed;
        self.dirty = true;
        self.populate_widgets();
        self.invalidate();
        Ok(())
    }
    unsafe fn invalidate(&self) {
        let _ = InvalidateRect(Some(self.panel), None, false);
        if !self.editor.is_invalid() {
            let _ = InvalidateRect(Some(self.editor), None, false);
        }
    }
    unsafe fn save(&mut self) -> Result<(), String> {
        if self.dashboard_controls.dashboard_page {
            self.apply_dashboard_thresholds()?;
        } else {
            self.apply_fields()?;
        }
        self.config.save(&self.profile)?;
        self.dirty = false;
        self.edit_backup = Some(self.config.clone());
        self.autosave = true;
        self.message = format!("Saved: {}", self.profile.display());
        Ok(())
    }
    unsafe fn command(&mut self, id: usize, notification: usize) -> Result<(), String> {
        if self.dashboard_command(id, notification)? {
            return Ok(());
        }
        if id == ID_WIDGET && notification == CBN_SELCHANGE as usize {
            let i = SendMessageW(self.widget_combo, CB_GETCURSEL, None, None).0;
            if i >= 0 {
                self.selected = Some(i as usize);
                self.load_fields();
                self.invalidate();
            }
            return Ok(());
        }
        match id {
            ID_ADD => {
                let mut n = self.config.widgets.len() + 1;
                while self
                    .config
                    .widgets
                    .iter()
                    .any(|w| w.id == format!("widget_{n}"))
                {
                    n += 1;
                }
                self.config.widgets.push(Widget {
                    id: format!("widget_{n}"),
                    ..Widget::default()
                });
                self.selected = Some(self.config.widgets.len() - 1);
                self.dirty = true;
                self.populate_widgets();
                self.load_fields();
                self.invalidate();
            }
            ID_DELETE => {
                if let Some(i) = self.selected {
                    self.config.widgets.remove(i);
                    self.selected = if self.config.widgets.is_empty() {
                        None
                    } else {
                        Some(i.min(self.config.widgets.len() - 1))
                    };
                    self.dirty = true;
                    self.populate_widgets();
                    self.load_fields();
                    self.invalidate();
                }
            }
            ID_UP | ID_DOWN => {
                if let Some(i) = self.selected {
                    let j = if id == ID_UP {
                        (i + 1).min(self.config.widgets.len() - 1)
                    } else {
                        i.saturating_sub(1)
                    };
                    self.config.widgets.swap(i, j);
                    self.selected = Some(j);
                    self.dirty = true;
                    self.populate_widgets();
                    self.load_fields();
                    self.invalidate();
                }
            }
            ID_APPLY => self.apply_fields()?,
            ID_SAVE => self.save()?,
            ID_IMPORT => {
                if let Some(path) = file_dialog(self.editor, false, false) {
                    let c = Config::load(&path)?;
                    self.config = c;
                    self.selected = (!self.config.widgets.is_empty()).then_some(0);
                    self.dirty = true;
                    self.populate_widgets();
                    self.load_fields();
                    self.populate_monitors();
                    self.load_dashboard_controls();
                    self.position_panel();
                    self.invalidate();
                }
            }
            ID_EXPORT => {
                if self.dashboard_controls.dashboard_page {
                    self.apply_dashboard_thresholds()?;
                } else {
                    self.apply_fields()?;
                }
                if let Some(path) = file_dialog(self.editor, true, false) {
                    self.config.save(&path)?;
                }
            }
            ID_IMAGE => {
                if let Some(path) = file_dialog(self.editor, false, true) {
                    set_text(self.fields[14], &path.to_string_lossy());
                }
            }
            ID_SETTINGS => {
                let index = SendMessageW(self.monitor_combo, CB_GETCURSEL, None, None).0;
                if index >= 0 {
                    let m = &self.monitors[index as usize];
                    self.config.monitor_id = m.id.clone();
                    self.config.monitor_name = m.name.clone();
                }
                let delay = text(self.delay)
                    .parse::<u32>()
                    .map_err(|_| "Invalid delay")?;
                if delay > 60 {
                    return Err("Delay must be between 0 and 60".into());
                }
                self.config.startup_delay_seconds = delay;
                self.dirty = true;
                self.position_panel();
                self.config.save(&self.profile)?;
                self.dirty = false;
                self.edit_backup = Some(self.config.clone());
                self.autosave = true;
                self.message="Display saved. Run the installer again to change the delay of existing startup tasks.".into();
            }
            _ => (),
        }
        Ok(())
    }
}
unsafe extern "system" fn editor_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        register_state(hwnd, lp);
    }

    if let Some(cell) = app(hwnd)
        && let Ok(mut state) = cell.try_borrow_mut()
    {
        let a = &mut *state;
        match msg {
            WM_PAINT => {
                paint(a, hwnd, true);
                return LRESULT(0);
            }
            WM_ERASEBKGND => return LRESULT(1),
            WM_SIZE | WM_DPICHANGED => {
                a.layout_controls();
                if let Some(r) = a.editor_renderer.as_mut() {
                    r.reset();
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                return LRESULT(0);
            }
            WM_COMMAND => {
                if let Err(e) = a.command(wp.0 & 0xffff, (wp.0 >> 16) & 0xffff) {
                    a.message = e.clone();
                    let words = platform::wide(e);
                    MessageBoxW(
                        Some(hwnd),
                        PCWSTR(words.as_ptr()),
                        w!("Check configuration"),
                        MB_OK | MB_ICONWARNING,
                    );
                }
                return LRESULT(0);
            }
            WM_LBUTTONDOWN => {
                if a.dashboard_controls.dashboard_page {
                    return LRESULT(0);
                }
                let x = (lp.0 as i16) as f32;
                let y = ((lp.0 >> 16) as i16) as f32;
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                let view = render::Viewport::new(
                    (rect.right - 480).max(1) as f32,
                    rect.bottom as f32,
                    &a.config,
                );
                let (cx, cy) = view.to_canvas(x, y);
                if x < (rect.right - 480) as f32
                    && let Some((i, w)) =
                        a.config.widgets.iter().enumerate().rev().find(|(_, w)| {
                            cx >= w.x && cy >= w.y && cx <= w.x + w.width && cy <= w.y + w.height
                        })
                {
                    let resize = (cx - w.x - w.width).abs() < 12. / view.scale
                        && (cy - w.y - w.height).abs() < 12. / view.scale;
                    a.drag = Some(Drag {
                        index: i,
                        x: cx,
                        y: cy,
                        original: w.clone(),
                        resize,
                    });
                    a.selected = Some(i);
                    a.load_fields();
                    SetCapture(hwnd);
                    a.invalidate();
                }
                return LRESULT(0);
            }
            WM_MOUSEMOVE => {
                if let Some(d) = &a.drag {
                    let mut rect = RECT::default();
                    let _ = GetClientRect(hwnd, &mut rect);
                    let view = render::Viewport::new(
                        (rect.right - 480).max(1) as f32,
                        rect.bottom as f32,
                        &a.config,
                    );
                    let (x, y) = view.to_canvas((lp.0 as i16) as f32, ((lp.0 >> 16) as i16) as f32);
                    let w = &mut a.config.widgets[d.index];
                    if d.resize {
                        w.width = (d.original.width + x - d.x).max(8.);
                        w.height = (d.original.height + y - d.y).max(8.);
                    } else {
                        w.x = (d.original.x + x - d.x).round();
                        w.y = (d.original.y + y - d.y).round();
                    }
                    a.dirty = true;
                    a.invalidate();
                }
                return LRESULT(0);
            }
            WM_LBUTTONUP => {
                if a.drag.take().is_some() {
                    let _ = ReleaseCapture();
                    a.load_fields();
                }
                return LRESULT(0);
            }
            WM_CLOSE => {
                if a.dirty {
                    let answer = MessageBoxW(
                        Some(hwnd),
                        w!("Save changes to the panel?"),
                        w!("CasePanel"),
                        MB_YESNOCANCEL | MB_ICONQUESTION,
                    );
                    if answer == IDCANCEL {
                        return LRESULT(0);
                    }
                    if answer == IDYES && a.save().is_err() {
                        return LRESULT(0);
                    }
                    if answer == IDNO
                        && let Some(original) = a.edit_backup.take()
                    {
                        a.config = original;
                        a.dirty = false;
                        a.position_panel();
                        a.invalidate();
                    }
                }
                let _ = DestroyWindow(hwnd);
                a.editor = HWND::default();
                a.status = HWND::default();
                a.fields.clear();
                a.editor_renderer = None;
                return LRESULT(0);
            }
            WM_DESTROY => {
                a.editor = HWND::default();
                a.status = HWND::default();
                a.fields.clear();
                a.editor_renderer = None;
                return LRESULT(0);
            }
            _ => (),
        }
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}
unsafe fn combo_add(hwnd: HWND, s: &str) {
    let words = platform::wide(s);
    let _ = SendMessageW(
        hwnd,
        CB_ADDSTRING,
        None,
        Some(LPARAM(words.as_ptr() as isize)),
    );
}
unsafe fn set_text(hwnd: HWND, s: &str) {
    let words = platform::wide(s);
    let _ = SetWindowTextW(hwnd, PCWSTR(words.as_ptr()));
}
unsafe fn text(hwnd: HWND) -> String {
    let n = GetWindowTextLengthW(hwnd).clamp(0, 32768) as usize;
    let mut buffer = vec![0u16; n + 1];
    let len = GetWindowTextW(hwnd, &mut buffer);
    String::from_utf16_lossy(&buffer[..len.max(0) as usize])
}
unsafe fn file_dialog(owner: HWND, save: bool, image: bool) -> Option<PathBuf> {
    let mut name = vec![0u16; 32768];
    let filter = platform::wide(if image {
        "Images\0*.png;*.jpg;*.jpeg;*.bmp\0All files\0*.*\0"
    } else {
        "CasePanel profile\0*.json\0"
    });
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: windows::core::PWSTR(name.as_mut_ptr()),
        nMaxFile: name.len() as u32,
        Flags: OFN_EXPLORER
            | OFN_NOCHANGEDIR
            | if save {
                OFN_OVERWRITEPROMPT
            } else {
                OFN_FILEMUSTEXIST
            },
        lpstrDefExt: if image { PCWSTR::null() } else { w!("json") },
        ..Default::default()
    };
    let ok = if save {
        GetSaveFileNameW(&mut dialog)
    } else {
        GetOpenFileNameW(&mut dialog)
    };
    if ok.as_bool() {
        Some(PathBuf::from(String::from_utf16_lossy(
            &name[..name.iter().position(|v| *v == 0).unwrap_or(name.len())],
        )))
    } else {
        None
    }
}
