use crate::config::CrawlConfig;
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
// CrawlStatus
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "state")]
pub enum CrawlStatus {
    Idle,
    Crawling {
        window_start: u64,
        window_end: u64,
        events_so_far: u64,
    },
    Paused,
    Complete,
    Error {
        message: String,
    },
}

// ---------------------------------------------------------------------------
// CrawlProgress — persisted to disk
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CrawlProgress {
    pub total_events: u64,
    pub current_window_start: u64,
    pub current_window_end: u64,
    pub started_at: u64,
}

// ---------------------------------------------------------------------------
// CrawlEntry
// ---------------------------------------------------------------------------

struct CrawlEntry {
    config: CrawlConfig,
    status: Arc<RwLock<CrawlStatus>>,
    progress: Arc<RwLock<CrawlProgress>>,
    handle: Option<JoinHandle<()>>,
    pause_notify: Arc<Notify>,
    resume_notify: Arc<Notify>,
}

// ---------------------------------------------------------------------------
// CrawlManager
// ---------------------------------------------------------------------------

pub struct CrawlManager {
    entries: RwLock<HashMap<String, CrawlEntry>>,
    stores: RwLock<HashMap<String, Arc<dyn NostrStore>>>,
    wot_manager: Arc<WotManager>,
    data_dir: PathBuf,
}

impl CrawlManager {
    pub fn new(
        crawls: HashMap<String, CrawlConfig>,
        stores: HashMap<String, Arc<dyn NostrStore>>,
        wot_manager: Arc<WotManager>,
    ) -> Arc<Self> {
        let data_dir = PathBuf::from("data/crawl");
        let mut entries = HashMap::new();

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        for (id, config) in crawls {
            let until = config.until.unwrap_or(now);
            entries.insert(
                id,
                CrawlEntry {
                    config,
                    status: Arc::new(RwLock::new(CrawlStatus::Idle)),
                    progress: Arc::new(RwLock::new(CrawlProgress {
                        total_events: 0,
                        current_window_start: until,
                        current_window_end: until,
                        started_at: now,
                    })),
                    handle: None,
                    pause_notify: Arc::new(Notify::new()),
                    resume_notify: Arc::new(Notify::new()),
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
            self.start_crawler(&id).await;
        }
    }

    async fn start_crawler(self: &Arc<Self>, id: &str) {
        let mut entries = self.entries.write().await;
        let entry = match entries.get_mut(id) {
            Some(e) => e,
            None => return,
        };

        if let Some(handle) = entry.handle.take() {
            handle.abort();
        }

        // Load progress from disk
        let progress_path = self.data_dir.join(format!("{}.progress.json", id));
        if let Ok(data) = tokio::fs::read_to_string(&progress_path).await {
            if let Ok(saved) = serde_json::from_str::<CrawlProgress>(&data) {
                *entry.progress.write().await = saved;
            }
        }

        if entry.config.paused {
            *entry.status.write().await = CrawlStatus::Paused;
            return;
        }

        let manager = Arc::clone(self);
        let crawl_id = id.to_string();
        let config = entry.config.clone();
        let status = Arc::clone(&entry.status);
        let progress = Arc::clone(&entry.progress);
        let pause_notify = Arc::clone(&entry.pause_notify);
        let resume_notify = Arc::clone(&entry.resume_notify);

        let handle = tokio::spawn(async move {
            crawl_loop(
                &crawl_id,
                &config,
                &manager,
                &status,
                &progress,
                &pause_notify,
                &resume_notify,
            )
            .await;
        });

        entry.handle = Some(handle);
    }

    pub async fn list_crawls(&self) -> Vec<CrawlInfo> {
        let entries = self.entries.read().await;
        let mut result = Vec::new();
        for (id, entry) in entries.iter() {
            result.push(CrawlInfo {
                id: id.clone(),
                config: entry.config.clone(),
                status: entry.status.read().await.clone(),
                progress: entry.progress.read().await.clone(),
            });
        }
        result
    }

    pub async fn get_crawl(&self, id: &str) -> Option<CrawlInfo> {
        let entries = self.entries.read().await;
        let entry = match entries.get(id) {
            Some(e) => e,
            None => return None,
        };
        let info = CrawlInfo {
            id: id.to_string(),
            config: entry.config.clone(),
            status: entry.status.read().await.clone(),
            progress: entry.progress.read().await.clone(),
        };
        Some(info)
    }

    pub async fn add_crawl(
        self: &Arc<Self>,
        id: String,
        config: CrawlConfig,
    ) -> Result<(), String> {
        let mut entries = self.entries.write().await;
        if entries.contains_key(&id) {
            return Err(format!("Crawl '{}' already exists", id));
        }

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let until = config.until.unwrap_or(now);

        entries.insert(
            id.clone(),
            CrawlEntry {
                config,
                status: Arc::new(RwLock::new(CrawlStatus::Idle)),
                progress: Arc::new(RwLock::new(CrawlProgress {
                    total_events: 0,
                    current_window_start: until,
                    current_window_end: until,
                    started_at: now,
                })),
                handle: None,
                pause_notify: Arc::new(Notify::new()),
                resume_notify: Arc::new(Notify::new()),
            },
        );
        drop(entries);
        self.start_crawler(&id).await;
        Ok(())
    }

    pub async fn update_crawl(
        self: &Arc<Self>,
        id: &str,
        config: CrawlConfig,
    ) -> Result<(), String> {
        {
            let mut entries = self.entries.write().await;
            let entry = entries
                .get_mut(id)
                .ok_or_else(|| format!("Crawl '{}' not found", id))?;

            if let Some(handle) = entry.handle.take() {
                handle.abort();
            }

            entry.config = config;
            *entry.status.write().await = CrawlStatus::Idle;
        }
        self.start_crawler(id).await;
        Ok(())
    }

    pub async fn remove_crawl(&self, id: &str) -> Result<CrawlConfig, String> {
        let mut entries = self.entries.write().await;
        let mut entry = entries
            .remove(id)
            .ok_or_else(|| format!("Crawl '{}' not found", id))?;

        if let Some(handle) = entry.handle.take() {
            handle.abort();
        }

        let progress_path = self.data_dir.join(format!("{}.progress.json", id));
        let _ = tokio::fs::remove_file(&progress_path).await;

        Ok(entry.config)
    }

    pub async fn pause_crawl(&self, id: &str) -> Result<(), String> {
        let entries = self.entries.read().await;
        let entry = entries
            .get(id)
            .ok_or_else(|| format!("Crawl '{}' not found", id))?;
        entry.pause_notify.notify_one();
        Ok(())
    }

    pub async fn resume_crawl(&self, id: &str) -> Result<(), String> {
        let entries = self.entries.read().await;
        let entry = entries
            .get(id)
            .ok_or_else(|| format!("Crawl '{}' not found", id))?;
        entry.resume_notify.notify_one();
        Ok(())
    }

    pub async fn update_stores(&self, stores: HashMap<String, Arc<dyn NostrStore>>) {
        *self.stores.write().await = stores;
    }
}

// ---------------------------------------------------------------------------
// CrawlInfo — serializable for API responses
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
pub struct CrawlInfo {
    pub id: String,
    pub config: CrawlConfig,
    pub status: CrawlStatus,
    pub progress: CrawlProgress,
}

// ---------------------------------------------------------------------------
// Crawl loop
// ---------------------------------------------------------------------------

async fn crawl_loop(
    crawl_id: &str,
    config: &CrawlConfig,
    manager: &Arc<CrawlManager>,
    status: &Arc<RwLock<CrawlStatus>>,
    progress: &Arc<RwLock<CrawlProgress>>,
    pause_notify: &Arc<Notify>,
    resume_notify: &Arc<Notify>,
) {
    let store = {
        let stores = manager.stores.read().await;
        match stores.get(&config.relay) {
            Some(s) => s.clone(),
            None => {
                tracing::error!(
                    "Crawl '{}': target relay '{}' not found",
                    crawl_id,
                    config.relay
                );
                *status.write().await = CrawlStatus::Error {
                    message: format!("Target relay '{}' not found", config.relay),
                };
                return;
            }
        }
    };

    // Resolve authors
    let authors = match resolve_authors(config, &manager.wot_manager).await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Crawl '{}': failed to resolve authors: {}", crawl_id, e);
            *status.write().await = CrawlStatus::Error {
                message: e.to_string(),
            };
            return;
        }
    };

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let until = config.until.unwrap_or(now);
    let window_secs = config.window_hours * 3600;

    // Resume from saved progress or start from `until`
    let mut window_end = {
        let p = progress.read().await;
        if p.current_window_start > config.since && p.current_window_start < until {
            p.current_window_start
        } else {
            until
        }
    };

    let rate_interval = if config.max_requests_per_second > 0 {
        std::time::Duration::from_millis(1000 / config.max_requests_per_second as u64)
    } else {
        std::time::Duration::from_millis(200)
    };

    let progress_path = manager
        .data_dir
        .join(format!("{}.progress.json", crawl_id));

    while window_end > config.since {
        let window_start = if window_end > window_secs {
            (window_end - window_secs).max(config.since)
        } else {
            config.since
        };

        // Check for pause
        if is_paused(pause_notify).await {
            *status.write().await = CrawlStatus::Paused;
            tracing::info!("Crawl '{}': paused", crawl_id);
            resume_notify.notified().await;
            tracing::info!("Crawl '{}': resumed", crawl_id);
        }

        let mut window_events = 0u64;

        {
            let mut p = progress.write().await;
            p.current_window_start = window_start;
            p.current_window_end = window_end;
        }

        *status.write().await = CrawlStatus::Crawling {
            window_start,
            window_end,
            events_so_far: progress.read().await.total_events,
        };

        // Chunk authors into batches
        let author_batches: Vec<Option<Vec<String>>> = if let Some(ref authors) = authors {
            if authors.is_empty() {
                vec![None]
            } else {
                authors
                    .chunks(config.batch_size)
                    .map(|chunk| Some(chunk.to_vec()))
                    .collect()
            }
        } else {
            vec![None]
        };

        for (batch_idx, author_batch) in author_batches.iter().enumerate() {
            let relay_idx = batch_idx % config.remote_relays.len();
            let relay_url = &config.remote_relays[relay_idx];

            let mut filter = serde_json::Map::new();
            if let Some(ref batch) = author_batch {
                filter.insert("authors".to_string(), serde_json::json!(batch));
            }
            if let Some(ref kinds) = config.kinds {
                filter.insert("kinds".to_string(), serde_json::json!(kinds));
            }
            filter.insert("since".to_string(), serde_json::json!(window_start));
            filter.insert("until".to_string(), serde_json::json!(window_end));

            match fetch_events(relay_url, &serde_json::Value::Object(filter)).await {
                Ok(events) => {
                    let count = events.len() as u64;
                    for event in &events {
                        if let Err(e) = store.save_event(event) {
                            tracing::warn!(
                                "Crawl '{}': failed to save event: {}",
                                crawl_id,
                                e
                            );
                        }
                    }
                    window_events += count;
                }
                Err(e) => {
                    tracing::warn!(
                        "Crawl '{}': failed to fetch from {}: {}",
                        crawl_id,
                        relay_url,
                        e
                    );
                }
            }

            tokio::time::sleep(rate_interval).await;
        }

        // Update progress
        {
            let mut p = progress.write().await;
            p.total_events += window_events;
        }

        // Save progress to disk
        if let Ok(json) = serde_json::to_string(&*progress.read().await) {
            let _ = tokio::fs::write(&progress_path, json).await;
        }

        tracing::info!(
            "Crawl '{}': window {}-{} complete, {} events",
            crawl_id,
            window_start,
            window_end,
            window_events,
        );

        window_end = window_start;
    }

    *status.write().await = CrawlStatus::Complete;
    tracing::info!(
        "Crawl '{}': complete, {} total events",
        crawl_id,
        progress.read().await.total_events
    );
}

async fn is_paused(notify: &Notify) -> bool {
    tokio::select! {
        biased;
        _ = notify.notified() => true,
        _ = tokio::time::sleep(std::time::Duration::from_millis(0)) => false,
    }
}

// ---------------------------------------------------------------------------
// Author resolution
// ---------------------------------------------------------------------------

async fn resolve_authors(
    config: &CrawlConfig,
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
        return Ok(Some(pubkeys));
    }

    if let Some(ref authors) = config.authors {
        return Ok(Some(authors.clone()));
    }

    Ok(None)
}

// ---------------------------------------------------------------------------
// WebSocket fetch (reused pattern from sync.rs)
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

    let sub_id = "crawl-0";
    let req = serde_json::json!(["REQ", sub_id, filter]);
    ws.send(Message::Text(req.to_string().into())).await?;

    let events = read_events_until_eose(&mut ws, sub_id).await?;

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
                        Ok(event) => events.push(event),
                        Err(e) => tracing::warn!("Failed to parse event: {}", e),
                    }
                }
            }
        }
        Ok::<_, anyhow::Error>(())
    });

    match timeout.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::warn!("WS read error: {}", e),
        Err(_) => tracing::warn!("Timeout waiting for EOSE on sub {}", sub_id),
    }

    Ok(events)
}
