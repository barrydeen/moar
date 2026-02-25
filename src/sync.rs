use crate::config::SyncConfig;
use crate::storage::NostrStore;
use crate::wot::WotManager;
use futures_util::{SinkExt, StreamExt};
use nostr::JsonUtil;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{Notify, RwLock};
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

// ---------------------------------------------------------------------------
// SyncStatus
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "state")]
pub enum SyncStatus {
    Idle,
    Syncing {
        relay_url: String,
        events_so_far: u64,
    },
    Ready,
    Error {
        message: String,
    },
}

// ---------------------------------------------------------------------------
// SyncEntry
// ---------------------------------------------------------------------------

struct SyncEntry {
    config: SyncConfig,
    status: Arc<RwLock<SyncStatus>>,
    last_synced: Arc<RwLock<Option<u64>>>,
    events_pulled: Arc<RwLock<u64>>,
    handle: Option<JoinHandle<()>>,
    notify: Arc<Notify>,
}

// ---------------------------------------------------------------------------
// SyncManager
// ---------------------------------------------------------------------------

pub struct SyncManager {
    entries: RwLock<HashMap<String, SyncEntry>>,
    stores: RwLock<HashMap<String, Arc<dyn NostrStore>>>,
    wot_manager: Arc<WotManager>,
    data_dir: PathBuf,
}

impl SyncManager {
    pub fn new(
        syncs: HashMap<String, SyncConfig>,
        stores: HashMap<String, Arc<dyn NostrStore>>,
        wot_manager: Arc<WotManager>,
    ) -> Arc<Self> {
        let data_dir = PathBuf::from("data/sync");
        let mut entries = HashMap::new();

        for (id, config) in syncs {
            entries.insert(
                id,
                SyncEntry {
                    config,
                    status: Arc::new(RwLock::new(SyncStatus::Idle)),
                    last_synced: Arc::new(RwLock::new(None)),
                    events_pulled: Arc::new(RwLock::new(0)),
                    handle: None,
                    notify: Arc::new(Notify::new()),
                },
            );
        }

        Arc::new(Self {
            entries: RwLock::new(entries),
            stores: RwLock::new(stores),
            wot_manager,
            data_dir,
        })
    }

    pub async fn start_all(self: &Arc<Self>) {
        let _ = tokio::fs::create_dir_all(&self.data_dir).await;

        let ids: Vec<String> = self.entries.read().await.keys().cloned().collect();
        for id in ids {
            self.start_syncer(&id).await;
        }
    }

    async fn start_syncer(self: &Arc<Self>, id: &str) {
        let mut entries = self.entries.write().await;
        let entry = match entries.get_mut(id) {
            Some(e) => e,
            None => return,
        };

        // Abort existing task if running
        if let Some(handle) = entry.handle.take() {
            handle.abort();
        }

        let manager = Arc::clone(self);
        let sync_id = id.to_string();
        let config = entry.config.clone();
        let status = Arc::clone(&entry.status);
        let last_synced = Arc::clone(&entry.last_synced);
        let events_pulled = Arc::clone(&entry.events_pulled);
        let notify = Arc::clone(&entry.notify);
        let since_path = self.data_dir.join(format!("{}.since", id));

        let handle = tokio::spawn(async move {
            loop {
                // Load since from disk
                let last_since = load_since(&since_path).await;

                // Resolve authors
                let authors = match resolve_authors(&config, &manager.wot_manager).await {
                    Ok(a) => a,
                    Err(e) => {
                        tracing::error!("Sync '{}': failed to resolve authors: {}", sync_id, e);
                        *status.write().await = SyncStatus::Error {
                            message: e.to_string(),
                        };
                        tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                        continue;
                    }
                };

                // Get the target store
                let store = {
                    let stores = manager.stores.read().await;
                    stores.get(&config.relay).cloned()
                };

                let store = match store {
                    Some(s) => s,
                    None => {
                        tracing::error!(
                            "Sync '{}': target relay '{}' not found",
                            sync_id,
                            config.relay
                        );
                        *status.write().await = SyncStatus::Error {
                            message: format!("Target relay '{}' not found", config.relay),
                        };
                        tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                        continue;
                    }
                };

                // Build filter batches (chunk authors into groups of 300)
                let author_batches: Vec<Option<Vec<String>>> = if let Some(ref authors) = authors {
                    if authors.is_empty() {
                        vec![None]
                    } else {
                        authors
                            .chunks(300)
                            .map(|chunk| Some(chunk.to_vec()))
                            .collect()
                    }
                } else {
                    vec![None]
                };

                let mut total_events: u64 = 0;
                let mut max_created_at = last_since;
                let mut any_error = false;

                // Distribute batches round-robin across remote relays
                for (batch_idx, author_batch) in author_batches.iter().enumerate() {
                    let relay_idx = batch_idx % config.remote_relays.len();
                    let relay_url = &config.remote_relays[relay_idx];

                    *status.write().await = SyncStatus::Syncing {
                        relay_url: relay_url.clone(),
                        events_so_far: total_events,
                    };

                    let filter = build_filter(&config, author_batch, last_since);

                    match fetch_events(relay_url, &filter).await {
                        Ok(events) => {
                            let count = events.len() as u64;
                            for event in &events {
                                let ts = event.created_at.as_u64();
                                if max_created_at.map_or(true, |s| ts > s) {
                                    max_created_at = Some(ts);
                                }
                                if let Err(e) = store.save_event(event) {
                                    tracing::warn!(
                                        "Sync '{}': failed to save event: {}",
                                        sync_id,
                                        e
                                    );
                                }
                            }
                            total_events += count;
                            tracing::info!(
                                "Sync '{}': pulled {} events from {}",
                                sync_id,
                                count,
                                relay_url
                            );
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Sync '{}': failed to fetch from {}: {}",
                                sync_id,
                                relay_url,
                                e
                            );
                            any_error = true;
                        }
                    }
                }

                // Update since on disk
                if let Some(since) = max_created_at {
                    if let Err(e) = save_since(&since_path, since).await {
                        tracing::warn!("Sync '{}': failed to save since: {}", sync_id, e);
                    }
                }

                // Update stats
                {
                    let mut ep = events_pulled.write().await;
                    *ep += total_events;
                }

                let now = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                *last_synced.write().await = Some(now);

                if any_error && total_events == 0 {
                    *status.write().await = SyncStatus::Error {
                        message: "All remote relays failed".to_string(),
                    };
                } else {
                    *status.write().await = SyncStatus::Ready;
                    tracing::info!(
                        "Sync '{}': complete, pulled {} events total",
                        sync_id,
                        total_events
                    );
                }

                // Wait for interval or manual trigger
                let sleep_secs = config.interval_minutes.max(1) * 60;
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(sleep_secs)) => {},
                    _ = notify.notified() => {
                        tracing::info!("Sync '{}': manually triggered", sync_id);
                    },
                }
            }
        });

        entry.handle = Some(handle);
    }

    pub async fn list_syncs(&self) -> Vec<SyncInfo> {
        let entries = self.entries.read().await;
        let mut result = Vec::new();
        for (id, entry) in entries.iter() {
            result.push(SyncInfo {
                id: id.clone(),
                config: entry.config.clone(),
                status: entry.status.read().await.clone(),
                last_synced: *entry.last_synced.read().await,
                events_pulled: *entry.events_pulled.read().await,
            });
        }
        result
    }

    pub async fn get_sync(&self, id: &str) -> Option<SyncInfo> {
        let entries = self.entries.read().await;
        let entry = entries.get(id)?;
        let info = SyncInfo {
            id: id.to_string(),
            config: entry.config.clone(),
            status: entry.status.read().await.clone(),
            last_synced: *entry.last_synced.read().await,
            events_pulled: *entry.events_pulled.read().await,
        };
        Some(info)
    }

    pub async fn add_sync(
        self: &Arc<Self>,
        id: String,
        config: SyncConfig,
    ) -> Result<(), String> {
        let mut entries = self.entries.write().await;
        if entries.contains_key(&id) {
            return Err(format!("Sync '{}' already exists", id));
        }
        entries.insert(
            id.clone(),
            SyncEntry {
                config,
                status: Arc::new(RwLock::new(SyncStatus::Idle)),
                last_synced: Arc::new(RwLock::new(None)),
                events_pulled: Arc::new(RwLock::new(0)),
                handle: None,
                notify: Arc::new(Notify::new()),
            },
        );
        drop(entries);
        self.start_syncer(&id).await;
        Ok(())
    }

    pub async fn update_sync(
        self: &Arc<Self>,
        id: &str,
        config: SyncConfig,
    ) -> Result<(), String> {
        {
            let mut entries = self.entries.write().await;
            let entry = entries
                .get_mut(id)
                .ok_or_else(|| format!("Sync '{}' not found", id))?;

            if let Some(handle) = entry.handle.take() {
                handle.abort();
            }

            entry.config = config;
            *entry.status.write().await = SyncStatus::Idle;
        }
        self.start_syncer(id).await;
        Ok(())
    }

    pub async fn remove_sync(&self, id: &str) -> Result<SyncConfig, String> {
        let mut entries = self.entries.write().await;
        let mut entry = entries
            .remove(id)
            .ok_or_else(|| format!("Sync '{}' not found", id))?;

        if let Some(handle) = entry.handle.take() {
            handle.abort();
        }

        // Remove since file
        let since_path = self.data_dir.join(format!("{}.since", id));
        let _ = tokio::fs::remove_file(&since_path).await;

        Ok(entry.config)
    }

    pub async fn trigger_sync(&self, id: &str) -> Result<(), String> {
        let entries = self.entries.read().await;
        let entry = entries
            .get(id)
            .ok_or_else(|| format!("Sync '{}' not found", id))?;
        entry.notify.notify_one();
        Ok(())
    }

    pub async fn update_stores(&self, stores: HashMap<String, Arc<dyn NostrStore>>) {
        *self.stores.write().await = stores;
    }
}

// ---------------------------------------------------------------------------
// SyncInfo — serializable sync status for API responses
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
pub struct SyncInfo {
    pub id: String,
    pub config: SyncConfig,
    pub status: SyncStatus,
    pub last_synced: Option<u64>,
    pub events_pulled: u64,
}

// ---------------------------------------------------------------------------
// Author resolution
// ---------------------------------------------------------------------------

async fn resolve_authors(
    config: &SyncConfig,
    wot_manager: &Arc<WotManager>,
) -> Result<Option<Vec<String>>, anyhow::Error> {
    if let Some(ref wot_id) = config.authors_from_wot {
        let wot_set = wot_manager
            .get_set(wot_id)
            .await
            .ok_or_else(|| anyhow::anyhow!("WoT '{}' not found", wot_id))?;

        let pubkeys: Vec<String> = wot_set.to_hex_vec();

        if pubkeys.is_empty() {
            return Err(anyhow::anyhow!("WoT '{}' has no pubkeys yet", wot_id));
        }

        tracing::info!(
            "Sync resolved {} authors from WoT '{}'",
            pubkeys.len(),
            wot_id
        );
        return Ok(Some(pubkeys));
    }

    if let Some(ref authors) = config.authors {
        return Ok(Some(authors.clone()));
    }

    Ok(None)
}

// ---------------------------------------------------------------------------
// Filter building
// ---------------------------------------------------------------------------

fn build_filter(
    config: &SyncConfig,
    author_batch: &Option<Vec<String>>,
    since: Option<u64>,
) -> serde_json::Value {
    let mut filter = serde_json::Map::new();

    if let Some(ref authors) = author_batch {
        filter.insert(
            "authors".to_string(),
            serde_json::json!(authors),
        );
    }

    if let Some(ref kinds) = config.kinds {
        filter.insert("kinds".to_string(), serde_json::json!(kinds));
    }

    if let Some(ref tags) = config.tags {
        for (key, values) in tags {
            filter.insert(
                format!("#{}", key),
                serde_json::json!(values),
            );
        }
    }

    if let Some(limit) = config.limit {
        filter.insert("limit".to_string(), serde_json::json!(limit));
    }

    if let Some(since) = since {
        // Add 1 to avoid re-fetching the last event
        filter.insert("since".to_string(), serde_json::json!(since + 1));
    }

    serde_json::Value::Object(filter)
}

// ---------------------------------------------------------------------------
// WebSocket fetch
// ---------------------------------------------------------------------------

type WsStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

async fn fetch_events(
    relay_url: &str,
    filter: &serde_json::Value,
) -> Result<Vec<nostr::Event>, anyhow::Error> {
    let (mut ws, _): (WsStream, _) = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio_tungstenite::connect_async(relay_url),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Connection timeout to {}", relay_url))?
    .map_err(|e| anyhow::anyhow!("WS connect to {} failed: {}", relay_url, e))?;

    let sub_id = "sync-0";
    let req = serde_json::json!(["REQ", sub_id, filter]);
    ws.send(Message::Text(req.to_string().into())).await?;

    let events = read_events_until_eose(&mut ws, sub_id).await?;

    // Send CLOSE
    let close = serde_json::json!(["CLOSE", sub_id]);
    let _ = ws.send(Message::Text(close.to_string().into())).await;
    let _ = ws.close(None).await;

    Ok(events)
}

async fn read_events_until_eose(
    ws: &mut WsStream,
    sub_id: &str,
) -> Result<Vec<nostr::Event>, anyhow::Error> {
    let mut events = Vec::new();

    let timeout = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        while let Some(msg) = ws.next().await {
            let msg = msg?;
            let text = match msg {
                Message::Text(t) => t.to_string(),
                Message::Close(_) => break,
                _ => continue,
            };

            let parsed: serde_json::Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let arr = match parsed.as_array() {
                Some(a) => a,
                None => continue,
            };

            if arr.is_empty() {
                continue;
            }

            let msg_type = arr[0].as_str().unwrap_or("");

            if msg_type == "EOSE" {
                if let Some(sid) = arr.get(1).and_then(|v| v.as_str()) {
                    if sid == sub_id {
                        break;
                    }
                }
            }

            if msg_type == "EVENT" && arr.len() >= 3 {
                if let Some(sid) = arr.get(1).and_then(|v| v.as_str()) {
                    if sid != sub_id {
                        continue;
                    }
                }
                if let Some(event_json) = arr.get(2) {
                    let event_str = event_json.to_string();
                    match nostr::Event::from_json(&event_str) {
                        Ok(event) => {
                            events.push(event);
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse event: {}", e);
                        }
                    }
                }
            }
        }
        Ok::<_, anyhow::Error>(())
    });

    match timeout.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            tracing::warn!("WS read error: {}", e);
        }
        Err(_) => {
            tracing::warn!("Timeout waiting for EOSE on sub {}", sub_id);
        }
    }

    Ok(events)
}

// ---------------------------------------------------------------------------
// Since persistence
// ---------------------------------------------------------------------------

async fn load_since(path: &PathBuf) -> Option<u64> {
    let data = tokio::fs::read_to_string(path).await.ok()?;
    data.trim().parse().ok()
}

async fn save_since(path: &PathBuf, since: u64) -> Result<(), anyhow::Error> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(path, since.to_string()).await?;
    Ok(())
}
