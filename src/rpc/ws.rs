//! WebSocket JSON-RPC 2.0 client for real-time Stellar RPC subscriptions.
//!
//! The HTTP [`RpcClient`](crate::rpc::client::RpcClient) covers request/
//! response RPC calls; this module adds a long-lived WebSocket connection
//! that supports server-push streaming, primarily the `events` subscription
//! used for real-time monitoring of ledger activity (future-proofing for
//! live config-change monitoring).
//!
//! # Wire protocol
//!
//! The connection speaks JSON-RPC 2.0 over a WebSocket (`wss://…/ws`):
//!
//! * Requests carry a monotonically increasing `id`; responses echo it back.
//! * Server-initiated messages without an `id` are notifications. For the
//!   `events` subscription the server acknowledges the request with a normal
//!   response and then pushes `events` notifications containing the
//!   subscription payload.
//!
//! The exact acknowledgement shape of the upstream Stellar RPC v2 events
//! endpoint is still being finalized ([stellar-rpc#774]), so ack fields are
//! deserialized leniently (missing fields default) and raw payloads are kept
//! available on the returned types.
//!
//! [stellar-rpc#774]: https://github.com/stellar/stellar-rpc/issues/774

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tracing::{debug, trace};

use crate::error::{AppError, AppResult};

/// A message received from the WebSocket RPC server.
#[derive(Debug, Clone)]
pub enum WsMessage {
    /// A response to one of our requests (carries either `result` or
    /// `error`). `id` is the request id the server echoed back.
    Response { id: Value, body: Value },
    /// A server-initiated push notification (no `id`).
    Notification {
        method: String,
        params: Value,
        body: Value,
    },
    /// The server closed the connection cleanly.
    Closed,
}

/// Event types that an `events` subscription filter can match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EventFilterType {
    /// Events emitted by contract execution.
    Contract,
    /// Diagnostic events emitted during transaction simulation.
    Diagnostic,
    /// System-level events.
    System,
    /// All event types.
    All,
}

/// One filter in an `events` subscription request.
///
/// A filter with empty `contract_ids`/`topics` matches every event of the
/// given `r#type`.
#[derive(Debug, Clone, Serialize)]
pub struct EventFilter {
    /// Which event types this filter matches.
    pub r#type: EventFilterType,
    /// Contract IDs (C-prefixed strkeys) whose events should be streamed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub contract_ids: Vec<String>,
    /// Topic filters. Each topic is a list of base64-encoded `ScVal` XDR
    /// segments; a `"*"` segment matches any value at that position.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub topics: Vec<Vec<String>>,
}

impl EventFilter {
    /// Create a filter for the given event type.
    pub fn new(r#type: EventFilterType) -> Self {
        Self {
            r#type,
            contract_ids: Vec::new(),
            topics: Vec::new(),
        }
    }

    /// Restrict the filter to a single contract id.
    #[must_use]
    pub fn with_contract_id(mut self, contract_id: &str) -> Self {
        self.contract_ids.push(contract_id.to_string());
        self
    }

    /// Add a topic filter (a list of base64-encoded `ScVal` XDR segments,
    /// where `"*"` is a wildcard segment).
    #[must_use]
    pub fn with_topic(mut self, topic: Vec<String>) -> Self {
        self.topics.push(topic);
        self
    }
}

/// Parameters for the `events` subscription method.
#[derive(Debug, Clone, Serialize)]
pub struct EventsParams {
    /// First ledger to stream events from.
    pub start_ledger: u32,
    /// Filters selecting which events are delivered.
    pub filters: Vec<EventFilter>,
    /// Optional inclusive last ledger to stream (omitted for an open-ended
    /// subscription).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_ledger: Option<u32>,
}

impl EventsParams {
    /// Create an open-ended `events` subscription starting at `start_ledger`.
    pub fn new(start_ledger: u32, filters: Vec<EventFilter>) -> Self {
        Self {
            start_ledger,
            filters,
            end_ledger: None,
        }
    }

    /// Bound the subscription to `end_ledger` (inclusive).
    #[must_use]
    pub fn with_end_ledger(mut self, end_ledger: u32) -> Self {
        self.end_ledger = Some(end_ledger);
        self
    }
}

/// Acknowledgement returned when the server accepts an `events`
/// subscription.
///
/// Fields are deserialized leniently because the upstream v2 ack shape is
/// still settling; the raw acknowledgement is available on
/// [`EventsSubscription`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventsAck {
    /// Latest ledger the server knows about at subscription time.
    #[serde(default)]
    pub latest_ledger: Option<u64>,
}

/// A live `events` subscription.
#[derive(Debug, Clone)]
pub struct EventsSubscription {
    /// Ledger the subscription started from.
    pub start_ledger: u32,
    /// Raw acknowledgement payload returned by the server.
    pub ack_raw: Value,
    /// Parsed acknowledgement.
    pub ack: EventsAck,
}

/// A single event pushed by the server on an `events` subscription.
#[derive(Debug, Clone)]
pub struct StreamedEvent {
    /// Server-assigned subscription id, when the notification carries one.
    pub subscription_id: Option<u64>,
    /// The event payload. Kept as raw JSON because the v2 event object shape
    /// is still being finalized upstream.
    pub event: Value,
    /// The full notification `params` object.
    pub raw: Value,
}

/// A minimal JSON-RPC 2.0 client over a WebSocket connection.
///
/// Unlike the HTTP [`RpcClient`](crate::rpc::client::RpcClient) the
/// connection is stateful: request ids increase monotonically and the caller
/// reads server-push notifications off the same stream.
///
/// # Network calls
/// `connect` opens one WebSocket connection; each `call`/`subscribe_events`
/// exchanges messages over it.
#[derive(Debug)]
pub struct WsRpcClient {
    stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    next_id: u64,
}

impl WsRpcClient {
    /// Connect to a WebSocket RPC endpoint (e.g. `wss://host/ws`).
    ///
    /// # Network calls
    /// Performs the WebSocket opening handshake with the endpoint.
    pub async fn connect(url: &str) -> AppResult<Self> {
        debug!(url, "connecting to WebSocket RPC endpoint");
        let (stream, _) = tokio_tungstenite::connect_async(url)
            .await
            .map_err(|e| AppError::WsConnect(format!("{url}: {e}")))?;
        trace!(url, "WebSocket RPC connection established");
        Ok(Self { stream, next_id: 1 })
    }

    /// Send a JSON-RPC 2.0 request and await the matching response.
    ///
    /// Server notifications received while waiting are skipped; the first
    /// response whose `id` matches the request is returned.
    ///
    /// # Network calls
    /// Sends one request frame and reads frames until the matching response.
    pub async fn call_raw(&mut self, method: &str, params: Value) -> AppResult<Value> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        trace!(method, id, "sending WebSocket RPC request");
        self.send_json(&request).await?;

        loop {
            match self.next_message().await? {
                WsMessage::Response {
                    id: response_id,
                    body,
                } if response_id == id => {
                    return rpc_result(&body);
                }
                // Notifications (e.g. events on other subscriptions) and
                // stale responses to earlier requests are unrelated to this
                // call; skip them and keep waiting for our own id.
                WsMessage::Notification { .. } | WsMessage::Response { .. } => {}
                WsMessage::Closed => return Err(AppError::WsClosed),
            }
        }
    }

    /// Send a JSON-RPC 2.0 request and deserialize the response `result`.
    ///
    /// # Network calls
    /// Delegates to [`Self::call_raw`].
    pub async fn call<T: serde::de::DeserializeOwned>(
        &mut self,
        method: &str,
        params: Value,
    ) -> AppResult<T> {
        let result = self.call_raw(method, params).await?;
        serde_json::from_value(result)
            .map_err(|e| AppError::General(format!("failed to deserialize RPC response: {e}")))
    }

    /// Subscribe to real-time contract events.
    ///
    /// Sends the `events` request and waits for the server's acknowledgement
    /// response. Streamed events are then delivered via [`Self::next_event`].
    ///
    /// # Network calls
    /// Sends one `events` request and reads frames until the ack response.
    pub async fn subscribe_events(
        &mut self,
        params: EventsParams,
    ) -> AppResult<EventsSubscription> {
        debug!(
            start_ledger = params.start_ledger,
            filter_count = params.filters.len(),
            "subscribing to events"
        );
        let ack_raw = self
            .call_raw("events", serde_json::to_value(params.clone())?)
            .await?;
        let ack: EventsAck = serde_json::from_value(ack_raw.clone())
            .map_err(|e| AppError::WsProtocol(format!("unrecognized events ack payload: {e}")))?;
        trace!(
            latest_ledger = ?ack.latest_ledger,
            "events subscription acknowledged"
        );
        Ok(EventsSubscription {
            start_ledger: params.start_ledger,
            ack_raw,
            ack,
        })
    }

    /// Read the next streamed event on the current `events` subscription.
    ///
    /// Skips stray responses and unrelated notifications; returns `None` when
    /// the server closes the connection.
    ///
    /// # Network calls
    /// Reads WebSocket frames until an `events` notification or a close.
    pub async fn next_event(&mut self) -> AppResult<Option<StreamedEvent>> {
        loop {
            match self.next_message().await? {
                WsMessage::Notification { method, params, .. } if method == "events" => {
                    let subscription_id = params.get("subscriptionId").and_then(Value::as_u64);
                    let event = params.get("event").cloned().unwrap_or(Value::Null);
                    trace!(?subscription_id, "received streamed event");
                    return Ok(Some(StreamedEvent {
                        subscription_id,
                        event,
                        raw: params,
                    }));
                }
                WsMessage::Notification { .. } | WsMessage::Response { .. } => {
                    // Not our events (or a checkpoint response); keep reading.
                }
                WsMessage::Closed => return Ok(None),
            }
        }
    }

    /// Read the next WebSocket frame, classified as an RPC message.
    ///
    /// Control frames (ping/pong) are handled transparently and are never
    /// returned to the caller.
    ///
    /// # Network calls
    /// Reads one WebSocket frame from the connection.
    pub async fn next_message(&mut self) -> AppResult<WsMessage> {
        loop {
            let message = self
                .stream
                .next()
                .await
                .ok_or(AppError::WsClosed)?
                .map_err(|e| AppError::WsProtocol(e.to_string()))?;

            let text = match message {
                Message::Text(text) => text.as_str().to_string(),
                Message::Binary(bytes) => String::from_utf8(bytes.to_vec())
                    .map_err(|_| AppError::WsProtocol("binary frame is not UTF-8".to_string()))?,
                Message::Ping(payload) => {
                    // Echo the ping so the peer knows we are alive; tungstenite
                    // auto-queues the pong, and an explicit send flushes it.
                    self.stream
                        .send(Message::Pong(payload))
                        .await
                        .map_err(|e| AppError::WsProtocol(e.to_string()))?;
                    continue;
                }
                Message::Pong(_) | Message::Frame(_) => continue,
                Message::Close(_) => return Ok(WsMessage::Closed),
            };

            let body: Value = serde_json::from_str(&text)
                .map_err(|e| AppError::WsProtocol(format!("malformed RPC message: {e}")))?;

            if let Some(method) = body.get("method").and_then(Value::as_str) {
                let params = body.get("params").cloned().unwrap_or(Value::Null);
                return Ok(WsMessage::Notification {
                    method: method.to_string(),
                    params,
                    body,
                });
            }
            let id = body.get("id").cloned().unwrap_or(Value::Null);
            return Ok(WsMessage::Response { id, body });
        }
    }

    /// Send a JSON text frame.
    ///
    /// # Network calls
    /// Writes one WebSocket frame to the connection.
    async fn send_json(&mut self, value: &Value) -> AppResult<()> {
        let text = serde_json::to_string(value)?;
        self.stream
            .send(Message::text(text))
            .await
            .map_err(|e| AppError::WsProtocol(e.to_string()))
    }

    /// Close the WebSocket connection cleanly.
    ///
    /// # Network calls
    /// Sends a WebSocket close frame.
    pub async fn close(&mut self) -> AppResult<()> {
        self.stream
            .close(None)
            .await
            .map_err(|e| AppError::WsProtocol(e.to_string()))
    }
}

/// Extract the JSON-RPC `result` from a response body, mapping `error`
/// objects onto [`AppError::Rpc`] (mirroring the HTTP client).
fn rpc_result(body: &Value) -> AppResult<Value> {
    if let Some(error) = body.get("error") {
        let code = error.get("code").and_then(Value::as_i64).unwrap_or(-1);
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown error")
            .to_string();
        debug!(code, message, "WebSocket RPC error");
        return Err(AppError::Rpc {
            status: code,
            message,
        });
    }

    body.get("result")
        .cloned()
        .ok_or_else(|| AppError::WsProtocol("response missing 'result' field".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;

    /// Marker reply that makes the mock server close the connection.
    const CLOSE_MARKER: &str = "__CLOSE__";

    /// Spawn a mock WebSocket RPC server on an ephemeral port.
    ///
    /// `handle` receives the raw text of each client frame and returns the
    /// raw texts to send back (possibly empty); the `__CLOSE__` marker closes
    /// the connection server-side. Returns the `ws://` URL.
    async fn spawn_mock_server(handle: impl Fn(String) -> Vec<String> + Send + 'static) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            while let Some(msg) = ws.next().await {
                let msg = msg.unwrap();
                match msg {
                    Message::Text(text) => {
                        for reply in handle(text.as_str().to_string()) {
                            if reply == CLOSE_MARKER {
                                let _ = ws.close(None).await;
                                return;
                            }
                            ws.send(Message::text(reply)).await.unwrap();
                        }
                    }
                    Message::Close(_) => {
                        let _ = ws.close(None).await;
                        return;
                    }
                    _ => {}
                }
            }
        });
        format!("ws://{addr}/ws")
    }

    /// Parse the JSON-RPC request id out of a request body.
    fn request_id(text: &str) -> u64 {
        let body: Value = serde_json::from_str(text).unwrap();
        body.get("id").and_then(Value::as_u64).unwrap()
    }

    /// A mock `health` response echoing the request id.
    fn health_reply(request: &str) -> String {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id(request),
            "result": { "status": "healthy" },
        })
        .to_string()
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct HealthResult {
        status: String,
    }

    #[tokio::test]
    async fn test_connect_and_call_health() {
        let url = spawn_mock_server(|request| vec![health_reply(&request)]).await;

        let mut client = WsRpcClient::connect(&url).await.unwrap();
        let result: HealthResult = client.call("health", serde_json::json!({})).await.unwrap();
        assert_eq!(
            result,
            HealthResult {
                status: "healthy".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn test_call_maps_rpc_error() {
        let url = spawn_mock_server(|request| {
            vec![
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": request_id(&request),
                    "error": { "code": -32601, "message": "method not found" },
                })
                .to_string(),
            ]
        })
        .await;

        let mut client = WsRpcClient::connect(&url).await.unwrap();
        let err = client
            .call::<Value>("noSuchMethod", serde_json::json!({}))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            AppError::Rpc {
                status: -32_601,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn test_call_skips_interleaved_notifications() {
        // The server pushes an unrelated notification before answering the
        // request; the client must skip it and still return the response.
        let url = spawn_mock_server(|request| {
            let id = request_id(&request);
            vec![
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "events",
                    "params": { "subscriptionId": 1, "event": { "type": "contract" } },
                })
                .to_string(),
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "status": "healthy" },
                })
                .to_string(),
            ]
        })
        .await;

        let mut client = WsRpcClient::connect(&url).await.unwrap();
        let result: HealthResult = client.call("health", serde_json::json!({})).await.unwrap();
        assert_eq!(
            result,
            HealthResult {
                status: "healthy".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn test_events_subscription_streams_events_then_close() {
        let url = spawn_mock_server(|request| {
            let body: Value = serde_json::from_str(&request).unwrap();
            let method = body.get("method").and_then(Value::as_str).unwrap();
            let id = request_id(&request);
            if method == "events" {
                let ack = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "latestLedger": 1234 },
                })
                .to_string();
                let event1 = serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "events",
                    "params": { "subscriptionId": 7, "event": { "ledger": 1234, "type": "contract" } },
                })
                .to_string();
                let event2 = serde_json::json!({
                    "jsonrpc": "2.0",
                    "method": "events",
                    "params": { "subscriptionId": 7, "event": { "ledger": 1235, "type": "contract" } },
                })
                .to_string();
                vec![ack, event1, event2, CLOSE_MARKER.to_string()]
            } else {
                vec![]
            }
        })
        .await;

        let mut client = WsRpcClient::connect(&url).await.unwrap();
        let filter = EventFilter::new(EventFilterType::Contract)
            .with_contract_id("CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC");
        let params = EventsParams::new(1_200, vec![filter]);
        let subscription = client.subscribe_events(params).await.unwrap();

        assert_eq!(subscription.start_ledger, 1_200);
        assert_eq!(subscription.ack.latest_ledger, Some(1_234));
        assert_eq!(
            subscription.ack_raw.get("latestLedger"),
            Some(&Value::from(1_234))
        );

        let first = client.next_event().await.unwrap().expect("first event");
        assert_eq!(first.subscription_id, Some(7));
        assert_eq!(first.event.get("ledger"), Some(&Value::from(1_234)));

        let second = client.next_event().await.unwrap().expect("second event");
        assert_eq!(second.event.get("ledger"), Some(&Value::from(1_235)));

        // Server closes the connection after the events; the stream ends.
        let closed = client.next_event().await.unwrap();
        assert!(closed.is_none());
    }

    #[tokio::test]
    async fn test_events_subscription_missing_ack_fields_is_tolerant() {
        // Upstream v2 ack shape is unsettled; a minimal ack must not fail.
        let url = spawn_mock_server(|request| {
            let id = request_id(&request);
            vec![
                serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {},
                })
                .to_string(),
            ]
        })
        .await;

        let mut client = WsRpcClient::connect(&url).await.unwrap();
        let params = EventsParams::new(1, Vec::new());
        let subscription = client.subscribe_events(params).await.unwrap();
        assert_eq!(subscription.ack.latest_ledger, None);
    }

    #[tokio::test]
    async fn test_connect_refused_returns_ws_connect_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let err = WsRpcClient::connect(&format!("ws://{addr}/ws"))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::WsConnect(_)));
    }

    #[tokio::test]
    async fn test_close_makes_connection_unusable() {
        let url = spawn_mock_server(|_| vec![]).await;
        let mut client = WsRpcClient::connect(&url).await.unwrap();
        client.close().await.unwrap();
        // After initiating the close handshake the connection is done: the
        // next read must fail fast (close or protocol error), never block.
        let result = tokio::time::timeout(std::time::Duration::from_secs(5), client.next_message())
            .await
            .expect("read after close must not block");
        assert!(
            result.is_err() || matches!(result, Ok(WsMessage::Closed)),
            "expected closed or error, got {result:?}"
        );
    }
}
