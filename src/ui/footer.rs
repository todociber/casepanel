use crate::{
    config::Config,
    model::{Quality, Store, now_ms},
};

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
