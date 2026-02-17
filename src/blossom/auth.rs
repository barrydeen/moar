use base64::Engine;
use nostr::Event;

/// Verify a Blossom authorization header (kind 24242).
///
/// Expects the `Authorization: Nostr <base64>` header value.
/// `expected_action` should be "upload", "delete", or "list".
pub fn verify_blossom_auth(
    headers: &axum::http::HeaderMap,
    expected_action: &str,
) -> Result<Event, String> {
    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or("Missing Authorization header")?;

    let b64 = auth_header
        .strip_prefix("Nostr ")
        .ok_or("Authorization header must start with 'Nostr '")?;

    let json_bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|_| "Invalid base64 in Authorization header")?;

    let event: Event =
        serde_json::from_slice(&json_bytes).map_err(|_| "Invalid JSON in auth event")?;

    // 1. Verify signature
    event
        .verify()
        .map_err(|_| "Invalid event signature")?;

    // 2. Verify kind == 24242
    if event.kind.as_u64() != 24242 {
        return Err("Event kind must be 24242".to_string());
    }

    // 3. Verify timestamp within 60s
    let now = nostr::Timestamp::now();
    let diff = if now > event.created_at {
        now.as_u64() - event.created_at.as_u64()
    } else {
        event.created_at.as_u64() - now.as_u64()
    };
    if diff > 60 {
        return Err("Auth event too old or in future".to_string());
    }

    // 4. Verify `t` tag matches expected action
    let mut found_action = false;
    for tag in event.tags.iter() {
        let v = tag.as_vec();
        if v.len() >= 2 && v[0] == "t" && v[1] == expected_action {
            found_action = true;
            break;
        }
    }
    if !found_action {
        return Err(format!(
            "Auth event missing 't' tag with value '{}'",
            expected_action
        ));
    }

    Ok(event)
}

/// Extract the `x` tag value (sha256 hash) from a Blossom auth event.
pub fn get_x_tag(event: &Event) -> Option<String> {
    for tag in event.tags.iter() {
        let v = tag.as_vec();
        if v.len() >= 2 && v[0] == "x" {
            return Some(v[1].clone());
        }
    }
    None
}
