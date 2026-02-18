use futures_util::{SinkExt, StreamExt};
use nostr::nips::nip47::{
    ListTransactionsRequestParams, LookupInvoiceRequestParams, MakeInvoiceRequestParams,
    NostrWalletConnectURI, Request, Response, TransactionType,
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

    async fn send_and_wait_timeout(
        &self,
        request: Request,
        timeout_secs: u64,
    ) -> Result<Response, anyhow::Error> {
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

        tracing::debug!(
            timeout_secs = timeout_secs,
            "NWC: subscribed, waiting for response"
        );

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

    pub async fn lookup_invoice(&self, payment_hash: &str) -> Result<InvoiceStatus, anyhow::Error> {
        tracing::debug!(payment_hash = %payment_hash, "NWC: looking up invoice");

        let request = Request::lookup_invoice(LookupInvoiceRequestParams {
            payment_hash: Some(payment_hash.to_string()),
            invoice: None,
        });

        let response = self.send_and_wait(request).await?;

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

    pub async fn list_transactions(
        &self,
    ) -> Result<Vec<nostr::nips::nip47::LookupInvoiceResponseResult>, anyhow::Error> {
        tracing::debug!("NWC: listing recent incoming transactions");

        let request = Request::list_transactions(ListTransactionsRequestParams {
            unpaid: Some(true),
            transaction_type: Some(TransactionType::Incoming),
            limit: Some(50),
            ..Default::default()
        });

        let response = self.send_and_wait(request).await?;

        let transactions = response
            .to_list_transactions()
            .map_err(|e| anyhow::anyhow!("NWC list_transactions failed: {}", e))?;

        tracing::debug!(count = transactions.len(), "NWC: got transactions");
        Ok(transactions)
    }

    /// Background polling loop that calls list_transactions to detect payment.
    /// Sends terminal status (Paid/Expired) via status_tx, then returns.
    pub async fn subscribe_and_watch_invoice(
        &self,
        payment_hash: String,
        status_tx: tokio::sync::watch::Sender<InvoiceStatus>,
    ) -> Result<(), anyhow::Error> {
        tracing::info!(payment_hash = %payment_hash, "NWC: watch started");
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3600);

        loop {
            match self.list_transactions().await {
                Ok(transactions) => {
                    if let Some(tx) = transactions
                        .iter()
                        .find(|tx| tx.payment_hash == payment_hash)
                    {
                        if tx.settled_at.is_some() {
                            tracing::info!(payment_hash = %payment_hash, "NWC: watch detected payment via list_transactions");
                            let _ = status_tx.send(InvoiceStatus::Paid);
                            return Ok(());
                        }

                        if let Some(ref preimage) = tx.preimage {
                            if !preimage.is_empty() {
                                tracing::info!(payment_hash = %payment_hash, "NWC: watch detected payment (has preimage) via list_transactions");
                                let _ = status_tx.send(InvoiceStatus::Paid);
                                return Ok(());
                            }
                        }

                        if let Some(expires_at) = tx.expires_at {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs();
                            if now > expires_at.as_u64() {
                                tracing::info!(payment_hash = %payment_hash, "NWC: watch detected expiry via list_transactions");
                                let _ = status_tx.send(InvoiceStatus::Expired);
                                return Ok(());
                            }
                        }

                        tracing::debug!(payment_hash = %payment_hash, "NWC: watch poll - found in transactions, still pending");
                    } else {
                        tracing::debug!(payment_hash = %payment_hash, "NWC: watch poll - not found in transactions, still pending");
                    }
                }
                Err(e) => {
                    tracing::debug!(payment_hash = %payment_hash, error = %e, "NWC: watch poll failed, will retry");
                }
            }

            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
                _ = tokio::time::sleep_until(deadline) => {
                    tracing::info!(payment_hash = %payment_hash, "NWC: watch max lifetime reached");
                    return Ok(());
                }
            }
        }
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
        assert!(client
            .uri
            .relay_url
            .as_str()
            .starts_with("wss://relay.example.com"));
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
