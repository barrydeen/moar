use crate::config::PaywallConfig;
use crate::nwc::{InvoiceStatus, NwcClient};
use nostr::PublicKey;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

// ---------------------------------------------------------------------------
// PaywallSet — shared pubkey set with expiration, used by PolicyEngine
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct PaywallSet {
    inner: Arc<std::sync::RwLock<HashMap<PublicKey, u64>>>,
}

impl PaywallSet {
    pub fn new_for_test() -> Self {
        Self::new()
    }

    fn new() -> Self {
        Self {
            inner: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }

    pub fn contains(&self, pk: &PublicKey) -> bool {
        let map = self.inner.read().unwrap();
        match map.get(pk) {
            Some(&expires_at) => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                now < expires_at
            }
            None => false,
        }
    }

    pub fn add(&self, pk: PublicKey, expires_at: u64) {
        let mut map = self.inner.write().unwrap();
        // Only update if the new expiration is later
        let entry = map.entry(pk).or_insert(0);
        if expires_at > *entry {
            *entry = expires_at;
        }
    }

    pub fn remove_expired(&self) -> usize {
        let mut map = self.inner.write().unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let before = map.len();
        map.retain(|_, expires_at| *expires_at > now);
        before - map.len()
    }

    pub fn len(&self) -> usize {
        self.inner.read().unwrap().len()
    }

    pub fn list_entries(&self) -> Vec<(PublicKey, u64)> {
        self.inner
            .read()
            .unwrap()
            .iter()
            .map(|(pk, &exp)| (*pk, exp))
            .collect()
    }

    fn replace(&self, entries: HashMap<PublicKey, u64>) {
        *self.inner.write().unwrap() = entries;
    }
}

// ---------------------------------------------------------------------------
// PendingPayment
// ---------------------------------------------------------------------------

struct PendingPayment {
    pubkey: PublicKey,
    #[allow(dead_code)]
    payment_hash: String,
    #[allow(dead_code)]
    amount_sats: u64,
    period_days: u32,
    created_at: u64,
    status: tokio::sync::watch::Receiver<InvoiceStatus>,
    _listener_handle: JoinHandle<()>,
}

// ---------------------------------------------------------------------------
// PaywallEntry
// ---------------------------------------------------------------------------

struct PaywallEntry {
    config: PaywallConfig,
    set: PaywallSet,
    nwc_client: NwcClient,
    pending_payments: Arc<RwLock<HashMap<String, PendingPayment>>>,
    handle: Option<JoinHandle<()>>,
}

// ---------------------------------------------------------------------------
// PaywallManager
// ---------------------------------------------------------------------------

pub struct PaywallManager {
    entries: RwLock<HashMap<String, PaywallEntry>>,
    data_dir: PathBuf,
}

#[derive(Serialize)]
pub struct PaywallInfo {
    pub id: String,
    pub price_sats: u64,
    pub period_days: u32,
    pub whitelist_count: usize,
}

#[derive(Serialize)]
pub struct WhitelistEntry {
    pub pubkey: String,
    pub expires_at: u64,
}

impl PaywallManager {
    pub fn new(paywalls: HashMap<String, PaywallConfig>) -> Result<Arc<Self>, anyhow::Error> {
        let data_dir = PathBuf::from("data/paywall");
        let mut entries = HashMap::new();

        for (id, config) in paywalls {
            let nwc_client = NwcClient::from_connection_string(&config.nwc_string)
                .map_err(|e| anyhow::anyhow!("Paywall '{}' invalid NWC string: {}", id, e))?;
            entries.insert(
                id,
                PaywallEntry {
                    config,
                    set: PaywallSet::new(),
                    nwc_client,
                    pending_payments: Arc::new(RwLock::new(HashMap::new())),
                    handle: None,
                },
            );
        }

        Ok(Arc::new(Self {
            entries: RwLock::new(entries),
            data_dir,
        }))
    }

    pub async fn start_all(self: &Arc<Self>) {
        let _ = tokio::fs::create_dir_all(&self.data_dir).await;

        let ids: Vec<String> = self.entries.read().await.keys().cloned().collect();
        for id in ids {
            self.start_background_task(&id).await;
        }
    }

    async fn start_background_task(self: &Arc<Self>, id: &str) {
        let mut entries = self.entries.write().await;
        let entry = match entries.get_mut(id) {
            Some(e) => e,
            None => return,
        };

        // Load from disk
        let disk_path = self.data_dir.join(format!("{}.bin", id));
        if let Ok(loaded) = load_from_disk(&disk_path).await {
            let count = loaded.len();
            entry.set.replace(loaded);
            tracing::info!("Paywall '{}' loaded from disk: {} entries", id, count);
        }

        let set = entry.set.clone();
        let pending = Arc::clone(&entry.pending_payments);
        let disk_path = self.data_dir.join(format!("{}.bin", id));
        let paywall_id = id.to_string();

        let handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3600)).await;

                // Prune expired entries
                let removed = set.remove_expired();
                if removed > 0 {
                    tracing::info!(
                        "Paywall '{}': pruned {} expired entries",
                        paywall_id,
                        removed
                    );
                }

                // Clean up old pending payments (older than 1 hour)
                {
                    let mut pending_map = pending.write().await;
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    let stale_keys: Vec<String> = pending_map
                        .iter()
                        .filter(|(_, p)| now - p.created_at >= 3600)
                        .map(|(k, _)| k.clone())
                        .collect();
                    for key in stale_keys {
                        if let Some(removed) = pending_map.remove(&key) {
                            removed._listener_handle.abort();
                            tracing::debug!(payment_hash = %key, "Paywall '{}': cleaned up stale pending payment", paywall_id);
                        }
                    }
                }

                // Persist to disk
                let entries = set.list_entries();
                if let Err(e) = save_to_disk(&disk_path, &entries).await {
                    tracing::warn!("Failed to save paywall '{}' to disk: {}", paywall_id, e);
                }
            }
        });

        entry.handle = Some(handle);
    }

    pub async fn get_set(&self, id: &str) -> Option<PaywallSet> {
        self.entries.read().await.get(id).map(|e| e.set.clone())
    }

    pub async fn create_invoice(
        &self,
        id: &str,
        pubkey: PublicKey,
    ) -> Result<crate::nwc::InvoiceResponse, anyhow::Error> {
        let entries = self.entries.read().await;
        let entry = entries
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Paywall '{}' not found", id))?;

        let amount_msats = entry.config.price_sats * 1000;
        let memo = format!(
            "Relay access - {} sats for {} days",
            entry.config.price_sats, entry.config.period_days
        );

        let response = entry.nwc_client.make_invoice(amount_msats, &memo).await?;

        // Spawn persistent listener for this invoice
        let (status_tx, status_rx) = tokio::sync::watch::channel(InvoiceStatus::Pending);
        let nwc = entry.nwc_client.clone();
        let ph = response.payment_hash.clone();
        let listener_handle = tokio::spawn(async move {
            if let Err(e) = nwc.subscribe_and_watch_invoice(ph.clone(), status_tx).await {
                tracing::warn!(payment_hash = %ph, error = %e, "NWC: watch task ended with error");
            }
        });

        // Store pending payment
        let pending = PendingPayment {
            pubkey,
            payment_hash: response.payment_hash.clone(),
            amount_sats: entry.config.price_sats,
            period_days: entry.config.period_days,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            status: status_rx,
            _listener_handle: listener_handle,
        };

        entry
            .pending_payments
            .write()
            .await
            .insert(response.payment_hash.clone(), pending);

        Ok(response)
    }

    pub async fn check_payment(
        &self,
        id: &str,
        payment_hash: &str,
    ) -> Result<InvoiceStatus, anyhow::Error> {
        let entries = self.entries.read().await;
        let entry = entries
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Paywall '{}' not found", id))?;

        // Read status from the background watcher (no NWC call)
        let status = {
            let pending_map = entry.pending_payments.read().await;
            match pending_map.get(payment_hash) {
                Some(pending) => pending.status.borrow().clone(),
                None => return Ok(InvoiceStatus::Expired),
            }
        };

        if status == InvoiceStatus::Paid {
            // Remove from pending and add pubkey to the whitelist
            let mut pending_map = entry.pending_payments.write().await;
            if let Some(pending) = pending_map.remove(payment_hash) {
                pending._listener_handle.abort();
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let expires_at = now + (pending.period_days as u64) * 24 * 3600;
                entry.set.add(pending.pubkey, expires_at);

                // Persist to disk
                let disk_path = self.data_dir.join(format!("{}.bin", id));
                let entries_list = entry.set.list_entries();
                if let Err(e) = save_to_disk(&disk_path, &entries_list).await {
                    tracing::warn!("Failed to persist paywall '{}' after payment: {}", id, e);
                }

                tracing::info!(
                    "Paywall '{}': pubkey {} granted access until {}",
                    id,
                    pending.pubkey.to_hex(),
                    expires_at
                );
            }
        }

        Ok(status)
    }

    pub async fn verify_nwc(&self, nwc_string: &str) -> Result<(), anyhow::Error> {
        let client = NwcClient::from_connection_string(nwc_string)?;
        client.get_info().await
    }

    pub async fn add_paywall(
        self: &Arc<Self>,
        id: String,
        config: PaywallConfig,
    ) -> Result<(), String> {
        let nwc_client = NwcClient::from_connection_string(&config.nwc_string)
            .map_err(|e| format!("Invalid NWC string: {}", e))?;

        let mut entries = self.entries.write().await;
        if entries.contains_key(&id) {
            return Err(format!("Paywall '{}' already exists", id));
        }
        entries.insert(
            id.clone(),
            PaywallEntry {
                config,
                set: PaywallSet::new(),
                nwc_client,
                pending_payments: Arc::new(RwLock::new(HashMap::new())),
                handle: None,
            },
        );
        drop(entries);
        self.start_background_task(&id).await;
        Ok(())
    }

    pub async fn update_paywall(
        self: &Arc<Self>,
        id: &str,
        config: PaywallConfig,
    ) -> Result<(), String> {
        let nwc_client = NwcClient::from_connection_string(&config.nwc_string)
            .map_err(|e| format!("Invalid NWC string: {}", e))?;

        let mut entries = self.entries.write().await;
        let entry = entries
            .get_mut(id)
            .ok_or_else(|| format!("Paywall '{}' not found", id))?;

        // Abort existing background task
        if let Some(handle) = entry.handle.take() {
            handle.abort();
        }

        entry.config = config;
        entry.nwc_client = nwc_client;
        drop(entries);
        self.start_background_task(id).await;
        Ok(())
    }

    pub async fn remove_paywall(&self, id: &str) -> Result<PaywallConfig, String> {
        let mut entries = self.entries.write().await;
        let mut entry = entries
            .remove(id)
            .ok_or_else(|| format!("Paywall '{}' not found", id))?;

        if let Some(handle) = entry.handle.take() {
            handle.abort();
        }

        // Remove disk file
        let disk_path = self.data_dir.join(format!("{}.bin", id));
        let _ = tokio::fs::remove_file(&disk_path).await;

        Ok(entry.config)
    }

    pub async fn list_paywalls(&self) -> Vec<PaywallInfo> {
        let entries = self.entries.read().await;
        entries
            .iter()
            .map(|(id, entry)| PaywallInfo {
                id: id.clone(),
                price_sats: entry.config.price_sats,
                period_days: entry.config.period_days,
                whitelist_count: entry.set.len(),
            })
            .collect()
    }

    pub async fn get_paywall_info(&self, id: &str) -> Option<PaywallInfo> {
        let entries = self.entries.read().await;
        entries.get(id).map(|entry| PaywallInfo {
            id: id.to_string(),
            price_sats: entry.config.price_sats,
            period_days: entry.config.period_days,
            whitelist_count: entry.set.len(),
        })
    }

    pub async fn get_whitelist(&self, id: &str) -> Option<Vec<WhitelistEntry>> {
        let entries = self.entries.read().await;
        entries.get(id).map(|entry| {
            entry
                .set
                .list_entries()
                .into_iter()
                .map(|(pk, exp)| WhitelistEntry {
                    pubkey: pk.to_hex(),
                    expires_at: exp,
                })
                .collect()
        })
    }

    pub async fn get_config(&self, id: &str) -> Option<PaywallConfig> {
        self.entries.read().await.get(id).map(|e| e.config.clone())
    }
}

// ---------------------------------------------------------------------------
// Disk persistence — binary format (32-byte pubkey + 8-byte LE u64 per entry)
// ---------------------------------------------------------------------------

async fn save_to_disk(
    path: &Path,
    entries: &[(PublicKey, u64)],
) -> Result<(), anyhow::Error> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut buf = Vec::with_capacity(entries.len() * 40);
    for (pk, expires_at) in entries {
        buf.extend_from_slice(pk.to_bytes().as_slice());
        buf.extend_from_slice(&expires_at.to_le_bytes());
    }
    tokio::fs::write(path, buf).await?;
    Ok(())
}

async fn load_from_disk(path: &Path) -> Result<HashMap<PublicKey, u64>, anyhow::Error> {
    let data = tokio::fs::read(path).await?;
    if data.len() % 40 != 0 {
        return Err(anyhow::anyhow!("Invalid paywall file size"));
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut map = HashMap::new();
    for chunk in data.chunks_exact(40) {
        let pk_bytes: [u8; 32] = chunk[..32].try_into().unwrap();
        let expires_at = u64::from_le_bytes(chunk[32..40].try_into().unwrap());
        // Skip already-expired entries on load
        if expires_at > now {
            if let Ok(pk) = PublicKey::from_slice(&pk_bytes) {
                map.insert(pk, expires_at);
            }
        }
    }
    Ok(map)
}
