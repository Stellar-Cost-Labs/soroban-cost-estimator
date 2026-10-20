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

/// The same report, but carrying a real WASM section size breakdown so the
/// table and JSON renderers are exercised with section data present.
fn section_report() -> CostReport {
    let bytes = std::fs::read("tests/fixtures/contract.wasm").expect("fixture readable");
    let breakdown = soroban_cost_estimator::wasm::parser::section_size_breakdown(&bytes)
        .expect("fixture is valid WASM");
    let mut report = sample_report();
    report.wasm_size = bytes.len() as u64;
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

/// A report carrying a real section breakdown renders the per-section size
/// table (issue #312).
#[test]
fn test_table_formatter_section_breakdown_snapshot() {
    insta::assert_snapshot!(
        "table_formatter_sections",
        TableFormatter.format(&section_report())
    );
}

/// The section breakdown also reaches the JSON output as a `section_sizes`
/// map (issue #312).
#[test]
fn test_json_formatter_section_breakdown_snapshot() {
    insta::assert_snapshot!(
        "json_formatter_sections",
        JsonFormatter.format(&section_report())
    );
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
        wasm_sections: Vec::new(),
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

#[cfg(test)]
mod json_schema_validation {
    use super::*;
    use soroban_cost_estimator::config_snapshot::{
        diff::diff_snapshots,
        model::{ConfigSnapshot, ContractComputeV0},
    };
    use soroban_cost_estimator::report::cost_report::{
        BatchCostReport, ContractCostSummary, CostDelta, CostProjection, CostTrend, HistoryEntry,
        ResourceWarning, format_batch_report_json,
    };
    use soroban_cost_estimator::report::fee_calc::FeeRates;
    use soroban_cost_estimator::report::formatter::ReportOptions;
    use soroban_cost_estimator::wasm::parser::{load_wasm, section_size_breakdown};

    fn assert_has_keys(obj: &serde_json::Map<String, serde_json::Value>, keys: &[&str]) {
        for key in keys {
            assert!(
                obj.contains_key(*key),
                "expected required JSON key '{key}' in object with keys: {:?}",
                obj.keys().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn schema_single_estimate_required_keys_and_types() {
        let mut report = section_report();
        report.rates = Some(FeeRates {
            fee_per_10k_insns: 100,
            fee_per_read_entry: 500,
            fee_per_write_entry: 1_000,
            fee_per_read_1kb: 200,
            fee_per_1kb: 300,
        });
        report.warnings = vec![ResourceWarning {
            resource: "cpu_instructions".to_string(),
            label: "CPU instructions".to_string(),
            used: 85_000_000,
            limit: 100_000_000,
            percent: 85,
            message: "CPU instructions at 85% of the network limit".to_string(),
        }];

        let raw = JsonFormatter.format(&report);
        let val: serde_json::Value =
            serde_json::from_str(&raw).expect("single estimate JSON must be valid");
        let obj = val.as_object().expect("top-level JSON must be an object");

        assert_has_keys(
            obj,
            &[
                "function",
                "wasm_hash",
                "wasm_size",
                "wasm_sections",
                "section_sizes",
                "cpu_instructions",
                "memory_bytes",
                "tx_size",
                "read_entries",
                "write_entries",
                "read_bytes",
                "write_bytes",
                "fee",
                "ledger_sequence",
                "network",
                "rpc_latency_ms",
                "warnings",
                "contract_meta",
                "suggestions",
                "optimization_suggestions",
            ],
        );

        assert!(obj["function"].is_string());
        assert!(obj["wasm_hash"].is_string());
        assert!(obj["wasm_size"].is_u64());
        assert!(obj["wasm_sections"].is_array());
        assert!(obj["section_sizes"].is_object());
        assert!(obj["cpu_instructions"].is_u64());
        assert!(obj["memory_bytes"].is_u64());
        assert!(obj["tx_size"].is_u64());
        assert!(obj["read_entries"].is_u64());
        assert!(obj["write_entries"].is_u64());
        assert!(obj["read_bytes"].is_u64());
        assert!(obj["write_bytes"].is_u64());
        assert!(obj["ledger_sequence"].is_u64());
        assert!(obj["network"].is_string());
        assert!(obj["rpc_latency_ms"].is_u64());
        assert!(obj["warnings"].is_array());
        assert!(obj["contract_meta"].is_object());
        assert!(obj["suggestions"].is_array());
        assert!(obj["optimization_suggestions"].is_array());
        // Internal `rates` field must never leak into JSON output.
        assert!(!obj.contains_key("rates"));

        // Validate nested `fee` object schema.
        let fee = obj["fee"].as_object().expect("fee must be an object");
        assert_has_keys(
            fee,
            &[
                "non_refundable_stroops",
                "refundable_stroops",
                "cpu_fee_stroops",
                "storage_fee_stroops",
                "bandwidth_fee_stroops",
                "base_fee_stroops",
                "total_stroops",
                "total_xlm",
                "fee_percentages",
            ],
        );
        assert!(fee["non_refundable_stroops"].is_i64());
        assert!(fee["refundable_stroops"].is_i64());
        assert!(fee["cpu_fee_stroops"].is_i64());
        assert!(fee["storage_fee_stroops"].is_i64());
        assert!(fee["bandwidth_fee_stroops"].is_i64());
        assert!(fee["base_fee_stroops"].is_i64());
        assert!(fee["total_stroops"].is_i64());
        assert!(fee["total_xlm"].is_string());
        assert!(fee["fee_percentages"].is_object());
    }

    #[test]
    fn schema_single_estimate_warnings_and_suggestions_items() {
        let mut report = sample_report();
        report.wasm_size = 70_000;
        report.rates = Some(FeeRates {
            fee_per_10k_insns: 100,
            fee_per_read_entry: 500,
            fee_per_write_entry: 1_000,
            fee_per_read_1kb: 200,
            fee_per_1kb: 300,
        });
        report.warnings = vec![ResourceWarning {
            resource: "cpu_instructions".to_string(),
            label: "CPU instructions".to_string(),
            used: 85_000_000,
            limit: 100_000_000,
            percent: 85,
            message: "CPU instructions at 85% of the network limit".to_string(),
        }];

        let raw = JsonFormatter.format(&report);
        let obj: serde_json::Value =
            serde_json::from_str(&raw).expect("single estimate JSON must be valid");

        let warning = obj["warnings"][0]
            .as_object()
            .expect("warning item must be an object");
        assert_has_keys(
            warning,
            &["resource", "label", "used", "limit", "percent", "message"],
        );
        assert!(warning["resource"].is_string());
        assert!(warning["label"].is_string());
        assert!(warning["used"].is_u64());
        assert!(warning["limit"].is_u64());
        assert!(warning["percent"].is_u64());
        assert!(warning["message"].is_string());

        // `suggestions` contains human-readable tip strings; `optimization_suggestions`
        // contains structured objects with `title`, `detail`, and `potential_savings_stroops`.
        assert!(obj["suggestions"][0].is_string());
        let opt_suggestion = obj["optimization_suggestions"][0]
            .as_object()
            .expect("optimization_suggestions item must be an object");
        assert_has_keys(
            opt_suggestion,
            &["title", "detail", "potential_savings_stroops"],
        );
        assert!(opt_suggestion["title"].is_string());
        assert!(opt_suggestion["detail"].is_string());
        assert!(opt_suggestion["potential_savings_stroops"].is_i64());
    }

    #[test]
    fn schema_single_estimate_optional_fields_absent_or_present() {
        let empty = empty_report();
        let quiet_raw = JsonFormatter.format_with(&empty, ReportOptions { quiet: true });
        let quiet_val: serde_json::Value =
            serde_json::from_str(&quiet_raw).expect("quiet JSON must parse");
        let quiet_obj = quiet_val.as_object().expect("must be object");

        assert!(!quiet_obj.contains_key("wasm_size"));
        assert!(!quiet_obj.contains_key("wasm_sections"));
        assert!(!quiet_obj.contains_key("section_sizes"));
        assert!(!quiet_obj.contains_key("history"));
        assert!(!quiet_obj.contains_key("projections"));
        assert!(!quiet_obj.contains_key("suggestions"));
        assert!(!quiet_obj.contains_key("optimization_suggestions"));

        let mut rich = sample_report();
        rich.history = Some(vec![HistoryEntry {
            timestamp: "2026-01-15T12:00:00Z".to_string(),
            ledger: 3_890_000,
            cpu_instructions: 500_000,
            total_stroops: 14_000,
            total_xlm: "0.0014000".to_string(),
            delta_stroops: -1_427,
            trend: CostTrend::Improvement,
        }]);
        rich.projections = Some(vec![
            CostProjection {
                invocations: 100,
                total_stroops: 1_542_700,
                total_xlm: "0.1542700".to_string(),
                usd: None,
            },
            CostProjection {
                invocations: 1_000,
                total_stroops: 15_427_000,
                total_xlm: "1.5427000".to_string(),
                usd: Some(0.185_124),
            },
        ]);

        let rich_raw = JsonFormatter.format(&rich);
        let rich_val: serde_json::Value =
            serde_json::from_str(&rich_raw).expect("rich JSON must parse");
        let rich_obj = rich_val.as_object().expect("must be object");

        let history = rich_obj["history"]
            .as_array()
            .expect("history must be an array");
        let h0 = history[0].as_object().expect("history item must be object");
        assert_has_keys(
            h0,
            &[
                "timestamp",
                "ledger",
                "cpu_instructions",
                "total_stroops",
                "total_xlm",
                "delta_stroops",
                "trend",
            ],
        );
        assert!(h0["timestamp"].is_string());
        assert!(h0["ledger"].is_u64());
        assert!(h0["cpu_instructions"].is_u64());
        assert!(h0["total_stroops"].is_i64());
        assert!(h0["total_xlm"].is_string());
        assert!(h0["delta_stroops"].is_i64());
        assert_eq!(h0["trend"], "improvement");

        let projections = rich_obj["projections"]
            .as_array()
            .expect("projections must be an array");
        let p0 = projections[0]
            .as_object()
            .expect("projection item must be object");
        assert_has_keys(p0, &["invocations", "total_stroops", "total_xlm"]);
        assert!(!p0.contains_key("usd"));

        let p1 = projections[1]
            .as_object()
            .expect("projection item must be object");
        assert!(p1["usd"].is_f64());
    }

    #[test]
    fn schema_batch_contract_cost_summary() {
        let contracts = vec![
            ContractCostSummary {
                path: "contracts/token.wasm".to_string(),
                wasm_hash: Some("deadbeef".to_string()),
                function_count: Some(4),
                upload_cost_stroops: Some(25_000),
                min_invocation_fee_stroops: Some(1_200),
                max_invocation_fee_stroops: Some(8_400),
                functions_simulated: Some(4),
                error: None,
            },
            ContractCostSummary::unloadable("contracts/broken.wasm", "invalid magic number"),
        ];
        let batch = BatchCostReport::new("testnet", contracts);
        let raw = format_batch_report_json(&batch);
        let val: serde_json::Value =
            serde_json::from_str(&raw).expect("batch report JSON must parse");
        let obj = val.as_object().expect("batch report must be an object");

        assert_has_keys(
            obj,
            &[
                "network",
                "contracts",
                "total_contracts",
                "succeeded",
                "failed",
            ],
        );
        assert!(obj["network"].is_string());
        assert!(obj["contracts"].is_array());
        assert!(obj["total_contracts"].is_u64());
        assert!(obj["succeeded"].is_u64());
        assert!(obj["failed"].is_u64());

        let rows = obj["contracts"].as_array().expect("contracts array");
        assert_eq!(rows.len(), 2);

        let ok_row = rows[0].as_object().expect("row 0 must be object");
        assert_has_keys(
            ok_row,
            &[
                "path",
                "wasm_hash",
                "function_count",
                "upload_cost_stroops",
                "min_invocation_fee_stroops",
                "max_invocation_fee_stroops",
                "functions_simulated",
            ],
        );
        assert!(ok_row["path"].is_string());
        assert!(ok_row["wasm_hash"].is_string());
        assert!(ok_row["function_count"].is_u64());
        assert!(ok_row["upload_cost_stroops"].is_i64());
        assert!(ok_row["min_invocation_fee_stroops"].is_i64());
        assert!(ok_row["max_invocation_fee_stroops"].is_i64());
        assert!(ok_row["functions_simulated"].is_u64());
        assert!(!ok_row.contains_key("error"));

        let err_row = rows[1].as_object().expect("row 1 must be object");
        assert_has_keys(err_row, &["path", "error"]);
        assert!(err_row["path"].is_string());
        assert!(err_row["error"].is_string());
        assert!(!err_row.contains_key("wasm_hash"));
        assert!(!err_row.contains_key("function_count"));
        assert!(!err_row.contains_key("upload_cost_stroops"));
        assert!(!err_row.contains_key("min_invocation_fee_stroops"));
        assert!(!err_row.contains_key("max_invocation_fee_stroops"));
        assert!(!err_row.contains_key("functions_simulated"));
    }

    #[test]
    fn schema_estimate_all_summary() {
        let reports = vec![
            batch_report("increment", 532_502, 15_427, 1),
            batch_report("decrement", 410_000, 12_000, 2),
        ];
        let summary =
            summarize_estimate_all(&reports, DEFAULT_PRECISION).expect("non-empty summary");
        let summary_val =
            serde_json::to_value(&summary).expect("EstimateAllSummary must serialize");
        let summary_obj = summary_val
            .as_object()
            .expect("EstimateAllSummary must serialize as object");
        assert_has_keys(
            summary_obj,
            &[
                "functions_evaluated",
                "min_fee_stroops",
                "max_fee_stroops",
                "avg_fee_stroops",
                "total_fee_stroops",
                "min_cpu_instructions",
                "max_cpu_instructions",
                "total_cpu_instructions",
                "total_write_entries",
                "total_read_entries",
                "min_total_xlm",
                "max_total_xlm",
                "avg_total_xlm",
            ],
        );
        assert!(summary_obj["functions_evaluated"].is_u64());
        assert!(summary_obj["min_fee_stroops"].is_i64());
        assert!(summary_obj["max_fee_stroops"].is_i64());
        assert!(summary_obj["avg_fee_stroops"].is_i64());
        assert!(summary_obj["total_fee_stroops"].is_i64());
        assert!(summary_obj["min_cpu_instructions"].is_u64());
        assert!(summary_obj["max_cpu_instructions"].is_u64());
        assert!(summary_obj["total_cpu_instructions"].is_u64());
        assert!(summary_obj["total_write_entries"].is_u64());
        assert!(summary_obj["total_read_entries"].is_u64());
        assert!(summary_obj["min_total_xlm"].is_string());
        assert!(summary_obj["max_total_xlm"].is_string());
        assert!(summary_obj["avg_total_xlm"].is_string());
    }

    #[test]
    fn schema_cost_delta() {
        let curr = sample_report();
        let delta = CostDelta::compute(0, 0, 0, None, &curr);
        let delta_val = serde_json::to_value(delta).expect("CostDelta must serialize");
        let delta_obj = delta_val.as_object().expect("CostDelta must be an object");

        assert_has_keys(
            delta_obj,
            &[
                "cpu_instructions",
                "memory_bytes",
                "fee_stroops",
                "read_entries",
                "write_entries",
            ],
        );
        assert!(delta_obj["read_entries"].is_null());
        assert!(delta_obj["write_entries"].is_null());

        for key in ["cpu_instructions", "memory_bytes", "fee_stroops"] {
            let m = delta_obj[key]
                .as_object()
                .expect("each metric delta must be an object");
            assert_has_keys(m, &["previous", "current", "absolute", "percent"]);
            assert!(m["previous"].is_i64());
            assert!(m["current"].is_i64());
            assert!(m["absolute"].is_i64());
            assert!(
                m["percent"].is_null(),
                "expected null percent when previous == 0 for {key}"
            );
        }

        let delta_with_io = CostDelta::compute(500_000, 1_024, 14_000, Some((1, 1)), &curr);
        let delta_io_val = serde_json::to_value(delta_with_io).expect("CostDelta must serialize");
        assert!(
            delta_io_val["cpu_instructions"]["percent"].is_f64(),
            "expected numeric percent when previous > 0"
        );
        assert!(delta_io_val["read_entries"].is_object());
        assert!(delta_io_val["write_entries"].is_object());
    }

    #[test]
    fn schema_config_diff() {
        let old_snap = ConfigSnapshot {
            network: "testnet".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            ledger: 100,
            protocol_version: Some(21),
            contract_compute: Some(ContractComputeV0 {
                ledger_max_instructions: 100_000_000,
                tx_max_instructions: 100_000_000,
                fee_rate_per_instructions_increment: 25,
                tx_memory_limit: 41_943_040,
            }),
            contract_ledger_cost: None,
            contract_historical_data: None,
            contract_events: None,
            contract_bandwidth: None,
            state_archival: None,
        };
        let new_snap = ConfigSnapshot {
            network: "testnet".to_string(),
            timestamp: "2026-01-02T00:00:00Z".to_string(),
            ledger: 200,
            protocol_version: None,
            contract_compute: Some(ContractComputeV0 {
                ledger_max_instructions: 100_000_000,
                tx_max_instructions: 100_000_000,
                fee_rate_per_instructions_increment: 50,
                tx_memory_limit: 41_943_040,
            }),
            contract_ledger_cost: None,
            contract_historical_data: None,
            contract_events: None,
            contract_bandwidth: None,
            state_archival: None,
        };

        let cfg_diff = diff_snapshots(&old_snap, &new_snap);
        let cfg_val = serde_json::to_value(&cfg_diff).expect("ConfigDiff must serialize");
        let cfg_obj = cfg_val.as_object().expect("ConfigDiff must be an object");

        assert_has_keys(
            cfg_obj,
            &[
                "old_snapshot",
                "new_snapshot",
                "changes",
                "has_pricing_changes",
            ],
        );
        assert!(cfg_obj["has_pricing_changes"].is_boolean());
        for snap_key in ["old_snapshot", "new_snapshot"] {
            let s = cfg_obj[snap_key]
                .as_object()
                .expect("snapshot info must be object");
            assert_has_keys(s, &["network", "ledger", "timestamp"]);
            assert!(s["network"].is_string());
            assert!(s["ledger"].is_u64());
            assert!(s["timestamp"].is_string());
        }

        let changes = cfg_obj["changes"]
            .as_array()
            .expect("changes must be array");
        assert_eq!(changes.len(), 1);
        let c0 = changes[0].as_object().expect("change entry must be object");
        assert_has_keys(
            c0,
            &[
                "field_path",
                "setting_id",
                "setting_name",
                "old_value",
                "new_value",
                "is_pricing_change",
                "explanation",
            ],
        );
        assert!(c0["field_path"].is_string());
        assert!(c0["setting_id"].is_u64() || c0["setting_id"].is_null());
        assert!(c0["setting_name"].is_string());
        assert!(c0["old_value"].is_string());
        assert!(c0["new_value"].is_string());
        assert!(c0["is_pricing_change"].is_boolean());
        assert!(c0["explanation"].is_string() || c0["explanation"].is_null());
    }

    #[test]
    fn schema_wasm_inspection_reports() {
        let bytes = std::fs::read("tests/fixtures/contract.wasm").expect("fixture readable");

        // 1. SectionSizeBreakdown schema (`wasm sections --json`).
        let breakdown = section_size_breakdown(&bytes).expect("valid WASM");
        let b_val = serde_json::to_value(&breakdown).expect("SectionSizeBreakdown must serialize");
        let b_obj = b_val
            .as_object()
            .expect("SectionSizeBreakdown must be object");
        assert_has_keys(b_obj, &["total_size", "header_size", "sections"]);
        assert!(b_obj["total_size"].is_u64());
        assert!(b_obj["header_size"].is_u64());
        assert!(b_obj["sections"].is_array());

        if let Some(first_sec) = b_obj["sections"].as_array().and_then(|a| a.first()) {
            let sec_obj = first_sec.as_object().expect("section entry must be object");
            assert_has_keys(
                sec_obj,
                &[
                    "name",
                    "id",
                    "offset",
                    "end",
                    "size",
                    "header_size",
                    "total_size",
                    "percent",
                ],
            );
            assert!(sec_obj["name"].is_string());
            assert!(sec_obj["id"].is_u64() || sec_obj["id"].is_null());
            assert!(sec_obj["offset"].is_u64() || sec_obj["offset"].is_null());
            assert!(sec_obj["end"].is_u64() || sec_obj["end"].is_null());
            assert!(sec_obj["size"].is_u64());
            assert!(sec_obj["header_size"].is_u64());
            assert!(sec_obj["total_size"].is_u64());
            assert!(sec_obj["percent"].is_f64());
        }

        // 2. WasmStructureSummary schema (`WasmInfo.summary`).
        let info =
            load_wasm(std::path::Path::new("tests/fixtures/contract.wasm")).expect("valid WASM");
        let s_val =
            serde_json::to_value(&info.summary).expect("WasmStructureSummary must serialize");
        let s_obj = s_val
            .as_object()
            .expect("WasmStructureSummary must be object");
        assert_has_keys(
            s_obj,
            &[
                "initial_pages",
                "max_pages",
                "imports_count",
                "exports_count",
                "has_start_function",
                "tables_count",
            ],
        );
        assert!(s_obj["initial_pages"].is_u64());
        assert!(s_obj["max_pages"].is_u64() || s_obj["max_pages"].is_null());
        assert!(s_obj["imports_count"].is_u64());
        assert!(s_obj["exports_count"].is_u64());
        assert!(s_obj["has_start_function"].is_boolean());
        assert!(s_obj["tables_count"].is_u64());
    }
}
