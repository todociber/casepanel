use super::{dashboard::PreparedText, footer};
use crate::{
    config::{Config, Widget},
    ha_client::{Entity, Event, Failure},
    ha_config::{HaActionType, HomeAssistantConfig},
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Default)]
pub struct Area {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Area {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Visual {
    Off,
    On,
    Unavailable,
    Trigger,
    Success,
    Confirm,
    SendingOn,
    SendingOff,
    SendingTrigger,
    Sensor,
}
pub struct Variant {
    pub fill: &'static str,
    pub border: &'static str,
    pub foreground: &'static str,
    pub border_width: f32,
    pub text: PreparedText,
}
pub struct Card {
    pub id: String,
    pub area: Area,
    pub icon: String,
    pub icon_at: (f32, f32),
    pub title: PreparedText,
    pub confirm_title: PreparedText,
    pub sublabel: Option<PreparedText>,
    pub variants: Vec<Variant>,
    pub visual: Visual,
}
#[derive(Default)]
pub struct Presentation {
    pub labels: Vec<PreparedText>,
    pub cards: Vec<Card>,
    pub retry: Option<Area>,
    pub previous: Option<Area>,
    pub next: Option<Area>,
    pub footer: footer::Presentation,
}
struct Pending {
    entity_id: String,
    before: Option<Entity>,
    kind: HaActionType,
}
pub enum Click {
    None,
    Confirm,
    Send(String),
}
#[derive(Default)]
pub struct State {
    pub config: HomeAssistantConfig,
    pub view: Presentation,
    pub connected: bool,
    connecting: bool,
    config_error: Option<String>,
    error: Option<&'static str>,
    action_error_until: Option<u64>,
    entities: BTreeMap<String, Entity>,
    pending: BTreeMap<String, Pending>,
    confirmations: BTreeMap<String, u64>,
    flashes: BTreeMap<String, u64>,
    page: usize,
}
impl State {
    pub fn configure(&mut self, result: Result<HomeAssistantConfig, String>) -> bool {
        let (config, error) = match result {
            Ok(c) => (c, None),
            Err(e) => (HomeAssistantConfig::default(), Some(e)),
        };
        if self.config == config && self.config_error == error {
            return false;
        }
        *self = Self {
            config,
            config_error: error,
            ..Self::default()
        };
        self.connecting = self.config.ready();
        true
    }
    pub fn retry(&mut self) {
        self.connecting = true;
        self.error = None;
        self.action_error_until = None;
    }
    pub fn apply(&mut self, event: Event, at: u64) {
        match event {
            Event::Poll(Ok(entities)) => {
                self.entities = entities;
                // A poll already in flight must not undo an optimistic click.
                for p in self.pending.values() {
                    if p.kind == HaActionType::Toggle
                        && let Some(previous) = &p.before
                    {
                        let mut optimistic = previous.clone();
                        optimistic.state = if previous.state == "on" { "off" } else { "on" }.into();
                        self.entities.insert(p.entity_id.clone(), optimistic);
                    }
                }
                self.connected = true;
                self.connecting = false;
                if self.action_error_until.is_none_or(|end| at >= end) {
                    self.error = None;
                }
            }
            Event::Poll(Err(error)) => {
                self.connected = false;
                self.connecting = false;
                self.error = Some(error.message());
            }
            Event::Action { id, result } => {
                if let Some(p) = self.pending.remove(&id) {
                    match result {
                        Ok(()) => {
                            self.connected = true;
                            if self.action_error_until.is_none_or(|end| at >= end) {
                                self.error = None;
                            }
                            if p.kind == HaActionType::Trigger {
                                self.flashes.insert(id, at + 1200);
                            }
                        }
                        Err(error) => {
                            if let Some(before) = p.before {
                                self.entities.insert(p.entity_id, before);
                            }
                            if matches!(
                                error,
                                Failure::Connection | Failure::Timeout | Failure::Authentication
                            ) {
                                self.connected = false;
                            }
                            self.error = Some(error.message());
                            self.action_error_until = Some(at + 3000);
                        }
                    }
                }
            }
        }
        self.refresh_selection(at);
    }
    pub fn click(&mut self, id: &str, at: u64) -> Click {
        let Some(action) = self.config.actions.iter().find(|a| a.id == id).cloned() else {
            return Click::None;
        };
        if !self.config.ready()
            || !self.connected
            || action.action_type == HaActionType::SensorReadOnly
            || self
                .pending
                .values()
                .any(|p| p.entity_id == action.entity_id)
        {
            return Click::None;
        }
        let entity = self.entities.get(&action.entity_id);
        if entity.is_none_or(|e| e.state == "unavailable")
            || action.action_type == HaActionType::Toggle
                && entity.is_none_or(|e| !matches!(e.state.as_str(), "on" | "off"))
        {
            return Click::None;
        }
        if action.confirm_required
            && self
                .confirmations
                .get(id)
                .is_none_or(|deadline| at >= *deadline)
        {
            self.confirmations.insert(id.into(), at + 3000);
            self.refresh_selection(at);
            return Click::Confirm;
        }
        self.confirmations.remove(id);
        self.flashes.remove(id);
        let before = entity.cloned();
        if action.action_type == HaActionType::Toggle
            && let Some(e) = self.entities.get_mut(&action.entity_id)
        {
            e.state = if e.state == "on" { "off" } else { "on" }.into();
        }
        self.pending.insert(
            id.into(),
            Pending {
                entity_id: action.entity_id,
                before,
                kind: action.action_type,
            },
        );
        self.refresh_selection(at);
        Click::Send(id.into())
    }
    pub fn expire(&mut self, at: u64) {
        if self.action_error_until.is_some_and(|end| at >= end) {
            self.action_error_until = None;
            if self.connected {
                self.error = None;
            }
        }
        self.confirmations.retain(|_, deadline| at < *deadline);
        self.flashes.retain(|_, deadline| at < *deadline);
        self.refresh_selection(at);
    }
    pub fn next_deadline(&self) -> Option<u64> {
        self.confirmations
            .values()
            .chain(self.flashes.values())
            .copied()
            .min()
    }
    pub fn change_page(&mut self, delta: isize) {
        self.page = self.page.saturating_add_signed(delta);
        self.confirmations.clear();
    }
    fn visual(&self, id: &str, at: u64) -> Visual {
        if self.confirmations.get(id).is_some_and(|end| at < *end) {
            return Visual::Confirm;
        }
        let Some(a) = self.config.actions.iter().find(|a| a.id == id) else {
            return Visual::Unavailable;
        };
        let state = self.entities.get(&a.entity_id).map(|e| e.state.as_str());
        if self.pending.contains_key(id) {
            return match (a.action_type, state) {
                (HaActionType::Toggle, Some("on")) => Visual::SendingOn,
                (HaActionType::Toggle, _) => Visual::SendingOff,
                _ => Visual::SendingTrigger,
            };
        }
        if self.flashes.get(id).is_some_and(|end| at < *end) {
            return Visual::Success;
        }
        match (a.action_type, state) {
            (_, None | Some("unavailable")) => Visual::Unavailable,
            (HaActionType::Toggle, Some("on")) => Visual::On,
            (HaActionType::Toggle, Some("off")) => Visual::Off,
            (HaActionType::Toggle, _) => Visual::Unavailable,
            (HaActionType::Trigger, _) => Visual::Trigger,
            (HaActionType::SensorReadOnly, Some("unknown")) => Visual::Unavailable,
            (HaActionType::SensorReadOnly, _) if self.connected => Visual::Sensor,
            _ => Visual::Unavailable,
        }
    }
    fn refresh_selection(&mut self, at: u64) {
        let selections: Vec<_> = self
            .view
            .cards
            .iter()
            .map(|c| self.visual(&c.id, at))
            .collect();
        for (card, visual) in self.view.cards.iter_mut().zip(selections) {
            card.visual = visual;
        }
    }
    pub fn prepare(&mut self, c: &Config, at: u64) {
        self.expire(at);
        let mut view = Presentation::default();
        let error_color = if c.dashboard.enable_semantic_colors {
            "#EF4444"
        } else {
            "#9CA3AF"
        };
        let width = c.canvas_width;
        view.labels.push(label(
            "ACCESOS",
            50.,
            34.,
            width * 0.5,
            70.,
            48.,
            "#FFFFFF",
            "left",
        ));
        let status = if self.config_error.is_some() {
            "● HA Configuración inválida"
        } else if !self.config.enabled {
            "● HA Desactivado"
        } else if !self.config.ready() {
            "● HA Sin configurar"
        } else if self.connecting {
            "● HA Conectando…"
        } else if self.connected {
            "● HA Conectado"
        } else {
            "● HA Sin conexión"
        };
        view.labels.push(label(
            status,
            width - 680.,
            46.,
            630.,
            40.,
            26.,
            if self.connected || !self.config.ready() {
                "#9CA3AF"
            } else {
                error_color
            },
            "right",
        ));
        if self.config.ready() && !self.connecting {
            view.retry = Some(Area {
                x: width - 230.,
                y: 98.,
                width: 180.,
                height: 42.,
            });
            view.labels.push(label(
                "Reintentar",
                width - 230.,
                101.,
                180.,
                36.,
                22.,
                "#BBBBBB",
                "center",
            ));
        }
        let note = self
            .config_error
            .as_deref()
            .or(self.error)
            .unwrap_or("Tu hogar, a un toque");
        view.labels.push(label(
            note,
            50.,
            110.,
            width - 330.,
            48.,
            22.,
            if self.error.is_some() || self.config_error.is_some() {
                error_color
            } else {
                "#6B7280"
            },
            "left",
        ));
        let bottom = if c.footer.enabled {
            c.footer_top()
        } else {
            c.canvas_height
        };
        if !self.config.ready() || self.config_error.is_some() {
            for (text, y, size, color) in [
                ("Conecta Home Assistant", 280., 40., "#FFFFFF"),
                (
                    "1. Abre config.toml junto a profile.json y añade [home_assistant].",
                    370.,
                    26.,
                    "#9CA3AF",
                ),
                (
                    "2. Define base_url y tu token de acceso de larga duración.",
                    425.,
                    26.,
                    "#9CA3AF",
                ),
                (
                    "3. Añade [[home_assistant.actions]] con tus entidades.",
                    480.,
                    26.,
                    "#9CA3AF",
                ),
                (
                    "Los cambios se cargan automáticamente. Usa config.example.toml como guía.",
                    555.,
                    22.,
                    "#6B7280",
                ),
            ] {
                view.labels
                    .push(label(text, 70., y, width - 140., 70., size, color, "left"));
            }
        } else {
            let columns = self.config.grid_columns;
            let rows = (((bottom - 220.) / 244.).floor() as usize).clamp(1, 12);
            let per_page = columns * rows;
            let pages = self.config.actions.len().div_ceil(per_page).max(1);
            self.page = self.page.min(pages - 1);
            if pages > 1 {
                let area = Area {
                    x: width - 640.,
                    y: 98.,
                    width: 50.,
                    height: 42.,
                };
                if self.page > 0 {
                    view.previous = Some(area);
                    view.labels.push(label(
                        "‹", area.x, 94., area.width, 45., 32., "#BBBBBB", "center",
                    ));
                }
                let area = Area {
                    x: width - 420.,
                    ..area
                };
                if self.page + 1 < pages {
                    view.next = Some(area);
                    view.labels.push(label(
                        "›", area.x, 94., area.width, 45., 32., "#BBBBBB", "center",
                    ));
                }
                view.labels.push(label(
                    &format!("{} / {pages}", self.page + 1),
                    width - 580.,
                    104.,
                    150.,
                    35.,
                    22.,
                    "#9CA3AF",
                    "center",
                ));
            }
            let tile_width = (width - 100. - 24. * (columns - 1) as f32) / columns as f32;
            let tile_height = 220f32.min((bottom - 210.).max(100.));
            for (index, action) in self
                .config
                .actions
                .iter()
                .skip(self.page * per_page)
                .take(per_page)
                .enumerate()
            {
                let area = Area {
                    x: 50. + (index % columns) as f32 * (tile_width + 24.),
                    y: 190. + (index / columns) as f32 * 244.,
                    width: tile_width,
                    height: tile_height,
                };
                let read_only = action.action_type == HaActionType::SensorReadOnly;
                let title_y = if read_only {
                    area.y + 25.
                } else {
                    area.y + 85.
                };
                let title_size = if read_only {
                    22.
                } else {
                    30f32.min(tile_width / 11.)
                };
                let title = label(
                    &action.label,
                    area.x + 24.,
                    title_y,
                    tile_width - 48.,
                    47.,
                    title_size,
                    "#FFFFFF",
                    "left",
                );
                let confirm_title = label(
                    "¿Confirmar?",
                    area.x + 24.,
                    title_y,
                    tile_width - 48.,
                    47.,
                    title_size,
                    "#FFFFFF",
                    "left",
                );
                let sublabel = action.sublabel.as_ref().map(|s| {
                    label(
                        s,
                        area.x + 24.,
                        if read_only {
                            area.y + 61.
                        } else {
                            area.y + 132.
                        },
                        tile_width - 48.,
                        28.,
                        18.,
                        "#9CA3AF",
                        "left",
                    )
                });
                let sensor = self
                    .entities
                    .get(&action.entity_id)
                    .map(|e| {
                        format!(
                            "{}{}{}",
                            e.state,
                            if e.attributes
                                .unit_of_measurement
                                .as_ref()
                                .is_some_and(|s| !s.is_empty())
                            {
                                " "
                            } else {
                                ""
                            },
                            e.attributes.unit_of_measurement.as_deref().unwrap_or("")
                        )
                    })
                    .unwrap_or_else(|| "—".into());
                let mut variants = Vec::new();
                for (mode, status) in [
                    (Visual::Off, "APAGADO"),
                    (Visual::On, "ENCENDIDO"),
                    (Visual::Unavailable, "NO DISPONIBLE"),
                    (Visual::Trigger, "TOCAR PARA EJECUTAR"),
                    (Visual::Success, "Ejecutado ✓"),
                    (Visual::Confirm, "Toca otra vez antes de 3 s"),
                    (Visual::SendingOn, "ENCENDIDO · Enviando…"),
                    (Visual::SendingOff, "APAGADO · Enviando…"),
                    (Visual::SendingTrigger, "Enviando…"),
                    (Visual::Sensor, sensor.as_str()),
                ] {
                    let lit = matches!(
                        mode,
                        Visual::On
                            | Visual::SendingOn
                            | Visual::Success
                            | Visual::Confirm
                            | Visual::Sensor
                    );
                    let border = if lit { "#FFFFFF" } else { "#27272A" };
                    variants.push(Variant {
                        fill: if matches!(mode, Visual::On | Visual::SendingOn) {
                            "#222226"
                        } else {
                            "#141414"
                        },
                        border,
                        foreground: if lit { "#FFFFFF" } else { "#6B7280" },
                        border_width: if lit { 1.5 } else { 1. },
                        text: label(
                            status,
                            area.x + 24.,
                            if mode == Visual::Sensor {
                                area.y + 103.
                            } else {
                                area.y + 177.
                            },
                            tile_width - 48.,
                            if mode == Visual::Sensor { 75. } else { 32. },
                            if mode == Visual::Sensor {
                                46f32.min(tile_width / 7.)
                            } else {
                                18f32.min(tile_width / 16.)
                            },
                            if lit { "#FFFFFF" } else { "#6B7280" },
                            if mode == Visual::Sensor {
                                "center"
                            } else {
                                "left"
                            },
                        ),
                    });
                }
                view.cards.push(Card {
                    id: action.id.clone(),
                    area,
                    icon: action.icon.clone(),
                    icon_at: if read_only {
                        (area.x + tile_width - 40., area.y + 35.)
                    } else {
                        (area.x + 42., area.y + 40.)
                    },
                    title,
                    confirm_title,
                    sublabel,
                    variants,
                    visual: self.visual(&action.id, at),
                });
            }
        }
        if c.footer.enabled {
            view.footer.lines.push(label(
                "HOME ASSISTANT",
                width * 0.35,
                c.footer_top() + 60.,
                width * 0.30,
                40.,
                26.,
                "#6B7280",
                "center",
            ));
        }
        self.view = view;
    }
}

#[allow(clippy::too_many_arguments)]
fn label(
    text: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    size: f32,
    color: &str,
    align: &str,
) -> PreparedText {
    PreparedText::new(Widget {
        x,
        y,
        width,
        height,
        text: text.into(),
        font_size: size,
        color: color.into(),
        align: align.into(),
        ..Widget::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ha_client::Attributes, ha_config::HaActionItem};
    #[test]
    fn trigger_flash_lasts_1200_ms_and_grid_paginates_without_losing_actions() {
        let mut s = ready_state(false);
        s.config.actions[0].action_type = HaActionType::Trigger;
        s.config.actions[0].entity_id = "script.night".into();
        let mut entity = s.entities.remove("light.office").unwrap();
        entity.entity_id = "script.night".into();
        s.entities.insert(entity.entity_id.clone(), entity);
        s.config.validate().unwrap();
        assert!(matches!(s.click("light", 0), Click::Send(_)));
        s.apply(
            Event::Action {
                id: "light".into(),
                result: Ok(()),
            },
            100,
        );
        assert_eq!(s.view.cards[0].visual as usize, Visual::Success as usize);
        s.expire(1299);
        assert_eq!(s.view.cards[0].visual as usize, Visual::Success as usize);
        s.expire(1300);
        assert_eq!(s.view.cards[0].visual as usize, Visual::Trigger as usize);
        for i in 0..20 {
            let mut action = s.config.actions[0].clone();
            action.id = format!("action-{i}");
            s.config.actions.push(action);
        }
        s.prepare(&Config::default(), 1400);
        assert_eq!(s.view.cards.len(), 9);
        assert!(s.view.next.is_some());
        assert!(s.view.previous.is_none());
        s.change_page(1);
        s.prepare(&Config::default(), 1400);
        assert_eq!(s.view.cards.len(), 9);
        assert!(s.view.previous.is_some());
        s.change_page(1);
        s.prepare(&Config::default(), 1400);
        assert_eq!(s.view.cards.len(), 3);
        assert!(s.view.next.is_none());
    }
    fn ready_state(confirm: bool) -> State {
        let mut s = State::default();
        s.configure(Ok(HomeAssistantConfig {
            base_url: "http://127.0.0.1:8123".into(),
            token: "test".into(),
            actions: vec![HaActionItem {
                id: "light".into(),
                label: "Office".into(),
                entity_id: "light.office".into(),
                action_type: HaActionType::Toggle,
                confirm_required: confirm,
                ..HaActionItem::default()
            }],
            ..HomeAssistantConfig::default()
        }));
        s.apply(
            Event::Poll(Ok(BTreeMap::from([(
                "light.office".into(),
                Entity {
                    entity_id: "light.office".into(),
                    state: "off".into(),
                    attributes: Attributes::default(),
                },
            )]))),
            0,
        );
        s.prepare(&Config::default(), 0);
        s
    }
    #[test]
    fn confirmation_expires_and_click_feedback_is_immediate_without_preparing() {
        let mut s = ready_state(true);
        assert!(matches!(s.click("light", 0), Click::Confirm));
        assert_eq!(s.view.cards[0].visual as usize, Visual::Confirm as usize);
        assert!(matches!(s.click("light", 3000), Click::Confirm));
        assert!(matches!(s.click("light", 3100), Click::Send(_)));
        assert_eq!(s.entities["light.office"].state, "on");
        assert!(matches!(s.click("light", 3200), Click::None));
        s.apply(
            Event::Action {
                id: "light".into(),
                result: Err(Failure::Timeout),
            },
            3300,
        );
        assert_eq!(s.entities["light.office"].state, "off");
        assert_eq!(s.view.cards[0].visual as usize, Visual::Off as usize);
    }
    #[test]
    fn in_flight_poll_does_not_override_optimism_and_read_only_never_sends() {
        let mut s = ready_state(false);
        let old = s.entities.clone();
        assert!(matches!(s.click("light", 0), Click::Send(_)));
        s.apply(Event::Poll(Ok(old)), 1);
        assert_eq!(s.entities["light.office"].state, "on");
        s.config.actions[0].action_type = HaActionType::SensorReadOnly;
        s.pending.clear();
        assert!(matches!(s.click("light", 2), Click::None));
    }
    #[test]
    fn successful_reconciliation_does_not_hide_an_action_error_before_it_is_visible() {
        let mut s = ready_state(false);
        let old = s.entities.clone();
        assert!(matches!(s.click("light", 0), Click::Send(_)));
        s.apply(
            Event::Action {
                id: "light".into(),
                result: Err(Failure::Http(503)),
            },
            10,
        );
        s.apply(Event::Poll(Ok(old)), 11);
        assert_eq!(s.entities["light.office"].state, "off");
        assert_eq!(s.error, Some(Failure::Http(503).message()));
        s.expire(3009);
        assert!(s.error.is_some());
        s.expire(3010);
        assert!(s.error.is_none());
    }
}
