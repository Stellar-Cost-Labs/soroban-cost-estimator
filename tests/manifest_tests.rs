//! End-to-end manifest execution against local RPC servers only.
use std::{path::Path, process::Command};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, method},
};

#[tokio::test]
async fn manifest_preflight_prevents_all_rpc_and_execution_uses_each_override() {
    let first = MockServer::start().await;
    let second = MockServer::start().await;
    for server in [&first, &second] {
        Mock::given(method("POST"))
            .and(body_string_contains("simulateTransaction"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "jsonrpc": "2.0", "id": 1,
                "result": {"latestLedger": 42, "minResourceFee": "100"}
            })))
            .mount(server)
            .await;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = std::env::temp_dir().join(format!("sce-manifest-rpc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Serialize the path as a TOML string so Windows backslashes cannot be
    // interpreted as escape sequences in the generated manifest.
    let wasm_path = toml::Value::String(
        root.join("tests/fixtures/zeroarg.wasm")
            .display()
            .to_string(),
    )
    .to_string();
    let fixture = std::fs::read_to_string(root.join("tests/fixtures/manifests/valid.toml"))
        .unwrap()
        .replace("\"../zeroarg.wasm\"", &wasm_path)
        .replace(
            "network = \"testnet\"",
            &format!("network = \"testnet\"\nrpc_url = \"{}\"", first.uri()),
        )
        .replace("http://127.0.0.1:1", &second.uri());
    let path = dir.join("suite.toml");
    std::fs::write(
        &path,
        fixture.replace("function = \"pong\"", "function = \"missing\""),
    )
    .unwrap();
    let run = |path: std::path::PathBuf, home: std::path::PathBuf| {
        tokio::task::spawn_blocking(move || {
            Command::new(env!("CARGO_BIN_EXE_soroban-cost-estimator"))
                .args(["estimate", "--manifest"])
                .arg(path)
                .args(["--format", "json", "--max-retries", "0"])
                .env("HOME", &home)
                .env("USERPROFILE", &home)
                .env("RUST_LOG", "error")
                .output()
                .unwrap()
        })
    };
    let invalid = run(path.clone(), dir.clone()).await.unwrap();
    assert!(!invalid.status.success());
    assert!(
        String::from_utf8_lossy(&invalid.stderr).contains("contracts[2]"),
        "expected validation to reach contracts[2], got: {}",
        String::from_utf8_lossy(&invalid.stderr)
    );
    assert!(first.received_requests().await.unwrap().is_empty());
    assert!(second.received_requests().await.unwrap().is_empty());

    std::fs::write(&path, fixture).unwrap();
    let valid = run(path, dir.clone()).await.unwrap();
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&valid.stdout).unwrap();
    assert_eq!(report["failed"], 0);
    assert_eq!(first.received_requests().await.unwrap().len(), 2);
    assert_eq!(second.received_requests().await.unwrap().len(), 2);
    std::fs::remove_dir_all(dir).unwrap();
}
