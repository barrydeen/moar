use super::NostrStore;
use crate::error::Result;
use heed::types::*;
use heed::{Database, Env, EnvOpenOptions, RwTxn};
use nostr::{Event, Filter, Kind, PublicKey};
use std::convert::TryInto;
use std::fs;
use std::ops::{Bound, RangeBounds};
use std::path::Path;
use std::sync::Arc;

/// A range over borrowed byte slices that implements `RangeBounds<[u8]>`.
/// Required because heed's `Bytes` codec has `EItem = [u8]` (unsized).
struct ByteRange<'a> {
    start: &'a [u8],
    end: &'a [u8],
}

impl<'a> ByteRange<'a> {
    fn new(start: &'a [u8], end: &'a [u8]) -> Self {
        Self { start, end }
    }
}

impl<'a> RangeBounds<[u8]> for ByteRange<'a> {
    fn start_bound(&self) -> Bound<&[u8]> {
        Bound::Included(self.start)
    }
    fn end_bound(&self) -> Bound<&[u8]> {
        Bound::Included(self.end)
    }
}

// ---------------------------------------------------------------------------
// Key sizes (all fixed-width indices use stack arrays)
// ---------------------------------------------------------------------------

const CREATED_KEY_LEN: usize = 8 + 32; // timestamp(8) + event_id(32)
const AUTHOR_KEY_LEN: usize = 32 + 8 + 32; // pubkey(32) + timestamp(8) + event_id(32)
const KIND_KEY_LEN: usize = 2 + 8 + 32; // kind(2) + timestamp(8) + event_id(32)
const AUTHOR_KIND_KEY_LEN: usize = 32 + 2 + 8 + 32; // pubkey(32) + kind(2) + ts(8) + id(32)

// ---------------------------------------------------------------------------
// Replaceable event kind ranges (NIP-01)
// ---------------------------------------------------------------------------

fn is_replaceable(kind: u16) -> bool {
    kind == 0 || kind == 3 || (10_000..20_000).contains(&kind)
}

fn is_parameterized_replaceable(kind: u16) -> bool {
    (30_000..40_000).contains(&kind)
}

// ---------------------------------------------------------------------------
// LmdbStore
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct LmdbStore {
    env: Arc<Env>,
    /// Primary store: EventId(32 bytes) → Event as raw JSON bytes
    events_db: Database<Bytes, Bytes>,
    // --- Secondary indices (key-only, value = Unit) ---
    /// Timestamp(BE 8) + EventId(32) = 40 bytes
    index_created: Database<Bytes, Unit>,
    /// Pubkey(32) + Timestamp(BE 8) + EventId(32) = 72 bytes
    index_author: Database<Bytes, Unit>,
    /// Kind(BE 2) + Timestamp(BE 8) + EventId(32) = 42 bytes
    index_kind: Database<Bytes, Unit>,
    /// TagKey + 0x00 + TagValue + 0x00 + Timestamp(BE 8) + EventId(32) (variable)
    index_tag: Database<Bytes, Unit>,
    /// Pubkey(32) + Kind(BE 2) + Timestamp(BE 8) + EventId(32) = 74 bytes
    index_author_kind: Database<Bytes, Unit>,
}

// ---------------------------------------------------------------------------
// Key encoding — stack-allocated for fixed-width indices
// ---------------------------------------------------------------------------

impl LmdbStore {
    #[inline]
    fn encode_created_key(event: &Event) -> [u8; CREATED_KEY_LEN] {
        let mut key = [0u8; CREATED_KEY_LEN];
        key[..8].copy_from_slice(&event.created_at.as_u64().to_be_bytes());
        key[8..40].copy_from_slice(event.id.as_bytes());
        key
    }

    #[inline]
    fn encode_author_key(event: &Event) -> [u8; AUTHOR_KEY_LEN] {
        let mut key = [0u8; AUTHOR_KEY_LEN];
        key[..32].copy_from_slice(event.pubkey.to_bytes().as_ref());
        key[32..40].copy_from_slice(&event.created_at.as_u64().to_be_bytes());
        key[40..72].copy_from_slice(event.id.as_bytes());
        key
    }

    #[inline]
    fn encode_kind_key(event: &Event) -> [u8; KIND_KEY_LEN] {
        let mut key = [0u8; KIND_KEY_LEN];
        key[..2].copy_from_slice(&event.kind.as_u16().to_be_bytes());
        key[2..10].copy_from_slice(&event.created_at.as_u64().to_be_bytes());
        key[10..42].copy_from_slice(event.id.as_bytes());
        key
    }

    #[inline]
    fn encode_author_kind_key(event: &Event) -> [u8; AUTHOR_KIND_KEY_LEN] {
        let mut key = [0u8; AUTHOR_KIND_KEY_LEN];
        key[..32].copy_from_slice(event.pubkey.to_bytes().as_ref());
        key[32..34].copy_from_slice(&event.kind.as_u16().to_be_bytes());
        key[34..42].copy_from_slice(&event.created_at.as_u64().to_be_bytes());
        key[42..74].copy_from_slice(event.id.as_bytes());
        key
    }

    fn encode_tag_key(tag_key: &str, tag_val: &str, event: &Event) -> Vec<u8> {
        let mut key = Vec::with_capacity(tag_key.len() + 1 + tag_val.len() + 1 + 40);
        key.extend_from_slice(tag_key.as_bytes());
        key.push(0);
        key.extend_from_slice(tag_val.as_bytes());
        key.push(0);
        key.extend_from_slice(&event.created_at.as_u64().to_be_bytes());
        key.extend_from_slice(event.id.as_bytes());
        key
    }
}

// ---------------------------------------------------------------------------
// Construction
// ---------------------------------------------------------------------------

impl LmdbStore {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        fs::create_dir_all(&path)?;

        let mut env_builder = EnvOpenOptions::new();
        env_builder.max_dbs(20);
        env_builder.map_size(10 * 1024 * 1024 * 1024); // 10 GB
        let env = unsafe { env_builder.open(path)? };

        let mut wtxn = env.write_txn()?;
        let events_db = env.create_database(&mut wtxn, Some("events"))?;
        let index_created = env.create_database(&mut wtxn, Some("idx_created"))?;
        let index_author = env.create_database(&mut wtxn, Some("idx_author"))?;
        let index_kind = env.create_database(&mut wtxn, Some("idx_kind"))?;
        let index_tag = env.create_database(&mut wtxn, Some("idx_tag"))?;
        let index_author_kind = env.create_database(&mut wtxn, Some("idx_author_kind"))?;
        wtxn.commit()?;

        Ok(Self {
            env: Arc::new(env),
            events_db,
            index_created,
            index_author,
            index_kind,
            index_tag,
            index_author_kind,
        })
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

impl LmdbStore {
    /// Insert all index entries for an event inside an existing write txn.
    fn insert_indices(&self, wtxn: &mut RwTxn, event: &Event) -> Result<()> {
        self.index_created
            .put(wtxn, &Self::encode_created_key(event), &())?;
        self.index_author
            .put(wtxn, &Self::encode_author_key(event), &())?;
        self.index_kind
            .put(wtxn, &Self::encode_kind_key(event), &())?;
        self.index_author_kind
            .put(wtxn, &Self::encode_author_kind_key(event), &())?;

        for tag in event.tags.iter() {
            let tag_vec = tag.as_vec();
            if tag_vec.len() >= 2 && tag_vec[0].len() == 1 {
                let tk = Self::encode_tag_key(&tag_vec[0], &tag_vec[1], event);
                self.index_tag.put(wtxn, &tk, &())?;
            }
        }
        Ok(())
    }

    /// Remove all index entries for an event inside an existing write txn.
    fn remove_indices(&self, wtxn: &mut RwTxn, event: &Event) -> Result<()> {
        self.index_created
            .delete(wtxn, &Self::encode_created_key(event))?;
        self.index_author
            .delete(wtxn, &Self::encode_author_key(event))?;
        self.index_kind
            .delete(wtxn, &Self::encode_kind_key(event))?;
        self.index_author_kind
            .delete(wtxn, &Self::encode_author_kind_key(event))?;

        for tag in event.tags.iter() {
            let tag_vec = tag.as_vec();
            if tag_vec.len() >= 2 && tag_vec[0].len() == 1 {
                let tk = Self::encode_tag_key(&tag_vec[0], &tag_vec[1], event);
                self.index_tag.delete(wtxn, &tk)?;
            }
        }
        Ok(())
    }

    /// Delete an event by ID within an existing write txn.
    /// Returns true if an event was found and removed.
    fn delete_event_txn(&self, wtxn: &mut RwTxn, id: &[u8; 32]) -> Result<bool> {
        let raw = match self.events_db.get(wtxn, id)? {
            Some(r) => r.to_vec(), // copy out before mutating
            None => return Ok(false),
        };
        let event: Event = serde_json::from_slice(&raw)?;
        self.remove_indices(wtxn, &event)?;
        self.events_db.delete(wtxn, id)?;
        Ok(true)
    }

    /// Extract the `d` tag value from an event (for parameterized replaceable events).
    fn get_d_tag(event: &Event) -> Option<String> {
        for tag in event.tags.iter() {
            let v = tag.as_vec();
            if v.len() >= 2 && v[0] == "d" {
                return Some(v[1].clone());
            }
        }
        None
    }

    /// Handle replaceable and parameterized-replaceable events per NIP-01.
    /// If a newer event already exists, returns `true` (meaning: skip the insert).
    /// If older events exist, deletes them.
    fn handle_replaceable(&self, wtxn: &mut RwTxn, event: &Event) -> Result<bool> {
        let kind_u16 = event.kind.as_u16();

        if is_replaceable(kind_u16) {
            let start = Self::make_author_kind_range_start(&event.pubkey, &event.kind);
            let end = Self::make_author_kind_range_end(&event.pubkey, &event.kind);
            let range = ByteRange::new(&start, &end);

            let iter = self.index_author_kind.range(wtxn, &range)?;
            let mut to_delete: Vec<[u8; 32]> = Vec::new();
            let mut dominated = false;

            for result in iter {
                let (key, _) = result?;
                if key.len() < AUTHOR_KIND_KEY_LEN {
                    continue;
                }
                let existing_ts = u64::from_be_bytes(key[34..42].try_into().unwrap());
                let mut existing_id = [0u8; 32];
                existing_id.copy_from_slice(&key[42..74]);

                if existing_id == *event.id.as_bytes() {
                    continue;
                }

                if existing_ts > event.created_at.as_u64()
                    || (existing_ts == event.created_at.as_u64()
                        && existing_id > *event.id.as_bytes())
                {
                    dominated = true;
                } else {
                    to_delete.push(existing_id);
                }
            }

            if dominated {
                return Ok(true);
            }
            for id in &to_delete {
                self.delete_event_txn(wtxn, id)?;
            }
        } else if is_parameterized_replaceable(kind_u16) {
            let d_tag = Self::get_d_tag(event).unwrap_or_default();
            let start = Self::make_author_kind_range_start(&event.pubkey, &event.kind);
            let end = Self::make_author_kind_range_end(&event.pubkey, &event.kind);
            let range = ByteRange::new(&start, &end);

            // Collect keys first, then process (avoid borrow conflict)
            let entries: Vec<_> = self
                .index_author_kind
                .range(wtxn, &range)?
                .filter_map(|r| r.ok())
                .filter(|(key, _)| key.len() >= AUTHOR_KIND_KEY_LEN)
                .map(|(key, _)| {
                    let mut existing_id = [0u8; 32];
                    existing_id.copy_from_slice(&key[42..74]);
                    let existing_ts = u64::from_be_bytes(key[34..42].try_into().unwrap());
                    (existing_id, existing_ts)
                })
                .collect();

            let mut to_delete: Vec<[u8; 32]> = Vec::new();
            let mut dominated = false;

            for (existing_id, existing_ts) in entries {
                if existing_id == *event.id.as_bytes() {
                    continue;
                }

                // Fetch the existing event to check its d-tag
                if let Some(raw) = self.events_db.get(wtxn, &existing_id)? {
                    let existing: Event = serde_json::from_slice(raw)?;
                    let existing_d = Self::get_d_tag(&existing).unwrap_or_default();
                    if existing_d != d_tag {
                        continue;
                    }

                    if existing_ts > event.created_at.as_u64()
                        || (existing_ts == event.created_at.as_u64()
                            && existing_id > *event.id.as_bytes())
                    {
                        dominated = true;
                    } else {
                        to_delete.push(existing_id);
                    }
                }
            }

            if dominated {
                return Ok(true);
            }
            for id in &to_delete {
                self.delete_event_txn(wtxn, id)?;
            }
        }

        Ok(false)
    }

    /// Deserialize raw JSON bytes into an Event.
    #[inline]
    fn decode_event(raw: &[u8]) -> Result<Event> {
        Ok(serde_json::from_slice(raw)?)
    }
}

// ---------------------------------------------------------------------------
// NostrStore implementation
// ---------------------------------------------------------------------------

impl NostrStore for LmdbStore {
    fn save_event(&self, event: &Event) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;

        // Duplicate check
        let id_bytes = event.id.as_bytes();
        if self.events_db.get(&wtxn, id_bytes)?.is_some() {
            return Ok(());
        }

        // Replaceable event handling (NIP-01)
        if self.handle_replaceable(&mut wtxn, event)? {
            return Ok(());
        }

        // Serialize once, store raw JSON bytes
        let raw = serde_json::to_vec(event)?;
        self.events_db.put(&mut wtxn, id_bytes, &raw)?;

        // Write all indices
        self.insert_indices(&mut wtxn, event)?;

        wtxn.commit()?;
        Ok(())
    }

    fn get_event(&self, id: &[u8; 32]) -> Result<Option<Event>> {
        let rtxn = self.env.read_txn()?;
        match self.events_db.get(&rtxn, id)? {
            Some(raw) => Ok(Some(Self::decode_event(raw)?)),
            None => Ok(None),
        }
    }

    fn delete_event(&self, id: &[u8; 32]) -> Result<bool> {
        let mut wtxn = self.env.write_txn()?;
        let deleted = self.delete_event_txn(&mut wtxn, id)?;
        wtxn.commit()?;
        Ok(deleted)
    }

    fn query(&self, filter: &Filter) -> Result<Vec<Event>> {
        let rtxn = self.env.read_txn()?;
        let limit = filter.limit.unwrap_or(100);
        let since_ts = filter.since.map(|s| s.as_u64()).unwrap_or(0);
        let until_ts = filter.until.map(|u| u.as_u64()).unwrap_or(u64::MAX);

        // -----------------------------------------------------------------
        // 1. ID lookup — most selective
        // -----------------------------------------------------------------
        if let Some(ids) = &filter.ids {
            let mut events = Vec::with_capacity(ids.len().min(limit));
            for id in ids {
                if let Some(raw) = self.events_db.get(&rtxn, id.as_bytes())? {
                    let event = Self::decode_event(raw)?;
                    if self.event_matches_filter(&event, filter) {
                        events.push(event);
                    }
                }
            }
            events.sort_unstable_by(|a, b| b.created_at.cmp(&a.created_at));
            events.truncate(limit);
            return Ok(events);
        }

        let mut candidates: Vec<Event> = Vec::new();

        // -----------------------------------------------------------------
        // 2. Author + Kind compound index (most common Nostr query)
        // -----------------------------------------------------------------
        if let (Some(authors), Some(kinds)) = (&filter.authors, &filter.kinds) {
            for pubkey in authors {
                for kind in kinds {
                    self.scan_author_kind_index(
                        &rtxn,
                        pubkey,
                        kind,
                        since_ts,
                        until_ts,
                        limit,
                        filter,
                        &mut candidates,
                    )?;
                }
            }
        }
        // -----------------------------------------------------------------
        // 3. Author index
        // -----------------------------------------------------------------
        else if let Some(authors) = &filter.authors {
            for pubkey in authors {
                self.scan_author_index(
                    &rtxn,
                    pubkey,
                    since_ts,
                    until_ts,
                    limit,
                    filter,
                    &mut candidates,
                )?;
            }
        }
        // -----------------------------------------------------------------
        // 4. Kind index
        // -----------------------------------------------------------------
        else if let Some(kinds) = &filter.kinds {
            for kind in kinds {
                self.scan_kind_index(
                    &rtxn,
                    kind,
                    since_ts,
                    until_ts,
                    limit,
                    filter,
                    &mut candidates,
                )?;
            }
        }
        // -----------------------------------------------------------------
        // 5. Tag index
        // -----------------------------------------------------------------
        else if !filter.generic_tags.is_empty() {
            if let Some((tag_char, values)) = filter.generic_tags.iter().next() {
                let tc = tag_char.to_string();
                for value in values {
                    self.scan_tag_index(
                        &rtxn,
                        &tc,
                        value,
                        since_ts,
                        until_ts,
                        limit,
                        filter,
                        &mut candidates,
                    )?;
                }
            }
        }
        // -----------------------------------------------------------------
        // 6. Global scan (index_created)
        // -----------------------------------------------------------------
        else {
            self.scan_created_index(&rtxn, since_ts, until_ts, limit, filter, &mut candidates)?;
        }

        candidates.sort_unstable_by(|a, b| b.created_at.cmp(&a.created_at));
        candidates.truncate(limit);
        Ok(candidates)
    }
}

// ---------------------------------------------------------------------------
// Query scan helpers — each seeks directly to the `until` boundary
// Uses rev_range for reverse iteration (heed 0.20 API)
// ---------------------------------------------------------------------------

impl LmdbStore {
    fn scan_author_kind_index(
        &self,
        rtxn: &heed::RoTxn,
        pubkey: &PublicKey,
        kind: &Kind,
        since_ts: u64,
        until_ts: u64,
        limit: usize,
        filter: &Filter,
        candidates: &mut Vec<Event>,
    ) -> Result<()> {
        let mut start = [0u8; AUTHOR_KIND_KEY_LEN];
        start[..32].copy_from_slice(pubkey.to_bytes().as_ref());
        start[32..34].copy_from_slice(&kind.as_u16().to_be_bytes());
        start[34..42].copy_from_slice(&since_ts.to_be_bytes());

        let mut end = [0xffu8; AUTHOR_KIND_KEY_LEN];
        end[..32].copy_from_slice(pubkey.to_bytes().as_ref());
        end[32..34].copy_from_slice(&kind.as_u16().to_be_bytes());
        end[34..42].copy_from_slice(&until_ts.to_be_bytes());

        let range = ByteRange::new(&start, &end);
        let iter = self.index_author_kind.rev_range(rtxn, &range)?;
        let mut count = 0;

        for result in iter {
            let (key, _) = result?;
            if key.len() < AUTHOR_KIND_KEY_LEN {
                continue;
            }
            let id_bytes = &key[42..74];
            if let Some(raw) = self.events_db.get(rtxn, id_bytes)? {
                let event = Self::decode_event(raw)?;
                if self.event_matches_tags_only(&event, filter) {
                    candidates.push(event);
                    count += 1;
                }
            }
            if count >= limit {
                break;
            }
        }
        Ok(())
    }

    fn scan_author_index(
        &self,
        rtxn: &heed::RoTxn,
        pubkey: &PublicKey,
        since_ts: u64,
        until_ts: u64,
        limit: usize,
        filter: &Filter,
        candidates: &mut Vec<Event>,
    ) -> Result<()> {
        let mut start = [0u8; AUTHOR_KEY_LEN];
        start[..32].copy_from_slice(pubkey.to_bytes().as_ref());
        start[32..40].copy_from_slice(&since_ts.to_be_bytes());

        let mut end = [0xffu8; AUTHOR_KEY_LEN];
        end[..32].copy_from_slice(pubkey.to_bytes().as_ref());
        end[32..40].copy_from_slice(&until_ts.to_be_bytes());

        let range = ByteRange::new(&start, &end);
        let iter = self.index_author.rev_range(rtxn, &range)?;
        let mut count = 0;

        for result in iter {
            let (key, _) = result?;
            if key.len() < AUTHOR_KEY_LEN {
                continue;
            }
            let id_bytes = &key[40..72];
            if let Some(raw) = self.events_db.get(rtxn, id_bytes)? {
                let event = Self::decode_event(raw)?;
                if self.event_matches_no_author(&event, filter) {
                    candidates.push(event);
                    count += 1;
                }
            }
            if count >= limit {
                break;
            }
        }
        Ok(())
    }

    fn scan_kind_index(
        &self,
        rtxn: &heed::RoTxn,
        kind: &Kind,
        since_ts: u64,
        until_ts: u64,
        limit: usize,
        filter: &Filter,
        candidates: &mut Vec<Event>,
    ) -> Result<()> {
        let mut start = [0u8; KIND_KEY_LEN];
        start[..2].copy_from_slice(&kind.as_u16().to_be_bytes());
        start[2..10].copy_from_slice(&since_ts.to_be_bytes());

        let mut end = [0xffu8; KIND_KEY_LEN];
        end[..2].copy_from_slice(&kind.as_u16().to_be_bytes());
        end[2..10].copy_from_slice(&until_ts.to_be_bytes());

        let range = ByteRange::new(&start, &end);
        let iter = self.index_kind.rev_range(rtxn, &range)?;
        let mut count = 0;

        for result in iter {
            let (key, _) = result?;
            if key.len() < KIND_KEY_LEN {
                continue;
            }
            let id_bytes = &key[10..42];
            if let Some(raw) = self.events_db.get(rtxn, id_bytes)? {
                let event = Self::decode_event(raw)?;
                if self.event_matches_no_kind(&event, filter) {
                    candidates.push(event);
                    count += 1;
                }
            }
            if count >= limit {
                break;
            }
        }
        Ok(())
    }

    fn scan_tag_index(
        &self,
        rtxn: &heed::RoTxn,
        tag_key: &str,
        tag_val: &str,
        since_ts: u64,
        until_ts: u64,
        limit: usize,
        filter: &Filter,
        candidates: &mut Vec<Event>,
    ) -> Result<()> {
        let mut start = Vec::with_capacity(tag_key.len() + 1 + tag_val.len() + 1 + 40);
        start.extend_from_slice(tag_key.as_bytes());
        start.push(0);
        start.extend_from_slice(tag_val.as_bytes());
        start.push(0);
        start.extend_from_slice(&since_ts.to_be_bytes());
        start.extend_from_slice(&[0u8; 32]);

        let mut end = Vec::with_capacity(tag_key.len() + 1 + tag_val.len() + 1 + 40);
        end.extend_from_slice(tag_key.as_bytes());
        end.push(0);
        end.extend_from_slice(tag_val.as_bytes());
        end.push(0);
        end.extend_from_slice(&until_ts.to_be_bytes());
        end.extend_from_slice(&[0xffu8; 32]);

        let range = ByteRange::new(&start, &end);
        let iter = self.index_tag.rev_range(rtxn, &range)?;
        let mut count = 0;

        for result in iter {
            let (key, _) = result?;
            if key.len() < 40 {
                continue;
            }
            let id_bytes = &key[key.len() - 32..];
            if let Some(raw) = self.events_db.get(rtxn, id_bytes)? {
                let event = Self::decode_event(raw)?;
                if self.event_matches_filter(&event, filter) {
                    candidates.push(event);
                    count += 1;
                }
            }
            if count >= limit {
                break;
            }
        }
        Ok(())
    }

    fn scan_created_index(
        &self,
        rtxn: &heed::RoTxn,
        since_ts: u64,
        until_ts: u64,
        limit: usize,
        filter: &Filter,
        candidates: &mut Vec<Event>,
    ) -> Result<()> {
        let mut start = [0u8; CREATED_KEY_LEN];
        start[..8].copy_from_slice(&since_ts.to_be_bytes());

        let mut end = [0xffu8; CREATED_KEY_LEN];
        end[..8].copy_from_slice(&until_ts.to_be_bytes());

        let range = ByteRange::new(&start, &end);
        let iter = self.index_created.rev_range(rtxn, &range)?;
        let mut count = 0;

        for result in iter {
            let (key, _) = result?;
            if key.len() < CREATED_KEY_LEN {
                continue;
            }
            let id_bytes = &key[8..40];
            if let Some(raw) = self.events_db.get(rtxn, id_bytes)? {
                let event = Self::decode_event(raw)?;
                if self.event_matches_filter(&event, filter) {
                    candidates.push(event);
                    count += 1;
                }
            }
            if count >= limit {
                break;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Filter matching — targeted variants that skip the indexed dimension
// ---------------------------------------------------------------------------

impl LmdbStore {
    /// Full filter match (used when no index dimension can be skipped).
    fn event_matches_filter(&self, event: &Event, filter: &Filter) -> bool {
        if let Some(ids) = &filter.ids {
            if !ids.contains(&event.id) {
                return false;
            }
        }
        if let Some(kinds) = &filter.kinds {
            if !kinds.contains(&event.kind) {
                return false;
            }
        }
        if let Some(authors) = &filter.authors {
            if !authors.contains(&event.pubkey) {
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
        self.check_tags(event, filter)
    }

    /// Skips author check (used when scanning author index).
    fn event_matches_no_author(&self, event: &Event, filter: &Filter) -> bool {
        if let Some(kinds) = &filter.kinds {
            if !kinds.contains(&event.kind) {
                return false;
            }
        }
        self.check_tags(event, filter)
    }

    /// Skips kind check (used when scanning kind index).
    fn event_matches_no_kind(&self, event: &Event, filter: &Filter) -> bool {
        if let Some(authors) = &filter.authors {
            if !authors.contains(&event.pubkey) {
                return false;
            }
        }
        self.check_tags(event, filter)
    }

    /// Skips author + kind + time checks (compound author_kind index with time in range).
    fn event_matches_tags_only(&self, event: &Event, filter: &Filter) -> bool {
        self.check_tags(event, filter)
    }

    /// Check generic_tags portion of the filter.
    fn check_tags(&self, event: &Event, filter: &Filter) -> bool {
        for (tag_char, allowed_values) in &filter.generic_tags {
            let char_key = tag_char.to_string();
            let mut found = false;
            for t in &event.tags {
                let t_vec = t.as_vec();
                if t_vec.len() >= 2 && t_vec[0] == char_key {
                    if allowed_values.contains(&t_vec[1]) {
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                return false;
            }
        }
        true
    }
}

// ---------------------------------------------------------------------------
// Range key helpers for compound author+kind index
// ---------------------------------------------------------------------------

impl LmdbStore {
    fn make_author_kind_range_start(pubkey: &PublicKey, kind: &Kind) -> [u8; AUTHOR_KIND_KEY_LEN] {
        let mut key = [0u8; AUTHOR_KIND_KEY_LEN];
        key[..32].copy_from_slice(pubkey.to_bytes().as_ref());
        key[32..34].copy_from_slice(&kind.as_u16().to_be_bytes());
        // remaining 40 bytes are 0x00 (min timestamp + min id)
        key
    }

    fn make_author_kind_range_end(pubkey: &PublicKey, kind: &Kind) -> [u8; AUTHOR_KIND_KEY_LEN] {
        let mut key = [0xffu8; AUTHOR_KIND_KEY_LEN];
        key[..32].copy_from_slice(pubkey.to_bytes().as_ref());
        key[32..34].copy_from_slice(&kind.as_u16().to_be_bytes());
        // remaining 40 bytes are 0xff (max timestamp + max id)
        key
    }
}
