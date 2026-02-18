use futures_util::{SinkExt, StreamExt};
use nostr::nips::nip47::{
    LookupInvoiceRequestParams, MakeInvoiceRequestParams, NostrWalletConnectURI, Request, Response,
};
use nostr::{Event, JsonUtil, Keys};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Clone)]
pub struct NwcClient {
    uri: NostrWalletConnectURI,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InvoiceResponse {
    pub invoice: String,
    pub payment_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InvoiceStatus {
    Pending,
    Paid,
    Expired,
}

impl NwcClient {
    pub fn from_connection_string(s: &str) -> Result<Self, anyhow::Error> {
        let uri = NostrWalletConnectURI::from_str(s.trim())
            .map_err(|e| anyhow::anyhow!("Invalid NWC connection string: {}", e))?;
        tracing::debug!(
            relay = %uri.relay_url,
            wallet_pubkey = %uri.public_key.to_hex(),
            "Parsed NWC connection string"
        );
        Ok(Self { uri })
    }

    fn keys(&self) -> Keys {
        Keys::new(self.uri.secret.clone())
    }

    async fn send_and_wait(&self, request: Request) -> Result<Response, anyhow::Error> {
        self.send_and_wait_timeout(request, 30).await
    }

    async fn send_and_wait_timeout(&self, request: Request, timeout_secs: u64) -> Result<Response, anyhow::Error> {
        let method = format!("{:?}", request.method);
        tracing::info!(method = %method, relay = %self.uri.relay_url, "NWC: sending request");

        let event = request
            .to_event(&self.uri)
            .map_err(|e| anyhow::anyhow!("Failed to build NWC event: {}", e))?;

        let event_id = event.id;
        let our_pk = self.keys().public_key();

        tracing::debug!(
            event_id = %event_id.to_hex(),
            our_pubkey = %our_pk.to_hex(),
            wallet_pubkey = %self.uri.public_key.to_hex(),
            "NWC: connecting to relay"
        );

        let (mut ws, _) = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            tokio_tungstenite::connect_async(self.uri.relay_url.as_str()),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Connection timeout to NWC relay: {}", self.uri.relay_url))?
        .map_err(|e| anyhow::anyhow!("WS connect failed to {}: {}", self.uri.relay_url, e))?;

        tracing::debug!("NWC: connected to relay, subscribing for response first");

        // Subscribe BEFORE sending the event to avoid race conditions.
        // If the wallet responds faster than we can subscribe (e.g. lookup_invoice
        // is just a DB lookup), and the relay treats NWC events as ephemeral,
        // we'd miss the response entirely.
        let sub = serde_json::json!(["REQ", "nwc-resp", {
            "kinds": [23195],
            "#p": [our_pk.to_hex()],
            "#e": [event_id.to_hex()],
            "limit": 1,
        }]);
        ws.send(Message::Text(sub.to_string().into())).await?;

        // Now send the request event
        let event_json = event.as_json();
        let req = format!(r#"["EVENT",{}]"#, event_json);
        ws.send(Message::Text(req.into())).await?;

        tracing::debug!("NWC: EVENT sent after subscription active");

        tracing::debug!(timeout_secs = timeout_secs, "NWC: subscribed, waiting for response");

        // Wait for response
        let result = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), async {
            while let Some(msg) = ws.next().await {
                let msg = msg?;
                let text = match msg {
                    Message::Text(t) => t.to_string(),
                    Message::Close(frame) => {
                        tracing::warn!("NWC: relay closed connection: {:?}", frame);
                        break;
                    }
                    _ => continue,
                };

                let parsed: serde_json::Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                let arr = match parsed.as_array() {
                    Some(a) if !a.is_empty() => a,
                    _ => continue,
                };

                let msg_type = arr[0].as_str().unwrap_or("");
                tracing::debug!(msg_type = %msg_type, "NWC: received relay message");

                match msg_type {
                    "OK" => {
                        let accepted = arr.get(2).and_then(|v| v.as_bool()).unwrap_or(false);
                        let reason = arr.get(3).and_then(|v| v.as_str()).unwrap_or("");
                        if accepted {
                            tracing::debug!("NWC: EVENT accepted by relay");
                        } else {
                            tracing::error!(reason = %reason, "NWC: EVENT rejected by relay");
                            return Err(anyhow::anyhow!("NWC relay rejected event: {}", reason));
                        }
                    }
                    "EOSE" => {
                        tracing::debug!("NWC: EOSE received, no cached response - waiting for live event");
                    }
                    "NOTICE" => {
                        let notice = arr.get(1).and_then(|v| v.as_str()).unwrap_or("");
                        tracing::warn!(notice = %notice, "NWC: relay NOTICE");
                    }
                    "EVENT" if arr.len() >= 3 => {
                        tracing::debug!("NWC: received response EVENT, decrypting");
                        let event_value = &arr[2];
                        let resp_event: Event = match Event::from_value(event_value.clone()) {
                            Ok(e) => e,
                            Err(e) => {
                                tracing::warn!("NWC: failed to parse response event: {}", e);
                                continue;
                            }
                        };

                        match Response::from_event(&self.uri, &resp_event) {
                            Ok(response) => {
                                tracing::info!(method = %method, "NWC: got response");
                                if let Some(ref err) = response.error {
                                    tracing::error!(
                                        code = ?err.code,
                                        message = %err.message,
                                        "NWC: wallet returned error"
                                    );
                                }
                                return Ok(response);
                            }
                            Err(e) => {
                                tracing::error!("NWC: failed to decrypt/parse response: {}", e);
                                continue;
                            }
                        }
                    }
                    _ => {}
                }
            }
            Err(anyhow::anyhow!("Connection closed without response"))
        })
        .await
        .map_err(|_| {
            tracing::error!(method = %method, relay = %self.uri.relay_url, "NWC: timeout waiting for response");
            anyhow::anyhow!("Timeout waiting for NWC {} response from {}", method, self.uri.relay_url)
        })??;

        let _ = ws.close(None).await;
        Ok(result)
    }

    pub async fn make_invoice(
        &self,
        amount_msats: u64,
        memo: &str,
    ) -> Result<InvoiceResponse, anyhow::Error> {
        tracing::info!(amount_msats = amount_msats, memo = %memo, "NWC: requesting invoice");

        let request = Request::make_invoice(MakeInvoiceRequestParams {
            amount: amount_msats,
            description: Some(memo.to_string()),
            description_hash: None,
            expiry: None,
        });

        let response = self.send_and_wait(request).await?;

        let result = response
            .to_make_invoice()
            .map_err(|e| anyhow::anyhow!("NWC make_invoice failed: {}", e))?;

        let invoice = result.invoice;
        let payment_hash = result.payment_hash;

        tracing::info!(payment_hash = %payment_hash, "NWC: invoice created");

        Ok(InvoiceResponse {
            invoice,
            payment_hash,
        })
    }

    pub async fn lookup_invoice(
        &self,
        payment_hash: &str,
    ) -> Result<InvoiceStatus, anyhow::Error> {
        tracing::debug!(payment_hash = %payment_hash, "NWC: looking up invoice");

        let request = Request::lookup_invoice(LookupInvoiceRequestParams {
            payment_hash: Some(payment_hash.to_string()),
            invoice: None,
        });

        let response = self.send_and_wait_timeout(request, 8).await?;

        let result = response
            .to_lookup_invoice()
            .map_err(|e| anyhow::anyhow!("NWC lookup_invoice failed: {}", e))?;

        // Check settled_at to determine if paid
        if result.settled_at.is_some() {
            tracing::info!(payment_hash = %payment_hash, "NWC: invoice is paid");
            return Ok(InvoiceStatus::Paid);
        }

        // Check if preimage is present (some wallets)
        if let Some(ref preimage) = result.preimage {
            if !preimage.is_empty() {
                tracing::info!(payment_hash = %payment_hash, "NWC: invoice is paid (has preimage)");
                return Ok(InvoiceStatus::Paid);
            }
        }

        // Check expiry
        if let Some(expires_at) = result.expires_at {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            if now > expires_at.as_u64() {
                tracing::debug!(payment_hash = %payment_hash, "NWC: invoice expired");
                return Ok(InvoiceStatus::Expired);
            }
        }

        tracing::debug!(payment_hash = %payment_hash, "NWC: invoice still pending");
        Ok(InvoiceStatus::Pending)
    }

    pub async fn subscribe_and_watch_invoice(
        &self,
        payment_hash: String,
        status_tx: tokio::sync::watch::Sender<InvoiceStatus>,
    ) -> Result<(), anyhow::Error> {
        let max_retries = 3u32;
        let mut attempt = 0u32;

        loop {
            match self
                .watch_invoice_connection(&payment_hash, &status_tx)
                .await
            {
                Ok(()) => return Ok(()),
                Err(e) => {
                    // If we already sent a terminal status, we're done
                    if *status_tx.borrow() != InvoiceStatus::Pending {
                        return Ok(());
                    }
                    attempt += 1;
                    if attempt > max_retries {
                        tracing::error!(
                            payment_hash = %payment_hash,
                            error = %e,
                            "NWC: watch giving up after {} retries",
                            max_retries
                        );
                        return Err(e);
                    }
                    let backoff = std::time::Duration::from_secs(2u64.pow(attempt));
                    tracing::warn!(
                        payment_hash = %payment_hash,
                        attempt = attempt,
                        backoff_secs = backoff.as_secs(),
                        error = %e,
                        "NWC: watch connection failed, retrying"
                    );
                    tokio::time::sleep(backoff).await;
                }
            }
        }
    }

    async fn watch_invoice_connection(
        &self,
        payment_hash: &str,
        status_tx: &tokio::sync::watch::Sender<InvoiceStatus>,
    ) -> Result<(), anyhow::Error> {
        tracing::info!(payment_hash = %payment_hash, "NWC: watch starting persistent connection");

        let (mut ws, _) = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            tokio_tungstenite::connect_async(self.uri.relay_url.as_str()),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Connection timeout to NWC relay"))?
        .map_err(|e| anyhow::anyhow!("WS connect failed: {}", e))?;

        let our_pk = self.keys().public_key();

        // Subscribe for NWC responses (23195) and notifications (23197)
        let sub = serde_json::json!(["REQ", "nwc-watch", {
            "kinds": [23195, 23197],
            "#p": [our_pk.to_hex()],
        }]);
        ws.send(Message::Text(sub.to_string().into())).await?;

        // Send initial lookup_invoice to catch already-paid
        self.send_lookup_on_ws(&mut ws, payment_hash).await?;

        let mut poll_interval = tokio::time::interval(std::time::Duration::from_secs(15));
        poll_interval.tick().await; // consume the immediate first tick
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3600);

        loop {
            tokio::select! {
                msg = ws.next() => {
                    let msg = match msg {
                        Some(Ok(m)) => m,
                        Some(Err(e)) => return Err(anyhow::anyhow!("WS error: {}", e)),
                        None => return Err(anyhow::anyhow!("WS connection closed")),
                    };
                    let text = match msg {
                        Message::Text(t) => t.to_string(),
                        Message::Close(_) => return Err(anyhow::anyhow!("WS closed by relay")),
                        _ => continue,
                    };

                    let parsed: serde_json::Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let arr = match parsed.as_array() {
                        Some(a) if a.len() >= 3 && a[0].as_str() == Some("EVENT") => a,
                        _ => continue,
                    };

                    let resp_event: Event = match Event::from_value(arr[2].clone()) {
                        Ok(e) => e,
                        Err(_) => continue,
                    };

                    let response = match Response::from_event(&self.uri, &resp_event) {
                        Ok(r) => r,
                        Err(_) => continue,
                    };

                    // Check if this response is about our payment_hash
                    if let Ok(result) = response.to_lookup_invoice() {
                        if result.payment_hash == payment_hash {
                            if result.settled_at.is_some() || result.preimage.as_ref().is_some_and(|p| !p.is_empty()) {
                                tracing::info!(payment_hash = %payment_hash, "NWC: watch detected payment");
                                let _ = status_tx.send(InvoiceStatus::Paid);
                                let _ = ws.close(None).await;
                                return Ok(());
                            }
                            if let Some(expires_at) = result.expires_at {
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs();
                                if now > expires_at.as_u64() {
                                    tracing::info!(payment_hash = %payment_hash, "NWC: watch detected expiry");
                                    let _ = status_tx.send(InvoiceStatus::Expired);
                                    let _ = ws.close(None).await;
                                    return Ok(());
                                }
                            }
                        }
                    }
                }
                _ = poll_interval.tick() => {
                    tracing::debug!(payment_hash = %payment_hash, "NWC: watch re-polling");
                    if let Err(e) = self.send_lookup_on_ws(&mut ws, payment_hash).await {
                        tracing::warn!(error = %e, "NWC: watch re-poll send failed");
                        return Err(e.into());
                    }
                }
                _ = tokio::time::sleep_until(deadline) => {
                    tracing::info!(payment_hash = %payment_hash, "NWC: watch max lifetime reached");
                    let _ = ws.close(None).await;
                    return Ok(());
                }
            }
        }
    }

    async fn send_lookup_on_ws<S>(&self, ws: &mut S, payment_hash: &str) -> Result<(), anyhow::Error>
    where
        S: SinkExt<Message> + Unpin,
        S::Error: std::fmt::Display,
    {
        let request = Request::lookup_invoice(LookupInvoiceRequestParams {
            payment_hash: Some(payment_hash.to_string()),
            invoice: None,
        });

        let event = request
            .to_event(&self.uri)
            .map_err(|e| anyhow::anyhow!("Failed to build NWC event: {}", e))?;

        let event_json = event.as_json();
        let msg = format!(r#"["EVENT",{}]"#, event_json);
        ws.send(Message::Text(msg.into()))
            .await
            .map_err(|e| anyhow::anyhow!("WS send failed: {}", e))?;
        Ok(())
    }

    pub async fn get_info(&self) -> Result<(), anyhow::Error> {
        let request = Request::get_info();
        let response = self.send_and_wait(request).await?;
        let _info = response
            .to_get_info()
            .map_err(|e| anyhow::anyhow!("NWC get_info failed: {}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_connection_string() {
        let conn = "nostr+walletconnect://b889ff5b1513b641e2a139f661a661364979c5beee91842f8f0ef42ab558e9d4?relay=wss%3A%2F%2Frelay.example.com&secret=71a8c14c1407c113601079c4302dab36460f0ccd0ad506f1f2dc73b5100e4f3c";
        let client = NwcClient::from_connection_string(conn).unwrap();
        assert!(client.uri.relay_url.as_str().starts_with("wss://relay.example.com"));
        assert_eq!(
            client.uri.public_key.to_hex(),
            "b889ff5b1513b641e2a139f661a661364979c5beee91842f8f0ef42ab558e9d4"
        );
    }

    #[test]
    fn parse_invalid_prefix() {
        let result = NwcClient::from_connection_string("invalid://test");
        assert!(result.is_err());
    }
}
