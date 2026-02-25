use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{
    doc, Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument,
};
use tokio::sync::Mutex;

const DEFAULT_SEARCHABLE_KINDS: &[u64] = &[0, 1, 30023, 30024];

#[derive(Clone)]
pub struct SearchSchema {
    pub event_id: Field,
    pub content: Field,
    pub pubkey: Field,
    pub kind: Field,
    pub created_at: Field,
    pub wot_tier: Field,
}

pub struct SearchIndex {
    index: Index,
    schema: SearchSchema,
    writer: Arc<Mutex<IndexWriter>>,
    reader: IndexReader,
    searchable_kinds: Vec<u64>,
}

impl SearchIndex {
    pub fn new(path: &str, heap_size_mb: usize) -> Result<Self, anyhow::Error> {
        Self::new_with_kinds(path, heap_size_mb, None)
    }

    pub fn new_with_kinds(
        path: &str,
        heap_size_mb: usize,
        kinds: Option<&[u64]>,
    ) -> Result<Self, anyhow::Error> {
        let dir = Path::new(path);
        std::fs::create_dir_all(dir)?;

        let mut schema_builder = Schema::builder();

        let event_id = schema_builder.add_text_field("event_id", STRING | STORED);
        let content = schema_builder.add_text_field("content", TEXT);
        let pubkey = schema_builder.add_text_field("pubkey", STRING);
        let kind = schema_builder.add_u64_field("kind", INDEXED | FAST);
        let created_at = schema_builder.add_u64_field("created_at", INDEXED | FAST);
        let wot_tier = schema_builder.add_u64_field("wot_tier", FAST);

        let schema = schema_builder.build();

        let index = Index::open_or_create(
            tantivy::directory::MmapDirectory::open(dir)?,
            schema.clone(),
        )?;

        let writer = index.writer(heap_size_mb * 1_000_000)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        let searchable_kinds = kinds
            .map(|k| k.to_vec())
            .unwrap_or_else(|| DEFAULT_SEARCHABLE_KINDS.to_vec());

        Ok(Self {
            index,
            schema: SearchSchema {
                event_id,
                content,
                pubkey,
                kind,
                created_at,
                wot_tier,
            },
            writer: Arc::new(Mutex::new(writer)),
            reader,
            searchable_kinds,
        })
    }

    pub fn is_searchable_kind(&self, kind: u64) -> bool {
        self.searchable_kinds.contains(&kind)
    }

    pub async fn add_event(
        &self,
        event_id_hex: &str,
        content: &str,
        pubkey_hex: &str,
        kind: u64,
        created_at: u64,
        wot_tier: u8,
    ) -> Result<(), anyhow::Error> {
        let writer = self.writer.lock().await;
        writer.add_document(doc!(
            self.schema.event_id => event_id_hex,
            self.schema.content => content,
            self.schema.pubkey => pubkey_hex,
            self.schema.kind => kind,
            self.schema.created_at => created_at,
            self.schema.wot_tier => wot_tier as u64,
        ))?;
        Ok(())
    }

    pub async fn delete_event(&self, event_id_hex: &str) -> Result<(), anyhow::Error> {
        let term = tantivy::Term::from_field_text(self.schema.event_id, event_id_hex);
        let writer = self.writer.lock().await;
        writer.delete_term(term);
        Ok(())
    }

    pub async fn commit(&self) -> Result<(), anyhow::Error> {
        let mut writer = self.writer.lock().await;
        writer.commit()?;
        Ok(())
    }

    pub fn search(
        &self,
        query_str: &str,
        limit: usize,
    ) -> Result<Vec<(String, f32)>, anyhow::Error> {
        let searcher = self.reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, vec![self.schema.content]);
        let query = query_parser.parse_query(query_str)?;

        let now_secs = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit * 3))?;

        let mut scored: Vec<(String, f32)> = Vec::new();

        for (bm25_score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;

            let event_id = doc
                .get_first(self.schema.event_id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let wot_tier = doc
                .get_first(self.schema.wot_tier)
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            let created_at = doc
                .get_first(self.schema.created_at)
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            let age_days = if now_secs > created_at {
                (now_secs - created_at) as f32 / 86400.0
            } else {
                0.0
            };

            let wot_boost = wot_tier as f32 * 0.5;
            let recency_boost = (1.0 - age_days / 365.0).max(0.0);
            let final_score = bm25_score * (1.0 + wot_boost + recency_boost);

            scored.push((event_id, final_score));
        }

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        Ok(scored)
    }

    pub async fn reindex_wot_tiers(
        &self,
        depth_map: &std::collections::HashMap<nostr::PublicKey, u8>,
    ) -> Result<u64, anyhow::Error> {
        let searcher = self.reader.searcher();
        let mut updated = 0u64;

        for segment_reader in searcher.segment_readers() {
            let store_reader = segment_reader.get_store_reader(64)?;
            let alive_bitset = segment_reader.alive_bitset();

            for doc_id in 0..segment_reader.max_doc() {
                if let Some(ref bitset) = alive_bitset {
                    if !bitset.is_alive(doc_id) {
                        continue;
                    }
                }

                let doc: TantivyDocument = store_reader.get(doc_id)?;

                let event_id_hex = doc
                    .get_first(self.schema.event_id)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let pubkey_hex = doc
                    .get_first(self.schema.pubkey)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let content = doc
                    .get_first(self.schema.content)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let kind = doc
                    .get_first(self.schema.kind)
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                let created_at = doc
                    .get_first(self.schema.created_at)
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                let old_tier = doc
                    .get_first(self.schema.wot_tier)
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u8;

                let new_tier = if let Ok(pk) = nostr::PublicKey::parse(&pubkey_hex) {
                    depth_map.get(&pk).copied().unwrap_or(0)
                } else {
                    0
                };

                if new_tier != old_tier {
                    let term = tantivy::Term::from_field_text(
                        self.schema.event_id,
                        &event_id_hex,
                    );
                    let writer = self.writer.lock().await;
                    writer.delete_term(term);
                    writer.add_document(doc!(
                        self.schema.event_id => event_id_hex,
                        self.schema.content => content,
                        self.schema.pubkey => pubkey_hex,
                        self.schema.kind => kind,
                        self.schema.created_at => created_at,
                        self.schema.wot_tier => new_tier as u64,
                    ))?;
                    updated += 1;
                }
            }
        }

        self.commit().await?;
        Ok(updated)
    }

    pub fn event_count(&self) -> u64 {
        let searcher = self.reader.searcher();
        searcher.num_docs()
    }
}
