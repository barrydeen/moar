use nostr::{Event, Filter, Kind, PublicKey};
use std::collections::HashSet;
use std::str::FromStr;

use crate::config::PolicyConfig;

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
    write_allowed: Option<HashSet<PublicKey>>,
    write_blocked: Option<HashSet<PublicKey>>,
    write_tagged: Option<HashSet<PublicKey>>,
    read_allowed: Option<HashSet<PublicKey>>,
    allowed_kinds: Option<HashSet<Kind>>,
    blocked_kinds: Option<HashSet<Kind>>,
}

impl PolicyEngine {
    pub fn new(config: PolicyConfig) -> Self {
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
            write_allowed,
            write_blocked,
            write_tagged,
            read_allowed,
            allowed_kinds,
            blocked_kinds,
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
    use crate::config::{EventPolicy, PolicyConfig, ReadPolicy, WritePolicy};
    use nostr::nips::nip19::ToBech32;
    use nostr::{EventBuilder, Keys, Kind};

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
        let engine = PolicyEngine::new(open_policy());
        assert!(engine.can_write(&event, None).is_allowed());
    }

    #[test]
    fn default_open_policy_allows_read() {
        let engine = PolicyEngine::new(open_policy());
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
        let engine = PolicyEngine::new(policy);
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
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy);
        let filter = Filter::new();
        let result = engine.can_read(&filter, None);
        // Should be Deny, NOT AuthRequired
        assert!(matches!(result, PolicyResult::Deny(_)));
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
            },
            ..Default::default()
        };
        let engine = PolicyEngine::new(policy);
        let filter = Filter::new();
        let result = engine.can_read(&filter, Some(&other_pk));
        assert!(matches!(result, PolicyResult::Deny(_)));
    }
}
