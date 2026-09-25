use soroban_cost_estimator::report::cost_report::CostReport;
use soroban_cost_estimator::report::fee_calc::FeeBreakdown;
use soroban_cost_estimator::report::formatter::{
    CsvFormatter, JsonFormatter, MarkdownFormatter, ReportFormatter, TableFormatter,
};

fn sample_report() -> CostReport {
    CostReport {
        function: "increment".to_string(),
        wasm_hash: "abc123def456".to_string(),
        wasm_size: 0,
        wasm_sections: Vec::new(),
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
        rates: None,
    }
}

fn empty_report() -> CostReport {
    CostReport {
        function: "(wasm upload)".to_string(),
        wasm_hash: "0000000000000000".to_string(),
        wasm_size: 0,
        wasm_sections: Vec::new(),
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
        rates: None,
    }
}

/// The same report, but carrying a real WASM section size breakdown so the
/// table and JSON renderers are exercised with section data present.
fn section_report() -> CostReport {
    let bytes = std::fs::read("tests/fixtures/contract.wasm").expect("fixture readable");
    let breakdown = soroban_cost_estimator::wasm::parser::section_size_breakdown(&bytes)
        .expect("fixture is valid WASM");
    let mut report = sample_report();
    report.wasm_size = bytes.len();
    report.wasm_sections = breakdown.sections;
    report
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

#[test]
fn test_table_formatter_sections_snapshot() {
    insta::assert_snapshot!(
        "table_formatter_sections",
        TableFormatter.format(&section_report())
    );
}

#[test]
fn test_json_formatter_sections_snapshot() {
    insta::assert_snapshot!(
        "json_formatter_sections",
        JsonFormatter.format(&section_report())
    );
}

#[test]
fn test_section_sizes_reconcile_with_wasm_size() {
    use soroban_cost_estimator::report::formatter::section_size_map;

    let report = section_report();
    let accounted: usize = report.wasm_sections.iter().map(|s| s.total_size).sum();
    assert_eq!(
        accounted, report.wasm_size,
        "section sizes must cover the file"
    );

    let map = section_size_map(&report);
    let mapped: usize = map
        .as_object()
        .expect("section_sizes is an object")
        .values()
        .map(|v| v.as_u64().expect("byte counts are integers") as usize)
        .sum();
    assert_eq!(
        mapped, report.wasm_size,
        "section_sizes map must cover the file"
    );

    let shares: f64 = report.wasm_sections.iter().map(|s| s.percent).sum();
    assert!(
        (shares - 100.0).abs() < 0.05,
        "shares should add up to 100%, got {shares}"
    );

    let table = TableFormatter.format(&report);
    assert!(table.contains("WASM size:"), "table should show the size");
    assert!(
        table.contains("WASM sections"),
        "table should show sections"
    );
    assert!(
        table.contains("contractspecv0"),
        "table should list spec bytes"
    );
}
