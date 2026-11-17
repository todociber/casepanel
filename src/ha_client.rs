use crate::ha_config::{HaActionItem, HaActionType, HomeAssistantConfig};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{mpsc, oneshot},
    time::Instant,
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_RESPONSE: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub struct Entity {
    pub entity_id: String,
    pub state: String,
    #[serde(default)]
    pub attributes: Attributes,
}
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Attributes {
    pub unit_of_measurement: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Connection,
    Timeout,
    Authentication,
    Http(u16),
    InvalidResponse,
    Busy,
}
impl Failure {
    pub fn message(&self) -> &'static str {
        match self {
            Self::Connection => "Error de conexión",
            Self::Timeout => "Tiempo de espera agotado",
            Self::Authentication => "Revisa el token de Home Assistant",
            Self::Http(_) => "Home Assistant rechazó la petición",
            Self::InvalidResponse => "Respuesta de Home Assistant no válida",
            Self::Busy => "Cola ocupada; vuelve a intentarlo",
        }
    }
}
pub enum Event {
    Poll(Result<BTreeMap<String, Entity>, Failure>),
    Action {
        id: String,
        result: Result<(), Failure>,
    },
}
enum Command {
    Wake,
    Retry,
    Action { id: String, queued: Instant },
}

pub struct Client {
    commands: mpsc::Sender<Command>,
    pub events: mpsc::Receiver<Event>,
    active: Arc<AtomicBool>,
    stop: Option<oneshot::Sender<()>>,
}
impl Client {
    pub fn start(config: HomeAssistantConfig) -> Self {
        let (commands, rx) = mpsc::channel(8);
        let (tx, events) = mpsc::channel(32);
        let (stop, cancel) = oneshot::channel();
        let active = Arc::new(AtomicBool::new(false));
        let worker_active = active.clone();
        std::thread::spawn(move || {
            match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .max_blocking_threads(2)
                .build()
            {
                Ok(runtime) => {
                    runtime.block_on(worker(config, worker_active, rx, tx, cancel));
                    runtime.shutdown_timeout(Duration::from_millis(100));
                }
                Err(_) => {
                    let _ = tx.try_send(Event::Poll(Err(Failure::Connection)));
                }
            }
        });
        Self {
            commands,
            events,
            active,
            stop: Some(stop),
        }
    }
    pub fn set_active(&self, active: bool) {
        if self.active.swap(active, Ordering::Relaxed) != active {
            let _ = self.commands.try_send(Command::Wake);
        }
    }
    pub fn retry(&self) -> bool {
        self.commands.try_send(Command::Retry).is_ok()
    }
    pub fn action(&self, id: &str) -> bool {
        self.commands
            .try_send(Command::Action {
                id: id.into(),
                queued: Instant::now(),
            })
            .is_ok()
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

fn interval(config: &HomeAssistantConfig, active: bool) -> Duration {
    Duration::from_secs(if active {
        config.poll_interval_secs
    } else {
        30
    })
}
async fn worker(
    config: HomeAssistantConfig,
    active: Arc<AtomicBool>,
    mut commands: mpsc::Receiver<Command>,
    events: mpsc::Sender<Event>,
    mut stop: oneshot::Receiver<()>,
) {
    let api = match Api::new(&config) {
        Ok(api) => api,
        Err(error) => {
            let _ = events.send(Event::Poll(Err(error))).await;
            return;
        }
    };
    let mut last_poll: Option<Instant> = None;
    loop {
        let due = last_poll.map_or_else(Instant::now, |last| {
            last + interval(&config, active.load(Ordering::Relaxed))
        });
        let command = tokio::select! {
            biased;
            _ = &mut stop => return,
            command = commands.recv() => match command { Some(c) => Some(c), None => return },
            _ = tokio::time::sleep_until(due) => None,
        };
        let event = match command {
            Some(Command::Wake) => continue,
            Some(Command::Retry) => {
                last_poll = None;
                continue;
            }
            Some(Command::Action { id, queued }) => {
                let result = if queued.elapsed() > REQUEST_TIMEOUT {
                    Err(Failure::Busy)
                } else {
                    tokio::select! {
                        _ = &mut stop => return,
                        result = api.action(&id) => result,
                    }
                };
                // Reconcile even after timeouts: an uncertain POST is never replayed.
                last_poll = None;
                Event::Action { id, result }
            }
            None => {
                let result = tokio::select! {
                    _ = &mut stop => return,
                    result = api.states() => result,
                };
                last_poll = Some(Instant::now());
                Event::Poll(result)
            }
        };
        tokio::select! { _ = &mut stop => return, result = events.send(event) => if result.is_err() { return; } }
    }
}

struct Api {
    client: reqwest::Client,
    config: HomeAssistantConfig,
}
impl Api {
    fn new(config: &HomeAssistantConfig) -> Result<Self, Failure> {
        config.validate().map_err(|_| Failure::InvalidResponse)?;
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()
            .map_err(|_| Failure::Connection)?;
        Ok(Self {
            client,
            config: config.clone(),
        })
    }
    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.config.base_url.trim_end_matches('/'))
    }
    async fn states(&self) -> Result<BTreeMap<String, Entity>, Failure> {
        let response = self
            .client
            .get(self.url("/api/states"))
            .bearer_auth(&self.config.token)
            .send()
            .await
            .map_err(network_error)?;
        let entities: Vec<Entity> = serde_json::from_slice(&read_body(response).await?)
            .map_err(|_| Failure::InvalidResponse)?;
        if entities.len() > 20_000 {
            return Err(Failure::InvalidResponse);
        }
        let configured: BTreeSet<_> = self
            .config
            .actions
            .iter()
            .map(|a| a.entity_id.as_str())
            .collect();
        let mut result = BTreeMap::new();
        for e in entities {
            if configured.contains(e.entity_id.as_str()) {
                if e.state.len() > 512
                    || e.attributes
                        .unit_of_measurement
                        .as_ref()
                        .is_some_and(|s| s.len() > 64)
                {
                    return Err(Failure::InvalidResponse);
                }
                result.insert(e.entity_id.clone(), e);
            }
        }
        Ok(result)
    }
    async fn action(&self, id: &str) -> Result<(), Failure> {
        let action = self
            .config
            .actions
            .iter()
            .find(|a| a.id == id)
            .ok_or(Failure::InvalidResponse)?;
        let path = service_path(action).ok_or(Failure::InvalidResponse)?;
        let response = self
            .client
            .post(self.url(path))
            .bearer_auth(&self.config.token)
            .json(&serde_json::json!({"entity_id": action.entity_id}))
            .send()
            .await
            .map_err(network_error)?;
        read_body(response).await?;
        Ok(())
    }
}
fn service_path(action: &HaActionItem) -> Option<&'static str> {
    match action.action_type {
        HaActionType::Toggle => Some("/api/services/homeassistant/toggle"),
        HaActionType::Trigger if action.entity_id.starts_with("script.") => {
            Some("/api/services/script/turn_on")
        }
        HaActionType::Trigger if action.entity_id.starts_with("scene.") => {
            Some("/api/services/scene/turn_on")
        }
        _ => None,
    }
}
fn network_error(error: reqwest::Error) -> Failure {
    if error.is_timeout() {
        Failure::Timeout
    } else {
        Failure::Connection
    }
}
async fn read_body(mut response: reqwest::Response) -> Result<Vec<u8>, Failure> {
    let status = response.status();
    if !status.is_success() {
        return Err(if matches!(status.as_u16(), 401 | 403) {
            Failure::Authentication
        } else {
            Failure::Http(status.as_u16())
        });
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_RESPONSE as u64)
    {
        return Err(Failure::InvalidResponse);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
            return Err(Failure::InvalidResponse);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc as test_channel,
    };
    fn server(
        replies: Vec<(String, Duration)>,
    ) -> (
        String,
        test_channel::Receiver<String>,
        std::thread::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = test_channel::channel();
        let worker = std::thread::spawn(move || {
            for (reply, delay) in replies {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                loop {
                    let mut buffer = [0u8; 4096];
                    let n = stream.read(&mut buffer).unwrap();
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..n]);
                    if let Some(index) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                        let header =
                            String::from_utf8_lossy(&request[..index]).to_ascii_lowercase();
                        let length = header
                            .lines()
                            .find_map(|line| {
                                line.strip_prefix("content-length:")
                                    .and_then(|s| s.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if request.len() >= index + 4 + length {
                            break;
                        }
                    }
                    assert!(request.len() < 16 * 1024);
                }
                let _ = tx.send(String::from_utf8_lossy(&request).to_string());
                std::thread::sleep(delay);
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        (url, rx, worker)
    }
    fn response(status: u16, body: &str) -> String {
        format!(
            "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }
    fn config(url: String) -> HomeAssistantConfig {
        HomeAssistantConfig {
            base_url: url,
            token: "test-token".into(),
            actions: vec![HaActionItem {
                id: "office".into(),
                label: "Office".into(),
                entity_id: "light.office".into(),
                action_type: HaActionType::Toggle,
                ..HaActionItem::default()
            }],
            ..HomeAssistantConfig::default()
        }
    }
    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }
    #[test]
    fn worker_wakes_on_navigation_and_does_not_keep_foreground_polling() {
        let (url, requests, server) = server(vec![
            (response(200, "[]"), Duration::ZERO),
            (response(200, "[]"), Duration::ZERO),
        ]);
        let mut c = config(url);
        c.poll_interval_secs = 1;
        let mut client = Client::start(c);
        assert!(matches!(
            client.events.blocking_recv(),
            Some(Event::Poll(Ok(_)))
        ));
        requests.recv_timeout(Duration::from_secs(3)).unwrap();
        client.set_active(true);
        assert!(matches!(
            client.events.blocking_recv(),
            Some(Event::Poll(Ok(_)))
        ));
        requests.recv_timeout(Duration::from_secs(3)).unwrap();
        client.set_active(false);
        std::thread::sleep(Duration::from_millis(1250));
        assert!(client.events.try_recv().is_err());
        drop(client);
        server.join().unwrap();
    }
    #[test]
    fn http_client_authenticates_filters_states_and_posts_only_the_configured_service() {
        let body = r#"[{"entity_id":"light.office","state":"off","attributes":{}},{"entity_id":"sensor.other","state":"42","attributes":{}}]"#;
        let (url, requests, server) = server(vec![
            (response(200, body), Duration::ZERO),
            (response(200, "[]"), Duration::ZERO),
        ]);
        runtime().block_on(async {
            let api = Api::new(&config(url)).unwrap();
            let states = api.states().await.unwrap();
            assert_eq!(states.len(), 1);
            assert_eq!(states["light.office"].state, "off");
            api.action("office").await.unwrap();
        });
        let get = requests.recv().unwrap();
        let post = requests.recv().unwrap();
        assert!(get.starts_with("GET /api/states "));
        assert!(
            get.to_ascii_lowercase()
                .contains("authorization: bearer test-token")
        );
        assert!(post.starts_with("POST /api/services/homeassistant/toggle "));
        assert!(post.contains(r#"{"entity_id":"light.office"}"#));
        server.join().unwrap();
    }
    #[test]
    fn http_failures_do_not_expose_bodies_or_follow_redirects() {
        let (url, _, server) = server(vec![(response(401, "server-body-secret"), Duration::ZERO)]);
        let error = runtime()
            .block_on(Api::new(&config(url)).unwrap().states())
            .unwrap_err();
        assert_eq!(error, Failure::Authentication);
        assert!(!error.message().contains("secret"));
        server.join().unwrap();
        let reply="HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into();
        let (url, _, server) = self::server(vec![(reply, Duration::ZERO)]);
        assert_eq!(
            runtime()
                .block_on(Api::new(&config(url)).unwrap().states())
                .unwrap_err(),
            Failure::Http(302)
        );
        server.join().unwrap();
    }
    #[test]
    fn http_timeout_covers_the_response_and_limits_response_size() {
        let (url, _, server) = server(vec![(response(200, "[]"), Duration::from_secs(4))]);
        let start = std::time::Instant::now();
        assert_eq!(
            runtime()
                .block_on(Api::new(&config(url)).unwrap().states())
                .unwrap_err(),
            Failure::Timeout
        );
        assert!(start.elapsed() < Duration::from_millis(4500));
        server.join().unwrap();
        let reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            MAX_RESPONSE + 1
        );
        let (url, _, server) = self::server(vec![(reply, Duration::ZERO)]);
        assert_eq!(
            runtime()
                .block_on(Api::new(&config(url)).unwrap().states())
                .unwrap_err(),
            Failure::InvalidResponse
        );
        server.join().unwrap();
    }
    #[test]
    fn polling_and_services_are_explicit_and_read_only_has_no_service() {
        let cfg = HomeAssistantConfig::default();
        assert_eq!(interval(&cfg, true), Duration::from_secs(5));
        assert_eq!(interval(&cfg, false), Duration::from_secs(30));
        let mut action = HaActionItem {
            entity_id: "sensor.test".into(),
            ..HaActionItem::default()
        };
        assert!(service_path(&action).is_none());
        action.action_type = HaActionType::Toggle;
        assert_eq!(
            service_path(&action),
            Some("/api/services/homeassistant/toggle")
        );
        action.action_type = HaActionType::Trigger;
        assert!(service_path(&action).is_none());
        action.entity_id = "scene.night".into();
        assert_eq!(service_path(&action), Some("/api/services/scene/turn_on"));
    }
}
