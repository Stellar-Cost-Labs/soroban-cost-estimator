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
