use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoarConfig {
    pub domain: String,
    pub port: u16,
    /// Hex pubkey of the admin allowed to access the admin UI.
    /// Only this pubkey can log in via NIP-98 auth.
    pub admin_pubkey: String,
    /// Directory for custom relay home pages (default: "pages").
    /// Each relay can have a `{relay_id}.html` file in this directory.
    #[serde(default = "default_pages_dir")]
    pub pages_dir: String,
    #[serde(default)]
    pub discovery_relays: Vec<String>,
    #[serde(default)]
    pub wots: HashMap<String, WotConfig>,
    pub relays: HashMap<String, RelayConfig>,
    #[serde(default)]
    pub blossoms: HashMap<String, BlossomConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WotConfig {
    pub seed: String,
    #[serde(default = "default_wot_depth")]
    pub depth: u8,
    #[serde(default = "default_update_interval")]
    pub update_interval_hours: u64,
}

fn default_wot_depth() -> u8 {
    1
}

fn default_update_interval() -> u64 {
    24
}

fn default_pages_dir() -> String {
    "pages".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayConfig {
    pub name: String,
    pub description: Option<String>,
    pub subdomain: String,
    pub db_path: String,
    #[serde(default)]
    pub policy: PolicyConfig,
    #[serde(default)]
    pub nip11: Nip11Config,
}

/// Optional NIP-11 relay information fields and limit overrides.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Nip11Config {
    pub icon: Option<String>,
    pub banner: Option<String>,
    pub contact: Option<String>,
    pub terms_of_service: Option<String>,
    pub max_message_length: Option<u64>,
    pub max_subscriptions: Option<u64>,
    pub max_subid_length: Option<u64>,
    pub max_limit: Option<u64>,
    pub max_event_tags: Option<u64>,
    pub default_limit: Option<u64>,
    pub created_at_lower_limit: Option<u64>,
    pub created_at_upper_limit: Option<u64>,
}

/// Composable policy configuration — every field is optional and defaults to
/// the most permissive value.  Users only specify what they want to restrict.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PolicyConfig {
    #[serde(default)]
    pub write: WritePolicy,
    #[serde(default)]
    pub read: ReadPolicy,
    #[serde(default)]
    pub events: EventPolicy,
    pub rate_limit: Option<RateLimitConfig>,
}

/// Controls who is allowed to publish events (EVENT messages).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WritePolicy {
    /// If true, the client must complete NIP-42 AUTH before sending EVENTs.
    #[serde(default)]
    pub require_auth: bool,
    /// If set, only these pubkeys may write.  `None` = anyone can write.
    pub allowed_pubkeys: Option<Vec<String>>,
    /// If set, these pubkeys are explicitly blocked from writing.
    pub blocked_pubkeys: Option<Vec<String>>,
    /// If set, events are only accepted if they contain a `p` tag referencing
    /// one of these pubkeys.  Useful for inbox/DM relays.
    pub tagged_pubkeys: Option<Vec<String>>,
    /// If set, only pubkeys in the referenced Web of Trust are allowed to write.
    pub wot: Option<String>,
}

impl Default for WritePolicy {
    fn default() -> Self {
        Self {
            require_auth: false,
            allowed_pubkeys: None,
            blocked_pubkeys: None,
            tagged_pubkeys: None,
            wot: None,
        }
    }
}

/// Controls who is allowed to query events (REQ messages).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadPolicy {
    /// If true, the client must complete NIP-42 AUTH before sending REQs.
    #[serde(default)]
    pub require_auth: bool,
    /// If set, only these pubkeys may read.  `None` = anyone can read.
    pub allowed_pubkeys: Option<Vec<String>>,
    /// If set, only pubkeys in the referenced Web of Trust are allowed to read.
    pub wot: Option<String>,
}

impl Default for ReadPolicy {
    fn default() -> Self {
        Self {
            require_auth: false,
            allowed_pubkeys: None,
            wot: None,
        }
    }
}

/// Controls which events are accepted based on their content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPolicy {
    /// If set, only these event kinds are accepted.
    pub allowed_kinds: Option<Vec<u64>>,
    /// If set, these event kinds are rejected.
    pub blocked_kinds: Option<Vec<u64>>,
    /// Minimum proof-of-work difficulty bits required (NIP-13).
    pub min_pow: Option<u8>,
    /// Maximum `content` field length in bytes.
    pub max_content_length: Option<usize>,
}

impl Default for EventPolicy {
    fn default() -> Self {
        Self {
            allowed_kinds: None,
            blocked_kinds: None,
            min_pow: None,
            max_content_length: None,
        }
    }
}

/// Per-relay rate limiting configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub writes_per_minute: Option<u32>,
    pub reads_per_minute: Option<u32>,
}

// ---------------------------------------------------------------------------
// Blossom (media server) configuration
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlossomConfig {
    pub name: String,
    pub description: Option<String>,
    pub subdomain: String,
    pub storage_path: String,
    #[serde(default)]
    pub policy: BlossomPolicyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlossomPolicyConfig {
    #[serde(default)]
    pub upload: BlossomUploadPolicy,
    #[serde(default)]
    pub list: BlossomListPolicy,
    pub max_file_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlossomUploadPolicy {
    pub allowed_pubkeys: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlossomListPolicy {
    #[serde(default)]
    pub require_auth: bool,
    pub allowed_pubkeys: Option<Vec<String>>,
}
