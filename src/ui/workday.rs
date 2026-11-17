use super::dashboard::PreparedText;
use crate::{
    config::{Config, Widget},
    platform,
    work_schedule::{DAYS, WorkdayConfig, clock},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Before,
    Working,
    Finished,
    Rest,
}

#[derive(Debug)]
struct Progress {
    phase: Phase,
    start: u32,
    end: u32,
    elapsed: u32,
    remaining: u32,
    until_start: u32,
}
impl Progress {
    fn at(schedule: &WorkdayConfig, weekday: u16, seconds: u32) -> Self {
        let Some((start, end)) = schedule.shift(weekday) else {
            return Self {
                phase: Phase::Rest,
                start: 0,
                end: 0,
                elapsed: 0,
                remaining: 0,
                until_start: 0,
            };
        };
        let elapsed = seconds.saturating_sub(start).min(end - start);
        Self {
            phase: if seconds < start {
                Phase::Before
            } else if seconds < end {
                Phase::Working
            } else {
                Phase::Finished
            },
            start,
            end,
            elapsed,
            remaining: end - start - elapsed,
            until_start: start.saturating_sub(seconds),
        }
    }
    fn fraction(&self) -> f32 {
        if self.end == 0 {
            0.
        } else {
            self.elapsed as f32 / (self.end - self.start) as f32
        }
    }
}
fn timeline(start: u32, end: u32) -> Vec<u32> {
    let step = (end - start).div_ceil(3600).div_ceil(10) * 3600;
    let mut ticks: Vec<_> = (start..end).step_by(step.max(1) as usize).collect();
    if ticks.len() > 1 && end - ticks[ticks.len() - 1] < step / 2 {
        ticks.pop();
    }
    ticks.push(end);
    ticks
}
fn duration(seconds: u32) -> String {
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

pub struct Block {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: &'static str,
}
#[derive(Default)]
pub struct Presentation {
    pub labels: Vec<PreparedText>,
    pub blocks: Vec<Block>,
    pub fraction: f32,
    pub accent: &'static str,
}
impl Presentation {
    pub fn now(c: &Config) -> Self {
        let mut local = platform::LocalTime::default();
        unsafe {
            platform::GetLocalTime(&mut local);
        }
        Self::at(c, &local)
    }
    fn at(c: &Config, local: &platform::LocalTime) -> Self {
        let seconds =
            u32::from(local.hour) * 3600 + u32::from(local.minute) * 60 + u32::from(local.second);
        let p = Progress::at(&c.workday, local.weekday, seconds);
        let ticks = if p.end > p.start {
            timeline(p.start, p.end)
        } else {
            vec![]
        };
        let next = c
            .workday
            .next_shift(local.weekday)
            .map(|(day, start, end)| {
                format!(
                    "Próxima jornada: {} · {}–{}",
                    DAYS[day],
                    clock(start),
                    clock(end)
                )
            })
            .unwrap_or_else(|| "No hay días laborables configurados".into());
        let accent = if c.dashboard.enable_semantic_colors {
            "#80E2BD"
        } else {
            "#FFFFFF"
        };
        let mut view = Self {
            fraction: p.fraction(),
            accent,
            ..Self::default()
        };
        // Use one logical layout on every canvas, keeping the persistent footer free.
        let sx = c.canvas_width / 1920.;
        let sy = if c.footer.enabled {
            c.footer_top()
        } else {
            c.canvas_height
        } / 1050.;
        let mut label = |value: String, x, y, width, height, size, color: &str, align: &str| {
            view.labels.push(PreparedText::new(Widget {
                x: x * sx,
                y: y * sy,
                width: width * sx,
                height: height * sy,
                text: value,
                font_size: size * sx.min(sy),
                color: color.into(),
                align: align.into(),
                ..Widget::default()
            }));
        };
        let day = DAYS.get(local.weekday as usize).copied().unwrap_or("Hoy");
        label(
            "GRÁFICAS / TU JORNADA".into(),
            60.,
            24.,
            1000.,
            54.,
            30.,
            "#9CA3AF",
            "left",
        );
        label(
            format!(
                "{day} · {:02}:{:02}:{:02} · Hora local",
                local.hour, local.minute, local.second
            ),
            1060.,
            24.,
            800.,
            54.,
            28.,
            "#9CA3AF",
            "right",
        );
        label(
            c.workday.title.clone(),
            110.,
            150.,
            1500.,
            85.,
            if c.workday.title.chars().count() > 28 {
                44.
            } else {
                64.
            },
            "#FFFFFF",
            "left",
        );
        let weekend = p.phase == Phase::Rest;
        label(
            if weekend {
                "LIBRE".into()
            } else {
                format!("{:.0}%", (p.fraction() * 100.).floor())
            },
            150.,
            365.,
            420.,
            120.,
            if weekend { 70. } else { 96. },
            accent,
            "center",
        );
        label(
            if weekend {
                "modo descanso"
            } else {
                "jornada completada"
            }
            .into(),
            150.,
            485.,
            420.,
            50.,
            28.,
            "#9CA3AF",
            "center",
        );
        label(
            if weekend {
                "HOY NO SE TRABAJA"
            } else {
                "FALTA PARA SALIR"
            }
            .into(),
            720.,
            265.,
            1080.,
            60.,
            32.,
            "#9CA3AF",
            "left",
        );
        label(
            if weekend {
                "¡A descansar!".into()
            } else {
                duration(p.remaining)
            },
            710.,
            325.,
            1100.,
            150.,
            if weekend { 94. } else { 118. },
            "#FFFFFF",
            "left",
        );
        let status = match p.phase {
            Phase::Before => format!("Empiezas en {}. Todavía hay paz.", duration(p.until_start)),
            Phase::Working if p.remaining <= 3600 => {
                "¡Ya casi es paja! La salida está cerca.".into()
            }
            Phase::Working => "Un segundo menos. Vos podés.".into(),
            Phase::Finished => "¡Ya es paja! Jornada terminada.".into(),
            Phase::Rest => "Hoy el contador también descansa.".into(),
        };
        label(status, 720., 490., 1080., 65., 36., accent, "left");
        label(
            if weekend {
                next.clone()
            } else {
                format!(
                    "{day} · {}–{} · {} de jornada",
                    clock(p.start),
                    clock(p.end),
                    duration(p.end - p.start)
                )
            },
            720.,
            565.,
            1080.,
            60.,
            30.,
            "#9CA3AF",
            "left",
        );
        label(
            if weekend {
                "Tu horario se configura en Ajustes → Jornada".into()
            } else {
                format!(
                    "{} transcurrido  /  {} restante",
                    duration(p.elapsed),
                    duration(p.remaining)
                )
            },
            720.,
            635.,
            1080.,
            55.,
            28.,
            "#9CA3AF",
            "left",
        );
        label(
            "ASÍ VA EL DÍA".into(),
            110.,
            810.,
            1200.,
            60.,
            28.,
            "#9CA3AF",
            "left",
        );
        if weekend {
            label(next, 110., 895., 1700., 70., 32., "#FFFFFF", "left");
        } else {
            for tick in &ticks {
                let x = 1680. * (tick - p.start) as f32 / (p.end - p.start) as f32;
                label(
                    clock(*tick),
                    70. + x,
                    930.,
                    100.,
                    45.,
                    24.,
                    "#9CA3AF",
                    "center",
                );
            }
        }
        let mut block = |x, y, width, height, color| {
            view.blocks.push(Block {
                x: x * sx,
                y: y * sy,
                width: width * sx,
                height: height * sy,
                color,
            })
        };
        block(60., 130., 1800., 640., "#101619");
        block(60., 795., 1800., 215., "#101619");
        if !weekend {
            for pair in ticks.windows(2) {
                let scale = 1680. / (p.end - p.start) as f32;
                let x = 120. + (pair[0] - p.start) as f32 * scale;
                let width = ((pair[1] - pair[0]) as f32 * scale - 8.).max(1.);
                block(x, 895., width, 22., "#283237");
                let fraction = ((p.start + p.elapsed).saturating_sub(pair[0]) as f32
                    / (pair[1] - pair[0]) as f32)
                    .clamp(0., 1.);
                if fraction > 0. {
                    block(x, 895., width * fraction, 22., accent);
                }
            }
        }
        view
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_minutes_days_and_rest_presentation_follow_configuration() {
        let mut c = Config::default();
        c.workday.days[6] = crate::work_schedule::WorkHours {
            enabled: true,
            start: "09:30".into(),
            end: "14:15".into(),
        };
        let p = Progress::at(&c.workday, 6, 12 * 3600);
        assert_eq!(p.phase, Phase::Working);
        assert_eq!(p.elapsed, 9000);
        assert_eq!(p.remaining, 8100);
        assert_eq!(
            timeline(34200, 51300),
            vec![34200, 37800, 41400, 45000, 48600, 51300]
        );
        for (start, end) in [(0, 86340), (28800, 28860), (28800, 64860)] {
            let ticks = timeline(start, end);
            assert_eq!(ticks.first(), Some(&start));
            assert_eq!(ticks.last(), Some(&end));
            assert!(ticks.len() <= 11);
            assert!(ticks.windows(2).all(|p| p[0] < p[1]));
        }
        c.workday.days[1].enabled = false;
        let rest = Presentation::at(
            &c,
            &platform::LocalTime {
                weekday: 1,
                hour: 12,
                ..Default::default()
            },
        );
        assert!(
            rest.labels
                .iter()
                .any(|l| l.widget.text.contains("Martes · 08:00–18:00"))
        );
        for hours in &mut c.workday.days {
            hours.enabled = false;
        }
        let rest = Presentation::at(&c, &platform::LocalTime::default());
        assert!(
            rest.labels
                .iter()
                .any(|l| l.widget.text == "No hay días laborables configurados")
        );
    }
    #[test]
    fn monday_to_thursday_boundaries_and_countdown_are_exact() {
        for day in 1..=4 {
            let before = Progress::at(&WorkdayConfig::default(), day, 8 * 3600 - 1);
            assert_eq!(before.phase, Phase::Before);
            assert_eq!((before.remaining, before.until_start), (36000, 1));
            let start = Progress::at(&WorkdayConfig::default(), day, 8 * 3600);
            assert_eq!(start.phase, Phase::Working);
            assert_eq!(start.fraction(), 0.);
            let halfway = Progress::at(&WorkdayConfig::default(), day, 13 * 3600);
            assert_eq!(halfway.fraction(), 0.5);
            assert_eq!(halfway.remaining, 18000);
            assert_eq!(
                Progress::at(&WorkdayConfig::default(), day, 18 * 3600 - 1).remaining,
                1
            );
            for seconds in [18 * 3600, 23 * 3600 + 3599] {
                let done = Progress::at(&WorkdayConfig::default(), day, seconds);
                assert_eq!(done.phase, Phase::Finished);
                assert_eq!(done.remaining, 0);
                assert_eq!(done.fraction(), 1.);
            }
        }
    }
    #[test]
    fn friday_is_nine_hours_and_weekends_have_no_shift() {
        assert_eq!(
            Progress::at(&WorkdayConfig::default(), 5, 8 * 3600).remaining,
            32400
        );
        assert_eq!(
            Progress::at(&WorkdayConfig::default(), 5, 17 * 3600 - 1).remaining,
            1
        );
        assert_eq!(
            Progress::at(&WorkdayConfig::default(), 5, 17 * 3600).phase,
            Phase::Finished
        );
        for day in [0, 6] {
            let weekend = Progress::at(&WorkdayConfig::default(), day, 12 * 3600);
            assert_eq!(weekend.phase, Phase::Rest);
            assert_eq!(weekend.remaining, 0);
            assert_eq!(weekend.fraction(), 0.);
        }
        assert_eq!(duration(36000), "10:00:00");
        assert_eq!(duration(1), "00:00:01");
        assert_eq!(duration(0), "00:00:00");
    }
}
