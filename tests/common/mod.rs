use moar::config::{PolicyConfig, RelayConfig};
use moar::policy::PolicyEngine;
use moar::server::{create_relay_router, RelayState};
use moar::storage::NostrStore;
use nostr::{Event, Filter, JsonUtil, RelayMessage};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};

// ---------------------------------------------------------------------------
// MockStore
// ---------------------------------------------------------------------------

pub struct MockStore {
    events: Mutex<HashMap<[u8; 32], Event>>,
}

impl MockStore {
    pub fn new() -> Self {
        Self {
            events: Mutex::new(HashMap::new()),
        }
    }
}

impl NostrStore for MockStore {
    fn save_event(&self, event: &Event) -> moar::error::Result<()> {
        let mut events = self.events.lock().unwrap();
        events.insert(*event.id.as_bytes(), event.clone());
        Ok(())
    }

    fn get_event(&self, id: &[u8; 32]) -> moar::error::Result<Option<Event>> {
        let events = self.events.lock().unwrap();
        Ok(events.get(id).cloned())
    }

    fn delete_event(&self, id: &[u8; 32]) -> moar::error::Result<bool> {
        let mut events = self.events.lock().unwrap();
        Ok(events.remove(id).is_some())
    }

    fn query(&self, filter: &Filter) -> moar::error::Result<Vec<Event>> {
        let events = self.events.lock().unwrap();
        let limit = filter.limit.unwrap_or(100);

        let mut results: Vec<Event> = events
            .values()
            .filter(|event| {
                if let Some(ids) = &filter.ids {
                    if !ids.contains(&event.id) {
                        return false;
                    }
                }
                if let Some(authors) = &filter.authors {
                    if !authors.contains(&event.pubkey) {
                        return false;
                    }
                }
                if let Some(kinds) = &filter.kinds {
                    if !kinds.contains(&event.kind) {
                        return false;
                    }
                }
                if let Some(since) = filter.since {
                    if event.created_at < since {
                        return false;
                    }
                }
                if let Some(until) = filter.until {
                    if event.created_at > until {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        results.truncate(limit);
        Ok(results)
    }
}

// ---------------------------------------------------------------------------
// spawn_relay
// ---------------------------------------------------------------------------

pub async fn spawn_relay(policy: PolicyConfig) -> (u16, Arc<MockStore>) {
    let store = Arc::new(MockStore::new());
    let store_dyn: Arc<dyn NostrStore> = store.clone();
    let policy_engine = Arc::new(PolicyEngine::new(policy.clone()));
    let config = RelayConfig {
        name: "test".into(),
        description: None,
        subdomain: "test".into(),
        db_path: "/tmp/moar-test-unused".into(),
        policy,
    };
    let state = Arc::new(RelayState::new(
        config,
        store_dyn,
        policy_engine,
        "test".into(),
        std::path::PathBuf::from("/tmp/moar-test-pages"),
    ));
    let app = create_relay_router(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (port, store)
}

// ---------------------------------------------------------------------------
// WsTestClient
// ---------------------------------------------------------------------------

pub struct WsTestClient {
    sink: futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    stream: futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
}

impl WsTestClient {
    pub async fn connect(port: u16) -> Self {
        let url = format!("ws://127.0.0.1:{}/", port);
        let (ws, _) = connect_async(&url).await.expect("failed to connect");
        let (sink, stream) = ws.split();
        Self { sink, stream }
    }

    pub async fn send_text(&mut self, text: &str) {
        self.sink
            .send(Message::Text(text.into()))
            .await
            .expect("failed to send");
    }

    pub async fn send_event(&mut self, event: &Event) {
        let msg = format!(r#"["EVENT",{}]"#, event.as_json());
        self.send_text(&msg).await;
    }

    pub async fn send_req(&mut self, sub_id: &str, filters: Vec<Filter>) {
        let filters_json: Vec<String> = filters.iter().map(|f| f.as_json()).collect();
        let msg = format!(r#"["REQ","{}",{}]"#, sub_id, filters_json.join(","));
        self.send_text(&msg).await;
    }

    pub async fn recv_text(&mut self) -> String {
        let timeout = tokio::time::Duration::from_secs(5);
        tokio::time::timeout(timeout, async {
            loop {
                match self.stream.next().await {
                    Some(Ok(Message::Text(text))) => return text.to_string(),
                    Some(Ok(_)) => continue, // skip non-text
                    Some(Err(e)) => panic!("ws error: {}", e),
                    None => panic!("ws stream ended"),
                }
            }
        })
        .await
        .expect("timeout waiting for ws message")
    }

    pub async fn expect_ok(&mut self) -> (bool, String) {
        let text = self.recv_text().await;
        let msg = RelayMessage::from_json(&text).expect("failed to parse relay message");
        match msg {
            RelayMessage::Ok {
                status, message, ..
            } => (status, message),
            other => panic!("expected OK, got: {:?}", other),
        }
    }

    pub async fn expect_notice(&mut self) -> String {
        let text = self.recv_text().await;
        let msg = RelayMessage::from_json(&text).expect("failed to parse relay message");
        match msg {
            RelayMessage::Notice { message } => message,
            other => panic!("expected NOTICE, got: {:?}", other),
        }
    }

    pub async fn expect_eose(&mut self) {
        let text = self.recv_text().await;
        let msg = RelayMessage::from_json(&text).expect("failed to parse relay message");
        match msg {
            RelayMessage::EndOfStoredEvents(_) => {}
            other => panic!("expected EOSE, got: {:?}", other),
        }
    }

    pub async fn expect_event(&mut self) -> Event {
        let text = self.recv_text().await;
        let msg = RelayMessage::from_json(&text).expect("failed to parse relay message");
        match msg {
            RelayMessage::Event { event, .. } => *event,
            other => panic!("expected EVENT, got: {:?}", other),
        }
    }
}
