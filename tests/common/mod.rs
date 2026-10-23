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

pub async fn stub_rate_limit(
    server: &MockServer,
    method_name: &str,
    retry_after: Option<&str>,
    up_to_times: u64,
) {
    let mut template = ResponseTemplate::new(429);
    if let Some(header_val) = retry_after {
        template = template.append_header("Retry-After", header_val);
    }
    Mock::given(method("POST"))
        .and(body_string_contains(format!(
            "\"method\":\"{method_name}\""
        )))
        .respond_with(template)
        .up_to_n_times(up_to_times)
        .mount(server)
        .await;
}
