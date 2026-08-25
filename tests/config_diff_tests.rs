use soroban_cost_estimator::config_snapshot::diff;
use soroban_cost_estimator::config_snapshot::model::*;
use soroban_cost_estimator::config_snapshot::store;
use std::path::PathBuf;
use std::sync::Mutex;

static HOME_MUTEX: Mutex<()> = Mutex::new(());

fn with_temp_home<F>(test: F)
where
    F: FnOnce(&PathBuf),
{
    let _guard = HOME_MUTEX.lock().expect("home mutex");
    let tmp = std::env::temp_dir().join(format!("sce-snapshot-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create temp home");
    let old_home = std::env::var_os("HOME");
    let old_userprofile = std::env::var_os("USERPROFILE");
    // SAFETY: tests serialize HOME changes with HOME_MUTEX.
    unsafe {
        std::env::set_var("HOME", &tmp);
        std::env::set_var("USERPROFILE", &tmp);
    }
    test(&tmp);
    match old_home {
        Some(home) => unsafe { std::env::set_var("HOME", home) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    match old_userprofile {
        Some(home) => unsafe { std::env::set_var("USERPROFILE", home) },
        None => unsafe { std::env::remove_var("USERPROFILE") },
    }
    let _ = std::fs::remove_dir_all(tmp);
}

fn make_snapshot(compute_fee: i64, bandwidth_fee: i64) -> ConfigSnapshot {
    ConfigSnapshot {
        network: "testnet".to_string(),
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        ledger: 100,
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
fn test_snapshot_export_import_round_trip() {
    with_temp_home(|tmp| {
        let source = tmp.join("source.json");
        let exported = tmp.join("shared.json");
        let snapshot = make_snapshot(100, 5);
        std::fs::write(&source, serde_json::to_string(&snapshot).expect("serialize snapshot"))
            .expect("write source");

        let export_path = store::export_snapshot(
            source.to_str().expect("source path"),
            exported.to_str().expect("export path"),
        )
        .expect("export snapshot");
        assert_eq!(export_path, exported);
        let exported_snapshot = store::load_snapshot_from_path(
            exported.to_str().expect("export path"),
        )
        .expect("load export");
        assert_eq!(exported_snapshot.network, "testnet");

        let imported = store::import_snapshot(exported.to_str().expect("export path"))
            .expect("import snapshot");
        assert!(imported.exists());
        let imported_snapshot =
            store::load_snapshot_from_path(imported.to_str().expect("import path"))
                .expect("load import");
        assert_eq!(imported_snapshot.ledger, 100);
    });
}

#[test]
fn test_snapshot_export_rejects_malformed_json() {
    with_temp_home(|tmp| {
        let source = tmp.join("malformed.json");
        std::fs::write(&source, "not json").expect("write malformed snapshot");
        let result = store::export_snapshot(
            source.to_str().expect("source path"),
            tmp.join("out.json").to_str().expect("out path"),
        );
        assert!(matches!(
            result,
            Err(soroban_cost_estimator::error::AppError::SnapshotParse(_))
        ));
    });
}

#[test]
fn test_no_changes() {
    let snap = make_snapshot(100, 5);
    let diff = diff::diff_snapshots(&snap, &snap);
    assert!(diff.changes.is_empty());
    assert!(!diff.has_pricing_changes);
}

#[test]
fn test_detects_fee_change() {
    let old = make_snapshot(100, 5);
    let new = make_snapshot(200, 5);
    let diff = diff::diff_snapshots(&old, &new);

    assert_eq!(diff.changes.len(), 1);
    assert_eq!(
        diff.changes[0].field_path,
        "contract_compute.fee_rate_per_instructions_increment"
    );
    assert_eq!(diff.changes[0].old_value, "100");
    assert_eq!(diff.changes[0].new_value, "200");
    assert!(diff.changes[0].is_pricing_change);
    assert!(diff.has_pricing_changes);
}

#[test]
fn test_detects_bandwidth_fee_change() {
    let old = make_snapshot(100, 5);
    let new = make_snapshot(100, 10);
    let diff = diff::diff_snapshots(&old, &new);

    assert_eq!(diff.changes.len(), 1);
    assert_eq!(
        diff.changes[0].field_path,
        "contract_bandwidth.fee_tx_size1_kb"
    );
    assert!(diff.changes[0].is_pricing_change);
}

#[test]
fn test_detects_multiple_changes() {
    let old = make_snapshot(100, 5);
    let mut new = make_snapshot(200, 10);
    new.ledger = 200;
    let diff = diff::diff_snapshots(&old, &new);

    // fee_rate_per_instructions_increment and fee_tx_size1_kb changed
    assert_eq!(diff.changes.len(), 2);
    assert!(diff.has_pricing_changes);
}

#[test]
fn test_format_diff_no_changes() {
    let snap = make_snapshot(100, 5);
    let diff = diff::diff_snapshots(&snap, &snap);
    let output = diff::format_diff(&diff);
    assert!(output.contains("No changes detected"));
}

#[test]
fn test_format_diff_with_changes() {
    let old = make_snapshot(100, 5);
    let new = make_snapshot(200, 5);
    let diff = diff::diff_snapshots(&old, &new);
    let output = diff::format_diff(&diff);
    assert!(output.contains("fee_rate_per_instructions_increment"));
    assert!(output.contains("100"));
    assert!(output.contains("200"));
}
