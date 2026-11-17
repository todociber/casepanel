use super::*;
use crate::config::{BottomBarMode, DashboardConfig};

const PAGE_WIDGETS: usize = 600;
const PAGE_DASHBOARD: usize = 601;
const PAGE_WORKDAY: usize = 602;
const WORK_TITLE: usize = 701;
const WORK_DAYS: usize = 710;
const WORK_TIMES: usize = 720;
const WORK_APPLY: usize = 740;
const WORK_DEFAULTS: usize = 741;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Page {
    Design,
    Dashboard,
    Workday,
}
const TOGGLES: usize = 610;
const FOOTER: usize = 615;
const MODE: usize = 621;
const THRESHOLDS: usize = 650;
const APPLY: usize = 670;
const MINIMAL: usize = 671;
const DEFAULTS: usize = 672;

pub struct Controls {
    pub page: Page,
    all: Vec<(usize, HWND)>,
    toggles: [HWND; 5],
    footer: HWND,
    mode: HWND,
    thresholds: [HWND; 8],
    work_title: HWND,
    work_days: [HWND; 7],
    work_times: [HWND; 14],
}
impl Default for Controls {
    fn default() -> Self {
        Self {
            page: Page::Dashboard,
            all: Vec::new(),
            toggles: [HWND::default(); 5],
            footer: HWND::default(),
            mode: HWND::default(),
            thresholds: [HWND::default(); 8],
            work_title: HWND::default(),
            work_days: [HWND::default(); 7],
            work_times: [HWND::default(); 14],
        }
    }
}

impl App {
    unsafe fn dashboard_control(
        &mut self,
        class: PCWSTR,
        label: &str,
        id: usize,
        style: WINDOW_STYLE,
    ) -> HWND {
        let handle = self.control(class, label, id, style);
        self.dashboard_controls.all.push((id, handle));
        handle
    }
    pub(super) unsafe fn create_dashboard_controls(&mut self) {
        self.dashboard_controls.all.clear();
        for (id, label) in [
            (PAGE_WIDGETS, "Diseño"),
            (PAGE_DASHBOARD, "Resumen"),
            (PAGE_WORKDAY, "Jornada"),
        ] {
            self.dashboard_control(w!("BUTTON"), label, id, WS_TABSTOP);
        }
        for (i, label) in [
            "Colores semánticos (blanco / ámbar / coral)",
            "Borde de alerta crítica (1 px, sin animación)",
            "Microetiquetas LOAD / CLOCK / COOLER / FANS",
            "Tiempo restante del bloque de IA (si se recibe)",
            "Tráfico de red debajo de Fans Case",
        ]
        .iter()
        .enumerate()
        {
            self.dashboard_controls.toggles[i] = self.dashboard_control(
                w!("BUTTON"),
                label,
                TOGGLES + i,
                WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            );
        }
        self.dashboard_controls.footer = self.dashboard_control(
            w!("BUTTON"),
            "Mostrar barra inferior",
            FOOTER,
            WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
        );
        self.dashboard_control(w!("STATIC"), "Barra inferior", 620, WINDOW_STYLE(0));
        self.dashboard_controls.mode = self.dashboard_control(
            w!("COMBOBOX"),
            "",
            MODE,
            WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE(CBS_DROPDOWNLIST as u32),
        );
        for label in [
            "Juego y energía",
            "Solo juego (original)",
            "Solo vatios CPU + GPU",
        ] {
            combo_add(self.dashboard_controls.mode, label);
        }
        for (id, label) in [
            (634, "Umbrales de alerta"),
            (635, "Advertencia"),
            (636, "Crítico"),
        ] {
            self.dashboard_control(w!("STATIC"), label, id, WINDOW_STYLE(0));
        }
        for (row, label) in [
            "Temperatura CPU (°C)",
            "Temperatura GPU (°C)",
            "Memoria GPU (°C)",
            "Carga / RAM / VRAM (%)",
        ]
        .iter()
        .enumerate()
        {
            self.dashboard_control(w!("STATIC"), label, 630 + row, WINDOW_STYLE(0));
            for col in 0..2 {
                let i = row * 2 + col;
                self.dashboard_controls.thresholds[i] = self.dashboard_control(
                    w!("EDIT"),
                    "",
                    THRESHOLDS + i,
                    WS_BORDER | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
                );
            }
        }
        for (id, label) in [
            (APPLY, "Aplicar umbrales"),
            (MINIMAL, "Estilo minimalista"),
            (DEFAULTS, "Restaurar opciones por defecto"),
        ] {
            self.dashboard_control(w!("BUTTON"), label, id, WS_TABSTOP);
        }
        self.dashboard_control(w!("STATIC"), "Vista previa en vivo · Guardar conserva los cambios.\r\nLos vatios representan CPU + GPU, no todo el equipo.", 673, WINDOW_STYLE(0));
        self.dashboard_control(
            w!("STATIC"),
            "Título del medidor (máximo 40 caracteres)",
            700,
            WINDOW_STYLE(0),
        );
        self.dashboard_controls.work_title = self.dashboard_control(
            w!("EDIT"),
            "",
            WORK_TITLE,
            WS_BORDER | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
        );
        self.dashboard_control(w!("STATIC"), "Día laborable", 702, WINDOW_STYLE(0));
        self.dashboard_control(w!("STATIC"), "Entrada", 703, WINDOW_STYLE(0));
        self.dashboard_control(w!("STATIC"), "Salida", 704, WINDOW_STYLE(0));
        for day in 0..7 {
            self.dashboard_controls.work_days[day] = self.dashboard_control(
                w!("BUTTON"),
                crate::work_schedule::DAYS[day],
                WORK_DAYS + day,
                WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32),
            );
            for col in 0..2 {
                self.dashboard_controls.work_times[day * 2 + col] = self.dashboard_control(
                    w!("EDIT"),
                    "",
                    WORK_TIMES + day * 2 + col,
                    WS_BORDER | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
                );
            }
        }
        self.dashboard_control(w!("BUTTON"), "Aplicar jornada", WORK_APPLY, WS_TABSTOP);
        self.dashboard_control(
            w!("BUTTON"),
            "Restaurar horario original",
            WORK_DEFAULTS,
            WS_TABSTOP,
        );
        self.dashboard_control(w!("STATIC"), "Horas HH:MM, en el mismo día; usa la hora local.\r\nDesmarca los días libres. No se descuentan pausas.\r\nGuardar conserva los cambios; cerrar permite descartarlos.", 742, WINDOW_STYLE(0));
        self.load_dashboard_controls();
    }
    pub(super) unsafe fn load_dashboard_controls(&self) {
        set_text(
            self.dashboard_controls.work_title,
            &self.config.workday.title,
        );
        for day in 0..7 {
            let hours = &self.config.workday.days[day];
            let _ = SendMessageW(
                self.dashboard_controls.work_days[day],
                BM_SETCHECK,
                Some(WPARAM(usize::from(hours.enabled))),
                None,
            );
            set_text(self.dashboard_controls.work_times[day * 2], &hours.start);
            set_text(self.dashboard_controls.work_times[day * 2 + 1], &hours.end);
        }
        let d = &self.config.dashboard;
        for (handle, checked) in self.dashboard_controls.toggles.iter().zip([
            d.enable_semantic_colors,
            d.enable_critical_pulse,
            d.show_bar_micro_labels,
            d.show_ai_reset_timer,
            d.show_network_in_header,
        ]) {
            let _ = SendMessageW(
                *handle,
                BM_SETCHECK,
                Some(WPARAM(usize::from(checked))),
                None,
            );
        }
        let _ = SendMessageW(
            self.dashboard_controls.footer,
            BM_SETCHECK,
            Some(WPARAM(usize::from(self.config.footer.enabled))),
            None,
        );
        let mode = match d.bottom_bar_mode {
            BottomBarMode::GameAndPower => 0,
            BottomBarMode::GameOnly => 1,
            BottomBarMode::TotalWattsOnly => 2,
        };
        let _ = SendMessageW(
            self.dashboard_controls.mode,
            CB_SETCURSEL,
            Some(WPARAM(mode)),
            None,
        );
        let t = &d.thresholds;
        for (handle, value) in self.dashboard_controls.thresholds.iter().zip([
            t.cpu_temp_warn,
            t.cpu_temp_crit,
            t.gpu_temp_warn,
            t.gpu_temp_crit,
            t.vram_temp_warn,
            t.vram_temp_crit,
            t.mem_usage_warn,
            t.mem_usage_crit,
        ]) {
            set_text(*handle, &value.to_string());
        }
    }
    pub(super) unsafe fn layout_dashboard_controls(&self, rect: &RECT, left: i32) {
        let active = self.dashboard_controls.page != Page::Design;
        for id in (ID_WIDGET..=ID_STATUS)
            .chain(300..315)
            .chain(400..415)
            .chain(500..505)
        {
            if [ID_SAVE, ID_IMPORT, ID_EXPORT, ID_STATUS].contains(&id) {
                continue;
            }
            if let Ok(handle) = GetDlgItem(Some(self.editor), id as i32) {
                let _ = ShowWindow(handle, if active { SW_HIDE } else { SW_SHOW });
            }
        }
        let scale = ((rect.bottom - 165) as f32 / 680.).clamp(0.65, 1.);
        let place = |id, x, y: i32, width, height: i32| {
            if let Ok(handle) = GetDlgItem(Some(self.editor), id) {
                let _ = MoveWindow(
                    handle,
                    left + x,
                    (y as f32 * scale) as i32,
                    width,
                    (height as f32 * scale) as i32,
                    true,
                );
            }
        };
        for (id, handle) in &self.dashboard_controls.all {
            let nav = (PAGE_WIDGETS..=PAGE_WORKDAY).contains(id);
            let visible = nav
                || if *id >= 700 {
                    self.dashboard_controls.page == Page::Workday
                } else {
                    self.dashboard_controls.page == Page::Dashboard
                };
            let _ = ShowWindow(*handle, if visible { SW_SHOW } else { SW_HIDE });
        }
        for (id, x) in [
            (PAGE_WIDGETS, 0),
            (PAGE_DASHBOARD, 150),
            (PAGE_WORKDAY, 300),
        ] {
            if let Ok(handle) = GetDlgItem(Some(self.editor), id as i32) {
                let _ = MoveWindow(handle, left + x, 5, 145, 28, true);
                let current = match self.dashboard_controls.page {
                    Page::Design => PAGE_WIDGETS,
                    Page::Dashboard => PAGE_DASHBOARD,
                    Page::Workday => PAGE_WORKDAY,
                };
                let _ = EnableWindow(handle, id != current);
            }
        }
        if !active {
            return;
        }
        for i in 0..5 {
            place((TOGGLES + i) as i32, 0, 62 + i as i32 * 34, 450, 28);
        }
        place(FOOTER as i32, 0, 232, 450, 28);
        place(620, 0, 278, 450, 20);
        place(MODE as i32, 0, 304, 450, 220);
        place(634, 0, 350, 450, 22);
        place(635, 236, 382, 100, 20);
        place(636, 348, 382, 100, 20);
        for row in 0..4 {
            let y = 410 + row * 38;
            place(630 + row, 0, y + 3, 230, 24);
            for col in 0..2 {
                place(
                    THRESHOLDS as i32 + row * 2 + col,
                    236 + col * 112,
                    y,
                    100,
                    26,
                );
            }
        }
        place(APPLY as i32, 0, 570, 450, 28);
        place(MINIMAL as i32, 0, 610, 200, 28);
        place(DEFAULTS as i32, 210, 610, 240, 28);
        place(673, 0, 650, 450, 42);
        place(700, 0, 62, 450, 24);
        place(WORK_TITLE as i32, 0, 92, 450, 30);
        place(702, 0, 150, 200, 24);
        place(703, 230, 150, 95, 24);
        place(704, 345, 150, 95, 24);
        for (row, day) in [1, 2, 3, 4, 5, 6, 0].into_iter().enumerate() {
            let y = 190 + row as i32 * 46;
            place((WORK_DAYS + day) as i32, 0, y, 220, 30);
            for col in 0..2 {
                place(
                    (WORK_TIMES + day * 2 + col) as i32,
                    230 + col as i32 * 115,
                    y,
                    95,
                    30,
                );
            }
        }
        place(WORK_APPLY as i32, 0, 530, 450, 30);
        place(WORK_DEFAULTS as i32, 0, 575, 450, 30);
        place(742, 0, 630, 450, 65);
        for (id, x) in [(ID_SAVE, 0), (ID_IMPORT, 150), (ID_EXPORT, 300)] {
            if let Ok(handle) = GetDlgItem(Some(self.editor), id as i32) {
                let _ = MoveWindow(handle, left + x, rect.bottom - 126, 140, 28, true);
            }
        }
        if let Ok(handle) = GetDlgItem(Some(self.editor), ID_STATUS as i32) {
            let _ = MoveWindow(handle, left, rect.bottom - 88, 450, 78, true);
        }
    }
    pub(super) unsafe fn apply_settings_page(&mut self) -> Result<(), String> {
        match self.dashboard_controls.page {
            Page::Design => self.apply_fields(),
            Page::Dashboard => self.apply_dashboard_thresholds(),
            Page::Workday => self.apply_workday(),
        }
    }
    unsafe fn apply_workday(&mut self) -> Result<(), String> {
        let mut schedule = self.config.workday.clone();
        schedule.title = text(self.dashboard_controls.work_title).trim().into();
        for day in 0..7 {
            schedule.days[day].enabled = SendMessageW(
                self.dashboard_controls.work_days[day],
                BM_GETCHECK,
                None,
                None,
            )
            .0 == 1;
            schedule.days[day].start = text(self.dashboard_controls.work_times[day * 2])
                .trim()
                .into();
            schedule.days[day].end = text(self.dashboard_controls.work_times[day * 2 + 1])
                .trim()
                .into();
        }
        schedule.validate()?;
        if schedule != self.config.workday {
            self.config.workday = schedule;
            self.dirty = true;
        }
        self.workday = workday::Presentation::now(&self.config);
        self.invalidate();
        Ok(())
    }
    pub(super) unsafe fn apply_dashboard_thresholds(&mut self) -> Result<(), String> {
        let mut values = [0f32; 8];
        for (value, handle) in values.iter_mut().zip(self.dashboard_controls.thresholds) {
            *value = text(handle)
                .trim()
                .replace(',', ".")
                .parse()
                .map_err(|_| "Umbrales: introduce números válidos")?;
        }
        let mut d = self.config.dashboard.clone();
        let t = &mut d.thresholds;
        [
            t.cpu_temp_warn,
            t.cpu_temp_crit,
            t.gpu_temp_warn,
            t.gpu_temp_crit,
            t.vram_temp_warn,
            t.vram_temp_crit,
            t.mem_usage_warn,
            t.mem_usage_crit,
        ] = values;
        d.validate()?;
        if d != self.config.dashboard {
            self.config.dashboard = d;
            self.dirty = true;
        }
        Ok(())
    }
    pub(super) unsafe fn dashboard_command(
        &mut self,
        id: usize,
        notification: usize,
    ) -> Result<bool, String> {
        if (PAGE_WIDGETS..=PAGE_WORKDAY).contains(&id) {
            if self.dashboard_controls.page == Page::Workday {
                self.apply_workday()?;
            }
            self.dashboard_controls.page = match id {
                PAGE_WIDGETS => Page::Design,
                PAGE_WORKDAY => Page::Workday,
                _ => Page::Dashboard,
            };
            if id == PAGE_WORKDAY {
                self.select_view(PanelView::Graphs);
            }
            self.layout_controls();
            self.invalidate();
            return Ok(true);
        }
        if id == WORK_APPLY {
            self.apply_workday()?;
            self.message.clear();
            return Ok(true);
        }
        if id == WORK_DEFAULTS {
            self.config.workday = crate::work_schedule::WorkdayConfig::default();
            self.dirty = true;
            self.load_dashboard_controls();
            self.workday = workday::Presentation::now(&self.config);
            self.invalidate();
            return Ok(true);
        }
        if (id == WORK_TITLE || (WORK_TIMES..WORK_TIMES + 14).contains(&id))
            && notification == EN_CHANGE as usize
            || (WORK_DAYS..WORK_DAYS + 7).contains(&id) && notification == BN_CLICKED as usize
        {
            self.dirty = true;
            self.message = match self.apply_workday() {
                Ok(()) => String::new(),
                Err(e) => format!("Jornada sin aplicar: {e}"),
            };
            return Ok(true);
        }
        if (TOGGLES..=FOOTER).contains(&id) && notification == BN_CLICKED as usize
            || id == MODE && notification == CBN_SELCHANGE as usize
        {
            let checked = |h| SendMessageW(h, BM_GETCHECK, None, None).0 == 1;
            let mut c = self.config.clone();
            let d = &mut c.dashboard;
            [
                d.enable_semantic_colors,
                d.enable_critical_pulse,
                d.show_bar_micro_labels,
                d.show_ai_reset_timer,
                d.show_network_in_header,
            ] = self.dashboard_controls.toggles.map(checked);
            c.footer.enabled = checked(self.dashboard_controls.footer);
            d.bottom_bar_mode =
                match SendMessageW(self.dashboard_controls.mode, CB_GETCURSEL, None, None).0 {
                    1 => BottomBarMode::GameOnly,
                    2 => BottomBarMode::TotalWattsOnly,
                    _ => BottomBarMode::GameAndPower,
                };
            if let Err(e) = c.validate() {
                self.load_dashboard_controls();
                return Err(e);
            }
            self.config = c;
            self.dirty = true;
            return Ok(true);
        }
        if (THRESHOLDS..THRESHOLDS + 8).contains(&id) && notification == EN_CHANGE as usize {
            match self.apply_dashboard_thresholds() {
                Ok(()) => self.message.clear(),
                Err(e) => self.message = format!("Umbrales sin aplicar: {e}"),
            }
            return Ok(true);
        }
        match id {
            APPLY => {
                self.apply_dashboard_thresholds()?;
                self.message.clear();
            }
            MINIMAL => {
                let thresholds = self.config.dashboard.thresholds.clone();
                self.config.dashboard = DashboardConfig {
                    thresholds,
                    ..DashboardConfig::minimal()
                };
                self.dirty = true;
                self.load_dashboard_controls();
            }
            DEFAULTS => {
                self.config.dashboard = DashboardConfig::default();
                self.dirty = true;
                self.load_dashboard_controls();
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}
