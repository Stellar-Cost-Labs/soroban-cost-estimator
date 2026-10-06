use soroban_cost_estimator::{
    error::AppError,
    report::fee_calc::xlm_to_stroops,
    rpc::client::{parse_header, parse_headers, resolve_endpoint},
    wasm::parser::load_wasm,
    xdr_helper::{build_simulation_tx_envelope, decode_config_entry_xdr, parse_contract_id},
};

use std::path::Path;

#[test]
fn file_not_found_propagates() {
    let result = load_wasm(Path::new("/definitely/nonexistent/contract.wasm"));

    assert!(matches!(result, Err(AppError::FileNotFound(_))));
}

#[test]
fn unknown_network_error_propagates() {
    let result = resolve_endpoint("invalid-network", None);

    assert!(matches!(result, Err(AppError::UnknownNetwork(_))));
}

#[test]
fn xdr_decode_error_propagates() {
    let result = decode_config_entry_xdr("not-valid-base64!!!", false);

    assert!(matches!(result, Err(AppError::XdrDecode(_))));
}

#[test]
fn invalid_contract_id_error_propagates() {
    let result = parse_contract_id("not-a-valid-contract-id");

    assert!(matches!(result, Err(AppError::TxConstruction(_))));
}

#[test]
fn missing_contract_id_error_propagates() {
    let result = build_simulation_tx_envelope(&[], None, Some("hello"), &[]);

    assert!(matches!(result, Err(AppError::TxConstruction(_))));
}

// ─────────────────────────────────────────────────────────────────────────
// Error-message style (Issue #390)
//
// The crate follows one house style for every error string — see the
// `# Message style` section on `AppError`:
//
//   1. lower-case first word (bar acronyms / proper nouns like WASM, RPC)
//   2. lead with what failed, cause after a colon
//   3. no trailing period
//
// These tests pin the exact rendered strings that the style pass changed, so a
// later edit cannot silently reintroduce the inconsistent phrasing.
// ─────────────────────────────────────────────────────────────────────────

/// Fails when `message` breaks the house style.
#[track_caller]
fn assert_house_style(message: &str) {
    assert!(!message.is_empty(), "error message must not be empty");
    assert!(
        !message.ends_with('.'),
        "error message must not end with a period: {message:?}"
    );
    let first = message.chars().next().expect("non-empty");
    let first_word = message.split_whitespace().next().unwrap_or_default();
    let allowed = ["rpc", "wasm", "xdr", "xlm", "json", "http", "websocket"];
    assert!(
        first.is_lowercase() || allowed.contains(&first_word.to_ascii_lowercase().as_str()),
        "error message must start lower-case (or with a known acronym): {message:?}"
    );
}

#[test]
fn file_not_found_message_is_lower_case() {
    let message = AppError::FileNotFound("contract.wasm".to_string()).to_string();

    assert_eq!(message, "file not found: contract.wasm");
    assert_house_style(&message);
}

#[test]
fn type_validation_message_leads_with_the_failure() {
    // Previously "argument type validation error: …", which named the
    // subsystem instead of the failure.
    let message =
        AppError::TypeValidation("arg 'abc' cannot be used as 'i64'".to_string()).to_string();

    assert_eq!(
        message,
        "failed to validate argument type: arg 'abc' cannot be used as 'i64'"
    );
    assert_house_style(&message);
}

#[test]
fn websocket_connect_message_leads_with_the_failure() {
    // Previously "WebSocket connection failed: …".
    let message = AppError::WsConnect("connect to wss://host/ws: refused".to_string()).to_string();

    assert_eq!(
        message,
        "failed to open WebSocket connection: connect to wss://host/ws: refused"
    );
    assert_house_style(&message);
}

#[test]
fn xdr_decode_messages_do_not_restate_the_prefix() {
    // The variant already renders "failed to decode XDR: …", so the payload
    // must be the bare cause.
    let message = AppError::XdrDecode("invalid base64".to_string()).to_string();

    assert_eq!(message, "failed to decode XDR: invalid base64");
    assert!(
        !message.contains("failed to decode XDR: failed to decode"),
        "the payload must not restate the prefix: {message}"
    );
}

#[test]
fn snapshot_not_found_message_is_typed() {
    let message = AppError::SnapshotNotFound("/tmp/snapshots/testnet.json".to_string()).to_string();

    assert_eq!(
        message,
        "failed to load snapshot: not found at /tmp/snapshots/testnet.json"
    );
    assert_house_style(&message);
}

#[test]
fn xlm_out_of_range_message_is_house_style() {
    let err = xlm_to_stroops("922337203685.4775808").expect_err("one stroop past i64::MAX");

    let message = err.to_string();
    assert_eq!(
        message,
        "failed to calculate fee: XLM value is out of range for stroops"
    );
    assert_house_style(&message);
}

#[test]
fn header_parse_error_message_names_the_expected_format() {
    let err = parse_header("NoSeparatorHere").expect_err("no '=' or ':' present");

    let message = err.to_string();
    assert_eq!(
        message,
        "failed to parse HTTP header: 'NoSeparatorHere' is not a KEY=VALUE pair (or KEY: VALUE)"
    );
    assert_house_style(&message);
}

#[test]
fn header_parse_errors_cover_every_rejection_reason() {
    for (input, expected) in [
        ("= value", "empty header name"),
        ("X-Api-Key=", "empty value"),
        ("X Custom=v", "invalid header name"),
    ] {
        let err = parse_header(input).expect_err(&format!("{input:?} must be rejected"));
        let message = err.to_string();
        assert!(
            message.contains(expected),
            "{input:?} should report {expected:?}; got: {message}"
        );
        assert_house_style(&message);
    }
}

#[test]
fn parse_headers_reports_the_first_malformed_entry() {
    let raw = vec![
        "X-Api-Key=ok".to_string(),
        "Broken".to_string(),
        "X-Other=also-ok".to_string(),
    ];

    let err = parse_headers(&raw).expect_err("one entry is malformed");
    assert!(matches!(err, AppError::InvalidHeader(_)));
    assert!(err.to_string().contains("Broken"), "got: {err}");
}
