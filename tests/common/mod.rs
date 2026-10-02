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
