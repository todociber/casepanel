use super::{
    dashboard::PreparedText,
    render::{convert_unit, resolve},
};
use crate::{
    config::{BottomBarMode, Config, Widget},
    model::{Quality, Store, now_ms},
    sensors::system::SystemReadings,
};

#[derive(Default)]
pub struct Presentation {
    pub pill: bool,
    pub lines: Vec<PreparedText>,
}

pub fn presentation(c: &Config, store: &Store, system: &SystemReadings, at: u64) -> Presentation {
    let mut result = Presentation::default();
    if !c.footer.enabled {
        return result;
    }
    let top = c.footer_top();
    let width = c.canvas_width * 0.30;
    let text = |value: String, y, height, size, color: &str| {
        PreparedText::new(Widget {
            x: (c.canvas_width - width) / 2.,
            y,
            width,
            height,
            text: value,
            font_size: size,
            color: color.into(),
            align: "center".into(),
            ..Widget::default()
        })
    };
    let reading = readout(store);
    let mode = c.dashboard.bottom_bar_mode;
    if mode != BottomBarMode::TotalWattsOnly
        && let Some(fps) = reading.fps
    {
        let name = if mode == BottomBarMode::GameAndPower {
            format!("● {}", reading.application)
        } else {
            reading.application
        };
        result.lines.push(text(
            name,
            top + 7.,
            29.,
            23.,
            if mode == BottomBarMode::GameOnly {
                "#999999"
            } else {
                "#FFFFFF"
            },
        ));
        result.lines.push(text(
            format!("{fps:.0} FPS"),
            top + 34.,
            85.,
            70.,
            "#FFFFFF",
        ));
        if let Some(ms) = reading.frame_ms {
            result.lines.push(text(
                format!("{ms:.1} ms por fotograma"),
                top + 119.,
                28.,
                23.,
                "#999999",
            ));
        }
    } else if mode == BottomBarMode::GameOnly {
        result
            .lines
            .push(text(reading.status.into(), top + 48., 50., 34., "#888888"));
    } else {
        result.pill = true;
        let watts = combined_watts(c, store, at)
            .map(|v| format!("{v:.0} W"))
            .unwrap_or_else(|| "— W".into());
        let value = if mode == BottomBarMode::TotalWattsOnly {
            watts
        } else {
            let plan = system.power_plan.as_deref().unwrap_or("Plan no disponible");
            let short = if plan.chars().count() > 24 {
                format!("{}…", plan.chars().take(23).collect::<String>())
            } else {
                plan.into()
            };
            format!("⚡ {short} • {watts}")
        };
        result.lines.push(text(
            value,
            top + 52.,
            50.,
            if mode == BottomBarMode::TotalWattsOnly {
                40.
            } else {
                28.
            },
            "#DDDDDD",
        ));
        let note = if mode == BottomBarMode::GameAndPower && reading.status != "Sin juego" {
            reading.status
        } else {
            "CPU + GPU"
        };
        result
            .lines
            .push(text(note.into(), top + 108., 28., 20., "#888888"));
    }
    result
}

fn combined_watts(c: &Config, store: &Store, at: u64) -> Option<f64> {
    let watts = |key| {
        let s = resolve(c, store, key)?;
        if s.current_quality(at) != Quality::Valid {
            return None;
        }
        s.value
            .and_then(|v| convert_unit(v, &s.unit, "W"))
            .filter(|v| *v >= 0.)
    };
    Some(watts("cpu.power")? + watts("gpu.power")?)
}

#[derive(Clone, Copy)]
pub struct Button {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub label: &'static str,
    pub enabled: bool,
}
pub fn buttons(c: &Config) -> [Button; 4] {
    let width = c.canvas_width * 0.15;
    let margin = c.canvas_width * 0.025;
    let gap = c.canvas_width / 64.;
    let positions = [
        margin,
        margin + width + gap,
        c.canvas_width - margin - width * 2. - gap,
        c.canvas_width - margin - width,
    ];
    std::array::from_fn(|i| Button {
        x: positions[i],
        y: c.footer_top() + 30.,
        width,
        height: 100.,
        label: ["Resumen", "Gráficas", "Accesos", "Ajustes"][i],
        enabled: i == 0 || i == 3,
    })
}
pub fn settings_hit(c: &Config, x: f32, y: f32) -> bool {
    if !c.footer.enabled {
        return false;
    }
    let button = buttons(c)[3];
    (button.x..button.x + button.width).contains(&x)
        && (button.y..button.y + button.height).contains(&y)
}
pub struct Readout {
    pub application: String,
    pub fps: Option<f64>,
    pub frame_ms: Option<f64>,
    pub status: &'static str,
}
pub fn readout(store: &Store) -> Readout {
    let fps = store.sensors.get("presentmon/game.fps");
    let frame = store.sensors.get("presentmon/game.frame_time");
    if let Some(s) = fps
        && s.current_quality(now_ms()) == Quality::Valid
        && s.value.is_some()
    {
        let name = s.message.strip_suffix(".exe").unwrap_or(&s.message);
        return Readout {
            application: if name.starts_with("FortniteClient") {
                "FORTNITE".into()
            } else {
                name.to_string()
            },
            fps: s.value,
            frame_ms: frame
                .filter(|f| f.current_quality(now_ms()) == Quality::Valid)
                .and_then(|f| f.value),
            status: "",
        };
    }
    Readout {
        application: String::new(),
        fps: None,
        frame_ms: None,
        status: match fps {
            Some(s) if s.message == "No game detected" => "Sin juego",
            Some(_) => "FPS no disponibles",
            None => "Esperando FPS",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Sensor;
    #[test]
    fn bottom_bar_modes_handle_live_games_missing_power_and_stale_samples() {
        let mut c = Config::default();
        let mut store = Store::default();
        let mut cpu = Sensor::new("cpu-test", "cpu.power", "CPU", "W", 1000);
        let mut gpu = Sensor::new("gpu-test", "gpu.power", "GPU", "mW", 1000);
        cpu.set(40.);
        gpu.set(77_000.);
        let now = now_ms();
        store.update(cpu);
        store.update(gpu);
        let system = SystemReadings {
            power_plan: Some("Equilibrado".into()),
            ..SystemReadings::default()
        };
        let view = presentation(&c, &store, &system, now);
        assert!(view.pill);
        assert!(view.lines[0].widget.text.contains("Equilibrado • 117 W"));
        let stale = presentation(&c, &store, &system, now + 4000);
        assert!(stale.lines[0].widget.text.contains("— W"));
        let mut fps = Sensor::new("presentmon", "game.fps", "FPS", "FPS", 1000);
        fps.set(144.);
        fps.message = "Elden Ring.exe".into();
        store.update(fps);
        let game = presentation(&c, &store, &system, now);
        assert!(!game.pill);
        assert_eq!(game.lines[0].widget.text, "● Elden Ring");
        c.dashboard.bottom_bar_mode = BottomBarMode::TotalWattsOnly;
        let watts = presentation(&c, &store, &system, now);
        assert!(watts.pill);
        assert_eq!(watts.lines[0].widget.text, "117 W");
        c.dashboard.bottom_bar_mode = BottomBarMode::GameOnly;
        let original = presentation(&c, &store, &system, now);
        assert_eq!(original.lines[0].widget.text, "Elden Ring");
        store.sensors.remove("gpu-test/gpu.power");
        assert!(combined_watts(&c, &store, now).is_none());
        store.sensors.remove("presentmon/game.fps");
        assert!(!presentation(&c, &store, &system, now).pill);
    }
    #[test]
    fn hit_testing_respects_reserved_slots_and_scaled_canvas() {
        let c = Config::default();
        let settings = buttons(&c)[3];
        assert!(settings_hit(&c, settings.x + 1., settings.y + 1.));
        assert!(!settings_hit(&c, buttons(&c)[1].x + 1., settings.y + 1.));
        assert!(!settings_hit(&c, settings.x, 900.));
    }
    #[test]
    fn stopped_game_clears_footer_instead_of_displaying_stale_fps() {
        let mut store = Store::default();
        let mut sensor = Sensor::new("presentmon", "game.fps", "FPS", "FPS", 1000);
        sensor.set(144.);
        sensor.message = "game.exe".into();
        store.update(sensor);
        assert_eq!(readout(&store).fps, Some(144.));
        let mut absent = Sensor::new("presentmon", "game.fps", "FPS", "FPS", 1000);
        absent.message = "No game detected".into();
        store.update(absent);
        assert_eq!(readout(&store).fps, None);
        assert_eq!(readout(&store).status, "Sin juego");
    }
}
