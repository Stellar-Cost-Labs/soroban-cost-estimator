use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, method},
};

pub async fn start_mock_rpc_server() -> MockServer {
    MockServer::start().await
}

pub async fn stub_jsonrpc_success(server: &MockServer, method_name: &str, result: Value) {
    Mock::given(method("POST"))
        .and(body_string_contains(format!(
            "\"method\":\"{method_name}\""
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": result
        })))
        .mount(server)
        .await;
}

/// Stubs a JSON-RPC method with a non-success HTTP status, so error paths
/// (retry exhaustion, gateway failover) can be exercised offline.
pub async fn stub_jsonrpc_http_error(server: &MockServer, method_name: &str, status: u16) {
    Mock::given(method("POST"))
        .and(body_string_contains(format!(
            "\"method\":\"{method_name}\""
        )))
        .respond_with(ResponseTemplate::new(status))
        .mount(server)
        .await;
}

pub async fn stub_jsonrpc_error(server: &MockServer, method_name: &str, code: i64, message: &str) {
    Mock::given(method("POST"))
        .and(body_string_contains(format!(
            "\"method\":\"{method_name}\""
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": code,
                "message": message
            }
        })))
        .mount(server)
        .await;
}

/// Stubs an oversized JSON-RPC response with a Content-Length header.
///
/// This tests the case where the Content-Length header correctly reports
/// the size, which should still be rejected.
pub async fn stub_oversized_response_with_content_length(server: &MockServer, method_name: &str) {
    // Create a JSON response that exceeds the 10 MB limit
    // The payload field will be large enough to push the total over 10 MB
    let json_prefix = r#"{"jsonrpc":"2.0","id":1,"result":{"data":""#;
    let json_suffix = r#"}}"#;
    let prefix_len = json_prefix.len();
    let suffix_len = json_suffix.len();
    // We need total > 10 MB, so payload > 10 MB - prefix - suffix
    let payload_size = 10 * 1024 * 1024 + 1 - prefix_len - suffix_len;
    let large_payload = "x".repeat(payload_size);
    let body_str = format!("{}{}{}", json_prefix, large_payload, json_suffix);

    Mock::given(method("POST"))
        .and(body_string_contains(format!(
            "\"method\":\"{method_name}\""
        )))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("Content-Length", body_str.len().to_string())
                .set_body_string(body_str),
        )
        .mount(server)
        .await;
}

/// Stubs an oversized JSON-RPC response without a Content-Length header.
///
/// This tests the case where Content-Length is missing or incorrect,
/// so we must enforce the bound while streaming the body.
pub async fn stub_oversized_response_without_content_length(
    server: &MockServer,
    method_name: &str,
) {
    // Create a JSON response that exceeds the 10 MB limit
    let json_prefix = r#"{"jsonrpc":"2.0","id":1,"result":{"data":""#;
    let json_suffix = r#"}}"#;
    let prefix_len = json_prefix.len();
    let suffix_len = json_suffix.len();
    let payload_size = 10 * 1024 * 1024 + 1 - prefix_len - suffix_len;
    let large_payload = "x".repeat(payload_size);
    let body_str = format!("{}{}{}", json_prefix, large_payload, json_suffix);

    Mock::given(method("POST"))
        .and(body_string_contains(format!(
            "\"method\":\"{method_name}\""
        )))
        .respond_with(ResponseTemplate::new(200).set_body_string(body_str))
        .mount(server)
        .await;
}
