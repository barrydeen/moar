use serde::Serialize;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

use crate::storage::NostrStore;

// ---------------------------------------------------------------------------
// Per-relay atomic counters (lock-free hot path)
// ---------------------------------------------------------------------------

pub struct RelayStats {
    pub active_connections: AtomicI64,
    pub total_connections: AtomicU64,
    pub events_saved: AtomicU64,
    pub events_rejected: AtomicU64,
    pub queries_served: AtomicU64,
    pub bytes_rx: AtomicU64,
    pub bytes_tx: AtomicU64,
    pub event_count: AtomicU64,
    pub storage_bytes: AtomicU64,
}

impl RelayStats {
    pub fn new() -> Self {
        Self {
            active_connections: AtomicI64::new(0),
            total_connections: AtomicU64::new(0),
            events_saved: AtomicU64::new(0),
            events_rejected: AtomicU64::new(0),
            queries_served: AtomicU64::new(0),
            bytes_rx: AtomicU64::new(0),
            bytes_tx: AtomicU64::new(0),
            event_count: AtomicU64::new(0),
            storage_bytes: AtomicU64::new(0),
        }
    }
}

// ---------------------------------------------------------------------------
// Time-series ring buffer (24h at 1-minute resolution)
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
pub struct TimeBucket {
    pub timestamp: u64,
    pub active_connections: i64,
    pub total_connections: u64,
    pub events_saved: u64,
    pub events_rejected: u64,
    pub queries_served: u64,
    pub bytes_rx: u64,
    pub bytes_tx: u64,
    pub event_count: u64,
    pub storage_bytes: u64,
}

const RING_CAPACITY: usize = 1440; // 24h * 60min

pub struct TimeSeriesRing {
    buckets: Vec<TimeBucket>,
    write_pos: usize,
    len: usize,
}

impl TimeSeriesRing {
    pub fn new() -> Self {
        Self {
            buckets: Vec::with_capacity(RING_CAPACITY),
            write_pos: 0,
            len: 0,
        }
    }

    pub fn push(&mut self, bucket: TimeBucket) {
        if self.buckets.len() < RING_CAPACITY {
            self.buckets.push(bucket);
        } else {
            self.buckets[self.write_pos] = bucket;
        }
        self.write_pos = (self.write_pos + 1) % RING_CAPACITY;
        if self.len < RING_CAPACITY {
            self.len += 1;
        }
    }

    pub fn entries(&self) -> Vec<TimeBucket> {
        if self.len < RING_CAPACITY {
            self.buckets.clone()
        } else {
            let mut result = Vec::with_capacity(RING_CAPACITY);
            result.extend_from_slice(&self.buckets[self.write_pos..]);
            result.extend_from_slice(&self.buckets[..self.write_pos]);
            result
        }
    }
}

fn snapshot(stats: &RelayStats) -> TimeBucket {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    TimeBucket {
        timestamp: now,
        active_connections: stats.active_connections.load(Relaxed),
        total_connections: stats.total_connections.load(Relaxed),
        events_saved: stats.events_saved.load(Relaxed),
        events_rejected: stats.events_rejected.load(Relaxed),
        queries_served: stats.queries_served.load(Relaxed),
        bytes_rx: stats.bytes_rx.load(Relaxed),
        bytes_tx: stats.bytes_tx.load(Relaxed),
        event_count: stats.event_count.load(Relaxed),
        storage_bytes: stats.storage_bytes.load(Relaxed),
    }
}

// ---------------------------------------------------------------------------
// System stats (cached, refreshed each tick)
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Default)]
pub struct SystemStats {
    pub cpu_usage_percent: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub disk_used_bytes: u64,
    pub disk_total_bytes: u64,
}

pub type SharedSystemStats = Arc<RwLock<SystemStats>>;

// ---------------------------------------------------------------------------
// Background task — runs every 60s
// ---------------------------------------------------------------------------

pub async fn stats_background_loop(
    relay_stats: Vec<(String, Arc<RelayStats>, Arc<RwLock<TimeSeriesRing>>, Arc<dyn NostrStore>, String)>,
    system_stats: SharedSystemStats,
) {
    use sysinfo::{Disks, System};

    let mut sys = System::new();
    let disks = Disks::new_with_refreshed_list();

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    interval.tick().await; // first tick is immediate — skip it

    loop {
        interval.tick().await;

        // Update per-relay stats
        for (_, stats, ring, store, db_path) in &relay_stats {
            // Snapshot into ring buffer
            let bucket = snapshot(stats);
            ring.write().await.push(bucket);

            // Update event count from DB metadata
            if let Ok(count) = store.event_count() {
                stats.event_count.store(count, Relaxed);
            }

            // Update storage size from data.mdb file
            let mdb_path = std::path::Path::new(db_path).join("data.mdb");
            if let Ok(meta) = tokio::fs::metadata(&mdb_path).await {
                stats.storage_bytes.store(meta.len(), Relaxed);
            }
        }

        // Update system stats
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        let cpu = sys.global_cpu_usage();
        let mem_used = sys.used_memory();
        let mem_total = sys.total_memory();

        let mut disk_used = 0u64;
        let mut disk_total = 0u64;
        for disk in disks.list() {
            disk_total += disk.total_space();
            disk_used += disk.total_space() - disk.available_space();
        }

        let mut ss = system_stats.write().await;
        ss.cpu_usage_percent = cpu;
        ss.memory_used_bytes = mem_used;
        ss.memory_total_bytes = mem_total;
        ss.disk_used_bytes = disk_used;
        ss.disk_total_bytes = disk_total;
    }
}
