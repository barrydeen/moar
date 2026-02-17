use crate::error::Result;
use nostr::{Event, Filter};

pub trait NostrStore: Send + Sync {
    fn save_event(&self, event: &Event) -> Result<()>;
    fn get_event(&self, id: &[u8; 32]) -> Result<Option<Event>>;
    fn delete_event(&self, id: &[u8; 32]) -> Result<bool>;
    fn query(&self, filter: &Filter) -> Result<Vec<Event>>;
    fn iter_all(&self) -> Result<Vec<Event>>;
    fn event_count(&self) -> Result<u64>;
    fn db_path(&self) -> &str;
}

pub mod lmdb;
