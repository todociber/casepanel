use super::{
    footer,
    render::{convert_unit, formatted, resolve},
};
use crate::{
    config::{Config, DashboardConfig, Widget, WidgetKind},
    model::{Quality, Store},
    platform,
    sensors::system::SystemReadings,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    White,
    Amber,
    Coral,
}
impl Color {
    pub fn hex(self) -> &'static str {
        match self {
            Self::White => "#FFFFFF",
            Self::Amber => "#F59E0B",
            Self::Coral => "#EF4444",
        }
    }
}
pub fn get_status_color(value: f32, warn: f32, crit: f32, config: &DashboardConfig) -> Color {
    if !config.enable_semantic_colors || !value.is_finite() || value < warn {
        Color::White
    } else if value >= crit {
        Color::Coral
    } else {
        Color::Amber
    }
}

pub struct PreparedText {
    pub widget: Widget,
    pub words: Vec<u16>,
}
impl PreparedText {
    pub fn new(widget: Widget) -> Self {
        let words = widget.text.encode_utf16().collect();
        Self { widget, words }
    }
}
pub struct WidgetState {
    pub words: Vec<u16>,
    pub color: String,
    pub dim: bool,
    pub fraction: Option<f32>,
}

/// One presentation snapshot shared by both windows; WM_PAINT never formats telemetry.
pub struct DashboardState {
    pub widgets: BTreeMap<String, WidgetState>,
    pub labels: Vec<PreparedText>,
    pub critical_cards: BTreeSet<String>,
    pub footer: footer::Presentation,
    cpu_name: String,
    memory_name: String,
}
impl DashboardState {
    pub fn new() -> Self {
        let inventory = platform::inventory();
        Self {
            widgets: BTreeMap::new(),
            labels: Vec::new(),
            critical_cards: BTreeSet::new(),
            footer: footer::Presentation::default(),
            cpu_name: inventory.cpu_name.replace(" 12-Core Processor", ""),
            memory_name: if inventory.memory_speed_mt > 0 {
                format!("{}-{}", inventory.memory_type, inventory.memory_speed_mt)
            } else {
                "RAM".into()
            },
        }
    }
    pub fn update(&mut self, c: &Config, store: &Store, system: &SystemReadings, at: u64) {
        self.widgets.clear();
        self.labels.clear();
        self.critical_cards.clear();
        let d = &c.dashboard;
        let t = &d.thresholds;
        for w in &c.widgets {
            let sensor = resolve(c, store, &w.sensor);
            let dim = !w.sensor.is_empty()
                && sensor.is_none_or(|s| {
                    !matches!(s.current_quality(at), Quality::Valid | Quality::Reported)
                });
            let value =
                sensor.and_then(|s| s.value.and_then(|v| convert_unit(v, &s.unit, &w.unit)));
            let key = sensor.map_or(w.sensor.as_str(), |s| s.key.as_str());
            let bounds = match (w.kind, key) {
                (WidgetKind::Gauge, "cpu.temperature") => Some((t.cpu_temp_warn, t.cpu_temp_crit)),
                (WidgetKind::Gauge, "gpu.temperature") => Some((t.gpu_temp_warn, t.gpu_temp_crit)),
                (WidgetKind::Bar, "cpu.load" | "gpu.load" | "ram.load" | "gpu.vram.load") => {
                    Some((t.mem_usage_warn, t.mem_usage_crit))
                }
                (WidgetKind::Value, "gpu.memory_temperature") => {
                    Some((t.vram_temp_warn, t.vram_temp_crit))
                }
                _ => None,
            };
            let color = bounds
                .map(|(warn, crit)| {
                    let source = sensor.and_then(|s| s.value);
                    // Compare temperatures in Celsius even if a widget displays Fahrenheit.
                    let source = if key.ends_with("temperature") {
                        sensor.and_then(|s| s.value.and_then(|v| convert_unit(v, &s.unit, "°C")))
                    } else {
                        source
                    };
                    get_status_color(
                        if dim {
                            f32::NAN
                        } else {
                            source.unwrap_or(f64::NAN) as f32
                        },
                        warn,
                        crit,
                        d,
                    )
                    .hex()
                })
                .unwrap_or(&w.color)
                .to_string();
            let text = match w.kind {
                WidgetKind::Value => formatted(w, sensor),
                WidgetKind::Clock => {
                    let mut local = platform::LocalTime::default();
                    unsafe {
                        platform::GetLocalTime(&mut local);
                    }
                    format!("{:02}:{:02}:{:02}", local.hour, local.minute, local.second)
                }
                WidgetKind::Text => {
                    let gpu = resolve(c, store, "gpu.temperature")
                        .map(|s| s.label.split(": ").next().unwrap_or("GPU"))
                        .unwrap_or("not detected");
                    w.text
                        .replace("{cpu_name}", &self.cpu_name)
                        .replace("{gpu_name}", gpu)
                        .replace("{memory_name}", &self.memory_name)
                }
                _ => String::new(),
            };
            self.widgets.insert(
                w.id.clone(),
                WidgetState {
                    words: text.encode_utf16().collect(),
                    color,
                    dim,
                    fraction: value.map(|v| ((v - w.min) / (w.max - w.min)).clamp(0., 1.) as f32),
                },
            );
            if d.show_bar_micro_labels && w.kind == WidgetKind::Bar {
                let label = match w.id.as_str() {
                    "cpu_load_bar" | "gpu_load_bar" => Some("LOAD"),
                    "cpu_clock_bar" | "gpu_clock_bar" => Some("CLOCK"),
                    "cpu_fan_bar" => Some("COOLER"),
                    "gpu_fan_bar" => Some("FANS"),
                    _ => None,
                };
                if let Some(label) = label {
                    self.labels.push(PreparedText::new(Widget {
                        x: w.x,
                        y: w.y - 20.,
                        width: w.width,
                        height: 17.,
                        text: label.into(),
                        font_size: 12.,
                        color: "#6B7280".into(),
                        ..Widget::default()
                    }));
                }
            }
            if d.show_ai_reset_timer
                && w.kind == WidgetKind::Value
                && key.starts_with("ai.")
                && !dim
                && let Some(reset) = sensor.and_then(|s| s.reset_at_ms)
                && let Some(timer) = reset_timer(reset, at)
            {
                self.labels.push(PreparedText::new(Widget {
                    x: w.x + w.width + 12.,
                    y: w.y + 4.,
                    width: 220.,
                    height: w.height,
                    text: timer,
                    font_size: 20.,
                    color: "#6B7280".into(),
                    ..Widget::default()
                }));
            }
        }
        if d.enable_semantic_colors && d.enable_critical_pulse {
            for (key, crit, cards) in [
                ("cpu.temperature", t.cpu_temp_crit, &["cpu_tile"][..]),
                ("cpu.load", t.mem_usage_crit, &["cpu_tile"][..]),
                ("gpu.temperature", t.gpu_temp_crit, &["gpu_tile"][..]),
                ("gpu.load", t.mem_usage_crit, &["gpu_tile"][..]),
                ("ram.load", t.mem_usage_crit, &["ram_tile"][..]),
                ("gpu.vram.load", t.mem_usage_crit, &["vram_tile"][..]),
                (
                    "gpu.memory_temperature",
                    t.vram_temp_crit,
                    &["gpu_tile", "vram_tile", "power_tile"][..],
                ),
            ] {
                if let Some(s) = resolve(c, store, key)
                    && s.current_quality(at) == Quality::Valid
                    && let Some(v) = s.value.and_then(|v| {
                        convert_unit(
                            v,
                            &s.unit,
                            if key.ends_with("temperature") {
                                "°C"
                            } else {
                                "%"
                            },
                        )
                    })
                    && v >= crit as f64
                {
                    self.critical_cards
                        .extend(cards.iter().map(|s| s.to_string()));
                }
            }
        }
        if d.show_network_in_header
            && let Some(w) = c.widgets.iter().find(|w| w.id == "case_bar")
        {
            let text = match (system.down_mb_s, system.up_mb_s) {
                (Some(down), Some(up)) => format!("↓ {down:.1} MB/s   ↑ {up:.1} MB/s"),
                _ => "↓ — MB/s   ↑ — MB/s".into(),
            };
            self.labels.push(PreparedText::new(Widget {
                x: w.x,
                y: w.y + w.height + 6.,
                width: w.width.max(420.),
                height: 32.,
                text,
                font_size: 20.,
                color: "#9CA3AF".into(),
                ..Widget::default()
            }));
        }
        self.footer = footer::presentation(c, store, system, at);
    }
}

fn reset_timer(reset: u64, now: u64) -> Option<String> {
    let remaining = reset.checked_sub(now)?;
    if remaining == 0 || remaining > 5 * 60 * 60 * 1000 {
        return None;
    }
    let minutes = remaining.div_ceil(60_000);
    Some(format!("• {}h {:02}m", minutes / 60, minutes % 60))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Sensor;
    #[test]
    fn optional_overlays_and_critical_borders_are_independently_switchable() {
        let mut c = Config::default();
        let mut store = Store::default();
        let now = crate::model::now_ms();
        let mut ai = Sensor::new("registry-ai", "ai.claude.5h", "Claude", "%", 5000);
        ai.set(25.);
        ai.quality = Quality::Reported;
        ai.reset_at_ms = Some(now + 8_040_000);
        store.update(ai);
        for (key, value, unit) in [
            ("cpu.temperature", 85., "°C"),
            ("gpu.temperature", 70., "°C"),
            ("gpu.memory_temperature", 95., "°C"),
            ("ram.load", 80., "%"),
            ("gpu.vram.load", 92., "%"),
            ("gpu.load", 92., "%"),
        ] {
            let mut sensor = Sensor::new("fixture", key, key, unit, 1000);
            sensor.set(value);
            store.update(sensor);
        }
        let system = SystemReadings {
            down_mb_s: Some(12.4),
            up_mb_s: Some(1.8),
            ..SystemReadings::default()
        };
        let mut view = DashboardState::new();
        view.update(&c, &store, &system, now);
        assert_eq!(view.labels.len(), 8);
        assert!(view.labels.iter().any(|l| l.widget.text == "• 2h 14m"));
        assert!(
            view.labels
                .iter()
                .any(|l| l.widget.text == "↓ 12.4 MB/s   ↑ 1.8 MB/s")
        );
        for (id, color) in [
            ("cpu_gauge", "#EF4444"),
            ("gpu_gauge", "#F59E0B"),
            ("gpu_memory_temp", "#EF4444"),
            ("ram_bar", "#F59E0B"),
            ("vram_bar", "#EF4444"),
            ("gpu_load_bar", "#EF4444"),
        ] {
            assert_eq!(view.widgets[id].color, color);
        }
        c.dashboard.show_bar_micro_labels = false;
        c.dashboard.enable_critical_pulse = false;
        view.update(&c, &store, &system, now);
        assert_eq!(view.labels.len(), 2);
        assert!(view.critical_cards.is_empty());
        assert_eq!(view.widgets["cpu_gauge"].color, "#EF4444");
        c.dashboard.show_network_in_header = false;
        view.update(&c, &store, &system, now);
        assert_eq!(view.labels.len(), 1);
        c.dashboard.show_ai_reset_timer = false;
        view.update(&c, &store, &system, now);
        assert!(view.labels.is_empty());
    }
    #[test]
    fn semantic_boundaries_and_monochrome_override() {
        let mut c = DashboardConfig::default();
        assert_eq!(get_status_color(74.9, 75., 85., &c), Color::White);
        assert_eq!(get_status_color(75., 75., 85., &c), Color::Amber);
        assert_eq!(get_status_color(85., 75., 85., &c), Color::Coral);
        assert_eq!(get_status_color(f32::NAN, 75., 85., &c), Color::White);
        c.enable_semantic_colors = false;
        assert_eq!(get_status_color(100., 75., 85., &c), Color::White);
    }
    #[test]
    fn reset_timer_requires_a_real_future_time_inside_the_window() {
        assert_eq!(reset_timer(8_040_000, 0).as_deref(), Some("• 2h 14m"));
        assert!(reset_timer(1, 2).is_none());
        assert!(reset_timer(0, 0).is_none());
        assert!(reset_timer(18_000_001, 0).is_none());
    }
    #[test]
    fn cache_handles_custom_units_stale_data_and_minimal_mode() {
        let mut c = Config::default();
        let mut store = Store::default();
        let mut sensor = Sensor::new("test", "cpu.temperature", "CPU", "°C", 1000);
        sensor.set(85.);
        let now = sensor.observed_ms;
        store.update(sensor);
        c.widgets
            .iter_mut()
            .find(|w| w.id == "cpu_gauge")
            .unwrap()
            .unit = "°F".into();
        let mut view = DashboardState::new();
        view.update(&c, &store, &SystemReadings::default(), now);
        assert_eq!(view.widgets["cpu_gauge"].color, "#EF4444");
        assert!(view.critical_cards.contains("cpu_tile"));
        view.update(&c, &store, &SystemReadings::default(), now + 3001);
        assert!(view.critical_cards.is_empty());
        assert_eq!(view.widgets["cpu_gauge"].color, "#FFFFFF");
        c.dashboard = DashboardConfig::minimal();
        view.update(&c, &store, &SystemReadings::default(), now);
        assert!(view.labels.is_empty());
        assert!(view.critical_cards.is_empty());
        assert_eq!(view.widgets["cpu_gauge"].color, "#FFFFFF");
    }
}
