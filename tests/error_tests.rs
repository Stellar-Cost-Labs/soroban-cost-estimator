use soroban_cost_estimator::{
    error::AppError,
    rpc::client::resolve_endpoint,
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
    let result = decode_config_entry_xdr("not-valid-base64!!!");

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
