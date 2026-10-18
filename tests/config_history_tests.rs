use soroban_cost_estimator::config_snapshot::history::{
    build_change_log_from_snapshots, last_changed_from_log,
};
use soroban_cost_estimator::config_snapshot::model::*;

fn make_snapshot(
    timestamp: &str,
    ledger: u32,
    compute_fee: i64,
    bandwidth_fee: i64,
) -> ConfigSnapshot {
    ConfigSnapshot {
        network: "testnet".to_string(),
        timestamp: timestamp.to_string(),
        ledger,
        protocol_version: None,
        contract_compute: Some(ContractComputeV0 {
            ledger_max_instructions: 1_000_000,
            tx_max_instructions: 100_000,
            fee_rate_per_instructions_increment: compute_fee,
            tx_memory_limit: 100,
        }),
        contract_ledger_cost: None,
        contract_historical_data: None,
        contract_events: None,
        contract_bandwidth: Some(ContractBandwidthV0 {
            ledger_max_txs_size_bytes: 1_000_000,
            tx_max_size_bytes: 100_000,
            fee_tx_size1_kb: bandwidth_fee,
        }),
        state_archival: None,
    }
}

#[test]
fn test_empty_with_fewer_than_two_snapshots() {
    let snap = make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5);
    assert!(build_change_log_from_snapshots(&[]).is_empty());
    assert!(build_change_log_from_snapshots(&[snap]).is_empty());
}

#[test]
fn test_no_changes_across_identical_snapshots() {
    let a = make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5);
    let b = make_snapshot("2026-01-02T00:00:00Z", 200, 100, 5);
    let log = build_change_log_from_snapshots(&[a, b]);
    assert!(log.is_empty());
}

#[test]
fn test_records_change_with_newer_snapshot_stamp() {
    let a = make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5);
    let b = make_snapshot("2026-01-02T00:00:00Z", 200, 200, 5);
    let log = build_change_log_from_snapshots(&[a, b]);

    assert_eq!(log.len(), 1);
    assert_eq!(
        log[0].field_path,
        "contract_compute.fee_rate_per_instructions_increment"
    );
    assert_eq!(log[0].timestamp, "2026-01-02T00:00:00Z");
    assert_eq!(log[0].ledger, 200);
    assert_eq!(log[0].old_value, "100");
    assert_eq!(log[0].new_value, "200");
    assert!(log[0].is_pricing_change);
}

#[test]
fn test_sorts_out_of_order_input_snapshots_by_timestamp() {
    let a = make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5);
    let b = make_snapshot("2026-01-02T00:00:00Z", 200, 200, 5);
    // Passed in reverse order — the function must sort before diffing.
    let log = build_change_log_from_snapshots(&[b, a]);
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].old_value, "100");
    assert_eq!(log[0].new_value, "200");
}

#[test]
fn test_multiple_transitions_produce_chronological_entries() {
    let a = make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5);
    let b = make_snapshot("2026-01-02T00:00:00Z", 200, 200, 5);
    let c = make_snapshot("2026-01-03T00:00:00Z", 300, 200, 10);
    let log = build_change_log_from_snapshots(&[a, b, c]);

    assert_eq!(log.len(), 2);
    assert_eq!(log[0].timestamp, "2026-01-02T00:00:00Z");
    assert_eq!(
        log[0].field_path,
        "contract_compute.fee_rate_per_instructions_increment"
    );
    assert_eq!(log[1].timestamp, "2026-01-03T00:00:00Z");
    assert_eq!(log[1].field_path, "contract_bandwidth.fee_tx_size1_kb");
}

#[test]
fn test_last_changed_keeps_most_recent_entry_per_field() {
    let a = make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5);
    let b = make_snapshot("2026-01-02T00:00:00Z", 200, 200, 5);
    let c = make_snapshot("2026-01-03T00:00:00Z", 300, 300, 5);
    let log = build_change_log_from_snapshots(&[a, b, c]);

    // Two changes to the same field across the log; last_changed keeps only
    // the most recent one.
    let last = last_changed_from_log(&log);
    assert_eq!(last.len(), 1);
    assert_eq!(last[0].timestamp, "2026-01-03T00:00:00Z");
    assert_eq!(last[0].old_value, "200");
    assert_eq!(last[0].new_value, "300");
}

#[test]
fn test_last_changed_tracks_independent_fields_separately() {
    let a = make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5);
    let b = make_snapshot("2026-01-02T00:00:00Z", 200, 200, 5);
    let c = make_snapshot("2026-01-03T00:00:00Z", 300, 200, 10);
    let log = build_change_log_from_snapshots(&[a, b, c]);

    let mut last = last_changed_from_log(&log);
    last.sort_by(|x, y| x.field_path.cmp(&y.field_path));
    assert_eq!(last.len(), 2);
    assert_eq!(last[0].field_path, "contract_bandwidth.fee_tx_size1_kb");
    assert_eq!(last[0].timestamp, "2026-01-03T00:00:00Z");
    assert_eq!(
        last[1].field_path,
        "contract_compute.fee_rate_per_instructions_increment"
    );
    assert_eq!(last[1].timestamp, "2026-01-02T00:00:00Z");
}

#[test]
fn test_snapshot_schema_version_written_on_serialize_and_legacy_accepted() {
    let snap = make_snapshot("2026-01-01T00:00:00Z", 100, 100, 5);
    let json = serde_json::to_string_pretty(&snap).expect("serialize snapshot");
    let raw: serde_json::Value = serde_json::from_str(&json).expect("parse json value");
    assert_eq!(
        raw.get("schema_version").and_then(|v| v.as_u64()),
        Some(u64::from(SNAPSHOT_SCHEMA_VERSION))
    );

    // Legacy unversioned snapshot JSON loads cleanly without modifying disk files
    let legacy_json = r#"{
        "network": "testnet",
        "timestamp": "2026-01-01T00:00:00Z",
        "ledger": 42,
        "contract_compute": null,
        "contract_ledger_cost": null,
        "contract_historical_data": null,
        "contract_events": null,
        "contract_bandwidth": null,
        "state_archival": null
    }"#;
    let dir = std::env::temp_dir().join(format!("sce-schema-legacy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let legacy_path = dir.join("legacy.json");
    std::fs::write(&legacy_path, legacy_json).expect("write legacy snapshot");

    let loaded = soroban_cost_estimator::config_snapshot::store::load_snapshot_from_path(
        &legacy_path.to_string_lossy(),
    )
    .expect("load legacy snapshot");
    assert_eq!(loaded.ledger, 42);
    assert_eq!(
        std::fs::read_to_string(&legacy_path).expect("read legacy file"),
        legacy_json,
        "reading a legacy snapshot must not rewrite it on disk"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_unsupported_future_snapshot_schema_version_rejected_with_file_and_guidance() {
    let future_json = r#"{
        "schema_version": 99,
        "network": "testnet",
        "timestamp": "2026-01-01T00:00:00Z",
        "ledger": 99,
        "contract_compute": null,
        "contract_ledger_cost": null,
        "contract_historical_data": null,
        "contract_events": null,
        "contract_bandwidth": null,
        "state_archival": null
    }"#;
    let dir = std::env::temp_dir().join(format!("sce-schema-future-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let future_path = dir.join("future-v99.json");
    std::fs::write(&future_path, future_json).expect("write future snapshot");

    let err = soroban_cost_estimator::config_snapshot::store::load_snapshot_from_path(
        &future_path.to_string_lossy(),
    )
    .expect_err("future schema version must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("future-v99.json"),
        "error should identify the file: {msg}"
    );
    assert!(
        msg.contains("unsupported future snapshot schema_version 99"),
        "error should report unsupported schema version: {msg}"
    );
    assert!(
        msg.contains("recommended action"),
        "error should include recommended action: {msg}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
