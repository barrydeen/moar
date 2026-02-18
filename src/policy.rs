use nostr::{Event, Filter, Kind, PublicKey};
use std::collections::HashSet;
use std::str::FromStr;

use crate::config::{Nip11Config, PolicyConfig};
use crate::paywall::PaywallSet;
use crate::wot::WotSet;

/// Result of a policy check.
#[derive(Debug, Clone)]
pub enum PolicyResult {
    /// The action is allowed.
    Allow,
    /// The action is denied, with a human-readable reason.
    Deny(String),
    /// The client must complete NIP-42 AUTH before retrying.
    AuthRequired,
}

impl PolicyResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, PolicyResult::Allow)
    }
}

/// A rule-based policy engine constructed from a [`PolicyConfig`].
///
/// All access-control decisions flow through this struct — there are no
/// hard-coded relay "types".  Each relay gets its own `PolicyEngine` built
/// from whatever rules were declared in the TOML config.
pub struct PolicyEngine {
    config: PolicyConfig,
    nip11: Nip11Config,
    write_allowed: Option<HashSet<PublicKey>>,
    write_blocked: Option<HashSet<PublicKey>>,
    write_tagged: Option<HashSet<PublicKey>>,
    read_allowed: Option<HashSet<PublicKey>>,
    allowed_kinds: Option<HashSet<Kind>>,
    blocked_kinds: Option<HashSet<Kind>>,
    write_wot: Option<WotSet>,
    read_wot: Option<WotSet>,
    write_paywall: Option<PaywallSet>,
    read_paywall: Option<PaywallSet>,
}

impl PolicyEngine {
    pub fn new(
        config: PolicyConfig,
        nip11: Nip11Config,
        write_wot: Option<WotSet>,
        read_wot: Option<WotSet>,
        write_paywall: Option<PaywallSet>,
        read_paywall: Option<PaywallSet>,
    ) -> Self {
        let write_allowed = config
            .write
            .allowed_pubkeys
            .as_ref()
            .map(|keys| parse_pubkeys(keys));

        let write_blocked = config
            .write
            .blocked_pubkeys
            .as_ref()
            .map(|keys| parse_pubkeys(keys));

        let write_tagged = config
            .write
            .tagged_pubkeys
            .as_ref()
            .map(|keys| parse_pubkeys(keys));

        let read_allowed = config
            .read
            .allowed_pubkeys
            .as_ref()
            .map(|keys| parse_pubkeys(keys));

        let allowed_kinds = config
            .events
            .allowed_kinds
            .as_ref()
            .map(|kinds| kinds.iter().map(|&k| Kind::from(k as u16)).collect());

        let blocked_kinds = config
            .events
            .blocked_kinds
            .as_ref()
            .map(|kinds| kinds.iter().map(|&k| Kind::from(k as u16)).collect());

        Self {
            config,
            nip11,
            write_allowed,
            write_blocked,
            write_tagged,
            read_allowed,
            allowed_kinds,
            blocked_kinds,
            write_wot,
            read_wot,
            write_paywall,
            read_paywall,
        }
    }

    /// Check whether an event may be written to this relay.
    ///
    /// `authed_pubkey` is the pubkey that completed NIP-42 AUTH on this
    /// connection, or `None` if the client has not authenticated.
    pub fn can_write(&self, event: &Event, authed_pubkey: Option<&PublicKey>) -> PolicyResult {
        // Auth gate
        if self.config.write.require_auth {
            if authed_pubkey.is_none() {
                return PolicyResult::AuthRequired;
            }
        }

        // Pubkey allow-list (checked against event author)
        if let Some(ref allowed) = self.write_allowed {
            if !allowed.contains(&event.pubkey) {
                return PolicyResult::Deny("pubkey not on write allow-list".into());
            }
        }

        // Web of Trust check (checked against event author, no auth needed)
        if let Some(ref wot) = self.write_wot {
            if !wot.contains(&event.pubkey) {
                return PolicyResult::Deny("pubkey not in web of trust".into());
            }
        }

        // Paywall check (checked against event author, no auth needed)
        if let Some(ref paywall) = self.write_paywall {
            if !paywall.contains(&event.pubkey) {
                return PolicyResult::Deny("payment required for write access".into());
            }
        }

        // Pubkey block-list
        if let Some(ref blocked) = self.write_blocked {
            if blocked.contains(&event.pubkey) {
                return PolicyResult::Deny("pubkey is blocked".into());
            }
        }

        // Tagged pubkeys — event must contain a `p` tag referencing one of these
        if let Some(ref tagged) = self.write_tagged {
            let has_matching_tag = event.tags.iter().any(|tag| {
                let tag_vec = tag.as_vec();
                if tag_vec.len() >= 2 && tag_vec[0] == "p" {
                    if let Ok(pk) = PublicKey::from_str(&tag_vec[1])
                        .or_else(|_| PublicKey::parse(&tag_vec[1]))
                    {
                        return tagged.contains(&pk);
                    }
                }
                false
            });
            if !has_matching_tag {
                return PolicyResult::Deny("event must tag an approved pubkey".into());
            }
        }

        // Kind allow-list
        if let Some(ref allowed) = self.allowed_kinds {
            if !allowed.contains(&event.kind) {
                return PolicyResult::Deny(format!("kind {} not allowed", event.kind.as_u16()));
            }
        }

        // Kind block-list
        if let Some(ref blocked) = self.blocked_kinds {
            if blocked.contains(&event.kind) {
                return PolicyResult::Deny(format!("kind {} is blocked", event.kind.as_u16()));
            }
        }

        // Content length
        if let Some(max_len) = self.config.events.max_content_length {
            if event.content.len() > max_len {
                return PolicyResult::Deny(format!(
                    "content too long ({} > {})",
                    event.content.len(),
                    max_len
                ));
            }
        }

        // PoW — NIP-13: count leading zero bits of the event ID
        if let Some(min_pow) = self.config.events.min_pow {
            let pow = leading_zero_bits(event.id.as_bytes());
            if pow < min_pow {
                return PolicyResult::Deny(format!("insufficient PoW ({} < {})", pow, min_pow));
            }
        }

        // NIP-11: max event tags
        if let Some(max_tags) = self.nip11.max_event_tags {
            if event.tags.len() as u64 > max_tags {
                return PolicyResult::Deny(format!(
                    "too many tags ({} > {})",
                    event.tags.len(),
                    max_tags
                ));
            }
        }

        // NIP-11: created_at lower limit (reject events too far in the past)
        if let Some(lower) = self.nip11.created_at_lower_limit {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            if event.created_at.as_u64() < now.saturating_sub(lower) {
                return PolicyResult::Deny("event created_at too far in the past".into());
            }
        }

        // NIP-11: created_at upper limit (reject events too far in the future)
        if let Some(upper) = self.nip11.created_at_upper_limit {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            if event.created_at.as_u64() > now + upper {
                return PolicyResult::Deny("event created_at too far in the future".into());
            }
        }

        PolicyResult::Allow
    }

    /// Check whether a REQ query is allowed on this relay.
    pub fn can_read(&self, _filter: &Filter, authed_pubkey: Option<&PublicKey>) -> PolicyResult {
        // Auth gate
        if self.config.read.require_auth {
            if authed_pubkey.is_none() {
                return PolicyResult::AuthRequired;
            }
        }

        // Pubkey allow-list (checked against authenticated identity)
        if let Some(ref allowed) = self.read_allowed {
            match authed_pubkey {
                Some(pk) if allowed.contains(pk) => {}
                _ => return PolicyResult::Deny("pubkey not on read allow-list".into()),
            }
        }

        // Web of Trust check (requires auth to identify reader)
        if let Some(ref wot) = self.read_wot {
            match authed_pubkey {
                Some(pk) if wot.contains(pk) => {}
                Some(_) => {
                    return PolicyResult::Deny("pubkey not in web of trust".into())
                }
                None => return PolicyResult::AuthRequired,
            }
        }

        // Paywall check (requires auth to identify reader)
        if let Some(ref paywall) = self.read_paywall {
            match authed_pubkey {
                Some(pk) if paywall.contains(pk) => {}
                Some(_) => {
                    return PolicyResult::Deny("payment required for read access".into())
                }
                None => return PolicyResult::AuthRequired,
            }
        }

        PolicyResult::Allow
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse a list of hex or npub strings into a set of [`PublicKey`]s,
/// silently skipping any that fail to parse.
fn parse_pubkeys(keys: &[String]) -> HashSet<PublicKey> {
    keys.iter()
        .filter_map(|s| {
            // Try bech32 (npub...) first, then hex
            PublicKey::from_str(s).or_else(|_| PublicKey::parse(s)).ok()
        })
        .collect()
}

/// Count leading zero bits of a byte slice (NIP-13 PoW).
fn leading_zero_bits(bytes: &[u8]) -> u8 {
    let mut count: u8 = 0;
    for &b in bytes {
        if b == 0 {
            count += 8;
        } else {
            count += b.leading_zeros() as u8;
            break;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EventPolicy, Nip11Config, PolicyConfig, ReadPolicy, WritePolicy};
    use nostr::nips::nip19::ToBech32;
    use nostr::{EventBuilder, Keys, Kind};

    fn default_nip11() -> Nip11Config {
        // Use permissive defaults for existing tests so they don't trip over
        // the new NIP-11 enforcement.
        Nip11Config {
            max_event_tags: None,
            created_at_lower_limit: None,
            created_at_upper_limit: None,
            ..Default::default()
        }
    }

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn make_event(keys: &Keys, content: &str) -> Event {
        EventBuilder::text_note(content, [])
            .to_event(keys)
            .unwrap()
    }

    fn make_event_kind(keys: &Keys, kind: u16, content: &str) -> Event {
        EventBuilder::new(Kind::from(kind), content, [])
            .to_event(keys)
            .unwrap()
    }

    fn open_policy() -> PolicyConfig {
        PolicyConfig::default()
    }

    fn hex_pubkey(keys: &Keys) -> String {
        keys.public_key().to_string()
    }

    fn npub_pubkey(keys: &Keys) -> String {
        keys.public_key().to_bech32().unwrap()
    }

    // -----------------------------------------------------------------------
    // Individual rule tests — can_write
    // -----------------------------------------------------------------------

    #[test]
    fn default_open_policy_allows_write() {
        let keys = Keys::generate();
        let event = make_event(&keys, "hello");
        let engine = PolicyEngine::new(open_policy(), default_nip11(), None, None, None, None);
        assert!(engine.can_write(&event, None).is_allowed());
    }

    #[test]
    fn default_open_policy_allows_read() {
        let engine = PolicyEngine::new(open_policy(), default_nip11(), None, None, None, None);
        let filter = Filter::new();
        assert!(engine.can_read(&filter, None).is_allowed());
    }

    #[test]
    fn require_auth_write_no_auth() {
        let keys = Keys::generate();
        let event = make_event(&keys, "hello");
        let policy = PolicyConfig {
            write: WritePolicy {
                require_auth: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(matches!(
            engine.can_write(&event, None),
            PolicyResult::AuthRequired
        ));
    }

    #[test]
    fn require_auth_write_with_auth() {
        let keys = Keys::generate();
        let event = make_event(&keys, "hello");
        let pk = keys.public_key();
        let policy = PolicyConfig {
            write: WritePolicy {
                require_auth: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(engine.can_write(&event, Some(&pk)).is_allowed());
    }

    #[test]
    fn require_auth_read_no_auth() {
        let policy = PolicyConfig {
            read: ReadPolicy {
                require_auth: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        let filter = Filter::new();
        assert!(matches!(
            engine.can_read(&filter, None),
            PolicyResult::AuthRequired
        ));
    }

    #[test]
    fn require_auth_read_with_auth() {
        let keys = Keys::generate();
        let pk = keys.public_key();
        let policy = PolicyConfig {
            read: ReadPolicy {
                require_auth: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        let filter = Filter::new();
        assert!(engine.can_read(&filter, Some(&pk)).is_allowed());
    }

    #[test]
    fn write_allow_list_permits_listed_hex() {
        let keys = Keys::generate();
        let event = make_event(&keys, "hello");
        let policy = PolicyConfig {
            write: WritePolicy {
                allowed_pubkeys: Some(vec![hex_pubkey(&keys)]),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(engine.can_write(&event, None).is_allowed());
    }

    #[test]
    fn write_allow_list_permits_listed_npub() {
        let keys = Keys::generate();
        let event = make_event(&keys, "hello");
        let policy = PolicyConfig {
            write: WritePolicy {
                allowed_pubkeys: Some(vec![npub_pubkey(&keys)]),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(engine.can_write(&event, None).is_allowed());
    }

    #[test]
    fn write_allow_list_rejects_unlisted() {
        let allowed_keys = Keys::generate();
        let other_keys = Keys::generate();
        let event = make_event(&other_keys, "hello");
        let policy = PolicyConfig {
            write: WritePolicy {
                allowed_pubkeys: Some(vec![hex_pubkey(&allowed_keys)]),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(matches!(
            engine.can_write(&event, None),
            PolicyResult::Deny(ref s) if s.contains("allow-list")
        ));
    }

    #[test]
    fn write_block_list_rejects_listed() {
        let keys = Keys::generate();
        let event = make_event(&keys, "hello");
        let policy = PolicyConfig {
            write: WritePolicy {
                blocked_pubkeys: Some(vec![hex_pubkey(&keys)]),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(matches!(
            engine.can_write(&event, None),
            PolicyResult::Deny(ref s) if s.contains("blocked")
        ));
    }

    #[test]
    fn write_block_list_permits_unlisted() {
        let blocked_keys = Keys::generate();
        let other_keys = Keys::generate();
        let event = make_event(&other_keys, "hello");
        let policy = PolicyConfig {
            write: WritePolicy {
                blocked_pubkeys: Some(vec![hex_pubkey(&blocked_keys)]),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(engine.can_write(&event, None).is_allowed());
    }

    #[test]
    fn kind_allow_list_permits_listed() {
        let keys = Keys::generate();
        let event = make_event(&keys, "hello"); // kind 1
        let policy = PolicyConfig {
            events: EventPolicy {
                allowed_kinds: Some(vec![1]),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(engine.can_write(&event, None).is_allowed());
    }

    #[test]
    fn kind_allow_list_rejects_unlisted() {
        let keys = Keys::generate();
        let event = make_event_kind(&keys, 4, "hello"); // kind 4
        let policy = PolicyConfig {
            events: EventPolicy {
                allowed_kinds: Some(vec![1]),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(matches!(
            engine.can_write(&event, None),
            PolicyResult::Deny(ref s) if s.contains("not allowed")
        ));
    }

    #[test]
    fn kind_allow_list_multiple_kinds() {
        let keys = Keys::generate();
        let event1 = make_event(&keys, "hello"); // kind 1
        let event4 = make_event_kind(&keys, 4, "hello");
        let event7 = make_event_kind(&keys, 7, "hello");
        let policy = PolicyConfig {
            events: EventPolicy {
                allowed_kinds: Some(vec![1, 4]),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(engine.can_write(&event1, None).is_allowed());
        assert!(engine.can_write(&event4, None).is_allowed());
        assert!(!engine.can_write(&event7, None).is_allowed());
    }

    #[test]
    fn kind_block_list_rejects_listed() {
        let keys = Keys::generate();
        let event = make_event(&keys, "hello"); // kind 1
        let policy = PolicyConfig {
            events: EventPolicy {
                blocked_kinds: Some(vec![1]),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(matches!(
            engine.can_write(&event, None),
            PolicyResult::Deny(ref s) if s.contains("blocked")
        ));
    }

    #[test]
    fn kind_block_list_permits_unlisted() {
        let keys = Keys::generate();
        let event = make_event_kind(&keys, 4, "hello"); // kind 4
        let policy = PolicyConfig {
            events: EventPolicy {
                blocked_kinds: Some(vec![1]),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(engine.can_write(&event, None).is_allowed());
    }

    #[test]
    fn max_content_length_at_limit() {
        let keys = Keys::generate();
        let content = "x".repeat(10);
        let event = make_event(&keys, &content);
        let policy = PolicyConfig {
            events: EventPolicy {
                max_content_length: Some(10),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(engine.can_write(&event, None).is_allowed());
    }

    #[test]
    fn max_content_length_over_by_one() {
        let keys = Keys::generate();
        let content = "x".repeat(11);
        let event = make_event(&keys, &content);
        let policy = PolicyConfig {
            events: EventPolicy {
                max_content_length: Some(10),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(matches!(
            engine.can_write(&event, None),
            PolicyResult::Deny(ref s) if s.contains("too long")
        ));
    }

    #[test]
    fn min_pow_rejects_insufficient() {
        let keys = Keys::generate();
        let event = make_event(&keys, "hello");
        // Require 128 bits of PoW — virtually impossible for a random event
        let policy = PolicyConfig {
            events: EventPolicy {
                min_pow: Some(128),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(matches!(
            engine.can_write(&event, None),
            PolicyResult::Deny(ref s) if s.contains("PoW")
        ));
    }

    #[test]
    fn min_pow_zero_allows() {
        let keys = Keys::generate();
        let event = make_event(&keys, "hello");
        let policy = PolicyConfig {
            events: EventPolicy {
                min_pow: Some(0),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(engine.can_write(&event, None).is_allowed());
    }

    // -----------------------------------------------------------------------
    // leading_zero_bits helper
    // -----------------------------------------------------------------------

    #[test]
    fn leading_zero_bits_all_zeros() {
        assert_eq!(leading_zero_bits(&[0, 0, 0, 0]), 32);
    }

    #[test]
    fn leading_zero_bits_first_byte_0x80() {
        assert_eq!(leading_zero_bits(&[0x80, 0, 0, 0]), 0);
    }

    #[test]
    fn leading_zero_bits_first_byte_0x01() {
        assert_eq!(leading_zero_bits(&[0x01, 0, 0, 0]), 7);
    }

    #[test]
    fn leading_zero_bits_second_byte_0x01() {
        assert_eq!(leading_zero_bits(&[0x00, 0x01, 0, 0]), 15);
    }

    #[test]
    fn leading_zero_bits_empty() {
        assert_eq!(leading_zero_bits(&[]), 0);
    }

    // -----------------------------------------------------------------------
    // Policy combination matrix
    // -----------------------------------------------------------------------

    #[test]
    fn combo_pubkey_on_both_allow_and_block() {
        // Pubkey is on both allow-list and block-list → Deny (allow passes, block catches)
        let keys = Keys::generate();
        let event = make_event(&keys, "hello");
        let pk_hex = hex_pubkey(&keys);
        let policy = PolicyConfig {
            write: WritePolicy {
                allowed_pubkeys: Some(vec![pk_hex.clone()]),
                blocked_pubkeys: Some(vec![pk_hex]),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(matches!(
            engine.can_write(&event, None),
            PolicyResult::Deny(_)
        ));
    }

    #[test]
    fn combo_kind_on_both_allowed_and_blocked() {
        // Kind on both allowed + blocked → Deny
        let keys = Keys::generate();
        let event = make_event(&keys, "hello"); // kind 1
        let policy = PolicyConfig {
            events: EventPolicy {
                allowed_kinds: Some(vec![1]),
                blocked_kinds: Some(vec![1]),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(matches!(
            engine.can_write(&event, None),
            PolicyResult::Deny(_)
        ));
    }

    #[test]
    fn combo_require_auth_no_auth_short_circuits() {
        // require_auth + no auth → AuthRequired (other rules irrelevant)
        let keys = Keys::generate();
        let event = make_event(&keys, "hello");
        let policy = PolicyConfig {
            write: WritePolicy {
                require_auth: true,
                allowed_pubkeys: Some(vec![hex_pubkey(&keys)]),
                ..Default::default()
            },
            events: EventPolicy {
                allowed_kinds: Some(vec![1]),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(matches!(
            engine.can_write(&event, None),
            PolicyResult::AuthRequired
        ));
    }

    #[test]
    fn combo_require_auth_authed_but_pubkey_not_on_allow_list() {
        // require_auth + authed + pubkey not on allow-list → Deny
        let authed_keys = Keys::generate();
        let event_keys = Keys::generate();
        let event = make_event(&event_keys, "hello");
        let policy = PolicyConfig {
            write: WritePolicy {
                require_auth: true,
                allowed_pubkeys: Some(vec![hex_pubkey(&authed_keys)]),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        let pk = authed_keys.public_key();
        // Auth passes, but event.pubkey is not on allow-list
        assert!(matches!(
            engine.can_write(&event, Some(&pk)),
            PolicyResult::Deny(ref s) if s.contains("allow-list")
        ));
    }

    #[test]
    fn combo_blocked_pubkey_allowed_kind() {
        // Blocked pubkey + allowed kind → Deny (pubkey checked before kind)
        let keys = Keys::generate();
        let event = make_event(&keys, "hello"); // kind 1
        let policy = PolicyConfig {
            write: WritePolicy {
                blocked_pubkeys: Some(vec![hex_pubkey(&keys)]),
                ..Default::default()
            },
            events: EventPolicy {
                allowed_kinds: Some(vec![1]),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(matches!(
            engine.can_write(&event, None),
            PolicyResult::Deny(ref s) if s.contains("blocked")
        ));
    }

    #[test]
    fn combo_allowed_kind_content_too_long() {
        // Allowed kind + content too long → Deny (content length checked after kind)
        let keys = Keys::generate();
        let content = "x".repeat(100);
        let event = make_event(&keys, &content);
        let policy = PolicyConfig {
            events: EventPolicy {
                allowed_kinds: Some(vec![1]),
                max_content_length: Some(10),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(matches!(
            engine.can_write(&event, None),
            PolicyResult::Deny(ref s) if s.contains("too long")
        ));
    }

    #[test]
    fn combo_allowed_kind_insufficient_pow() {
        // Allowed kind + insufficient PoW → Deny (PoW checked last)
        let keys = Keys::generate();
        let event = make_event(&keys, "hello");
        let policy = PolicyConfig {
            events: EventPolicy {
                allowed_kinds: Some(vec![1]),
                min_pow: Some(128),
                ..Default::default()
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        assert!(matches!(
            engine.can_write(&event, None),
            PolicyResult::Deny(ref s) if s.contains("PoW")
        ));
    }

    #[test]
    fn combo_read_allowed_pubkeys_no_auth_no_require_auth() {
        // Read allowed_pubkeys set + no auth + no require_auth → Deny (not AuthRequired)
        // This documents the subtle behavior: code falls through to Deny match arm
        let keys = Keys::generate();
        let policy = PolicyConfig {
            read: ReadPolicy {
                require_auth: false,
                allowed_pubkeys: Some(vec![hex_pubkey(&keys)]),
                wot: None,
                paywall: None,
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        let filter = Filter::new();
        let result = engine.can_read(&filter, None);
        // Should be Deny, NOT AuthRequired
        assert!(matches!(result, PolicyResult::Deny(_)));
    }

    // -----------------------------------------------------------------------
    // Paywall tests
    // -----------------------------------------------------------------------

    #[test]
    fn paywall_write_allows_whitelisted() {
        use crate::paywall::PaywallSet;
        let keys = Keys::generate();
        let event = make_event(&keys, "hello");
        let paywall = PaywallSet::new_for_test();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        paywall.add(keys.public_key(), now + 3600);
        let engine = PolicyEngine::new(open_policy(), default_nip11(), None, None, Some(paywall), None);
        assert!(engine.can_write(&event, None).is_allowed());
    }

    #[test]
    fn paywall_write_denies_non_whitelisted() {
        use crate::paywall::PaywallSet;
        let keys = Keys::generate();
        let event = make_event(&keys, "hello");
        let paywall = PaywallSet::new_for_test();
        let engine = PolicyEngine::new(open_policy(), default_nip11(), None, None, Some(paywall), None);
        assert!(matches!(
            engine.can_write(&event, None),
            PolicyResult::Deny(ref s) if s.contains("payment required")
        ));
    }

    #[test]
    fn paywall_write_denies_expired() {
        use crate::paywall::PaywallSet;
        let keys = Keys::generate();
        let event = make_event(&keys, "hello");
        let paywall = PaywallSet::new_for_test();
        // Expired 1 second ago
        paywall.add(keys.public_key(), 1);
        let engine = PolicyEngine::new(open_policy(), default_nip11(), None, None, Some(paywall), None);
        assert!(matches!(
            engine.can_write(&event, None),
            PolicyResult::Deny(ref s) if s.contains("payment required")
        ));
    }

    #[test]
    fn paywall_read_allows_whitelisted() {
        use crate::paywall::PaywallSet;
        let keys = Keys::generate();
        let pk = keys.public_key();
        let paywall = PaywallSet::new_for_test();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        paywall.add(pk, now + 3600);
        let engine = PolicyEngine::new(open_policy(), default_nip11(), None, None, None, Some(paywall));
        let filter = Filter::new();
        assert!(engine.can_read(&filter, Some(&pk)).is_allowed());
    }

    #[test]
    fn paywall_read_requires_auth() {
        use crate::paywall::PaywallSet;
        let paywall = PaywallSet::new_for_test();
        let engine = PolicyEngine::new(open_policy(), default_nip11(), None, None, None, Some(paywall));
        let filter = Filter::new();
        assert!(matches!(
            engine.can_read(&filter, None),
            PolicyResult::AuthRequired
        ));
    }

    #[test]
    fn paywall_read_denies_non_whitelisted() {
        use crate::paywall::PaywallSet;
        let keys = Keys::generate();
        let pk = keys.public_key();
        let paywall = PaywallSet::new_for_test();
        let engine = PolicyEngine::new(open_policy(), default_nip11(), None, None, None, Some(paywall));
        let filter = Filter::new();
        assert!(matches!(
            engine.can_read(&filter, Some(&pk)),
            PolicyResult::Deny(ref s) if s.contains("payment required")
        ));
    }

    #[test]
    fn combo_read_require_auth_allowed_pubkeys_authed_but_unlisted() {
        // Read require_auth + allowed_pubkeys + authed but unlisted → Deny
        let allowed_keys = Keys::generate();
        let other_keys = Keys::generate();
        let other_pk = other_keys.public_key();
        let policy = PolicyConfig {
            read: ReadPolicy {
                require_auth: true,
                allowed_pubkeys: Some(vec![hex_pubkey(&allowed_keys)]),
                wot: None,
                paywall: None,
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy, default_nip11(), None, None, None, None);
        let filter = Filter::new();
        let result = engine.can_read(&filter, Some(&other_pk));
        assert!(matches!(result, PolicyResult::Deny(_)));
    }

    // -----------------------------------------------------------------------
    // NIP-11 enforcement tests
    // -----------------------------------------------------------------------

    #[test]
    fn nip11_max_event_tags_allows_under_limit() {
        let keys = Keys::generate();
        let tags: Vec<nostr::Tag> = (0..5)
            .map(|i| nostr::Tag::custom(nostr::TagKind::Custom(std::borrow::Cow::Borrowed("t")), vec![format!("tag{}", i)]))
            .collect();
        let event = EventBuilder::new(Kind::from(1u16), "hello", tags)
            .to_event(&keys)
            .unwrap();
        let nip11 = Nip11Config {
            max_event_tags: Some(10),
            ..default_nip11()
        };
        let engine = PolicyEngine::new(open_policy(), nip11, None, None, None, None);
        assert!(engine.can_write(&event, None).is_allowed());
    }

    #[test]
    fn nip11_max_event_tags_rejects_over_limit() {
        let keys = Keys::generate();
        let tags: Vec<nostr::Tag> = (0..11)
            .map(|i| nostr::Tag::custom(nostr::TagKind::Custom(std::borrow::Cow::Borrowed("t")), vec![format!("tag{}", i)]))
            .collect();
        let event = EventBuilder::new(Kind::from(1u16), "hello", tags)
            .to_event(&keys)
            .unwrap();
        let nip11 = Nip11Config {
            max_event_tags: Some(10),
            ..default_nip11()
        };
        let engine = PolicyEngine::new(open_policy(), nip11, None, None, None, None);
        assert!(matches!(
            engine.can_write(&event, None),
            PolicyResult::Deny(ref s) if s.contains("too many tags")
        ));
    }

    #[test]
    fn nip11_created_at_lower_limit_rejects_old_event() {
        let keys = Keys::generate();
        // Create event with timestamp far in the past (epoch)
        let event = EventBuilder::new(Kind::from(1u16), "old event", [])
            .custom_created_at(nostr::Timestamp::from(1u64))
            .to_event(&keys)
            .unwrap();
        let nip11 = Nip11Config {
            created_at_lower_limit: Some(3600), // only allow events from last hour
            ..default_nip11()
        };
        let engine = PolicyEngine::new(open_policy(), nip11, None, None, None, None);
        assert!(matches!(
            engine.can_write(&event, None),
            PolicyResult::Deny(ref s) if s.contains("too far in the past")
        ));
    }

    #[test]
    fn nip11_created_at_upper_limit_rejects_future_event() {
        let keys = Keys::generate();
        let far_future = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 7200; // 2 hours in the future
        let event = EventBuilder::new(Kind::from(1u16), "future event", [])
            .custom_created_at(nostr::Timestamp::from(far_future))
            .to_event(&keys)
            .unwrap();
        let nip11 = Nip11Config {
            created_at_upper_limit: Some(900), // only allow 15 min ahead
            ..default_nip11()
        };
        let engine = PolicyEngine::new(open_policy(), nip11, None, None, None, None);
        assert!(matches!(
            engine.can_write(&event, None),
            PolicyResult::Deny(ref s) if s.contains("too far in the future")
        ));
    }

    #[test]
    fn nip11_created_at_allows_recent_event() {
        let keys = Keys::generate();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let event = EventBuilder::new(Kind::from(1u16), "recent event", [])
            .custom_created_at(nostr::Timestamp::from(now))
            .to_event(&keys)
            .unwrap();
        let nip11 = Nip11Config {
            created_at_lower_limit: Some(3600),
            created_at_upper_limit: Some(900),
            ..default_nip11()
        };
        let engine = PolicyEngine::new(open_policy(), nip11, None, None, None, None);
        assert!(engine.can_write(&event, None).is_allowed());
    }
}
