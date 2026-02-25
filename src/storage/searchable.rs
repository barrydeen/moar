use crate::config::SearchConfig;
use crate::error::Result;
use crate::search::SearchIndex;
use crate::storage::NostrStore;
use crate::wot::WotDepthMap;
use nostr::{Event, Filter};
use std::sync::Arc;

pub struct SearchableStore {
    inner: Arc<dyn NostrStore>,
    search: Arc<SearchIndex>,
    depth_map: Option<WotDepthMap>,
    config: SearchConfig,
}

impl SearchableStore {
    pub fn new(
        inner: Arc<dyn NostrStore>,
        search: Arc<SearchIndex>,
        depth_map: Option<WotDepthMap>,
        config: SearchConfig,
    ) -> Self {
        Self {
            inner,
            search,
            depth_map,
            config,
        }
    }

    pub fn search_index(&self) -> &Arc<SearchIndex> {
        &self.search
    }

    fn get_wot_tier(&self, pubkey: &nostr::PublicKey) -> u8 {
        self.depth_map
            .as_ref()
            .map(|dm| dm.get_depth(pubkey).unwrap_or(0))
            .unwrap_or(0)
    }

    fn should_index(&self, event: &Event, wot_tier: u8) -> bool {
        let kind = event.kind.as_u64();
        if !self.search.is_searchable_kind(kind) {
            return false;
        }

        // WoT-only mode
        if self.config.wot_only && wot_tier == 0 {
            return false;
        }

        // Always index WoT members
        if wot_tier > 0 {
            return true;
        }

        let content = event.content.as_str();

        // Min content length
        if content.len() < self.config.min_content_length {
            return false;
        }

        // Skip if content is mostly URLs (>80% ratio)
        let url_chars: usize = content
            .split_whitespace()
            .filter(|w| w.starts_with("http://") || w.starts_with("https://"))
            .map(|w| w.len())
            .sum();
        if content.len() > 0 && url_chars as f64 / content.len() as f64 > 0.8 {
            return false;
        }

        // Skip if excessive hashtags (>10)
        let hashtag_count = content.matches('#').count();
        if hashtag_count > 10 {
            return false;
        }

        true
    }

    pub async fn index_existing(&self) -> Result<u64> {
        let events = self.inner.iter_all()?;
        let mut indexed = 0u64;

        for event in &events {
            let wot_tier = self.get_wot_tier(&event.pubkey);
            if self.should_index(event, wot_tier) {
                if let Err(e) = self
                    .search
                    .add_event(
                        &event.id.to_hex(),
                        event.content.as_str(),
                        &event.pubkey.to_hex(),
                        event.kind.as_u64(),
                        event.created_at.as_u64(),
                        wot_tier,
                    )
                    .await
                {
                    tracing::warn!("Failed to index event {}: {}", event.id, e);
                } else {
                    indexed += 1;
                }

                // Batch commit every 1000 events
                if indexed % 1000 == 0 {
                    if let Err(e) = self.search.commit().await {
                        tracing::warn!("Failed to commit search index: {}", e);
                    }
                }
            }
        }

        if let Err(e) = self.search.commit().await {
            tracing::warn!("Failed to commit search index: {}", e);
        }

        tracing::info!("Indexed {} existing events into search", indexed);
        Ok(indexed)
    }
}

impl NostrStore for SearchableStore {
    fn save_event(&self, event: &Event) -> Result<()> {
        self.inner.save_event(event)?;

        let wot_tier = self.get_wot_tier(&event.pubkey);
        if self.should_index(event, wot_tier) {
            let search = self.search.clone();
            let event_id = event.id.to_hex();
            let content = event.content.to_string();
            let pubkey = event.pubkey.to_hex();
            let kind = event.kind.as_u64();
            let created_at = event.created_at.as_u64();

            tokio::spawn(async move {
                if let Err(e) = search
                    .add_event(&event_id, &content, &pubkey, kind, created_at, wot_tier)
                    .await
                {
                    tracing::warn!("Failed to index event {}: {}", event_id, e);
                }
            });
        }

        Ok(())
    }

    fn get_event(&self, id: &[u8; 32]) -> Result<Option<Event>> {
        self.inner.get_event(id)
    }

    fn delete_event(&self, id: &[u8; 32]) -> Result<bool> {
        let result = self.inner.delete_event(id)?;
        if result {
            let hex = bytes_to_hex(id);
            let search = self.search.clone();
            tokio::spawn(async move {
                if let Err(e) = search.delete_event(&hex).await {
                    tracing::warn!("Failed to delete event from search index: {}", e);
                }
            });
        }
        Ok(result)
    }

    fn query(&self, filter: &Filter) -> Result<Vec<Event>> {
        if let Some(ref search_query) = filter.search {
            let limit = filter.limit.unwrap_or(100);
            let results = self
                .search
                .search(search_query, limit)
                .map_err(|e| crate::error::Error::Internal(e.to_string()))?;

            let mut events = Vec::with_capacity(results.len());
            for (event_id_hex, _score) in &results {
                if let Some(id) = hex_to_32bytes(event_id_hex) {
                    if let Ok(Some(event)) = self.inner.get_event(&id) {
                        // Apply remaining filter criteria
                        if matches_filter_without_search(&event, filter) {
                            events.push(event);
                        }
                    }
                }
            }

            Ok(events)
        } else {
            self.inner.query(filter)
        }
    }

    fn iter_all(&self) -> Result<Vec<Event>> {
        self.inner.iter_all()
    }

    fn event_count(&self) -> Result<u64> {
        self.inner.event_count()
    }

    fn db_path(&self) -> &str {
        self.inner.db_path()
    }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_to_32bytes(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for i in 0..32 {
        bytes[i] = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

fn matches_filter_without_search(event: &Event, filter: &Filter) -> bool {
    if let Some(ref authors) = filter.authors {
        if !authors.contains(&event.pubkey) {
            return false;
        }
    }
    if let Some(ref kinds) = filter.kinds {
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
}
