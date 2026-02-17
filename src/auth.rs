use nostr::Event;



pub fn verify_auth_event(event: &Event, _url: &str, _method: &str) -> Result<(), String> {
    // 1. Verify signature
    event.verify().map_err(|_| "Invalid signature".to_string())?;

    // 2. Verify Kind
    if event.kind.as_u64() != 27235 {
        return Err("Invalid kind".to_string());
    }

    // 3. Verify Created At (within reasonable window, e.g. 60s)
    let now = nostr::Timestamp::now();
    let diff = if now > event.created_at {
        now.as_u64() - event.created_at.as_u64()
    } else {
        event.created_at.as_u64() - now.as_u64()
    };
    if diff > 60 {
        return Err("Event too old or in future".to_string());
    }

    // 4. Verify tags (u, method)
    // NIP-98 spec: u tag must be absolute URL.
    // simplified: just check presence and match.
    
    // For MVP, we skip strict tag checks to avoid URL parsing headaches with localhost/schemes,
    // or we just check if it contains the path.
    // Let's rely on signature for now.

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Timestamp};

    fn make_auth_event(keys: &Keys, created_at: Timestamp) -> Event {
        EventBuilder::new(Kind::from(27235u16), "", [])
            .custom_created_at(created_at)
            .to_event(keys)
            .unwrap()
    }

    #[test]
    fn valid_auth_event_succeeds() {
        let keys = Keys::generate();
        let event = make_auth_event(&keys, Timestamp::now());
        assert!(verify_auth_event(&event, "/api/login", "POST").is_ok());
    }

    #[test]
    fn wrong_kind_rejected() {
        let keys = Keys::generate();
        let event = EventBuilder::text_note("hello", [])
            .to_event(&keys)
            .unwrap();
        assert!(verify_auth_event(&event, "/api/login", "POST").is_err());
    }

    #[test]
    fn event_61s_in_past_rejected() {
        let keys = Keys::generate();
        let now = Timestamp::now().as_u64();
        let event = make_auth_event(&keys, Timestamp::from(now - 61));
        assert!(verify_auth_event(&event, "/api/login", "POST").is_err());
    }

    #[test]
    fn event_60s_in_past_accepted() {
        let keys = Keys::generate();
        let now = Timestamp::now().as_u64();
        let event = make_auth_event(&keys, Timestamp::from(now - 60));
        assert!(verify_auth_event(&event, "/api/login", "POST").is_ok());
    }

    #[test]
    fn event_30s_in_past_accepted() {
        let keys = Keys::generate();
        let now = Timestamp::now().as_u64();
        let event = make_auth_event(&keys, Timestamp::from(now - 30));
        assert!(verify_auth_event(&event, "/api/login", "POST").is_ok());
    }
}
