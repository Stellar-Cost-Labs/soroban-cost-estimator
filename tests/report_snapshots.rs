use soroban_cost_estimator::report::cost_report::{
    CostReport, format_estimate_all_table, summarize_estimate_all,
};
use soroban_cost_estimator::report::fee_calc::{DEFAULT_PRECISION, FeeBreakdown};
use soroban_cost_estimator::report::formatter::{
    CsvFormatter, JsonFormatter, MarkdownFormatter, ReportFormatter, TableFormatter,
};
use soroban_cost_estimator::wasm::parser::ContractMeta;

fn sample_report() -> CostReport {
    CostReport {
        function: "increment".to_string(),
        wasm_hash: "abc123def456".to_string(),
        wasm_size: 14_432,
        cpu_instructions: 532_502,
        memory_bytes: 0,
        tx_size: 156,
        read_entries: 1,
        write_entries: 1,
        read_bytes: 0,
        write_bytes: 136,
        fee: FeeBreakdown {
            non_refundable_stroops: 4_496,
            refundable_stroops: 10_931,
            cpu_fee_stroops: 372,
            storage_fee_stroops: 4_063,
            bandwidth_fee_stroops: 61,
            base_fee_stroops: 100,
            total_stroops: 15_427,
            total_xlm: "0.0015427".to_string(),
            fee_percentages: std::collections::BTreeMap::new(),
        },
        ledger: 3_894_195,
        network: "testnet".to_string(),
        rpc_latency_ms: 87,
        rates: None,
        warnings: Vec::new(),
        history: None,
        projections: None,
        contract_meta: ContractMeta::default(),
    }
}

fn empty_report() -> CostReport {
    CostReport {
        function: "(wasm upload)".to_string(),
        wasm_hash: "0000000000000000".to_string(),
        wasm_size: 0,
        cpu_instructions: 0,
        memory_bytes: 0,
        tx_size: 0,
        read_entries: 0,
        write_entries: 0,
        read_bytes: 0,
        write_bytes: 0,
        fee: FeeBreakdown {
            non_refundable_stroops: 0,
            refundable_stroops: 0,
            cpu_fee_stroops: 0,
            storage_fee_stroops: 0,
            bandwidth_fee_stroops: 0,
            base_fee_stroops: 0,
            total_stroops: 0,
            total_xlm: "0.0000000".to_string(),
            fee_percentages: std::collections::BTreeMap::new(),
        },
        ledger: 0,
        network: "mainnet".to_string(),
        rpc_latency_ms: 0,
        rates: None,
        warnings: Vec::new(),
        history: None,
        projections: None,
        contract_meta: ContractMeta::default(),
    }
}

#[test]
fn test_table_formatter_snapshots() {
    let report = sample_report();
    let empty = empty_report();

    insta::assert_snapshot!("table_formatter_sample", TableFormatter.format(&report));
    insta::assert_snapshot!("table_formatter_empty", TableFormatter.format(&empty));
}

#[test]
fn test_json_formatter_snapshots() {
    let report = sample_report();
    let empty = empty_report();

    insta::assert_snapshot!("json_formatter_sample", JsonFormatter.format(&report));
    insta::assert_snapshot!("json_formatter_empty", JsonFormatter.format(&empty));
}

#[test]
fn test_csv_formatter_snapshots() {
    let report = sample_report();
    let empty = empty_report();

    insta::assert_snapshot!("csv_formatter_sample", CsvFormatter.format(&report));
    insta::assert_snapshot!("csv_formatter_empty", CsvFormatter.format(&empty));
}

#[test]
fn test_markdown_formatter_snapshots() {
    let report = sample_report();
    let empty = empty_report();

    insta::assert_snapshot!(
        "markdown_formatter_sample",
        MarkdownFormatter.format(&report)
    );
    insta::assert_snapshot!("markdown_formatter_empty", MarkdownFormatter.format(&empty));
}

/// Builds a per-function report for the `estimate-all` summary fixtures.
fn batch_report(function: &str, cpu: u64, fee_stroops: i64, writes: u32) -> CostReport {
    CostReport {
        function: function.to_string(),
        wasm_hash: "abc123def456".to_string(),
        wasm_size: 14_432,
        cpu_instructions: cpu,
        memory_bytes: 0,
        tx_size: 156,
        read_entries: 1,
        write_entries: writes,
        read_bytes: 0,
        write_bytes: 136,
        fee: FeeBreakdown {
            non_refundable_stroops: 4_496,
            refundable_stroops: fee_stroops - 4_496,
            cpu_fee_stroops: 372,
            storage_fee_stroops: 4_063,
            bandwidth_fee_stroops: 61,
            base_fee_stroops: 100,
            total_stroops: fee_stroops,
            total_xlm: soroban_cost_estimator::report::fee_calc::stroops_to_xlm(
                fee_stroops,
                DEFAULT_PRECISION,
            ),
            fee_percentages: std::collections::BTreeMap::new(),
        },
        ledger: 3_894_195,
        network: "testnet".to_string(),
        rpc_latency_ms: 87,
        rates: None,
        warnings: Vec::new(),
        history: None,
        projections: None,
        contract_meta: ContractMeta::default(),
    }
}

/// Renders the batch table for `reports`, or an empty string when there is no
/// summary to show.
fn batch_table(reports: &[CostReport]) -> String {
    let summary = summarize_estimate_all(reports, DEFAULT_PRECISION);
    format_estimate_all_table(reports, summary.as_ref())
}

/// A single-function batch still gets a header row, one data row, and a
/// summary footer (issue #320).
#[test]
fn test_estimate_all_table_snapshot_single_function() {
    let reports = vec![batch_report("increment", 532_502, 15_427, 1)];
    insta::assert_snapshot!("estimate_all_table_single", batch_table(&reports));
}

/// A multi-function batch exercises the footer aggregates: min/max/average fee
/// and the CPU instruction range across functions (issue #320).
#[test]
fn test_estimate_all_table_snapshot_multi_function() {
    let reports = vec![
        batch_report("increment", 532_502, 15_427, 1),
        batch_report("decrement", 410_000, 12_000, 2),
        batch_report("reset", 98_765, 9_001, 3),
    ];
    insta::assert_snapshot!("estimate_all_table_multi", batch_table(&reports));
}

/// `--quiet` drops the footer row: the body table is still rendered, so the
/// per-function data is never lost.
#[test]
fn test_estimate_all_table_snapshot_quiet_has_no_footer() {
    let reports = vec![batch_report("increment", 532_502, 15_427, 1)];
    insta::assert_snapshot!(
        "estimate_all_table_quiet",
        format_estimate_all_table(&reports, None)
    );
}

// ── Config-drift snapshots (`config diff`, `config snapshot`) ─────────────
//
// The formatters above lock down `estimate`, `estimate --json` and
// `estimate-all`. The remaining report surfaces that issue #357 calls out are
// the config-drift ones: `config diff` (human table, `--summary`, `--format
// csv`, `--format markdown`), the `estimate --diff` side-by-side table, and
// the `config snapshot --json` render. They are pure functions over a
// `ConfigDiff` / `ConfigSnapshot`, so they snapshot deterministically offline.

use soroban_cost_estimator::config_snapshot::diff::{
    ConfigDiff, FieldDiff, SnapshotInfo, format_diff, format_diff_csv, format_diff_markdown,
    format_diff_summary,
};
use soroban_cost_estimator::config_snapshot::model::{ConfigSnapshot, ContractComputeV0};
use soroban_cost_estimator::report::diff::format_cost_report_diff_sized;

/// Two testnet snapshots whose pricing and non-pricing settings both moved:
/// one small pricing bump (within the default 10% band), one large repricing
/// (>50%), and one non-pricing TTL change.
fn sample_config_diff() -> ConfigDiff {
    ConfigDiff {
        old_snapshot: SnapshotInfo {
            network: "testnet".to_string(),
            timestamp: "2026-09-01T00:00:00+00:00".to_string(),
            ledger: 3_800_000,
        },
        new_snapshot: SnapshotInfo {
            network: "testnet".to_string(),
            timestamp: "2026-09-15T00:00:00+00:00".to_string(),
            ledger: 3_894_195,
        },
        changes: vec![
            FieldDiff {
                field_path: "contract_compute.tx_max_instructions".to_string(),
                setting_id: Some(0),
                setting_name: "Contract Compute V0".to_string(),
                old_value: "1000000".to_string(),
                new_value: "2750000".to_string(),
                is_pricing_change: true,
                explanation: None,
            },
            FieldDiff {
                field_path: "contract_ledger_cost.fee_write_ledger_entry".to_string(),
                setting_id: Some(1),
                setting_name: "Contract Ledger Cost V0".to_string(),
                old_value: "1000".to_string(),
                new_value: "1040".to_string(),
                is_pricing_change: true,
                explanation: None,
            },
            FieldDiff {
                field_path: "state_archival.max_entry_ttl".to_string(),
                setting_id: Some(6),
                setting_name: "State Archival".to_string(),
                old_value: "3110400".to_string(),
                new_value: "5184000".to_string(),
                is_pricing_change: false,
                explanation: None,
            },
        ],
        has_pricing_changes: true,
    }
}

/// `config diff` — the human-readable drift report, uncolored so the snapshot
/// is stable regardless of whether stdout is a terminal.
#[test]
fn test_config_diff_table_snapshot() {
    let diff = sample_config_diff();
    insta::assert_snapshot!("config_diff_table", format_diff(&diff, false, false, None));
}

/// `config diff --pricing-only --threshold 25` — non-pricing changes are
/// summarized away and the change over the threshold is marked.
#[test]
fn test_config_diff_table_pricing_only_snapshot() {
    let diff = sample_config_diff();
    insta::assert_snapshot!(
        "config_diff_table_pricing_only",
        format_diff(&diff, false, true, Some(25.0))
    );
}

/// `config diff --summary` — the one-line CI/`--quiet` form.
#[test]
fn test_config_diff_summary_snapshot() {
    let diff = sample_config_diff();
    insta::assert_snapshot!("config_diff_summary", format_diff_summary(&diff));
}

/// `config diff --format csv` — the spreadsheet-friendly form.
#[test]
fn test_config_diff_csv_snapshot() {
    let diff = sample_config_diff();
    insta::assert_snapshot!("config_diff_csv", format_diff_csv(&diff));
}

/// `config diff --format markdown` — the paste-into-a-PR form.
#[test]
fn test_config_diff_markdown_snapshot() {
    let diff = sample_config_diff();
    insta::assert_snapshot!("config_diff_markdown", format_diff_markdown(&diff));
}

/// `estimate --diff` — the side-by-side cost table for two WASM builds. Uses
/// the explicit-color entry point so the snapshot never depends on whether
/// the test runner's stdout is a TTY.
#[test]
fn test_estimate_diff_table_snapshot() {
    let old = sample_report();
    let mut new = sample_report();
    new.wasm_size = 12_288;
    new.cpu_instructions = 480_000;
    new.write_bytes = 96;
    insta::assert_snapshot!(
        "estimate_diff_table",
        format_cost_report_diff_sized(&old, &new, None, false)
    );
}

/// `config snapshot --json` — the machine-readable render of a saved
/// snapshot, which is what tooling consumes when a snapshot is "shown"
/// rather than diffed.
#[test]
fn test_config_snapshot_show_json_snapshot() {
    let snapshot = ConfigSnapshot {
        network: "testnet".to_string(),
        timestamp: "2026-09-15T00:00:00+00:00".to_string(),
        ledger: 3_894_195,
        protocol_version: Some(23),
        contract_compute: Some(ContractComputeV0 {
            ledger_max_instructions: 2_750_000_000,
            tx_max_instructions: 2_750_000_000,
            fee_rate_per_instructions_increment: 25,
            tx_memory_limit: 52_428_800,
        }),
        contract_ledger_cost: None,
        contract_historical_data: None,
        contract_events: None,
        contract_bandwidth: None,
        state_archival: None,
    };
    insta::assert_snapshot!(
        "config_snapshot_show_json",
        serde_json::to_string_pretty(&snapshot).unwrap()
    );
}
