use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fmt,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct IntegrationConfig {
    pub home_assistant: HomeAssistantConfig,
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct HomeAssistantConfig {
    pub enabled: bool,
    pub base_url: String,
    pub token: String,
    pub poll_interval_secs: u64,
    pub grid_columns: usize,
    pub actions: Vec<HaActionItem>,
}
impl Default for HomeAssistantConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            base_url: String::new(),
            token: String::new(),
            poll_interval_secs: 5,
            grid_columns: 3,
            actions: Vec::new(),
        }
    }
}
// Configuration errors and diagnostics must never include the bearer token.
impl fmt::Debug for HomeAssistantConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HomeAssistantConfig")
            .field("enabled", &self.enabled)
            .field("base_url", &self.base_url)
            .field("token", &"[redacted]")
            .field("poll_interval_secs", &self.poll_interval_secs)
            .field("grid_columns", &self.grid_columns)
            .field("actions", &self.actions)
            .finish()
    }
}
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct HaActionItem {
    pub id: String,
    pub label: String,
    pub sublabel: Option<String>,
    pub entity_id: String,
    pub action_type: HaActionType,
    pub icon: String,
    pub confirm_required: bool,
}
impl Default for HaActionItem {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            sublabel: None,
            entity_id: String::new(),
            action_type: HaActionType::SensorReadOnly,
            icon: "power".into(),
            confirm_required: false,
        }
    }
}
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HaActionType {
    #[serde(alias = "toggle")]
    Toggle,
    #[serde(alias = "trigger")]
    Trigger,
    #[default]
    #[serde(alias = "sensor_read_only")]
    SensorReadOnly,
}
impl HomeAssistantConfig {
    pub fn ready(&self) -> bool {
        self.enabled
            && !self.base_url.is_empty()
            && !self.token.is_empty()
            && !self.actions.is_empty()
    }
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=3600).contains(&self.poll_interval_secs)
            || !(1..=6).contains(&self.grid_columns)
            || self.actions.len() > 96
        {
            return Err(
                "Home Assistant: intervalo 1–3600 s, columnas 1–6 y máximo 96 tarjetas".into(),
            );
        }
        if !self.base_url.is_empty() {
            let url = reqwest::Url::parse(&self.base_url)
                .map_err(|_| "Home Assistant: base_url no es una URL válida")?;
            if !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err("Home Assistant: usa una URL http/https sin credenciales, consulta ni fragmento".into());
            }
        }
        if self.token.len() > 8192 || self.token.bytes().any(|b| !(33..=126).contains(&b)) {
            return Err("Home Assistant: el token contiene caracteres no válidos".into());
        }
        let mut ids = BTreeSet::new();
        for a in &self.actions {
            let valid_id = !a.id.is_empty()
                && a.id.len() <= 64
                && a.id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b));
            let entity = a
                .entity_id
                .split_once('.')
                .filter(|(domain, name)| !domain.is_empty() && !name.is_empty());
            let valid_entity = a.entity_id.len() <= 256
                && entity.is_some_and(|(d, n)| {
                    [d, n].iter().all(|s| {
                        s.bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                    })
                });
            if !valid_id
                || !ids.insert(&a.id)
                || !valid_entity
                || a.label.trim().is_empty()
                || a.label.len() > 256
                || a.sublabel.as_ref().is_some_and(|s| s.len() > 256)
                || a.icon.len() > 32
            {
                return Err(
                    "Home Assistant: tarjeta no válida; revisa ID único, etiqueta y entity_id"
                        .into(),
                );
            }
            if a.action_type == HaActionType::Trigger
                && !matches!(entity.map(|(d, _)| d), Some("script" | "scene"))
            {
                return Err(
                    "Home Assistant: Trigger requiere una entidad script.* o scene.*".into(),
                );
            }
        }
        Ok(())
    }
}

pub fn default_path(profile: &Path) -> PathBuf {
    profile.with_file_name("config.toml")
}

/// Compare bounded contents at 1 Hz; atomic edits and same-timestamp replacements are detected.
pub struct ConfigFile {
    pub path: PathBuf,
    contents: Option<Vec<u8>>,
    missing: bool,
    first: bool,
}
impl ConfigFile {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            contents: None,
            missing: false,
            first: true,
        }
    }
    pub fn poll(&mut self) -> Option<Result<HomeAssistantConfig, String>> {
        let bytes = match std::fs::metadata(&self.path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if self.missing && !self.first {
                    return None;
                }
                if self.first {
                    let create = || -> Result<(), String> {
                        use std::io::Write;
                        if let Some(parent) = self.path.parent() {
                            std::fs::create_dir_all(parent)
                                .map_err(|_| "No se puede crear la carpeta de configuración")?;
                        }
                        let text = toml::to_string_pretty(&IntegrationConfig::default())
                            .map_err(|_| "No se puede crear config.toml")?;
                        match std::fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&self.path)
                        {
                            Ok(mut file) => file
                                .write_all(text.as_bytes())
                                .map_err(|_| "No se puede escribir config.toml".into()),
                            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
                            Err(_) => Err("No se puede crear config.toml".into()),
                        }
                    };
                    if let Err(e) = create() {
                        return Some(Err(e));
                    }
                }
                self.missing = true;
                self.first = false;
                self.contents = None;
                return Some(Ok(HomeAssistantConfig::default()));
            }
            Err(_) => return Some(Err("No se puede leer config.toml".into())),
            Ok(m) if m.len() > 64 * 1024 => return Some(Err("config.toml excede 64 KiB".into())),
            Ok(_) => match std::fs::read(&self.path) {
                Ok(bytes) if bytes.len() <= 64 * 1024 => bytes,
                _ => return Some(Err("No se puede leer config.toml".into())),
            },
        };
        self.missing = false;
        self.first = false;
        if self.contents.as_ref() == Some(&bytes) {
            return None;
        }
        let parsed = std::str::from_utf8(&bytes)
            .map_err(|_| "config.toml debe usar UTF-8".to_string())
            .and_then(|text| {
                toml::from_str::<IntegrationConfig>(text)
                    // TOML diagnostics contain the input line; never expose it when it may hold a token.
                    .map_err(|_| "config.toml no es TOML válido".to_string())
            })
            .and_then(|c| {
                c.home_assistant.validate()?;
                Ok(c.home_assistant)
            });
        self.contents = Some(bytes);
        Some(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_watcher_creates_defaults_and_recovers_after_invalid_edits() {
        let dir = std::env::temp_dir().join(format!(
            "casepanel-ha-{}-{}",
            std::process::id(),
            crate::model::now_ms()
        ));
        let path = dir.join("config.toml");
        let mut file = ConfigFile::new(path.clone());
        assert_eq!(
            file.poll().unwrap().unwrap(),
            HomeAssistantConfig::default()
        );
        assert!(path.exists());
        file.poll();
        assert!(file.poll().is_none());
        std::fs::write(&path, "[home_assistant]\ntoken = 'secret-test' broken").unwrap();
        let error = file.poll().unwrap().unwrap_err();
        assert!(!error.contains("secret-test"));
        assert!(file.poll().is_none());
        std::fs::write(&path, "[home_assistant]\ngrid_columns = 4").unwrap();
        assert_eq!(file.poll().unwrap().unwrap().grid_columns, 4);
        std::fs::remove_file(&path).unwrap();
        assert!(!file.poll().unwrap().unwrap().ready());
        std::fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn partial_settings_are_compatible_and_debug_redacts_credentials() {
        let cfg: IntegrationConfig =
            toml::from_str("[home_assistant]\ntoken='secret-test'\n").unwrap();
        assert!(cfg.home_assistant.enabled);
        assert_eq!(cfg.home_assistant.poll_interval_secs, 5);
        assert_eq!(cfg.home_assistant.grid_columns, 3);
        assert!(!format!("{cfg:?}").contains("secret-test"));
        let old: IntegrationConfig = toml::from_str("").unwrap();
        assert!(old.home_assistant.actions.is_empty());
        let example: IntegrationConfig =
            toml::from_str(include_str!("../config.example.toml")).unwrap();
        example.home_assistant.validate().unwrap();
        assert!(!example.home_assistant.ready());
        assert!(!example.home_assistant.actions[2].confirm_required);
    }
    #[test]
    fn invalid_origins_duplicate_cards_and_trigger_domains_are_rejected() {
        let mut c: IntegrationConfig =
            toml::from_str(include_str!("../config.example.toml")).unwrap();
        c.home_assistant
            .actions
            .push(c.home_assistant.actions[0].clone());
        assert!(c.home_assistant.validate().is_err());
        c.home_assistant.actions.pop();
        c.home_assistant.base_url = "https://user:secret@example.org".into();
        assert!(c.home_assistant.validate().is_err());
        c.home_assistant.base_url.clear();
        c.home_assistant.actions[1].entity_id = "light.office".into();
        assert!(c.home_assistant.validate().is_err());
    }
}
