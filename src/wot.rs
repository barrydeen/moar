use crate::config::WotConfig;
use futures_util::{SinkExt, StreamExt};
use nostr::PublicKey;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;

// ---------------------------------------------------------------------------
// WotSet — shared pubkey set used by PolicyEngine
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct WotSet {
    inner: Arc<std::sync::RwLock<HashSet<PublicKey>>>,
}

impl WotSet {
    fn new() -> Self {
        Self {
            inner: Arc::new(std::sync::RwLock::new(HashSet::new())),
        }
    }

    pub fn contains(&self, pk: &PublicKey) -> bool {
        self.inner.read().unwrap().contains(pk)
    }

    pub fn len(&self) -> usize {
        self.inner.read().unwrap().len()
    }

    fn replace(&self, set: HashSet<PublicKey>) {
        *self.inner.write().unwrap() = set;
    }
}

// ---------------------------------------------------------------------------
// WotStatus
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "state")]
pub enum WotStatus {
    Pending,
    Building {
        depth_progress: u8,
        total_depth: u8,
    },
    Ready,
    Error {
        message: String,
    },
}

// ---------------------------------------------------------------------------
// WotEntry
// ---------------------------------------------------------------------------

struct WotEntry {
    config: WotConfig,
    set: WotSet,
    status: Arc<RwLock<WotStatus>>,
    last_updated: Arc<RwLock<Option<u64>>>,
    handle: Option<JoinHandle<()>>,
}

// ---------------------------------------------------------------------------
// WotManager
// ---------------------------------------------------------------------------

pub struct WotManager {
    entries: RwLock<HashMap<String, WotEntry>>,
    discovery_relays: RwLock<Vec<String>>,
    data_dir: PathBuf,
}

impl WotManager {
    pub fn new(discovery_relays: Vec<String>, wots: HashMap<String, WotConfig>) -> Arc<Self> {
        let data_dir = PathBuf::from("data/wot");
        let mut entries = HashMap::new();

        for (id, config) in wots {
            let set = WotSet::new();
            entries.insert(
                id,
                WotEntry {
                    config,
                    set,
                    status: Arc::new(RwLock::new(WotStatus::Pending)),
                    last_updated: Arc::new(RwLock::new(None)),
                    handle: None,
                },
            );
        }

        Arc::new(Self {
            entries: RwLock::new(entries),
            discovery_relays: RwLock::new(discovery_relays),
            data_dir,
        })
    }

    pub async fn start_all(self: &Arc<Self>) {
        let _ = tokio::fs::create_dir_all(&self.data_dir).await;

        let ids: Vec<String> = self.entries.read().await.keys().cloned().collect();
        for id in ids {
            self.start_builder(&id).await;
        }
    }

    async fn start_builder(self: &Arc<Self>, id: &str) {
        let mut entries = self.entries.write().await;
        let entry = match entries.get_mut(id) {
            Some(e) => e,
            None => return,
        };

        // Try loading from disk
        let disk_path = self.data_dir.join(format!("{}.bin", id));
        if let Ok(set) = load_pubkeys_from_disk(&disk_path).await {
            let freshness_hours = entry.config.update_interval_hours;
            if let Ok(meta) = tokio::fs::metadata(&disk_path).await {
                if let Ok(modified) = meta.modified() {
                    let age_secs = SystemTime::now()
                        .duration_since(modified)
                        .unwrap_or_default()
                        .as_secs();
                    if age_secs < freshness_hours * 3600 {
                        let count = set.len();
                        entry.set.replace(set);
                        *entry.status.write().await = WotStatus::Ready;
                        *entry.last_updated.write().await = Some(
                            modified
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs(),
                        );
                        tracing::info!(
                            "WoT '{}' loaded from disk: {} pubkeys",
                            id,
                            count
                        );
                    }
                }
            }
        }

        let manager = Arc::clone(self);
        let wot_id = id.to_string();
        let config = entry.config.clone();
        let set = entry.set.clone();
        let status = Arc::clone(&entry.status);
        let last_updated = Arc::clone(&entry.last_updated);
        let disk_path = self.data_dir.join(format!("{}.bin", id));

        let handle = tokio::spawn(async move {
            loop {
                // Skip build if already fresh (loaded from disk on first iteration)
                let should_build = {
                    let s = status.read().await;
                    !matches!(*s, WotStatus::Ready)
                };

                if should_build {
                    let relays = manager.discovery_relays.read().await.clone();
                    match build_wot(&config, &relays, &set, &status).await {
                        Ok(()) => {
                            let now = SystemTime::now()
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            *last_updated.write().await = Some(now);

                            // Save to disk
                            let pubkeys: HashSet<PublicKey> =
                                set.inner.read().unwrap().clone();
                            if let Err(e) = save_pubkeys_to_disk(&disk_path, &pubkeys).await {
                                tracing::warn!("Failed to save WoT '{}' to disk: {}", wot_id, e);
                            }
                        }
                        Err(e) => {
                            tracing::error!("WoT '{}' build failed: {}", wot_id, e);
                            *status.write().await = WotStatus::Error {
                                message: e.to_string(),
                            };
                            // Retry after 5 minutes on total failure
                            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                            continue;
                        }
                    }
                }

                // Sleep until next refresh
                let sleep_hours = config.update_interval_hours.max(1);
                tracing::info!(
                    "WoT '{}' sleeping {} hours until next refresh",
                    wot_id,
                    sleep_hours
                );
                tokio::time::sleep(std::time::Duration::from_secs(sleep_hours * 3600)).await;

                // Mark as needing rebuild for next iteration
                *status.write().await = WotStatus::Pending;
            }
        });

        entry.handle = Some(handle);
    }

    pub async fn get_set(&self, id: &str) -> Option<WotSet> {
        self.entries.read().await.get(id).map(|e| e.set.clone())
    }

    pub async fn get_status(&self, id: &str) -> Option<WotStatus> {
        let entries = self.entries.read().await;
        let entry = entries.get(id)?;
        let status = entry.status.read().await.clone();
        Some(status)
    }

    pub async fn list_wots(&self) -> Vec<WotInfo> {
        let entries = self.entries.read().await;
        let mut result = Vec::new();
        for (id, entry) in entries.iter() {
            result.push(WotInfo {
                id: id.clone(),
                config: entry.config.clone(),
                status: entry.status.read().await.clone(),
                pubkey_count: entry.set.len(),
                last_updated: *entry.last_updated.read().await,
            });
        }
        result
    }

    pub async fn add_wot(self: &Arc<Self>, id: String, config: WotConfig) -> Result<(), String> {
        let mut entries = self.entries.write().await;
        if entries.contains_key(&id) {
            return Err(format!("WoT '{}' already exists", id));
        }
        entries.insert(
            id.clone(),
            WotEntry {
                config,
                set: WotSet::new(),
                status: Arc::new(RwLock::new(WotStatus::Pending)),
                last_updated: Arc::new(RwLock::new(None)),
                handle: None,
            },
        );
        drop(entries);
        self.start_builder(&id).await;
        Ok(())
    }

    pub async fn update_wot(
        self: &Arc<Self>,
        id: &str,
        config: WotConfig,
    ) -> Result<(), String> {
        {
            let mut entries = self.entries.write().await;
            let entry = entries.get_mut(id).ok_or_else(|| format!("WoT '{}' not found", id))?;

            // Abort existing builder
            if let Some(handle) = entry.handle.take() {
                handle.abort();
            }

            entry.config = config;
            *entry.status.write().await = WotStatus::Pending;
        }
        self.start_builder(id).await;
        Ok(())
    }

    pub async fn remove_wot(&self, id: &str) -> Result<WotConfig, String> {
        let mut entries = self.entries.write().await;
        let mut entry = entries
            .remove(id)
            .ok_or_else(|| format!("WoT '{}' not found", id))?;

        if let Some(handle) = entry.handle.take() {
            handle.abort();
        }

        // Remove disk file
        let disk_path = self.data_dir.join(format!("{}.bin", id));
        let _ = tokio::fs::remove_file(&disk_path).await;

        Ok(entry.config)
    }

    pub async fn get_discovery_relays(&self) -> Vec<String> {
        self.discovery_relays.read().await.clone()
    }

    pub async fn set_discovery_relays(&self, relays: Vec<String>) {
        *self.discovery_relays.write().await = relays;
    }

    pub async fn wot_ids_referencing(&self, wot_id: &str) -> bool {
        self.entries.read().await.contains_key(wot_id)
    }
}

// ---------------------------------------------------------------------------
// WotInfo — serializable WoT status for API responses
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
pub struct WotInfo {
    pub id: String,
    pub config: WotConfig,
    pub status: WotStatus,
    pub pubkey_count: usize,
    pub last_updated: Option<u64>,
}

// ---------------------------------------------------------------------------
// Background WoT builder
// ---------------------------------------------------------------------------

async fn build_wot(
    config: &WotConfig,
    discovery_relays: &[String],
    set: &WotSet,
    status: &Arc<RwLock<WotStatus>>,
) -> Result<(), anyhow::Error> {
    if discovery_relays.is_empty() {
        return Err(anyhow::anyhow!("No discovery relays configured"));
    }

    let seed = PublicKey::parse(&config.seed)
        .map_err(|e| anyhow::anyhow!("Invalid seed pubkey: {}", e))?;
    let max_depth = config.depth.clamp(1, 4);

    *status.write().await = WotStatus::Building {
        depth_progress: 0,
        total_depth: max_depth,
    };

    let mut all_pubkeys: HashSet<PublicKey> = HashSet::new();
    all_pubkeys.insert(seed);
    let mut current_layer: HashSet<PublicKey> = HashSet::new();
    current_layer.insert(seed);
    let mut queried: HashSet<PublicKey> = HashSet::new();

    for depth in 1..=max_depth {
        let to_query: Vec<PublicKey> = current_layer
            .iter()
            .filter(|pk| !queried.contains(pk))
            .copied()
            .collect();

        if to_query.is_empty() {
            break;
        }

        tracing::info!(
            "WoT depth {}/{}: querying {} pubkeys across {} relays",
            depth,
            max_depth,
            to_query.len(),
            discovery_relays.len()
        );

        // Chunk into batches of 300
        let batches: Vec<Vec<String>> = to_query
            .chunks(300)
            .map(|chunk| chunk.iter().map(|pk| pk.to_hex()).collect())
            .collect();

        // Distribute batches round-robin across relays
        let mut relay_batches: HashMap<usize, Vec<Vec<String>>> = HashMap::new();
        for (i, batch) in batches.iter().enumerate() {
            let relay_idx = i % discovery_relays.len();
            relay_batches
                .entry(relay_idx)
                .or_default()
                .push(batch.clone());
        }

        // Query all relays concurrently
        let mut handles = Vec::new();
        for (relay_idx, batches) in relay_batches {
            let relay_url = discovery_relays[relay_idx].clone();
            handles.push(tokio::spawn(async move {
                query_relay_batches(&relay_url, batches).await
            }));
        }

        let mut next_layer: HashSet<PublicKey> = HashSet::new();
        let mut any_success = false;

        for handle in handles {
            match handle.await {
                Ok(Ok(followed_pks)) => {
                    any_success = true;
                    for pk in followed_pks {
                        if all_pubkeys.insert(pk) {
                            next_layer.insert(pk);
                        }
                    }
                }
                Ok(Err(e)) => {
                    tracing::warn!("Relay query failed: {}", e);
                }
                Err(e) => {
                    tracing::warn!("Relay query task panicked: {}", e);
                }
            }
        }

        if !any_success {
            return Err(anyhow::anyhow!("All relays failed at depth {}", depth));
        }

        for pk in &to_query {
            queried.insert(*pk);
        }

        tracing::info!(
            "WoT depth {}/{}: found {} new pubkeys, total {}",
            depth,
            max_depth,
            next_layer.len(),
            all_pubkeys.len()
        );

        current_layer = next_layer;

        *status.write().await = WotStatus::Building {
            depth_progress: depth,
            total_depth: max_depth,
        };
    }

    set.replace(all_pubkeys.clone());
    *status.write().await = WotStatus::Ready;

    tracing::info!("WoT build complete: {} pubkeys", all_pubkeys.len());
    Ok(())
}

// ---------------------------------------------------------------------------
// Relay querying
// ---------------------------------------------------------------------------

async fn query_relay_batches(
    relay_url: &str,
    batches: Vec<Vec<String>>,
) -> Result<HashSet<PublicKey>, anyhow::Error> {
    let (mut ws, _): (WsStream, _) = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio_tungstenite::connect_async(relay_url),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Connection timeout to {}", relay_url))?
    .map_err(|e| anyhow::anyhow!("WS connect to {} failed: {}", relay_url, e))?;

    let mut all_followed = HashSet::new();

    for (i, batch) in batches.iter().enumerate() {
        let sub_id = format!("wot-{}", i);
        let req = serde_json::json!(["REQ", sub_id, {"authors": batch, "kinds": [3]}]);

        ws.send(Message::Text(req.to_string().into())).await?;

        let followed = read_until_eose(&mut ws, &sub_id).await?;
        all_followed.extend(followed);

        // Send CLOSE
        let close = serde_json::json!(["CLOSE", sub_id]);
        ws.send(Message::Text(close.to_string().into())).await?;

        // Small delay between batches
        if i < batches.len() - 1 {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }

    // Close WS
    let _ = ws.close(None).await;

    Ok(all_followed)
}

type WsStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

async fn read_until_eose(
    ws: &mut WsStream,
    sub_id: &str,
) -> Result<HashSet<PublicKey>, anyhow::Error> {
    let mut followed = HashSet::new();
    let timeout = tokio::time::timeout(std::time::Duration::from_secs(30), async {
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
                if let Some(event_obj) = arr.get(2) {
                    if let Some(tags) = event_obj.get("tags").and_then(|t| t.as_array()) {
                        for tag in tags {
                            if let Some(tag_arr) = tag.as_array() {
                                if tag_arr.len() >= 2
                                    && tag_arr[0].as_str() == Some("p")
                                {
                                    if let Some(hex) = tag_arr[1].as_str() {
                                        if let Ok(pk) = PublicKey::parse(hex) {
                                            followed.insert(pk);
                                        }
                                    }
                                }
                            }
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

    Ok(followed)
}

// ---------------------------------------------------------------------------
// Disk persistence — binary format (concatenated 32-byte pubkeys)
// ---------------------------------------------------------------------------

async fn save_pubkeys_to_disk(
    path: &Path,
    pubkeys: &HashSet<PublicKey>,
) -> Result<(), anyhow::Error> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut buf = Vec::with_capacity(pubkeys.len() * 32);
    for pk in pubkeys {
        buf.extend_from_slice(pk.to_bytes().as_slice());
    }
    tokio::fs::write(path, buf).await?;
    Ok(())
}

async fn load_pubkeys_from_disk(path: &Path) -> Result<HashSet<PublicKey>, anyhow::Error> {
    let data = tokio::fs::read(path).await?;
    if data.len() % 32 != 0 {
        return Err(anyhow::anyhow!("Invalid WoT file size"));
    }
    let mut set = HashSet::new();
    for chunk in data.chunks_exact(32) {
        let bytes: [u8; 32] = chunk.try_into().unwrap();
        if let Ok(pk) = PublicKey::from_slice(&bytes) {
            set.insert(pk);
        }
    }
    Ok(set)
}
