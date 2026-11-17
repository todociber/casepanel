use serde::{Deserialize, Serialize};

// Windows local weekdays: Sunday = 0.
pub const DAYS: [&str; 7] = [
    "Domingo",
    "Lunes",
    "Martes",
    "Miércoles",
    "Jueves",
    "Viernes",
    "Sábado",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkHours {
    pub enabled: bool,
    pub start: String,
    pub end: String,
}
impl Default for WorkHours {
    fn default() -> Self {
        Self {
            enabled: false,
            start: "08:00".into(),
            end: "18:00".into(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkdayConfig {
    pub title: String,
    pub days: [WorkHours; 7],
}
impl Default for WorkdayConfig {
    fn default() -> Self {
        Self {
            title: "Ya es paja".into(),
            days: std::array::from_fn(|day| WorkHours {
                enabled: (1..=5).contains(&day),
                end: if day == 5 { "17:00" } else { "18:00" }.into(),
                ..WorkHours::default()
            }),
        }
    }
}
pub fn parse_time(value: &str) -> Option<u32> {
    let bytes = value.as_bytes();
    if bytes.len() != 5
        || bytes[2] != b':'
        || ![bytes[0], bytes[1], bytes[3], bytes[4]]
            .iter()
            .all(u8::is_ascii_digit)
    {
        return None;
    }
    let hour = u32::from(bytes[0] - b'0') * 10 + u32::from(bytes[1] - b'0');
    let minute = u32::from(bytes[3] - b'0') * 10 + u32::from(bytes[4] - b'0');
    (hour < 24 && minute < 60).then_some(hour * 3600 + minute * 60)
}
pub fn clock(seconds: u32) -> String {
    format!("{:02}:{:02}", seconds / 3600, seconds / 60 % 60)
}
impl WorkdayConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty()
            || self.title.chars().count() > 40
            || self.title.chars().any(char::is_control)
        {
            return Err(
                "Jornada: el título debe tener entre 1 y 40 caracteres, sin saltos de línea".into(),
            );
        }
        for (day, hours) in self.days.iter().enumerate() {
            let Some((start, end)) = parse_time(&hours.start).zip(parse_time(&hours.end)) else {
                return Err(format!(
                    "{}: introduce horas en formato HH:MM (00:00–23:59)",
                    DAYS[day]
                ));
            };
            if end <= start {
                return Err(format!(
                    "{}: la salida debe ser posterior a la entrada, dentro del mismo día",
                    DAYS[day]
                ));
            }
        }
        Ok(())
    }
    pub fn shift(&self, weekday: u16) -> Option<(u32, u32)> {
        let hours = self.days.get(usize::from(weekday))?;
        if !hours.enabled {
            return None;
        }
        let (start, end) = parse_time(&hours.start).zip(parse_time(&hours.end))?;
        (end > start).then_some((start, end))
    }
    pub fn next_shift(&self, weekday: u16) -> Option<(usize, u32, u32)> {
        (1..=7).find_map(|offset| {
            let day = (usize::from(weekday) + offset) % 7;
            self.shift(day as u16).map(|(start, end)| (day, start, end))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_partial_profiles_and_custom_weekends_round_trip() {
        let defaults: WorkdayConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(defaults, WorkdayConfig::default());
        assert_eq!(defaults.shift(1), Some((28800, 64800)));
        assert_eq!(defaults.shift(5), Some((28800, 61200)));
        assert_eq!(defaults.shift(6), None);
        let mut custom = defaults;
        for hours in &mut custom.days {
            hours.enabled = false;
        }
        assert_eq!(custom.next_shift(5), None);
        custom.days[6] = WorkHours {
            enabled: true,
            start: "09:30".into(),
            end: "14:15".into(),
        };
        custom.validate().unwrap();
        assert_eq!(custom.next_shift(6), Some((6, 34200, 51300)));
        let copy: WorkdayConfig =
            serde_json::from_str(&serde_json::to_string(&custom).unwrap()).unwrap();
        assert_eq!(copy, custom);
    }
    #[test]
    fn invalid_hours_and_titles_are_rejected() {
        for value in [
            "8:00",
            "24:00",
            "08:60",
            "-1:00",
            "12:34:56",
            "ab:cd",
            "１２:00",
        ] {
            assert_eq!(parse_time(value), None);
        }
        for (start, end) in [("18:00", "08:00"), ("08:00", "08:00"), ("bad", "17:00")] {
            let mut c = WorkdayConfig::default();
            c.days[1].start = start.into();
            c.days[1].end = end.into();
            assert!(c.validate().is_err());
        }
        for title in ["".into(), " ".into(), "x".repeat(41), "a\nb".into()] {
            assert!(
                WorkdayConfig {
                    title,
                    ..WorkdayConfig::default()
                }
                .validate()
                .is_err()
            );
        }
    }
}
