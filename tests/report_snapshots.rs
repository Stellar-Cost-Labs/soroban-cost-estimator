use soroban_cost_estimator::report::cost_report::CostReport;
use soroban_cost_estimator::report::fee_calc::FeeBreakdown;
use soroban_cost_estimator::report::formatter::{
    CsvFormatter, JsonFormatter, MarkdownFormatter, ReportFormatter, TableFormatter,
};

fn sample_report() -> CostReport {
    CostReport {
        function: "increment".to_string(),
        wasm_hash: "abc123def456".to_string(),
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
            total_stroops: 15_427,
            total_xlm: "0.0015427".to_string(),
        },
        ledger: 3_894_195,
        network: "testnet".to_string(),
        rpc_latency_ms: 87,
    }
}

fn empty_report() -> CostReport {
    CostReport {
        function: "(wasm upload)".to_string(),
        wasm_hash: "0000000000000000".to_string(),
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
            total_stroops: 0,
            total_xlm: "0.0000000".to_string(),
        },
        ledger: 0,
        network: "mainnet".to_string(),
        rpc_latency_ms: 0,
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
