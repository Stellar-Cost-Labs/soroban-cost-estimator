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
        settings_last_modified: std::collections::BTreeMap::new(),
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

// ── Snapshot ordering (#514) ───────────────────────────────────────────────

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use soroban_cost_estimator::config_snapshot::store;

/// Serializes the tests that redirect `HOME`, because
/// `std::env::set_var` is not thread-safe.
static HOME_MUTEX: Mutex<()> = Mutex::new(());

/// Runs a test with `HOME` pointed at a temporary directory so the snapshot
/// store never touches the real user's home directory.
fn with_temp_home<F>(test: F)
where
    F: FnOnce(&Path) + std::panic::UnwindSafe,
{
    let guard = HOME_MUTEX.lock().expect("snapshot test mutex");

    let suffix: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let tmp = std::env::temp_dir().join(format!(
        "soroban_snapshot_test_{}_{}",
        std::process::id(),
        suffix
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("create temp home");

    let old_home = std::env::var_os("HOME");
    let old_userprofile = std::env::var_os("USERPROFILE");
    // SAFETY: serialized by HOME_MUTEX.
    unsafe {
        std::env::set_var("HOME", &tmp);
        std::env::set_var("USERPROFILE", &tmp);
    }

    let result = std::panic::catch_unwind(|| test(&tmp));

    // SAFETY: serialized by HOME_MUTEX.
    unsafe {
        match old_home {
            Some(old) => std::env::set_var("HOME", old),
            None => std::env::remove_var("HOME"),
        }
        match old_userprofile {
            Some(old) => std::env::set_var("USERPROFILE", old),
            None => std::env::remove_var("USERPROFILE"),
        }
    }

    let _ = std::fs::remove_dir_all(&tmp);
    drop(guard);

    if let Err(e) = result {
        std::panic::resume_unwind(e);
    }
}

/// Writes a snapshot file into the snapshots directory of a temporary home.
///
/// The file name follows the store's convention: `{network}-{timestamp}.json`
/// with the timestamp's `:` replaced by `-`.
fn write_snapshot(home: &Path, network: &str, timestamp: &str, ledger: u32) -> PathBuf {
    write_named_snapshot(
        home,
        &format!("{network}-{}.json", timestamp.replace(':', "-")),
        timestamp,
        ledger,
    )
}

/// Writes a valid snapshot under an arbitrary file name.
///
/// Used to model a snapshot whose *name* does not encode a usable timestamp
/// while its content is still a well-formed snapshot.
fn write_named_snapshot(home: &Path, filename: &str, timestamp: &str, ledger: u32) -> PathBuf {
    let dir = home.join(".soroban-cost-estimator").join("snapshots");
    std::fs::create_dir_all(&dir).expect("create snapshots dir");
    let path = dir.join(filename);
    let content = format!(
        r#"{{
  "network": "testnet",
  "timestamp": "{timestamp}",
  "ledger": {ledger},
  "contract_compute": null,
  "contract_ledger_cost": null,
  "contract_historical_data": null,
  "contract_events": null,
  "contract_bandwidth": null,
  "state_archival": null
}}"#
    );
    std::fs::write(&path, content).expect("write snapshot");
    path
}

/// Writes an arbitrary file into the snapshots directory of a temporary home.
fn write_raw(home: &Path, filename: &str, content: &str) -> PathBuf {
    let dir = home.join(".soroban-cost-estimator").join("snapshots");
    std::fs::create_dir_all(&dir).expect("create snapshots dir");
    let path = dir.join(filename);
    std::fs::write(&path, content).expect("write file");
    path
}

/// The file names of the snapshots listed for `network`, oldest first.
fn listed_names(network: &str) -> Vec<String> {
    store::list_snapshots(network)
        .expect("list snapshots")
        .iter()
        .map(|path| {
            path.file_name()
                .expect("file name")
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

#[test]
fn test_timestamp_from_filename_recovers_the_embedded_timestamp() {
    assert_eq!(
        store::timestamp_from_filename("testnet", "testnet-2026-01-01T00-00-00Z.json"),
        Some("2026-01-01T00:00:00Z".to_string())
    );
    // Offsets are part of the timestamp and must survive the round trip.
    assert_eq!(
        store::timestamp_from_filename("testnet", "testnet-2026-01-01T00-00-00+02-00.json"),
        Some("2026-01-01T00:00:00+02:00".to_string())
    );
    // A negative offset keeps its sign.
    assert_eq!(
        store::timestamp_from_filename("testnet", "testnet-2026-01-01T00-00-00-05-00.json"),
        Some("2026-01-01T00:00:00-05:00".to_string())
    );
    // Fractional seconds are produced by `chrono::Utc::now().to_rfc3339()`.
    assert_eq!(
        store::timestamp_from_filename(
            "testnet",
            "testnet-2026-01-01T00-00-00.123456789+00-00.json"
        ),
        Some("2026-01-01T00:00:00.123456789+00:00".to_string())
    );
}

#[test]
fn test_timestamp_from_filename_rejects_invalid_names() {
    // Not a timestamp at all.
    assert_eq!(
        store::timestamp_from_filename("testnet", "testnet-not-a-timestamp.json"),
        None
    );
    // Another network's file.
    assert_eq!(
        store::timestamp_from_filename("testnet", "mainnet-2026-01-01T00-00-00Z.json"),
        None
    );
    // Missing the network prefix / the `.json` suffix.
    assert_eq!(
        store::timestamp_from_filename("testnet", "2026-01-01T00-00-00Z.json"),
        None
    );
    assert_eq!(
        store::timestamp_from_filename("testnet", "testnet-2026-01-01T00-00-00Z"),
        None
    );
    // Empty timestamp.
    assert_eq!(
        store::timestamp_from_filename("testnet", "testnet-.json"),
        None
    );
}

#[test]
fn test_snapshot_listing_is_chronological_across_time_boundaries() {
    with_temp_home(|home| {
        // Hour, day, month and year boundaries in one series.
        let timestamps = [
            "2025-12-31T23:59:59Z",
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:00:01Z",
            "2026-02-01T00:00:00Z",
            "2026-12-31T23:59:59Z",
            "2027-01-01T00:00:00Z",
        ];
        // Written in a deliberately shuffled order: the listing must not
        // depend on the filesystem's enumeration order.
        for (index, order) in [4, 0, 5, 2, 1, 3].iter().enumerate() {
            write_snapshot(home, "testnet", timestamps[*order], index as u32);
        }

        let expected: Vec<String> = timestamps
            .iter()
            .map(|ts| format!("testnet-{}.json", ts.replace(':', "-")))
            .collect();
        assert_eq!(listed_names("testnet"), expected);
    });
}

#[test]
fn test_load_latest_snapshot_returns_the_newest_by_embedded_timestamp() {
    with_temp_home(|home| {
        write_snapshot(home, "testnet", "2025-06-01T00:00:00Z", 1);
        write_snapshot(home, "testnet", "2026-06-01T00:00:00Z", 3);
        write_snapshot(home, "testnet", "2024-06-01T00:00:00Z", 2);

        let latest = store::load_latest_snapshot("testnet").expect("load latest");
        assert_eq!(latest.timestamp, "2026-06-01T00:00:00Z");
        assert_eq!(latest.ledger, 3);
    });
}

#[test]
fn test_snapshot_listing_orders_different_offsets_chronologically() {
    with_temp_home(|home| {
        // 10:00+02:00 is 08:00Z — chronologically *older* than 09:00Z, even
        // though its file name sorts later.
        write_snapshot(home, "testnet", "2026-01-01T10:00:00+02:00", 1);
        write_snapshot(home, "testnet", "2026-01-01T09:00:00Z", 2);

        let names = listed_names("testnet");
        assert_eq!(
            names,
            vec![
                "testnet-2026-01-01T10-00-00+02-00.json".to_string(),
                "testnet-2026-01-01T09-00-00Z.json".to_string(),
            ]
        );

        let latest = store::load_latest_snapshot("testnet").expect("load latest");
        assert_eq!(latest.timestamp, "2026-01-01T09:00:00Z");
        assert_eq!(latest.ledger, 2);
    });
}

#[test]
fn test_files_with_invalid_embedded_timestamps_are_ignored_by_latest_selection() {
    with_temp_home(|home| {
        write_raw(home, "testnet-not-a-timestamp.json", "{}");
        write_snapshot(home, "testnet", "2026-01-01T00:00:00Z", 7);

        // The invalid name cannot be placed on the timeline, so it is listed
        // last and never selected as "latest".
        let names = listed_names("testnet");
        assert_eq!(
            names,
            vec![
                "testnet-2026-01-01T00-00-00Z.json".to_string(),
                "testnet-not-a-timestamp.json".to_string(),
            ]
        );

        let latest = store::load_latest_snapshot("testnet").expect("load latest");
        assert_eq!(latest.timestamp, "2026-01-01T00:00:00Z");
        assert_eq!(latest.ledger, 7);
    });
}

#[test]
fn test_latest_selection_falls_back_to_file_name_order_without_valid_timestamps() {
    with_temp_home(|home| {
        write_named_snapshot(home, "testnet-aaa.json", "2026-01-01T00:00:00Z", 1);
        write_named_snapshot(home, "testnet-zzz.json", "2026-01-02T00:00:00Z", 2);

        // No file encodes a usable timestamp, so the lexicographically
        // greatest name wins — a hand-written snapshot stays reachable.
        let latest = store::load_latest_snapshot("testnet").expect("load latest");
        assert_eq!(latest.ledger, 2);
        let names = listed_names("testnet");
        assert_eq!(
            names,
            vec![
                "testnet-aaa.json".to_string(),
                "testnet-zzz.json".to_string(),
            ]
        );
    });
}

#[test]
fn test_unrelated_files_and_other_networks_are_ignored() {
    with_temp_home(|home| {
        write_snapshot(home, "testnet", "2026-01-01T00:00:00Z", 1);
        write_snapshot(home, "mainnet", "2026-01-02T00:00:00Z", 2);
        write_raw(home, "notes.txt", "not a snapshot");
        write_raw(home, "testnet.json", "{}");
        // Matches the `{network}-*.json` pattern but carries no timestamp,
        // so it is listed last instead of being ordered chronologically.
        write_raw(home, "testnet-backup.json", "{}");

        assert_eq!(
            listed_names("testnet"),
            vec![
                "testnet-2026-01-01T00-00-00Z.json".to_string(),
                "testnet-backup.json".to_string(),
            ]
        );
        assert_eq!(
            listed_names("mainnet"),
            vec!["mainnet-2026-01-02T00-00-00Z.json".to_string()]
        );
    });
}

#[test]
fn test_missing_snapshots_reports_an_error() {
    with_temp_home(|_home| {
        let error = store::load_latest_snapshot("testnet").expect_err("no snapshots");
        assert!(
            error.to_string().contains("testnet"),
            "error should name the network: {error}"
        );
    });
}
