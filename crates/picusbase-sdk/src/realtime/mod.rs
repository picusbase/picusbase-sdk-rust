use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealtimeMessage {
    pub topic: String,
    pub event: String,
    pub payload: serde_json::Value,
}

impl RealtimeMessage {
    pub fn is_db_event(&self) -> bool {
        matches!(self.event.as_str(), "INSERT" | "UPDATE" | "DELETE")
    }

    pub fn as_db_payload<T: serde::de::DeserializeOwned>(&self) -> Option<RealtimeDBPayload<T>> {
        if self.is_db_event() {
            serde_json::from_value(self.payload.clone()).ok()
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealtimeDBPayload<T> {
    pub schema: String,
    pub table: String,
    pub operation: String,
    pub record: Option<T>,
    pub old_record: Option<T>,
    #[serde(default)]
    pub fetch_required: bool,
    pub id: Option<serde_json::Value>,
    pub commit_timestamp: Option<String>,
}

impl<T> RealtimeDBPayload<T> {
    pub fn should_fetch(&self) -> bool {
        self.fetch_required
    }

    pub fn get_timestamp(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.commit_timestamp
            .as_ref()
            .and_then(|ts| ts.parse().ok())
    }
}

/// Backend-side (non-wasm) client for subscribing to realtime-service's
/// CDC broadcast topics directly, without going through a GraphQL layer.
/// Topics are named `"{schema}:{table}"`; messages come through raw as the
/// CDC pipeline emits them (`{"op": "INSERT"|"UPDATE"|"DELETE", "schema",
/// "table", "record", ["old_record"]}`), not wrapped in `RealtimeMessage`.
#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use futures::{Stream, StreamExt};
    use picus_errors::{ErrorCategory, ErrorLevel, PicusError};
    use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

    #[derive(Clone)]
    pub struct RealtimeClient {
        url: String,
    }

    impl RealtimeClient {
        pub fn new(url: &str, token: Option<&str>) -> Self {
            let url = match token {
                Some(token) if !token.is_empty() => format!(
                    "{}{}token={}",
                    url,
                    if url.contains('?') { '&' } else { '?' },
                    token
                ),
                _ => url.to_string(),
            };
            Self { url }
        }

        pub async fn subscribe(
            &self,
            topic: &str,
        ) -> Result<impl Stream<Item = Result<serde_json::Value, PicusError>>, PicusError> {
            let (ws_stream, _) = connect_async(&self.url).await.map_err(|e| {
                PicusError::new(
                    "REALTIME",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    1,
                    format!("Failed to connect to realtime service: {}", e),
                )
            })?;

            let (mut write, read) = ws_stream.split();

            let join_msg = serde_json::json!({ "event": "join", "topic": topic });
            futures::SinkExt::send(&mut write, Message::Text(join_msg.to_string()))
                .await
                .map_err(|e| {
                    PicusError::new(
                        "REALTIME",
                        ErrorCategory::NET,
                        ErrorLevel::Error,
                        2,
                        format!("Failed to send join message: {}", e),
                    )
                })?;

            let stream = read.filter_map(|msg| async {
                match msg {
                    Ok(Message::Text(text)) => match serde_json::from_str(&text) {
                        Ok(val) => Some(Ok(val)),
                        Err(e) => Some(Err(PicusError::new(
                            "REALTIME",
                            ErrorCategory::DATA,
                            ErrorLevel::Error,
                            4,
                            format!("Failed to parse message: {}", e),
                        ))),
                    },
                    Ok(Message::Close(_)) => None,
                    Err(e) => Some(Err(PicusError::new(
                        "REALTIME",
                        ErrorCategory::NET,
                        ErrorLevel::Error,
                        6,
                        format!("WebSocket error: {}", e),
                    ))),
                    _ => None,
                }
            });

            Ok(stream)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod wasm {
    use futures::stream::{SplitSink, SplitStream};
    use futures::{SinkExt, StreamExt};
    use gloo_net::websocket::{futures::WebSocket, Message};
    use picus_errors::{ErrorCategory, ErrorLevel, PicusError};

    pub type WsSender = SplitSink<WebSocket, Message>;
    pub type WsReceiver = SplitStream<WebSocket>;

    pub struct Backoff {
        attempts: u32,
        base_ms: u32,
        max_ms: u32,
    }

    impl Backoff {
        pub fn new(base_ms: u32, max_ms: u32) -> Self {
            Self {
                attempts: 0,
                base_ms,
                max_ms,
            }
        }

        pub fn reset(&mut self) {
            self.attempts = 0;
        }

        pub fn next_delay_ms(&mut self) -> u32 {
            let exponent = 2u32.pow(self.attempts.min(30)); // prevent overflow
            let delay = (self.base_ms * exponent).min(self.max_ms);
            self.attempts += 1;

            // Add Jitter (0-1000ms)
            // let jitter = (js_sys::Math::random() * 1000.0) as u32;
            let jitter = 0;
            delay + jitter
        }
    }

    pub struct RealtimeClient;

    impl RealtimeClient {
        pub fn connect(
            url: &str,
            token: &str,
        ) -> Result<(RealtimeSender, RealtimeReceiver), PicusError> {
            let url_with_token = format!("{}?token={}", url, token);
            let ws = WebSocket::open(&url_with_token).map_err(|e| {
                PicusError::new(
                    "REALTIME",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    1,
                    format!("Failed to open websocket: {}", e),
                )
            })?;

            let (write, read) = ws.split();

            Ok((
                RealtimeSender { inner: write },
                RealtimeReceiver { inner: read },
            ))
        }
    }

    pub struct RealtimeSender {
        inner: WsSender,
    }

    impl RealtimeSender {
        pub async fn subscribe(&mut self, topic: &str) -> Result<(), PicusError> {
            let msg = serde_json::json!({
                "event": "join",
                "topic": topic
            });

            self.inner
                .send(Message::Text(msg.to_string()))
                .await
                .map_err(|e| {
                    PicusError::new(
                        "REALTIME",
                        ErrorCategory::NET,
                        ErrorLevel::Error,
                        2,
                        format!("Failed to subscribe: {}", e),
                    )
                })?;

            Ok(())
        }

        pub async fn send_broadcast(
            &mut self,
            topic: &str,
            payload: serde_json::Value,
        ) -> Result<(), PicusError> {
            let msg = serde_json::json!({
                "event": "broadcast",
                "topic": topic,
                "payload": payload
            });

            self.inner
                .send(Message::Text(msg.to_string()))
                .await
                .map_err(|e| {
                    PicusError::new(
                        "REALTIME",
                        ErrorCategory::NET,
                        ErrorLevel::Error,
                        3,
                        format!("Failed to send broadcast: {}", e),
                    )
                })?;

            Ok(())
        }
    }

    pub struct RealtimeReceiver {
        inner: WsReceiver,
    }

    impl RealtimeReceiver {
        /// Messages arrive raw as the CDC pipeline / broadcast handler emit them
        /// (`{"op": "INSERT"|"UPDATE"|"DELETE", "schema", "table", "record", ["old_record"]}`
        /// for CDC events, or `{"event": "broadcast", "topic", "payload"}` for
        /// client-to-client broadcasts) — never wrapped in `RealtimeMessage`.
        pub async fn next(&mut self) -> Option<Result<serde_json::Value, PicusError>> {
            match self.inner.next().await {
                Some(Ok(Message::Text(text))) => match serde_json::from_str(&text) {
                    Ok(val) => Some(Ok(val)),
                    Err(e) => Some(Err(PicusError::new(
                        "REALTIME",
                        ErrorCategory::DATA,
                        ErrorLevel::Error,
                        4,
                        format!("Failed to parse message: {}", e),
                    ))),
                },
                Some(Ok(Message::Bytes(_))) => Some(Err(PicusError::new(
                    "REALTIME",
                    ErrorCategory::DATA,
                    ErrorLevel::Warning,
                    5,
                    "Received binary message, expected text".to_string(),
                ))),
                Some(Err(e)) => Some(Err(PicusError::new(
                    "REALTIME",
                    ErrorCategory::NET,
                    ErrorLevel::Error,
                    6,
                    format!("WebSocket error: {}", e),
                ))),
                None => None, // Stream closed
            }
        }
    }
}
