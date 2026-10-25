use serde::{Deserialize, Serialize};
use stellar_xdr::ReadXdr;
use tracing::{debug, trace};

use crate::error::{AppError, AppResult};
use crate::rpc::client::RpcClient;

/// Parameters for the `simulateTransaction` RPC call.
#[derive(Debug, Serialize)]
pub struct SimulateTransactionParams {
    /// Base64-encoded TransactionEnvelope XDR.
    pub transaction: String,
    /// Optional resource configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_config: Option<Value>,
}

use serde_json::Value;

/// Response from the `simulateTransaction` RPC call.
///
/// Soroban RPC serializes its JSON fields in **camelCase** (`latestLedger`,
/// `minResourceFee`, `transactionData`, `cost.cpuInsns`, …), so the struct
/// uses `rename_all = "camelCase"` to match. Without it every field would
/// silently default to `None` and reports would render as all zeros with
/// ledger 0 — the exact failure seen when the invocation path was first
/// exercised against a live deployed contract.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulateTransactionResponse {
    /// Transaction data XDR (base64-encoded SorobanTransactionData).
    #[serde(default)]
    pub transaction_data: Option<String>,
    /// Resource usage results.
    #[serde(default)]
    pub cost: Option<CostResult>,
    /// Failure details if the simulation could not execute (issue #532).
    ///
    /// The RPC has returned two shapes over its lifetime: the legacy
    /// flattened string, and a structured object carrying a machine-readable
    /// `code` plus an optional `data` payload. Both are accepted and
    /// normalised into a [`SimulationError`] so callers never have to parse
    /// the message to tell failure classes apart.
    #[serde(default, deserialize_with = "deserialize_simulation_error")]
    pub error: Option<SimulationError>,
    /// Latest ledger sequence number. May arrive as a JSON string or number
    /// depending on the RPC version, so both are accepted.
    #[serde(default, deserialize_with = "deserialize_u64_flexible_opt")]
    pub latest_ledger: Option<u64>,
    /// Events emitted during simulation (issue #525). Each item is the
    /// RPC's raw JSON event object (`{ type, topic: [b64 ScVal XDR], data:
    /// b64 ScVal XDR, … }`); older RPC versions returned plain base64 XDR
    /// strings. Items are kept as raw `Value` so a version-drifted or
    /// malformed payload can never fail the whole response —
    /// [`crate::report::cost_report::build_simulation_diagnostics`]
    /// summarizes them leniently.
    #[serde(default)]
    pub events: Option<Vec<Value>>,
    /// Minimum resource fee (base64-encoded int64 XDR).
    #[serde(default)]
    pub min_resource_fee: Option<String>,
    /// Restore fee if state archival entry needs to be restored.
    #[serde(default)]
    pub restore_fee: Option<String>,
    /// State changes produced by the simulation (issue #525): raw
    /// `LedgerEntryChange` objects (`{ key, created?, updated?, deleted? }`,
    /// payloads base64 XDR). Kept as raw `Value` for the same leniency
    /// reason as `events`.
    #[serde(default)]
    pub state_changes: Option<Vec<Value>>,
}

impl SimulateTransactionResponse {
    /// Latest ledger sequence the simulation ran against, as a `u32`.
    ///
    /// Soroban returns `latestLedger` as a `u64`, but ledger sequences fit in
    /// `u32`. The conversion is therefore saturating: when the field is absent,
    /// or holds a value beyond `u32::MAX`, `0` is returned and callers surface
    /// it as "unknown ledger" (issue #329).
    #[must_use]
    pub fn ledger_sequence(&self) -> u32 {
        self.latest_ledger
            .and_then(|ledger| u32::try_from(ledger).ok())
            .unwrap_or(0)
    }
}

/// Maximum number of bytes retained for a simulation error's structured
/// `data` payload.
///
/// Error payloads come from the remote endpoint, so they are untrusted input.
/// Keeping the retained details bounded means a misbehaving (or hostile)
/// server cannot make the CLI buffer or print an unbounded blob, and keeps the
/// error's JSON representation small enough to read.
pub const MAX_SIMULATION_ERROR_DATA_BYTES: usize = 1_024;

/// Maximum nesting depth retained inside a simulation error's `data`.
const MAX_ERROR_DATA_DEPTH: usize = 4;

/// Maximum number of array items (or object keys) retained per level.
const MAX_ERROR_DATA_ITEMS: usize = 16;

/// Maximum number of characters retained from one string in the payload.
const MAX_ERROR_DATA_STRING_CHARS: usize = 128;

/// A structured `simulateTransaction` failure.
///
/// Wraps the three fields the RPC may report: a machine-readable `code`, a
/// human-readable `message`, and an optional structured `data` payload. The
/// type is `Serialize` so integrators get a stable JSON representation — the
/// keys are always `code`, `data`, and `message`, and `code`/`data` are
/// omitted when the RPC did not supply them.
///
/// `data` is bounded by [`MAX_SIMULATION_ERROR_DATA_BYTES`] (and by depth,
/// item count, and string length) when it is decoded, so serialising it
/// cannot produce an unbounded document.
///
/// `Display` renders only the concise human message plus the code when one is
/// present — the payload is accessible through the fields or the JSON
/// representation, never dumped into an error line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationError {
    /// Machine-readable error code, when the RPC supplied one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<i64>,
    /// Human-readable failure message. Never empty.
    pub message: String,
    /// Structured details, bounded when decoded (see the type docs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl SimulationError {
    /// A message-only failure, for callers that have no code or payload.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            code: None,
            message: message.into(),
            data: None,
        }
    }

    /// A stable JSON representation of this failure.
    ///
    /// The keys are always the same three (`code`, `data`, `message`), which
    /// makes the document safe to index into from a pipeline. Serialisation of
    /// an already-bounded `Value` cannot fail; the empty object is a defensive
    /// fallback rather than a reachable path.
    #[must_use]
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| Value::Object(serde_json::Map::new()))
    }
}

impl std::fmt::Display for SimulationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.code {
            Some(code) => write!(f, "{} (error code {code})", self.message),
            None => f.write_str(&self.message),
        }
    }
}

/// Untagged mirror of the two shapes the RPC's `error` field has taken.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SimulationErrorPayload {
    /// Legacy shape: a single flattened message string.
    Message(String),
    /// Structured shape: an object with a code, a message and optional data.
    Structured(SimulationErrorFields),
}

/// The structured variant of [`SimulationErrorPayload`].
///
/// Every field is optional at this layer: the RPC is free to omit `message`
/// (falling back to a message derived from the code) and to send keys the
/// tool does not know about, which are ignored.
#[derive(Debug, Deserialize)]
struct SimulationErrorFields {
    #[serde(default, deserialize_with = "deserialize_i64_flexible_opt")]
    code: Option<i64>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    data: Option<Value>,
}

/// Deserialize an optional `i64` that may arrive as a JSON string, number, or
/// null.
///
/// RPC versions disagree on whether error codes are quoted (as the
/// `latestLedger`/`cpuInsns` fields are), so both forms are accepted rather
/// than failing the whole response over a quoting difference.
fn deserialize_i64_flexible_opt<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(text)) => text
            .trim()
            .parse::<i64>()
            .map(Some)
            .map_err(serde::de::Error::custom),
        Some(serde_json::Value::Number(number)) => number
            .as_i64()
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom("expected i64")),
        Some(_) => Err(serde::de::Error::custom("expected string, number, or null")),
    }
}

/// Deserialize the `error` field of a `simulateTransaction` response.
///
/// Accepts `null`, the legacy string form, and the structured object form,
/// normalising all of them to `Option<SimulationError>`.
fn deserialize_simulation_error<'de, D>(
    deserializer: D,
) -> Result<Option<SimulationError>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    let Some(value) = value else {
        return Ok(None);
    };

    let Ok(payload) = serde_json::from_value::<SimulationErrorPayload>(value.clone()) else {
        // An error payload the tool does not recognise must not fail the whole
        // response (that would hide the failure behind a JSON error). Keep it
        // as bounded structured details instead.
        return Ok(Some(SimulationError {
            code: None,
            message: "simulation failed with an unrecognized error payload".to_string(),
            data: bound_error_data(value),
        }));
    };

    Ok(Some(match payload {
        SimulationErrorPayload::Message(message) => SimulationError {
            code: None,
            message,
            data: None,
        },
        SimulationErrorPayload::Structured(fields) => {
            let message = fields
                .message
                .filter(|message| !message.is_empty())
                .unwrap_or_else(|| match fields.code {
                    Some(code) => format!("simulation failed with error code {code}"),
                    None => "simulation failed".to_string(),
                });
            SimulationError {
                code: fields.code,
                message,
                data: fields.data.and_then(bound_error_data),
            }
        }
    }))
}

/// Bound an untrusted error payload: cap its depth, item counts, per-string
/// length, and finally its serialised size.
///
/// The common case (a small object such as `{"type": "invokeHostFunction"}`)
/// passes through untouched. Anything larger is normalised into a smaller,
/// deterministic value; when even the normalised form is too big for
/// [`MAX_SIMULATION_ERROR_DATA_BYTES`], it is replaced by a truncated JSON
/// string that states how many bytes were dropped, rather than being silently
/// discarded.
fn bound_error_data(value: Value) -> Option<Value> {
    let bounded = bound_value(value, 0);
    let serialized = serde_json::to_string(&bounded).ok()?;
    if serialized.len() <= MAX_SIMULATION_ERROR_DATA_BYTES {
        return Some(bounded);
    }

    // Serialising the retained fragment as a JSON string can escape every
    // character it contains (worst case doubles its length), so only a
    // quarter of the budget holds the fragment; the marker tail is fixed and
    // short. That keeps the *serialised* payload inside
    // `MAX_SIMULATION_ERROR_DATA_BYTES` even in the worst case.
    let mut end = MAX_SIMULATION_ERROR_DATA_BYTES / 4;
    while end > 0 && !serialized.is_char_boundary(end) {
        end -= 1;
    }
    let dropped = serialized.len() - end;
    Some(Value::String(format!(
        "{}… ({dropped} bytes truncated)",
        &serialized[..end]
    )))
}

/// Recursive half of [`bound_error_data`].
fn bound_value(value: Value, depth: usize) -> Value {
    if depth >= MAX_ERROR_DATA_DEPTH {
        return Value::String("… (nested value omitted)".to_string());
    }

    match value {
        Value::String(text) => Value::String(truncate_chars(&text, MAX_ERROR_DATA_STRING_CHARS)),
        Value::Array(items) => {
            let total = items.len();
            let mut bounded: Vec<Value> = items
                .into_iter()
                .take(MAX_ERROR_DATA_ITEMS)
                .map(|item| bound_value(item, depth + 1))
                .collect();
            if total > MAX_ERROR_DATA_ITEMS {
                bounded.push(Value::String(format!(
                    "… ({} more items)",
                    total - MAX_ERROR_DATA_ITEMS
                )));
            }
            Value::Array(bounded)
        }
        Value::Object(map) => {
            let total = map.len();
            let mut bounded = serde_json::Map::new();
            for (key, item) in map.into_iter().take(MAX_ERROR_DATA_ITEMS) {
                bounded.insert(
                    truncate_chars(&key, MAX_ERROR_DATA_STRING_CHARS),
                    bound_value(item, depth + 1),
                );
            }
            if total > MAX_ERROR_DATA_ITEMS {
                bounded.insert(
                    "…truncated".to_string(),
                    Value::String(format!("{} more keys", total - MAX_ERROR_DATA_ITEMS)),
                );
            }
            Value::Object(bounded)
        }
        other => other,
    }
}

/// Truncate `text` to at most `max_chars` characters, appending an ellipsis
/// marker when anything was dropped.
fn truncate_chars(text: &str, max_chars: usize) -> String {
    match text.char_indices().nth(max_chars) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}

/// Cost breakdown from the simulation result.
///
/// The RPC serializes these as `cpuInsns`/`memBytes` (camelCase). Values may
/// arrive as JSON strings or as JSON numbers depending on the RPC version,
/// so both are accepted.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CostResult {
    /// CPU instructions consumed.
    #[serde(deserialize_with = "deserialize_u64_flexible")]
    pub cpu_insns: u64,
    /// Memory bytes used.
    #[serde(deserialize_with = "deserialize_u64_flexible")]
    pub mem_bytes: u64,
}

/// Deserialize a u64 that may arrive as a JSON string or a JSON number.
fn deserialize_u64_flexible<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(s) => s.parse::<u64>().map_err(serde::de::Error::custom),
        serde_json::Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| serde::de::Error::custom("expected u64")),
        _ => Err(serde::de::Error::custom("expected string or number")),
    }
}

/// Deserialize an optional u64 that may arrive as a JSON string, number, or
/// null.
fn deserialize_u64_flexible_opt<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::String(s) => {
            s.parse::<u64>().map(Some).map_err(serde::de::Error::custom)
        }
        serde_json::Value::Number(n) => n
            .as_u64()
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom("expected u64")),
        _ => Err(serde::de::Error::custom("expected string, number, or null")),
    }
}

/// Calls `simulateTransaction` with the given base64-encoded transaction envelope.
///
/// # Network calls
/// Makes one `simulateTransaction` RPC call to the Soroban RPC endpoint.
pub async fn simulate_transaction(
    client: &RpcClient,
    transaction_xdr: &str,
) -> AppResult<SimulateTransactionResponse> {
    simulate_transaction_inner(client, transaction_xdr, true).await
}

/// Like [`simulate_transaction`], but bypasses the client's request
/// deduplication so every call performs a fresh network round-trip.
///
/// `estimate --repeat` uses this to benchmark the RPC endpoint: with dedup a
/// repeated identical `simulateTransaction` would be served from the cache
/// after the first call, so latency and fee determinism could not be measured.
///
/// # Network calls
/// Makes one (retryable) `simulateTransaction` RPC call per invocation.
pub async fn simulate_transaction_uncached(
    client: &RpcClient,
    transaction_xdr: &str,
) -> AppResult<SimulateTransactionResponse> {
    simulate_transaction_inner(client, transaction_xdr, false).await
}

/// Shared implementation for the cached and uncached `simulateTransaction`
/// entry points.
async fn simulate_transaction_inner(
    client: &RpcClient,
    transaction_xdr: &str,
    dedup: bool,
) -> AppResult<SimulateTransactionResponse> {
    debug!(dedup, "calling simulateTransaction");
    let params = SimulateTransactionParams {
        transaction: transaction_xdr.to_string(),
        resource_config: None,
    };
    let params = serde_json::to_value(params)?;

    let response: SimulateTransactionResponse = if dedup {
        client.call("simulateTransaction", params).await?
    } else {
        client.call_uncached("simulateTransaction", params).await?
    };

    if let Some(ref error) = response.error {
        // The structured details are logged, never spliced into the error
        // line: `Display` for the error is the concise human message.
        debug!(
            code = ?error.code,
            message = %error.message,
            "simulation returned error"
        );
        return Err(AppError::SimulationFailed(Box::new(error.clone())));
    }

    trace!(
        has_cost = response.cost.is_some(),
        has_transaction_data = response.transaction_data.is_some(),
        latest_ledger = ?response.latest_ledger,
        "simulateTransaction succeeded"
    );
    Ok(response)
}

/// Extracts the authoritative total resource fee (stroops) from the
/// `transaction_data` field of a `simulateTransaction` response.
///
/// `SorobanTransactionData.resource_fee` is the total Soroban resource fee
/// the transaction must cover: the non-refundable portion (instructions,
/// ledger I/O, transaction size) plus the refundable portion (events, ledger
/// rent bumps). It is the XDR-derived authority for the total; the RPC's
/// `minResourceFee` field should carry the same value.
pub fn parse_transaction_data_resource_fee(
    transaction_data: &Option<String>,
) -> AppResult<Option<i64>> {
    match transaction_data {
        Some(data_b64) => {
            let bytes =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data_b64)
                    .map_err(|e| AppError::XdrDecode(format!("invalid base64: {e}")))?;

            let data =
                stellar_xdr::SorobanTransactionData::from_xdr(&bytes, stellar_xdr::Limits::none())
                    .map_err(|e| AppError::XdrDecode(format!("SorobanTransactionData: {e}")))?;

            Ok(Some(data.resource_fee))
        }
        None => Ok(None),
    }
}

/// Resource consumption derived from the simulation's `transactionData`.
///
/// Modern Soroban RPC versions no longer return a `cost` object from
/// `simulateTransaction`; the authoritative resource usage now lives inside
/// the `transactionData` XDR (`SorobanTransactionData.resources`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimulationResources {
    /// CPU instructions consumed.
    pub cpu_insns: u64,
    /// Number of ledger entries read.
    pub read_entries: usize,
    /// Number of ledger entries written.
    pub write_entries: usize,
    /// Disk bytes read.
    pub read_bytes: u64,
    /// Bytes written.
    pub write_bytes: u64,
}

/// Extracts resource consumption from the `transactionData` field of a
/// `simulateTransaction` response.
///
/// Modern RPC versions return the resources used by the simulation (CPU
/// instructions, footprint sizes, disk read/write bytes) inside
/// `SorobanTransactionData.resources`, while older versions returned a
/// separate `cost` object. This is the modern-shape source for those values.
pub fn parse_transaction_data_resources(
    transaction_data: &Option<String>,
) -> AppResult<Option<SimulationResources>> {
    match transaction_data {
        Some(data_b64) => {
            let bytes =
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data_b64)
                    .map_err(|e| AppError::XdrDecode(format!("invalid base64: {e}")))?;

            let data =
                stellar_xdr::SorobanTransactionData::from_xdr(&bytes, stellar_xdr::Limits::none())
                    .map_err(|e| AppError::XdrDecode(format!("SorobanTransactionData: {e}")))?;

            Ok(Some(SimulationResources {
                cpu_insns: data.resources.instructions as u64,
                read_entries: data.resources.footprint.read_only.len(),
                write_entries: data.resources.footprint.read_write.len(),
                read_bytes: data.resources.disk_read_bytes as u64,
                write_bytes: data.resources.write_bytes as u64,
            }))
        }
        None => Ok(None),
    }
}

/// Extracts the resource fee from the simulation response in stroops.
///
/// Modern RPC versions return `min_resource_fee` as a plain decimal string
/// of stroops (e.g. `"15427"`); older versions returned a base64-encoded
/// XDR int64. Both forms are accepted.
pub fn parse_resource_fee(min_resource_fee: &Option<String>) -> AppResult<Option<i64>> {
    match min_resource_fee {
        Some(fee_str) => {
            // Modern form: plain decimal stroops.
            if let Ok(stroops) = fee_str.trim().parse::<i64>() {
                return Ok(Some(stroops));
            }

            // Legacy form: base64-encoded XDR int64 (big-endian, 8 bytes).
            let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, fee_str)
                .map_err(|e| AppError::General(format!("resource fee: invalid base64: {e}")))?;

            let fee_bytes: [u8; 8] = bytes[..8].try_into().map_err(|_| {
                AppError::General(format!(
                    "resource fee: invalid XDR length: {} bytes",
                    bytes.len()
                ))
            })?;
            Ok(Some(i64::from_be_bytes(fee_bytes)))
        }
        None => Ok(None),
    }
}

/// Aggregate latency statistics over repeated simulation runs.
///
/// Produced by [`summarize_latencies`] so the `estimate --repeat` summary is
/// computed in one place and unit-testable independently of CLI formatting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatencyStats {
    /// Number of latency samples the statistics were computed from.
    pub count: usize,
    /// Fastest observed round-trip, in milliseconds.
    pub min_ms: u64,
    /// Slowest observed round-trip, in milliseconds.
    pub max_ms: u64,
    /// Arithmetic mean round-trip, in whole milliseconds.
    pub mean_ms: u64,
    /// Population standard deviation, in milliseconds.
    pub stddev_ms: f64,
}

/// Computes [`LatencyStats`] from a slice of per-run latencies (milliseconds).
///
/// Returns `None` when `samples` is empty. The mean uses integer-only
/// arithmetic (`sum / count`) to avoid floating-point drift; the standard
/// deviation is the population standard deviation
/// (`sqrt(Σ(x - μ)² / n)`) computed in `f64` for display.
pub fn summarize_latencies(samples: &[u64]) -> Option<LatencyStats> {
    if samples.is_empty() {
        return None;
    }
    let count = samples.len();
    let min_ms = samples.iter().copied().min().unwrap_or(0);
    let max_ms = samples.iter().copied().max().unwrap_or(0);
    let sum: u64 = samples.iter().copied().sum();
    let mean_ms = sum / count as u64;
    let variance = samples
        .iter()
        .map(|&sample| {
            let delta = sample as f64 - mean_ms as f64;
            delta * delta
        })
        .sum::<f64>()
        / count as f64;
    Some(LatencyStats {
        count,
        min_ms,
        max_ms,
        mean_ms,
        stddev_ms: variance.sqrt(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use stellar_xdr::WriteXdr;

    #[test]
    fn test_summarize_latencies_empty_is_none() {
        assert_eq!(summarize_latencies(&[]), None);
    }

    #[test]
    fn test_summarize_latencies_single_sample() {
        let stats = summarize_latencies(&[42]).expect("one sample");
        assert_eq!(stats.count, 1);
        assert_eq!(stats.min_ms, 42);
        assert_eq!(stats.max_ms, 42);
        assert_eq!(stats.mean_ms, 42);
        assert!(stats.stddev_ms.abs() < f64::EPSILON);
    }

    #[test]
    fn test_summarize_latencies_computes_min_max_mean_stddev() {
        // Samples: 10, 20, 30, 40 → mean 25, population stddev sqrt(125)≈11.18.
        let stats = summarize_latencies(&[10, 20, 30, 40]).expect("samples");
        assert_eq!(stats.count, 4);
        assert_eq!(stats.min_ms, 10);
        assert_eq!(stats.max_ms, 40);
        assert_eq!(stats.mean_ms, 25);
        assert!((stats.stddev_ms - 11.180_339_887).abs() < 1e-6);
    }

    #[test]
    fn test_summarize_latencies_uses_integer_mean() {
        // Integer-only mean: (1 + 2) / 2 == 1, never 1.5.
        let stats = summarize_latencies(&[1, 2]).expect("samples");
        assert_eq!(stats.mean_ms, 1);
    }

    /// Regression: Soroban RPC returns camelCase JSON. Without
    /// `rename_all = "camelCase"` every field would deserialize to `None`,
    /// producing an all-zero cost report with ledger 0.
    #[test]
    fn test_simulate_response_deserializes_camelcase() {
        let json = r#"{
            "transactionData": null,
            "cost": { "cpuInsns": "100000", "memBytes": "2000" },
            "error": null,
            "latestLedger": "3894195",
            "events": [],
            "minResourceFee": null,
            "restoreFee": null,
            "stateChanges": []
        }"#;
        let resp: SimulateTransactionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.latest_ledger, Some(3_894_195));
        let cost = resp.cost.expect("cost should parse from cpuInsns/memBytes");
        assert_eq!(cost.cpu_insns, 100_000);
        assert_eq!(cost.mem_bytes, 2_000);

        // The live testnet RPC returns numeric JSON values, not strings:
        // both forms must deserialize.
        let numeric_json = r#"{
            "cost": { "cpuInsns": 100000, "memBytes": 2000 },
            "latestLedger": 3894195
        }"#;
        let resp_num: SimulateTransactionResponse = serde_json::from_str(numeric_json).unwrap();
        assert_eq!(resp_num.latest_ledger, Some(3_894_195));
        let cost_num = resp_num
            .cost
            .expect("cost should parse from numeric cpuInsns/memBytes");
        assert_eq!(cost_num.cpu_insns, 100_000);
        assert_eq!(cost_num.mem_bytes, 2_000);
    }

    /// Issue #329: the ledger sequence must propagate out of the simulation
    /// response, tolerating both the string and numeric forms the RPC uses.
    #[test]
    fn test_ledger_sequence_extracts_latest_ledger() {
        for json in [
            r#"{ "latestLedger": 1234567 }"#,
            r#"{ "latestLedger": "1234567" }"#,
        ] {
            let resp: SimulateTransactionResponse =
                serde_json::from_str(json).expect("response should parse");
            assert_eq!(resp.ledger_sequence(), 1_234_567);
        }
    }

    // ── Structured simulation errors (#532) ──────────────────────────

    /// Legacy responses report the failure as one flattened string; nothing
    /// is invented for the code or the payload.
    #[test]
    fn test_simulate_response_deserializes_legacy_string_error() {
        let resp: SimulateTransactionResponse =
            serde_json::from_str(r#"{ "error": "host invocation failed", "latestLedger": 7 }"#)
                .expect("response should parse");

        let error = resp.error.expect("legacy string error should be retained");
        assert_eq!(error.code, None);
        assert_eq!(error.message, "host invocation failed");
        assert_eq!(error.data, None);
        assert_eq!(error.to_string(), "host invocation failed");
    }

    /// The structured shape keeps the machine-readable code and the details
    /// so callers can tell failure classes apart without parsing the message.
    #[test]
    fn test_simulate_response_deserializes_structured_error() {
        let resp: SimulateTransactionResponse = serde_json::from_str(
            r#"{
                "error": {
                    "code": -3,
                    "message": "invocation failed",
                    "data": { "type": "auth", "detail": "missing signature" }
                },
                "latestLedger": "42"
            }"#,
        )
        .expect("response should parse");

        let error = resp.error.expect("structured error should be retained");
        assert_eq!(error.code, Some(-3));
        assert_eq!(error.message, "invocation failed");
        let data = error.data.as_ref().expect("details should be retained");
        assert_eq!(data["type"], "auth");
        assert_eq!(data["detail"], "missing signature");
        // The human message stays concise: no payload dumped into it.
        assert_eq!(error.to_string(), "invocation failed (error code -3)");
        assert!(!error.to_string().contains("missing signature"));
    }

    /// A structured error that omits `message` still produces a usable one,
    /// derived from the code, rather than an empty error line.
    #[test]
    fn test_simulate_response_structured_error_without_message() {
        let resp: SimulateTransactionResponse =
            serde_json::from_str(r#"{ "error": { "code": 9 } }"#).expect("response should parse");

        let error = resp.error.expect("error should be retained");
        assert_eq!(error.code, Some(9));
        assert_eq!(error.message, "simulation failed with error code 9");
        assert_eq!(error.data, None);
    }

    /// RPC versions disagree on whether the code is quoted; both forms are
    /// accepted rather than failing the response over a quoting difference.
    #[test]
    fn test_simulate_response_structured_error_accepts_string_code() {
        let resp: SimulateTransactionResponse =
            serde_json::from_str(r#"{ "error": { "code": "12", "message": "quoted code" } }"#)
                .expect("response should parse");

        let error = resp.error.expect("error should be retained");
        assert_eq!(error.code, Some(12));
        assert_eq!(error.message, "quoted code");
    }

    /// A payload the tool does not recognise is kept as bounded details
    /// instead of failing the whole response and hiding the failure.
    #[test]
    fn test_simulate_response_unrecognized_error_payload_is_kept() {
        let resp: SimulateTransactionResponse =
            serde_json::from_str(r#"{ "error": [1, 2, 3] }"#).expect("response should parse");

        let error = resp.error.expect("error should be retained");
        assert_eq!(error.code, None);
        assert!(
            error.message.contains("unrecognized"),
            "got: {}",
            error.message
        );
        assert_eq!(error.data, Some(serde_json::json!([1, 2, 3])));
    }

    /// No error at all is still `None` — both an explicit `null` and an
    /// omitted field.
    #[test]
    fn test_simulate_response_absent_error_is_none() {
        for json in [
            r#"{ "error": null, "latestLedger": 1 }"#,
            r#"{ "latestLedger": 1 }"#,
        ] {
            let resp: SimulateTransactionResponse =
                serde_json::from_str(json).expect("response should parse");
            assert!(resp.error.is_none(), "expected no error for {json}");
        }
    }

    /// Error details come from an untrusted endpoint, so they are bounded:
    /// deep nesting, long arrays, long strings and an oversized payload are
    /// all normalised rather than retained in full.
    #[test]
    fn test_simulate_response_error_data_is_bounded() {
        let long_string = "x".repeat(10_000);
        let deep = serde_json::json!({ "a": { "b": { "c": { "d": { "e": "too deep" } } } } });
        let many_items = serde_json::json!((0..100).collect::<Vec<u8>>());
        let json = format!(
            r#"{{ "error": {{ "code": 1, "message": "boom", "data": {{ "long": {long_string:?}, "deep": {deep}, "items": {many_items} }} }} }}"#
        );

        let resp: SimulateTransactionResponse =
            serde_json::from_str(&json).expect("response should parse");
        let error = resp.error.expect("error should be retained");
        let data = error.data.expect("details should be retained");

        let serialized = serde_json::to_string(&data).expect("bounded data serialises");
        assert!(
            serialized.len() <= MAX_SIMULATION_ERROR_DATA_BYTES + 64,
            "payload must stay bounded, was {} bytes: {serialized}",
            serialized.len()
        );
    }

    /// A small payload is preserved exactly, so the common case loses no
    /// information.
    #[test]
    fn test_simulate_response_small_error_data_is_preserved() {
        let resp: SimulateTransactionResponse = serde_json::from_str(
            r#"{ "error": { "message": "boom", "data": { "type": "invokeHostFunction" } } }"#,
        )
        .expect("response should parse");

        let data = resp.error.and_then(|e| e.data).expect("details retained");
        assert_eq!(data, serde_json::json!({ "type": "invokeHostFunction" }));
    }

    /// The JSON representation is stable for machine consumers: the same keys
    /// for every structured error, and no keys the RPC did not supply.
    #[test]
    fn test_simulation_error_json_representation() {
        let error = SimulationError {
            code: Some(3),
            message: "boom".to_string(),
            data: Some(serde_json::json!({ "k": 1 })),
        };
        let json = error.to_json();
        assert_eq!(json["code"], 3);
        assert_eq!(json["message"], "boom");
        assert_eq!(json["data"]["k"], 1);

        let bare = SimulationError::new("no details");
        let bare_json = bare.to_json();
        assert_eq!(bare_json["message"], "no details");
        assert!(bare_json.get("code").is_none(), "{bare_json}");
        assert!(bare_json.get("data").is_none(), "{bare_json}");
    }

    /// The application error keeps the code and details, and renders the
    /// concise human message.
    #[test]
    fn test_app_error_retains_simulation_details() {
        let error = SimulationError {
            code: Some(-3),
            message: "invocation failed".to_string(),
            data: Some(serde_json::json!({ "type": "auth" })),
        };
        let app_error = crate::error::AppError::SimulationFailed(Box::new(error.clone()));

        assert_eq!(
            app_error.to_string(),
            "failed to simulate transaction: invocation failed (error code -3)"
        );
        match app_error {
            crate::error::AppError::SimulationFailed(retained) => {
                assert_eq!(retained.code, Some(-3));
                assert_eq!(retained.data, error.data);
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn test_ledger_sequence_absent_is_zero() {
        let resp: SimulateTransactionResponse =
            serde_json::from_str(r#"{ "cost": { "cpuInsns": 1, "memBytes": 2 } }"#)
                .expect("response should parse");
        assert_eq!(resp.ledger_sequence(), 0);
    }

    #[test]
    fn test_ledger_sequence_out_of_range_is_zero() {
        let overflowing = u64::from(u32::MAX) + 1;
        let json = format!(r#"{{ "latestLedger": {overflowing} }}"#);
        let resp: SimulateTransactionResponse =
            serde_json::from_str(&json).expect("response should parse");
        assert_eq!(resp.ledger_sequence(), 0);
    }

    /// Round-trips a `SorobanTransactionData` through base64-XDR and asserts
    /// the `resource_fee` (the authoritative total resource fee) is extracted.
    #[test]
    fn test_parse_transaction_data_resource_fee_roundtrip() {
        let data = stellar_xdr::SorobanTransactionData {
            ext: stellar_xdr::SorobanTransactionDataExt::V0,
            resources: stellar_xdr::SorobanResources {
                footprint: stellar_xdr::LedgerFootprint {
                    read_only: stellar_xdr::VecM::default(),
                    read_write: stellar_xdr::VecM::default(),
                },
                instructions: 100_000,
                disk_read_bytes: 0,
                write_bytes: 0,
            },
            resource_fee: 1_234_567,
        };

        let xdr = data
            .to_xdr(stellar_xdr::Limits::none())
            .expect("XDR encode");
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &xdr);

        let fee = parse_transaction_data_resource_fee(&Some(b64))
            .expect("parse should succeed")
            .expect("fee should be present");
        assert_eq!(fee, 1_234_567);
    }

    #[test]
    fn test_parse_transaction_data_resource_fee_absent() {
        assert_eq!(parse_transaction_data_resource_fee(&None).unwrap(), None);
    }

    #[test]
    fn test_parse_transaction_data_resource_fee_invalid() {
        assert!(parse_transaction_data_resource_fee(&Some("not-base64!".to_string())).is_err());
    }

    /// The `minResourceFee` field is a base64-encoded XDR int64 (big-endian).
    #[test]
    fn test_parse_resource_fee_roundtrip() {
        let fee: i64 = 42_000;
        let xdr = fee.to_xdr(stellar_xdr::Limits::none()).expect("XDR encode");
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &xdr);
        assert_eq!(parse_resource_fee(&Some(b64)).unwrap(), Some(42_000));
        assert_eq!(parse_resource_fee(&None).unwrap(), None);
    }

    /// Modern RPC versions return `minResourceFee` as a plain decimal string
    /// of stroops (e.g. `"15427"`), not base64 XDR.
    #[test]
    fn test_parse_resource_fee_accepts_decimal_string() {
        assert_eq!(
            parse_resource_fee(&Some("15427".to_string())).unwrap(),
            Some(15_427)
        );
    }

    /// Regression: modern Soroban RPC responses no longer include a `cost`
    /// object — resource usage must be read from `transactionData` XDR.
    /// This fixture is a real `SorobanTransactionData` captured from a live
    /// testnet `simulateTransaction` call (increment contract, step=5),
    /// base64-encoded exactly as the RPC returns it.
    #[test]
    fn test_parse_transaction_data_resources_live_fixture() {
        // Captured from soroban-testnet.stellar.org on 2026-07-31.
        let live_transaction_data = "AAAAAAAAAAEAAAAH6hS8qZjpjw3bM46OXO9uGfBzeKO3HotPiGjO3IV+Ts0AAAABAAAABgAAAAEmU1Fc+h02S4iEBnpjdCESXpKHG/bOUxC3DeRWUy9+mQAAABQAAAABAAggFgAAAAAAAACIAAAAAAAAPEM=";

        let resources = parse_transaction_data_resources(&Some(live_transaction_data.to_string()))
            .expect("parse should succeed")
            .expect("resources should be present");

        // 0x082016 = 532,502 instructions; footprint: 1 read (contract code)
        // + 1 write (the incremented counter); 136 write bytes.
        assert_eq!(resources.cpu_insns, 532_502);
        assert_eq!(resources.read_entries, 1);
        assert_eq!(resources.write_entries, 1);
        assert_eq!(resources.read_bytes, 0);
        assert_eq!(resources.write_bytes, 136);

        // The same XDR also yields the authoritative total resource fee.
        let fee = parse_transaction_data_resource_fee(&Some(live_transaction_data.to_string()))
            .expect("parse should succeed")
            .expect("fee should be present");
        assert_eq!(fee, 15_427);
    }

    #[test]
    fn test_parse_transaction_data_resources_synthetic_multiple_footprint_entries() {
        let key1 = stellar_xdr::LedgerKey::Account(stellar_xdr::LedgerKeyAccount {
            account_id: stellar_xdr::AccountId(stellar_xdr::PublicKey::PublicKeyTypeEd25519(
                stellar_xdr::Uint256([1u8; 32]),
            )),
        });
        let key2 = stellar_xdr::LedgerKey::Account(stellar_xdr::LedgerKeyAccount {
            account_id: stellar_xdr::AccountId(stellar_xdr::PublicKey::PublicKeyTypeEd25519(
                stellar_xdr::Uint256([2u8; 32]),
            )),
        });
        let key3 = stellar_xdr::LedgerKey::Account(stellar_xdr::LedgerKeyAccount {
            account_id: stellar_xdr::AccountId(stellar_xdr::PublicKey::PublicKeyTypeEd25519(
                stellar_xdr::Uint256([3u8; 32]),
            )),
        });

        let data = stellar_xdr::SorobanTransactionData {
            ext: stellar_xdr::SorobanTransactionDataExt::V0,
            resources: stellar_xdr::SorobanResources {
                footprint: stellar_xdr::LedgerFootprint {
                    read_only: vec![key1].try_into().unwrap(),
                    read_write: vec![key2, key3].try_into().unwrap(),
                },
                instructions: 750_000,
                disk_read_bytes: 512,
                write_bytes: 1024,
            },
            resource_fee: 50_000,
        };

        let xdr = data
            .to_xdr(stellar_xdr::Limits::none())
            .expect("XDR encode");
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &xdr);

        let resources = parse_transaction_data_resources(&Some(b64))
            .expect("parse should succeed")
            .expect("resources should be present");

        assert_eq!(resources.cpu_insns, 750_000);
        assert_eq!(resources.read_entries, 1);
        assert_eq!(resources.write_entries, 2);
        assert_eq!(resources.read_bytes, 512);
        assert_eq!(resources.write_bytes, 1024);
    }

    #[test]
    fn test_parse_transaction_data_resources_empty_footprint() {
        let data = stellar_xdr::SorobanTransactionData {
            ext: stellar_xdr::SorobanTransactionDataExt::V0,
            resources: stellar_xdr::SorobanResources {
                footprint: stellar_xdr::LedgerFootprint {
                    read_only: stellar_xdr::VecM::default(),
                    read_write: stellar_xdr::VecM::default(),
                },
                instructions: 0,
                disk_read_bytes: 0,
                write_bytes: 0,
            },
            resource_fee: 0,
        };

        let xdr = data
            .to_xdr(stellar_xdr::Limits::none())
            .expect("XDR encode");
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &xdr);

        let resources = parse_transaction_data_resources(&Some(b64))
            .expect("parse should succeed")
            .expect("resources should be present");

        assert_eq!(resources.cpu_insns, 0);
        assert_eq!(resources.read_entries, 0);
        assert_eq!(resources.write_entries, 0);
        assert_eq!(resources.read_bytes, 0);
        assert_eq!(resources.write_bytes, 0);
    }

    #[test]
    fn test_parse_transaction_data_resources_absent() {
        assert_eq!(parse_transaction_data_resources(&None).unwrap(), None);
    }

    #[test]
    fn test_parse_transaction_data_resources_invalid() {
        assert!(parse_transaction_data_resources(&Some("not-base64!".to_string())).is_err());
    }
}
