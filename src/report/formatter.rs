//! Report formatting trait and implementations.
//!
//! Provides a common `ReportFormatter` abstraction for producing
//! human-readable or machine-readable output from a `CostReport`.
//!
//! # Implementations
//!
//! - [`TableFormatter`] — human-readable table with fee breakdown
//! - [`JsonFormatter`] — pretty-printed JSON
//! - [`CsvFormatter`] — comma-separated values
//! - [`MarkdownFormatter`] — GitHub-flavored Markdown table

use std::fmt;

use crate::report::cost_report::CostReport;

/// Formats a [`CostReport`] into a specific output representation.
///
/// All implementations are deterministic and produce stable output for the
/// same input, making them safe for snapshot testing and piping into scripts.
pub trait ReportFormatter {
    /// Format the report into the target representation.
    fn format(&self, report: &CostReport) -> String;

    /// Human-readable name of this format (e.g. `"table"`, `"json"`).
    fn name(&self) -> &'static str;
}

/// Formats a cost report as a human-readable table with fee breakdown.
///
/// This is the default output format used by the CLI.
pub struct TableFormatter;

impl ReportFormatter for TableFormatter {
    fn format(&self, report: &CostReport) -> String {
        let mut output = String::new();

        output.push_str(&format!("Function: {}\n", report.function));
        output.push_str(&format!(
            "Network: {} (ledger {})\n",
            report.network, report.ledger
        ));
        output.push_str(&format!("RPC round-trip: {} ms\n", report.rpc_latency_ms));
        output.push_str(&format!("WASM hash: {}\n\n", report.wasm_hash));

        let mut table = comfy_table::Table::new();
        table.set_header(vec!["Resource", "Consumed", "Fee (stroops)"]);

        table.add_row(vec![
            "CPU Instructions",
            &report.cpu_instructions.to_string(),
            "",
        ]);
        table.add_row(vec!["Memory Bytes", &report.memory_bytes.to_string(), ""]);
        table.add_row(vec!["Read Entries", &report.read_entries.to_string(), ""]);
        table.add_row(vec!["Write Entries", &report.write_entries.to_string(), ""]);
        table.add_row(vec!["Read Bytes", &report.read_bytes.to_string(), ""]);
        table.add_row(vec!["Write Bytes", &report.write_bytes.to_string(), ""]);
        table.add_row(vec!["Transaction Size", &report.tx_size.to_string(), ""]);

        output.push_str(&table.to_string());
        output.push('\n');

        output.push_str("\nFee Breakdown:\n");
        let total = report.fee.total_stroops;
        output.push_str(&format!(
            "  Non-refundable: {} stroops ({})\n",
            report.fee.non_refundable_stroops,
            crate::report::cost_report::fee_percentage(report.fee.non_refundable_stroops, total),
        ));
        output.push_str(&format!(
            "  Refundable:     {} stroops ({})\n",
            report.fee.refundable_stroops,
            crate::report::cost_report::fee_percentage(report.fee.refundable_stroops, total),
        ));
        output.push_str("\n  Components (of non-refundable):\n");
        output.push_str(&format!(
            "    CPU:        {} stroops ({})\n",
            report.fee.cpu_fee_stroops,
            crate::report::cost_report::fee_percentage(report.fee.cpu_fee_stroops, total),
        ));
        output.push_str(&format!(
            "    Storage:    {} stroops ({})\n",
            report.fee.storage_fee_stroops,
            crate::report::cost_report::fee_percentage(report.fee.storage_fee_stroops, total),
        ));
        output.push_str(&format!(
            "    Bandwidth:  {} stroops ({})\n",
            report.fee.bandwidth_fee_stroops,
            crate::report::cost_report::fee_percentage(report.fee.bandwidth_fee_stroops, total),
        ));
        output.push_str(&format!(
            "\n  Total:          {} stroops ({})\n",
            report.fee.total_stroops, report.fee.total_xlm,
        ));

        // ASCII bar chart for a quick visual summary of where the fee goes.
        output.push_str(&crate::report::cost_report::format_cost_breakdown_chart(
            report.fee.total_stroops,
            report.fee.non_refundable_stroops,
            report.fee.refundable_stroops,
        ));

        // Resource-limit warnings (#322) — only when something nears a ceiling.
        output.push_str(&crate::report::cost_report::format_resource_warnings(
            &report.warnings,
        ));

        // Historical trend table (#321) — only populated with `--history`.
        if let Some(history) = &report.history {
            output.push_str(&crate::report::cost_report::format_cost_history(history));
        }

        output.push('\n');
        output.push_str(&crate::report::cost_report::format_suggestions(
            &report.suggest_optimizations(),
        ));

        output
    }

    fn name(&self) -> &'static str {
        "table"
    }
}

impl fmt::Display for TableFormatter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "table")
    }
}

/// Formats a cost report as pretty-printed JSON.
pub struct JsonFormatter;

impl ReportFormatter for JsonFormatter {
    fn format(&self, report: &CostReport) -> String {
        let mut value = serde_json::to_value(report).unwrap_or(serde_json::Value::Null);
        let suggestions =
            serde_json::to_value(report.suggest_optimizations()).unwrap_or(serde_json::Value::Null);
        if let serde_json::Value::Object(ref mut map) = value {
            map.insert("suggestions".to_string(), suggestions);
        }
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
    }

    fn name(&self) -> &'static str {
        "json"
    }
}

impl fmt::Display for JsonFormatter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "json")
    }
}

/// Formats a cost report as CSV.
///
/// Output includes a header row followed by a single data row containing
/// all resource and fee fields. Values containing commas, quotes, or
/// newlines are escaped per RFC 4180.
pub struct CsvFormatter;

impl ReportFormatter for CsvFormatter {
    fn format(&self, report: &CostReport) -> String {
        let mut output = String::from(
            "function,network,ledger,wasm_hash,cpu_instructions,memory_bytes,\
             read_entries,write_entries,read_bytes,write_bytes,tx_size,\
             non_refundable_stroops,refundable_stroops,total_stroops,total_xlm,\
             rpc_latency_ms\n",
        );

        let row = csv_row(&[
            &report.function,
            &report.network,
            &report.ledger.to_string(),
            &report.wasm_hash,
            &report.cpu_instructions.to_string(),
            &report.memory_bytes.to_string(),
            &report.read_entries.to_string(),
            &report.write_entries.to_string(),
            &report.read_bytes.to_string(),
            &report.write_bytes.to_string(),
            &report.tx_size.to_string(),
            &report.fee.non_refundable_stroops.to_string(),
            &report.fee.refundable_stroops.to_string(),
            &report.fee.total_stroops.to_string(),
            &report.fee.total_xlm,
            &report.rpc_latency_ms.to_string(),
        ]);
        output.push_str(&row);
        output.push('\n');

        output
    }

    fn name(&self) -> &'static str {
        "csv"
    }
}

impl fmt::Display for CsvFormatter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "csv")
    }
}

/// Formats a cost report as a GitHub-flavored Markdown document.
///
/// Output includes a header section and a resource table followed by
/// a fee breakdown section.
pub struct MarkdownFormatter;

impl ReportFormatter for MarkdownFormatter {
    fn format(&self, report: &CostReport) -> String {
        let mut output = String::new();

        output.push_str(&format!("## Cost Report: `{}`\n\n", report.function));
        output.push_str(&format!(
            "- **Network:** {} (ledger {})\n",
            report.network, report.ledger
        ));
        output.push_str(&format!("- **WASM hash:** `{}`\n", report.wasm_hash));
        output.push_str(&format!(
            "- **RPC round-trip:** {} ms\n\n",
            report.rpc_latency_ms
        ));

        // Resource table
        output.push_str("### Resources\n\n");
        output.push_str("| Resource | Consumed |\n");
        output.push_str("| --- | --- |\n");
        output.push_str(&format!(
            "| CPU Instructions | {} |\n",
            report.cpu_instructions
        ));
        output.push_str(&format!("| Memory Bytes | {} |\n", report.memory_bytes));
        output.push_str(&format!("| Read Entries | {} |\n", report.read_entries));
        output.push_str(&format!("| Write Entries | {} |\n", report.write_entries));
        output.push_str(&format!("| Read Bytes | {} |\n", report.read_bytes));
        output.push_str(&format!("| Write Bytes | {} |\n", report.write_bytes));
        output.push_str(&format!("| Transaction Size | {} |\n", report.tx_size));

        // Resource-limit warnings (#322) as a GitHub callout.
        if !report.warnings.is_empty() {
            output.push_str("\n### Resource Limit Warnings\n\n");
            for warning in &report.warnings {
                output.push_str(&format!("> **Warning:** {}\n\n", warning.message));
            }
        }

        // Fee breakdown, nested in a collapsible section so long reports stay
        // compact when posted as a GitHub PR comment (#325).
        output.push_str("\n<details>\n<summary>Fee breakdown</summary>\n\n");
        output.push_str("### Fee Breakdown\n\n");
        output.push_str("| Component | Stroops | % of Total |\n");
        output.push_str("| --- | --- | --- |\n");
        let total = report.fee.total_stroops;
        let pct = crate::report::cost_report::fee_percentage;
        output.push_str(&format!(
            "| Non-refundable | {} | {} |\n",
            report.fee.non_refundable_stroops,
            pct(report.fee.non_refundable_stroops, total)
        ));
        output.push_str(&format!(
            "| Refundable | {} | {} |\n",
            report.fee.refundable_stroops,
            pct(report.fee.refundable_stroops, total)
        ));
        output.push_str(&format!(
            "| CPU | {} | {} |\n",
            report.fee.cpu_fee_stroops,
            pct(report.fee.cpu_fee_stroops, total)
        ));
        output.push_str(&format!(
            "| Storage | {} | {} |\n",
            report.fee.storage_fee_stroops,
            pct(report.fee.storage_fee_stroops, total)
        ));
        output.push_str(&format!(
            "| Bandwidth | {} | {} |\n",
            report.fee.bandwidth_fee_stroops,
            pct(report.fee.bandwidth_fee_stroops, total)
        ));
        output.push_str(&format!(
            "| **Total** | **{}** ({}) | **100.0%** |\n",
            report.fee.total_stroops, report.fee.total_xlm,
        ));

        // Optimization suggestions
        output.push_str("\n### Optimization Suggestions\n\n");
        let suggestions = report.suggest_optimizations();
        if suggestions.is_empty() {
            output.push_str(
                "No cost optimizations identified (fee rates unavailable or no reducible resources).\n",
            );
        } else {
            for s in &suggestions {
                output.push_str(&format!(
                    "- **{}**: {} (potential saving: {} stroops)\n",
                    s.title, s.detail, s.potential_savings_stroops
                ));
            }
        }
        output.push_str("\n</details>\n");

        output
    }

    fn name(&self) -> &'static str {
        "markdown"
    }
}

impl fmt::Display for MarkdownFormatter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "markdown")
    }
}

/// Escape a value for CSV output per RFC 4180.
///
/// If the value contains a comma, double-quote, or newline, it is wrapped
/// in double-quotes and internal double-quotes are doubled.
fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        let escaped = value.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        value.to_string()
    }
}

/// Build a CSV row from a slice of field values.
fn csv_row(fields: &[&str]) -> String {
    fields
        .iter()
        .map(|f| csv_escape(f))
        .collect::<Vec<_>>()
        .join(",")
}

/// Returns the formatter for the given format name.
///
/// Recognized names: `"table"`, `"json"`, `"csv"`, `"markdown"`.
/// Returns `None` for unknown names.
pub fn formatter_by_name(name: &str) -> Option<Box<dyn ReportFormatter>> {
    match name {
        "table" => Some(Box::new(TableFormatter)),
        "json" => Some(Box::new(JsonFormatter)),
        "csv" => Some(Box::new(CsvFormatter)),
        "markdown" => Some(Box::new(MarkdownFormatter)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::fee_calc::FeeBreakdown;

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
            rates: None,
            warnings: Vec::new(),
            history: None,
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
            rates: None,
            warnings: Vec::new(),
            history: None,
        }
    }

    // ── Table formatter ──────────────────────────────────────────────

    #[test]
    fn test_table_formatter_contains_function_name() {
        let formatter = TableFormatter;
        let output = formatter.format(&sample_report());
        assert!(output.contains("increment"));
    }

    #[test]
    fn test_table_formatter_contains_network() {
        let formatter = TableFormatter;
        let output = formatter.format(&sample_report());
        assert!(output.contains("testnet"));
        assert!(output.contains("3894195"));
    }

    #[test]
    fn test_table_formatter_contains_rpc_latency() {
        let formatter = TableFormatter;
        let output = formatter.format(&sample_report());
        assert!(output.contains("RPC round-trip: 87 ms"));
    }

    #[test]
    fn test_table_formatter_contains_fee_breakdown() {
        let formatter = TableFormatter;
        let output = formatter.format(&sample_report());
        assert!(output.contains("Non-refundable: 4496 stroops"));
        assert!(output.contains("Refundable:     10931 stroops"));
        assert!(output.contains("Total:          15427 stroops (0.0015427)"));
    }

    #[test]
    fn test_table_formatter_empty_report() {
        let formatter = TableFormatter;
        let output = formatter.format(&empty_report());
        assert!(output.contains("(wasm upload)"));
        assert!(output.contains("mainnet"));
    }

    #[test]
    fn test_table_formatter_name() {
        let formatter = TableFormatter;
        assert_eq!(formatter.name(), "table");
    }

    #[test]
    fn test_table_formatter_display() {
        assert_eq!(TableFormatter.to_string(), "table");
    }

    #[test]
    fn test_table_formatter_contains_chart() {
        let formatter = TableFormatter;
        let output = formatter.format(&sample_report());
        assert!(output.contains("Fee Breakdown Chart:"));
        assert!(output.contains("Non-refundable"));
        assert!(output.contains("Refundable"));
        // Only rows after the chart header belong to the chart — the fee
        // breakdown section above also names both components.
        let chart_start = output
            .find("Fee Breakdown Chart:")
            .expect("table output should contain the fee breakdown chart");
        let chart_lines: Vec<&str> = output[chart_start..]
            .lines()
            .filter(|l| l.contains("Non-refundable") || l.contains("Refundable"))
            .collect();
        assert_eq!(
            chart_lines.len(),
            2,
            "chart should render one row per non-zero fee component"
        );
        for line in &chart_lines {
            assert!(line.contains('#'), "chart line should contain '#': {line}");
        }
    }

    #[test]
    fn test_table_formatter_empty_report_no_chart() {
        let formatter = TableFormatter;
        let output = formatter.format(&empty_report());
        // Chart section should not be present when all fees are zero
        assert!(!output.contains("Fee Breakdown Chart:"));
    }

    // ── JSON formatter ───────────────────────────────────────────────

    #[test]
    fn test_json_formatter_valid_json() {
        let formatter = JsonFormatter;
        let output = formatter.format(&sample_report());
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(parsed["function"], "increment");
        assert_eq!(parsed["cpu_instructions"], 532_502);
    }

    #[test]
    fn test_json_formatter_contains_all_fields() {
        let formatter = JsonFormatter;
        let output = formatter.format(&sample_report());
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["wasm_hash"], "abc123def456");
        assert_eq!(parsed["ledger"], 3_894_195);
        assert_eq!(parsed["network"], "testnet");
        assert_eq!(parsed["rpc_latency_ms"], 87);
        assert_eq!(parsed["fee"]["total_stroops"], 15_427);
        assert_eq!(parsed["fee"]["total_xlm"], "0.0015427");
    }

    #[test]
    fn test_json_formatter_empty_report() {
        let formatter = JsonFormatter;
        let output = formatter.format(&empty_report());
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["function"], "(wasm upload)");
        assert_eq!(parsed["cpu_instructions"], 0);
    }

    #[test]
    fn test_json_formatter_deterministic() {
        let formatter = JsonFormatter;
        let a = formatter.format(&sample_report());
        let b = formatter.format(&sample_report());
        assert_eq!(a, b);
    }

    #[test]
    fn test_json_formatter_name() {
        assert_eq!(JsonFormatter.name(), "json");
    }

    // ── CSV formatter ────────────────────────────────────────────────

    #[test]
    fn test_csv_formatter_has_header() {
        let formatter = CsvFormatter;
        let output = formatter.format(&sample_report());
        let first_line = output.lines().next().unwrap();
        assert!(first_line.starts_with("function,"));
        let field_count = first_line.split(',').count();
        assert_eq!(field_count, 16);
    }

    #[test]
    fn test_csv_formatter_data_row_values() {
        let formatter = CsvFormatter;
        let output = formatter.format(&sample_report());
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2); // header + 1 data row
        let data = lines[1];
        assert!(data.contains("increment"));
        assert!(data.contains("testnet"));
        assert!(data.contains("532502"));
        assert!(data.contains("15427"));
    }

    #[test]
    fn test_csv_formatter_empty_report() {
        let formatter = CsvFormatter;
        let output = formatter.format(&empty_report());
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[1].contains("(wasm upload)"));
    }

    #[test]
    fn test_csv_escape_plain() {
        assert_eq!(csv_escape("hello"), "hello");
        assert_eq!(csv_escape("123"), "123");
    }

    #[test]
    fn test_csv_escape_comma() {
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
    }

    #[test]
    fn test_csv_escape_quote() {
        assert_eq!(csv_escape(r#"say "hi""#), r#""say ""hi""""#);
    }

    #[test]
    fn test_csv_escape_newline() {
        assert_eq!(csv_escape("line1\nline2"), "\"line1\nline2\"");
    }

    #[test]
    fn test_csv_formatter_special_characters() {
        let report = CostReport {
            function: "func,\"test\"".to_string(),
            ..sample_report()
        };
        let formatter = CsvFormatter;
        let output = formatter.format(&report);
        let lines: Vec<&str> = output.lines().collect();
        assert!(lines[1].contains(r#""func,""test""""#));
    }

    #[test]
    fn test_csv_formatter_name() {
        assert_eq!(CsvFormatter.name(), "csv");
    }

    // ── Markdown formatter ───────────────────────────────────────────

    #[test]
    fn test_markdown_formatter_has_header() {
        let formatter = MarkdownFormatter;
        let output = formatter.format(&sample_report());
        assert!(output.starts_with("## Cost Report: `increment`"));
    }

    #[test]
    fn test_markdown_formatter_has_resource_table() {
        let formatter = MarkdownFormatter;
        let output = formatter.format(&sample_report());
        assert!(output.contains("### Resources"));
        assert!(output.contains("| CPU Instructions | 532502 |"));
        assert!(output.contains("| Memory Bytes | 0 |"));
    }

    #[test]
    fn test_markdown_formatter_has_fee_table() {
        let formatter = MarkdownFormatter;
        let output = formatter.format(&sample_report());
        assert!(output.contains("### Fee Breakdown"));
        assert!(output.contains("| Non-refundable | 4496 |"));
        assert!(output.contains("| Refundable | 10931 |"));
        assert!(output.contains("| **Total** | **15427** (0.0015427) |"));
    }

    #[test]
    fn test_markdown_formatter_network_info() {
        let formatter = MarkdownFormatter;
        let output = formatter.format(&sample_report());
        assert!(output.contains("**Network:** testnet (ledger 3894195)"));
        assert!(output.contains("**WASM hash:** `abc123def456`"));
        assert!(output.contains("**RPC round-trip:** 87 ms"));
    }

    #[test]
    fn test_markdown_formatter_empty_report() {
        let formatter = MarkdownFormatter;
        let output = formatter.format(&empty_report());
        assert!(output.contains("(wasm upload)"));
        assert!(output.contains("**Network:** mainnet (ledger 0)"));
    }

    #[test]
    fn test_markdown_formatter_deterministic() {
        let formatter = MarkdownFormatter;
        let a = formatter.format(&sample_report());
        let b = formatter.format(&sample_report());
        assert_eq!(a, b);
    }

    #[test]
    fn test_markdown_formatter_name() {
        assert_eq!(MarkdownFormatter.name(), "markdown");
    }

    // ── formatter_by_name ────────────────────────────────────────────

    #[test]
    fn test_formatter_by_name_all_variants() {
        assert_eq!(formatter_by_name("table").unwrap().name(), "table");
        assert_eq!(formatter_by_name("json").unwrap().name(), "json");
        assert_eq!(formatter_by_name("csv").unwrap().name(), "csv");
        assert_eq!(formatter_by_name("markdown").unwrap().name(), "markdown");
    }

    #[test]
    fn test_formatter_by_name_unknown_returns_none() {
        assert!(formatter_by_name("xml").is_none());
        assert!(formatter_by_name("").is_none());
    }

    // ── Cross-format consistency ─────────────────────────────────────

    #[test]
    fn test_all_formatters_produce_non_empty_output() {
        let report = sample_report();
        let formatters: Vec<Box<dyn ReportFormatter>> = vec![
            Box::new(TableFormatter),
            Box::new(JsonFormatter),
            Box::new(CsvFormatter),
            Box::new(MarkdownFormatter),
        ];
        for formatter in &formatters {
            let output = formatter.format(&report);
            assert!(
                !output.is_empty(),
                "{} formatter produced empty output",
                formatter.name()
            );
        }
    }

    // ── Resource warnings & history rendering (#321, #322) ───────────

    fn sample_warning() -> crate::report::cost_report::ResourceWarning {
        crate::report::cost_report::ResourceWarning {
            resource: "cpu_instructions".to_string(),
            label: "CPU instructions".to_string(),
            used: 900,
            limit: 1_000,
            percent: 90,
            message: "CPU instructions at 90% of the network limit (900 of 1000)".to_string(),
        }
    }

    #[test]
    fn test_json_formatter_always_includes_warnings_array() {
        let formatter = JsonFormatter;
        let output = formatter.format(&sample_report());
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(parsed["warnings"], serde_json::json!([]));
    }

    #[test]
    fn test_json_formatter_serializes_warnings_and_history() {
        let mut report = sample_report();
        report.warnings = vec![sample_warning()];
        report.history = Some(vec![crate::report::cost_report::HistoryEntry {
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            ledger: 10,
            cpu_instructions: 100,
            total_stroops: 20_000,
            total_xlm: "0.0020000".to_string(),
            delta_stroops: 5_000,
            trend: crate::report::cost_report::CostTrend::Regression,
        }]);

        let output = JsonFormatter.format(&report);
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(parsed["warnings"][0]["resource"], "cpu_instructions");
        assert_eq!(parsed["warnings"][0]["percent"], 90);
        assert_eq!(parsed["history"][0]["trend"], "regression");
        assert_eq!(parsed["history"][0]["delta_stroops"], 5_000);
    }

    #[test]
    fn test_json_formatter_omits_history_when_not_requested() {
        let output = JsonFormatter.format(&sample_report());
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert!(parsed.get("history").is_none());
    }

    #[test]
    fn test_json_formatter_includes_empty_history_when_requested() {
        let mut report = sample_report();
        report.history = Some(Vec::new());
        let output = JsonFormatter.format(&report);
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(parsed["history"], serde_json::json!([]));
    }

    #[test]
    fn test_table_formatter_renders_resource_warnings() {
        let mut report = sample_report();
        report.warnings = vec![sample_warning()];
        let output = TableFormatter.format(&report);
        assert!(output.contains("Resource limit warnings:"));
        assert!(output.contains("CPU instructions at 90%"));
    }

    #[test]
    fn test_table_formatter_omits_warnings_when_clean() {
        let output = TableFormatter.format(&sample_report());
        assert!(!output.contains("Resource limit warnings:"));
    }

    #[test]
    fn test_table_formatter_renders_history() {
        let mut report = sample_report();
        report.history = Some(crate::report::cost_report::build_history_entries(
            report.fee.total_stroops,
            7,
            &[crate::report::cost_report::HistoricalRun {
                timestamp: "2026-01-01T00:00:00Z".to_string(),
                ledger: 100,
                cpu_instructions: 1_000,
                total_stroops: 20_000,
            }],
        ));
        let output = TableFormatter.format(&report);
        assert!(output.contains("Cost history (previous runs, newest first):"));
        assert!(output.contains("+4573"));
    }

    #[test]
    fn test_markdown_formatter_has_collapsible_details() {
        let output = MarkdownFormatter.format(&sample_report());
        assert!(output.contains("<details>"));
        assert!(output.contains("</details>"));
    }

    #[test]
    fn test_markdown_formatter_renders_resource_warnings() {
        let mut report = sample_report();
        report.warnings = vec![sample_warning()];
        let output = MarkdownFormatter.format(&report);
        assert!(output.contains("### Resource Limit Warnings"));
        assert!(output.contains("> **Warning:** CPU instructions at 90%"));
    }
}
