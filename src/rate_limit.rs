use dashmap::DashMap;
use std::collections::VecDeque;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Per-IP connection and rate tracking shared across all WebSocket connections.
pub struct IpTracker {
    map: DashMap<IpAddr, IpState>,
}

struct IpState {
    connections: AtomicU32,
    write_timestamps: Mutex<VecDeque<Instant>>,
    read_timestamps: Mutex<VecDeque<Instant>>,
    last_active: Mutex<Instant>,
}

impl IpState {
    fn new() -> Self {
        Self {
            connections: AtomicU32::new(0),
            write_timestamps: Mutex::new(VecDeque::new()),
            read_timestamps: Mutex::new(VecDeque::new()),
            last_active: Mutex::new(Instant::now()),
        }
    }

    fn touch(&self) {
        if let Ok(mut t) = self.last_active.lock() {
            *t = Instant::now();
        }
    }
}

impl IpTracker {
    pub fn new() -> Self {
        Self {
            map: DashMap::new(),
        }
    }

    /// Try to register a new connection for this IP. Returns `true` if allowed.
    pub fn try_connect(&self, ip: IpAddr, max_connections: Option<u32>) -> bool {
        let entry = self.map.entry(ip).or_insert_with(IpState::new);
        let state = entry.value();

        if let Some(max) = max_connections {
            let current = state.connections.load(Ordering::Relaxed);
            if current >= max {
                return false;
            }
        }

        state.connections.fetch_add(1, Ordering::Relaxed);
        state.touch();
        true
    }

    /// Decrement connection count for this IP. Removes the entry if connections
    /// drop to zero.
    pub fn disconnect(&self, ip: IpAddr) {
        if let Some(entry) = self.map.get(&ip) {
            let prev = entry.connections.fetch_sub(1, Ordering::Relaxed);
            if prev <= 1 {
                drop(entry);
                self.map.remove(&ip);
            }
        }
    }

    /// Sliding-window rate check for writes. Returns `true` if the write is
    /// allowed (under the limit).
    pub fn check_write_rate(&self, ip: IpAddr, limit: Option<u32>) -> bool {
        let limit = match limit {
            Some(l) => l,
            None => return true,
        };
        let entry = match self.map.get(&ip) {
            Some(e) => e,
            None => return true,
        };
        entry.touch();
        check_rate(&entry.write_timestamps, limit)
    }

    /// Sliding-window rate check for reads. Returns `true` if the read is
    /// allowed (under the limit).
    pub fn check_read_rate(&self, ip: IpAddr, limit: Option<u32>) -> bool {
        let limit = match limit {
            Some(l) => l,
            None => return true,
        };
        let entry = match self.map.get(&ip) {
            Some(e) => e,
            None => return true,
        };
        entry.touch();
        check_rate(&entry.read_timestamps, limit)
    }

    /// Remove entries with 0 connections that have been inactive for over 10
    /// minutes.
    pub fn cleanup(&self) {
        let cutoff = Instant::now() - Duration::from_secs(600);
        self.map.retain(|_ip, state| {
            if state.connections.load(Ordering::Relaxed) > 0 {
                return true;
            }
            if let Ok(t) = state.last_active.lock() {
                *t > cutoff
            } else {
                false
            }
        });
    }
}

/// Sliding window check: prune timestamps older than 60s, then check count < limit.
/// Records a new timestamp if allowed.
fn check_rate(timestamps: &Mutex<VecDeque<Instant>>, limit: u32) -> bool {
    let mut ts = timestamps.lock().unwrap();
    let window = Instant::now() - Duration::from_secs(60);

    // Remove expired entries
    while let Some(&front) = ts.front() {
        if front < window {
            ts.pop_front();
        } else {
            break;
        }
    }

    if ts.len() as u32 >= limit {
        return false;
    }

    ts.push_back(Instant::now());
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn localhost() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
    }

    fn other_ip() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))
    }

    #[test]
    fn connection_limit_allows_under_max() {
        let tracker = IpTracker::new();
        assert!(tracker.try_connect(localhost(), Some(2)));
        assert!(tracker.try_connect(localhost(), Some(2)));
    }

    #[test]
    fn connection_limit_rejects_at_max() {
        let tracker = IpTracker::new();
        assert!(tracker.try_connect(localhost(), Some(2)));
        assert!(tracker.try_connect(localhost(), Some(2)));
        assert!(!tracker.try_connect(localhost(), Some(2)));
    }

    #[test]
    fn disconnect_frees_slot() {
        let tracker = IpTracker::new();
        assert!(tracker.try_connect(localhost(), Some(1)));
        assert!(!tracker.try_connect(localhost(), Some(1)));
        tracker.disconnect(localhost());
        assert!(tracker.try_connect(localhost(), Some(1)));
    }

    #[test]
    fn no_limit_always_allows() {
        let tracker = IpTracker::new();
        for _ in 0..100 {
            assert!(tracker.try_connect(localhost(), None));
        }
    }

    #[test]
    fn different_ips_independent() {
        let tracker = IpTracker::new();
        assert!(tracker.try_connect(localhost(), Some(1)));
        assert!(tracker.try_connect(other_ip(), Some(1)));
        assert!(!tracker.try_connect(localhost(), Some(1)));
        assert!(!tracker.try_connect(other_ip(), Some(1)));
    }

    #[test]
    fn write_rate_allows_under_limit() {
        let tracker = IpTracker::new();
        tracker.try_connect(localhost(), None);
        assert!(tracker.check_write_rate(localhost(), Some(5)));
        assert!(tracker.check_write_rate(localhost(), Some(5)));
    }

    #[test]
    fn write_rate_blocks_at_limit() {
        let tracker = IpTracker::new();
        tracker.try_connect(localhost(), None);
        for _ in 0..3 {
            assert!(tracker.check_write_rate(localhost(), Some(3)));
        }
        assert!(!tracker.check_write_rate(localhost(), Some(3)));
    }

    #[test]
    fn read_rate_blocks_at_limit() {
        let tracker = IpTracker::new();
        tracker.try_connect(localhost(), None);
        for _ in 0..3 {
            assert!(tracker.check_read_rate(localhost(), Some(3)));
        }
        assert!(!tracker.check_read_rate(localhost(), Some(3)));
    }

    #[test]
    fn no_rate_limit_always_allows() {
        let tracker = IpTracker::new();
        tracker.try_connect(localhost(), None);
        for _ in 0..100 {
            assert!(tracker.check_write_rate(localhost(), None));
            assert!(tracker.check_read_rate(localhost(), None));
        }
    }

    #[test]
    fn cleanup_removes_inactive() {
        let tracker = IpTracker::new();
        // Manually insert an entry with 0 connections and old last_active
        tracker.map.insert(localhost(), IpState::new());
        {
            let entry = tracker.map.get(&localhost()).unwrap();
            let mut t = entry.last_active.lock().unwrap();
            *t = Instant::now() - Duration::from_secs(700);
        }
        tracker.cleanup();
        assert!(!tracker.map.contains_key(&localhost()));
    }

    #[test]
    fn cleanup_keeps_active_connections() {
        let tracker = IpTracker::new();
        tracker.try_connect(localhost(), None);
        // Even with old last_active, should keep because connections > 0
        {
            let entry = tracker.map.get(&localhost()).unwrap();
            let mut t = entry.last_active.lock().unwrap();
            *t = Instant::now() - Duration::from_secs(700);
        }
        tracker.cleanup();
        assert!(tracker.map.contains_key(&localhost()));
    }
}
