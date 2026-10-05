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

/// Rendering options that are not part of the report data itself.
///
/// Currently one switch: [`quiet`](Self::quiet), set by the CLI's global
/// `--quiet` flag, which drops every *advisory* section (optimization tips and
/// the quantified savings breakdown) while leaving the measured cost data
/// untouched. Machine consumers that already read the JSON keys are
/// unaffected — see [`JsonFormatter`] — so a caller can always ask for the
/// structured data explicitly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReportOptions {
    /// Suppress advisory output (optimization tips and savings breakdown).
    pub quiet: bool,
}

impl ReportOptions {
    /// Options with advisory output enabled — the default.
    #[must_use]
    pub fn verbose_advisory() -> Self {
        Self { quiet: false }
    }

    /// Options with advisory output suppressed, as `--quiet` requests.
    #[must_use]
    pub fn quiet() -> Self {
        Self { quiet: true }
    }
}

/// Formats a [`CostReport`] into a specific output representation.
///
/// All implementations are deterministic and produce stable output for the
/// same input, making them safe for snapshot testing and piping into scripts.
pub trait ReportFormatter {
    /// Format the report into the target representation.
    ///
    /// Equivalent to calling [`Self::format_with`] with the default
    /// [`ReportOptions`]; it exists so callers that do not care about
    /// `--quiet` keep a one-argument entry point.
    fn format(&self, report: &CostReport) -> String {
        self.format_with(report, ReportOptions::default())
    }

    /// Format the report into the target representation, honouring
    /// `options`.
    fn format_with(&self, report: &CostReport, options: ReportOptions) -> String;

    /// Human-readable name of this format (e.g. `"table"`, `"json"`).
    fn name(&self) -> &'static str;
}

/// Formats a cost report as a human-readable table with fee breakdown.
///
/// This is the default output format used by the CLI.
pub struct TableFormatter;

fn build_resource_table(report: &CostReport) -> comfy_table::Table {
    let mut table = comfy_table::Table::new();
    if crate::cli::should_colorize() {
        table.enforce_styling();
    } else {
        table.force_no_tty();
    }
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
    table
}

impl TableFormatter {
    /// Format a report as a human-readable table, choosing whether to append
    /// the fee-distribution bar chart.
    ///
    /// `show_chart` is supplied by the CLI, which disables the chart in
    /// `--quiet` mode and for piped/non-TTY output; `chart_width` is the
    /// terminal width the chart bars are scaled to. [`ReportFormatter::format`]
    /// always renders the chart at the default width so output stays
    /// deterministic for snapshot tests and libraries that call it directly.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn format_with_options(
        &self,
        report: &CostReport,
        show_chart: bool,
        chart_width: usize,
        options: ReportOptions,
    ) -> String {
        let mut output = String::new();

        output.push_str(&format!("Function: {}\n", report.function));
        output.push_str(&format!(
            "Network: {} (ledger {})\n",
            report.network, report.ledger
        ));
        output.push_str(&format!(
            "Simulated at ledger sequence: {}\n",
            crate::report::cost_report::format_ledger_sequence(report.ledger)
        ));
        output.push_str(&format!("RPC round-trip: {} ms\n", report.rpc_latency_ms));
        output.push_str(&format!("WASM hash: {}\n", report.wasm_hash));
        output.push_str(&format!("WASM size: {} bytes\n", report.wasm_size));
        output.push('\n');

        // Contract metadata from the WASM `contractmeta` section: always
        // rendered (present or absent) so the report states whether the
        // binary carried one.
        output.push_str(&crate::wasm::parser::format_contract_meta(
            &report.contract_meta,
        ));
        output.push_str("\n\n");

        let table = build_resource_table(report);
        output.push_str(&table.to_string());
        output.push('\n');

        output.push_str("\nFee Breakdown:\n\n");
        let pct = &report.fee.fee_percentages;
        let mut fee_table = comfy_table::Table::new();
        fee_table.set_header(vec!["Component", "Fee"]);
        fee_table.add_row(vec![
            "CPU Instructions",
            &format!(
                "{} stroops ({})",
                report.fee.cpu_fee_stroops,
                pct.get("cpu_instructions")
                    .map(String::as_str)
                    .unwrap_or("")
            ),
        ]);
        fee_table.add_row(vec![
            "Storage I/O",
            &format!(
                "{} stroops ({})",
                report.fee.storage_fee_stroops,
                pct.get("storage_read_write")
                    .map(String::as_str)
                    .unwrap_or("")
            ),
        ]);
        fee_table.add_row(vec![
            "Transaction Size",
            &format!(
                "{} stroops ({})",
                report.fee.bandwidth_fee_stroops,
                pct.get("transaction_size")
                    .map(String::as_str)
                    .unwrap_or("")
            ),
        ]);
        fee_table.add_row(vec![
            "Base Fee",
            &format!(
                "{} stroops ({})",
                report.fee.base_fee_stroops,
                pct.get("base_fee").map(String::as_str).unwrap_or("")
            ),
        ]);
        fee_table.add_row(vec![
            "Rent Fee",
            &format!(
                "{} stroops ({})",
                report.fee.refundable_stroops,
                pct.get("rent").map(String::as_str).unwrap_or("")
            ),
        ]);
        fee_table.add_row(vec![
            "Total",
            &format!(
                "{} stroops ({})",
                report.fee.total_stroops, report.fee.total_xlm
            ),
        ]);

        output.push_str(&fee_table.to_string());
        output.push('\n');

        // The fee bar chart is a human-only visual; the CLI omits it for
        // `--quiet` and non-TTY output by passing `show_chart = false`.
        // `render_fee_bar_chart` returns an empty string when there is nothing
        // to visualize, so the surrounding blank lines stay constant.
        if show_chart {
            output.push_str(&crate::report::cost_report::render_fee_bar_chart(
                &report.fee,
                chart_width,
            ));
        }

        if let Some(ref projections) = report.projections {
            output.push_str(&crate::report::cost_report::format_projections_table(
                projections,
            ));
        }

        // Advisory sections (contextual tips and the structured savings
        // breakdown) are omitted entirely under `--quiet`.
        if !options.quiet {
            let tips = crate::report::cost_report::generate_optimization_tips(report);
            if !tips.is_empty() {
                output.push('\n');
                output.push_str(&crate::report::cost_report::format_tips(&tips));
            }
            output.push('\n');
            output.push_str(&crate::report::cost_report::format_suggestions(
                &report.suggest_optimizations(),
            ));
        }

        output
    }
}

impl ReportFormatter for TableFormatter {
    fn format_with(&self, report: &CostReport, options: ReportOptions) -> String {
        self.format_with_options(
            report,
            true,
            crate::report::cost_report::DEFAULT_CHART_WIDTH,
            options,
        )
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
    /// Emits the report plus two derived keys:
    ///
    /// * `suggestions` — contextual, human-readable tips from
    ///   [`crate::report::cost_report::generate_optimization_tips`]
    ///   (issue #323).
    /// * `optimization_suggestions` — the structured per-resource savings
    ///   records from [`CostReport::suggest_optimizations`], each with a
    ///   quantified `potential_savings_stroops`.
    ///
    /// `suggestions` is the documented key for the tip list; the structured
    /// records live under `optimization_suggestions` so the two are never
    /// confused for one another.
    ///
    /// `--quiet` omits both keys (issue #323) so a machine consumer that
    /// wants the data simply leaves the flag off.
    fn format_with(&self, report: &CostReport, options: ReportOptions) -> String {
        let mut value = serde_json::to_value(report).unwrap_or(serde_json::Value::Null);
        if let serde_json::Value::Object(ref mut map) = value {
            if !options.quiet {
                let tips = serde_json::to_value(
                    crate::report::cost_report::generate_optimization_tips(report),
                )
                .unwrap_or(serde_json::Value::Null);
                let structured = serde_json::to_value(report.suggest_optimizations())
                    .unwrap_or(serde_json::Value::Null);
                map.insert("suggestions".to_string(), tips);
                map.insert("optimization_suggestions".to_string(), structured);
            }
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
    fn format_with(&self, report: &CostReport, _options: ReportOptions) -> String {
        let mut output = String::from(
            "function,network,ledger,wasm_hash,wasm_size,cpu_instructions,memory_bytes,\
             read_entries,write_entries,read_bytes,write_bytes,tx_size,\
             non_refundable_stroops,refundable_stroops,total_stroops,total_xlm,\
             rpc_latency_ms\n",
        );

        let row = csv_row(&[
            &report.function,
            &report.network,
            &report.ledger.to_string(),
            &report.wasm_hash,
            &report.wasm_size.to_string(),
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
    #[allow(clippy::too_many_lines)]
    fn format_with(&self, report: &CostReport, options: ReportOptions) -> String {
        let mut output = String::new();

        output.push_str(&format!("## Cost Report: `{}`\n\n", report.function));
        output.push_str(&format!(
            "- **Network:** {} (ledger {})\n",
            report.network, report.ledger
        ));
        output.push_str(&format!(
            "- **Simulated at ledger sequence:** `{}`\n",
            crate::report::cost_report::format_ledger_sequence(report.ledger)
        ));
        output.push_str(&format!("- **WASM hash:** `{}`\n", report.wasm_hash));
        output.push_str(&format!("- **WASM size:** {} bytes\n", report.wasm_size));
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

        // Fee breakdown
        output.push_str("\n### Fee Breakdown\n\n");
        output.push_str("| Component | Stroops | % of Total |\n");
        output.push_str("| --- | --- | --- |\n");
        let pct = &report.fee.fee_percentages;
        output.push_str(&format!(
            "| CPU Instructions | {} | {} |\n",
            report.fee.cpu_fee_stroops,
            pct.get("cpu_instructions")
                .map(String::as_str)
                .unwrap_or("")
        ));
        output.push_str(&format!(
            "| Storage I/O | {} | {} |\n",
            report.fee.storage_fee_stroops,
            pct.get("storage_read_write")
                .map(String::as_str)
                .unwrap_or("")
        ));
        output.push_str(&format!(
            "| Transaction Size | {} | {} |\n",
            report.fee.bandwidth_fee_stroops,
            pct.get("transaction_size")
                .map(String::as_str)
                .unwrap_or("")
        ));
        output.push_str(&format!(
            "| Base Fee | {} | {} |\n",
            report.fee.base_fee_stroops,
            pct.get("base_fee").map(String::as_str).unwrap_or("")
        ));
        output.push_str(&format!(
            "| Rent Fee | {} | {} |\n",
            report.fee.refundable_stroops,
            pct.get("rent").map(String::as_str).unwrap_or("")
        ));
        output.push_str(&format!(
            "| **Total** | **{}** ({}) | **100.0%** |\n",
            report.fee.total_stroops, report.fee.total_xlm,
        ));

        if let Some(ref projections) = report.projections {
            output.push_str("\n### Cost Projections\n\n");
            output.push_str("| Invocations | Total Stroops | Total XLM | USD |\n");
            output.push_str("| ---: | ---: | ---: | ---: |\n");
            for p in projections {
                let usd_str = p
                    .usd
                    .map(|u| format!("${u:.2}"))
                    .unwrap_or_else(|| "-".to_string());
                output.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    crate::report::cost_report::format_thousands_u64(p.invocations),
                    crate::report::cost_report::format_thousands(p.total_stroops),
                    p.total_xlm,
                    usd_str
                ));
            }
        }

        // Advisory sections — omitted entirely under `--quiet`.
        if !options.quiet {
            let tips = crate::report::cost_report::generate_optimization_tips(report);
            if !tips.is_empty() {
                output.push_str("\n### Optimization Tips\n\n");
                for tip in &tips {
                    output.push_str(&format!("- {tip}\n"));
                }
            }

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
        }

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
            projections: None,
            contract_meta: crate::wasm::parser::ContractMeta::default(),
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
            projections: None,
            contract_meta: crate::wasm::parser::ContractMeta::default(),
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
    fn test_table_formatter_shows_grouped_ledger_sequence() {
        let formatter = TableFormatter;
        let output = formatter.format(&sample_report());
        assert!(
            output.contains("Simulated at ledger sequence: 3,894,195"),
            "got: {output}"
        );
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
        assert!(output.contains("CPU Instructions"));
        assert!(output.contains("Rent Fee"));
        assert!(output.contains("Total"));
        assert!(output.contains("15427 stroops (0.0015427)"));
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
        assert!(output.contains("Fee Distribution:"));
        // Only rows after the chart header belong to the chart — the fee
        // breakdown table above also names several components.
        let chart_start = output
            .find("Fee Distribution:")
            .expect("table output should contain the fee bar chart");
        let chart_lines: Vec<&str> = output[chart_start..]
            .lines()
            .filter(|line| line.contains(" | "))
            .collect();
        assert_eq!(
            chart_lines.len(),
            4,
            "chart should render one row per fee component"
        );
        for label in ["CPU", "Storage I/O", "Bandwidth", "Rent"] {
            assert!(
                output[chart_start..].contains(label),
                "chart should visualize {label}: {output}"
            );
        }
        assert!(
            output[chart_start..].contains('█'),
            "chart should use block characters: {output}"
        );
    }

    #[test]
    fn test_table_formatter_empty_report_no_chart() {
        let formatter = TableFormatter;
        let output = formatter.format(&empty_report());
        // Chart section should not be present when all fees are zero
        assert!(!output.contains("Fee Distribution:"));
    }

    #[test]
    fn test_table_formatter_can_disable_chart() {
        let formatter = TableFormatter;
        let output =
            formatter.format_with_options(&sample_report(), false, 80, ReportOptions::default());
        assert!(!output.contains("Fee Distribution:"));
        // Everything else in the report is still rendered.
        assert!(output.contains("Fee Breakdown:"));
        assert!(output.contains("Optimization Suggestions:"));
    }

    #[test]
    fn test_table_formatter_contract_meta() {
        let formatter = TableFormatter;
        let mut report = sample_report();
        report.contract_meta = crate::wasm::parser::ContractMeta {
            name: Some("MetaContract".to_string()),
            version: Some("9.9.9".to_string()),
            description: None,
            author: None,
            sdk_version: Some("25.3.2".to_string()),
            entries: vec![
                ("name".to_string(), "MetaContract".to_string()),
                ("version".to_string(), "9.9.9".to_string()),
                ("rssdkver".to_string(), "25.3.2".to_string()),
            ],
        };
        let output = formatter.format(&report);
        assert!(output.contains("Contract meta: present"));
        assert!(output.contains("name: MetaContract"));
        assert!(output.contains("version: 9.9.9"));
        assert!(output.contains("sdk_version: 25.3.2"));
        // Recognized keys are folded into typed fields, not repeated below.
        assert!(!output.contains("  rssdkver:"));

        // JSON payload carries the same metadata.
        let json = JsonFormatter.format(&report);
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(parsed["contract_meta"]["name"], "MetaContract");
        assert_eq!(parsed["contract_meta"]["sdk_version"], "25.3.2");
    }

    #[test]
    fn test_table_formatter_contract_meta_absent() {
        let formatter = TableFormatter;
        let output = formatter.format(&empty_report());
        assert!(output.contains("Contract meta: absent"));
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
        assert_eq!(parsed["ledger_sequence"], 3_894_195);
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
        assert_eq!(field_count, 17);
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
        assert!(output.contains("| CPU Instructions | 372 |"));
        assert!(output.contains("| Rent Fee | 10931 |"));
        assert!(output.contains("| **Total** | **15427** (0.0015427) |"));
    }

    #[test]
    fn test_markdown_formatter_network_info() {
        let formatter = MarkdownFormatter;
        let output = formatter.format(&sample_report());
        assert!(output.contains("**Network:** testnet (ledger 3894195)"));
        assert!(output.contains("- **Simulated at ledger sequence:** `3,894,195`"));
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

    // ── Optimization tips (#323) ──────────────────────────────────────

    /// A report whose fee is dominated by ledger writes must surface the
    /// contextual tip in the table output (issue #323).
    fn dominant_write_report() -> CostReport {
        let mut report = sample_report();
        report.write_entries = 3;
        report.fee.storage_fee_stroops = 11_110;
        report.fee.refundable_stroops = 0;
        report
    }

    #[test]
    fn test_table_formatter_shows_optimization_tips() {
        let output = TableFormatter.format(&dominant_write_report());
        assert!(output.contains("Optimization Tips:"));
        assert!(output.contains("writing 3 ledger entries"));
    }

    /// `--quiet` must drop the advisory sections but keep every measured value.
    #[test]
    fn test_table_formatter_quiet_omits_advisory_sections() {
        let report = dominant_write_report();
        let output = TableFormatter.format_with(&report, ReportOptions::quiet());

        assert!(!output.contains("Optimization Tips:"), "{output}");
        assert!(!output.contains("Optimization Suggestions:"), "{output}");
        assert!(!output.contains("writing 3 ledger entries"), "{output}");
        // The cost data is untouched.
        assert!(output.contains("15427 stroops (0.0015427)"));
        assert!(output.contains("Write Entries"));
    }

    #[test]
    fn test_markdown_formatter_shows_and_omits_tips() {
        let report = dominant_write_report();

        let output = MarkdownFormatter.format(&report);
        assert!(output.contains("### Optimization Tips"));
        assert!(output.contains("writing 3 ledger entries"));

        let quiet = MarkdownFormatter.format_with(&report, ReportOptions::quiet());
        assert!(!quiet.contains("### Optimization Tips"), "{quiet}");
        assert!(!quiet.contains("### Optimization Suggestions"), "{quiet}");
        assert!(quiet.contains("| **Total** | **15427** (0.0015427) |"));
    }

    /// `suggestions` carries the tip strings; the structured per-resource
    /// savings live under `optimization_suggestions` (issue #323).
    #[test]
    fn test_json_formatter_exposes_suggestions_key() {
        let output = JsonFormatter.format(&dominant_write_report());
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");

        let suggestions = parsed["suggestions"]
            .as_array()
            .expect("`suggestions` must be an array of tip strings");
        assert!(
            suggestions.iter().any(|s| s
                .as_str()
                .is_some_and(|s| s.contains("writing 3 ledger entries"))),
            "got: {suggestions:?}"
        );
        assert!(parsed["optimization_suggestions"].is_array());
    }

    /// `--quiet` omits both advisory keys from the JSON report.
    #[test]
    fn test_json_formatter_quiet_omits_suggestions() {
        let report = dominant_write_report();
        let output = JsonFormatter.format_with(&report, ReportOptions::quiet());
        let parsed: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");

        assert!(parsed.get("suggestions").is_none(), "{output}");
        assert!(parsed.get("optimization_suggestions").is_none(), "{output}");
        // The measured data is still there.
        assert_eq!(parsed["fee"]["total_stroops"], 15_427);
        assert_eq!(parsed["write_entries"], 3);
    }

    /// `format` must be exactly `format_with` under the default options, so the
    /// two entry points can never drift.
    #[test]
    fn test_format_matches_format_with_default_options() {
        let report = sample_report();
        let formatters: Vec<Box<dyn ReportFormatter>> = vec![
            Box::new(TableFormatter),
            Box::new(JsonFormatter),
            Box::new(CsvFormatter),
            Box::new(MarkdownFormatter),
        ];
        for formatter in &formatters {
            assert_eq!(
                formatter.format(&report),
                formatter.format_with(&report, ReportOptions::verbose_advisory()),
                "{} formatter disagreed with the default options",
                formatter.name()
            );
        }
    }

    #[test]
    fn test_report_options_defaults() {
        assert!(!ReportOptions::default().quiet);
        assert!(!ReportOptions::verbose_advisory().quiet);
        assert!(ReportOptions::quiet().quiet);
    }

    /// `wasm_size` is now a report field, so every format must carry it.
    #[test]
    fn test_all_formatters_report_wasm_size() {
        let report = sample_report();
        let table = TableFormatter.format(&report);
        assert!(table.contains("WASM size: 14432 bytes"), "{table}");

        let markdown = MarkdownFormatter.format(&report);
        assert!(
            markdown.contains("**WASM size:** 14432 bytes"),
            "{markdown}"
        );

        let csv = CsvFormatter.format(&report);
        let header = csv.lines().next().unwrap_or_default();
        assert!(header.contains("wasm_size"), "{header}");
        assert!(csv.contains("14432"), "{csv}");

        let json = JsonFormatter.format(&report);
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(parsed["wasm_size"], 14_432);
    }

    #[test]
    fn test_formatters_with_projections() {
        use crate::report::cost_report::CostProjection;
        let mut report = sample_report();
        report.projections = Some(vec![CostProjection {
            invocations: 100,
            total_stroops: 1_552_700,
            total_xlm: "0.1552700".to_string(),
            usd: Some(0.18),
        }]);

        let table_out = TableFormatter.format(&report);
        assert!(table_out.contains("Cost Projections:"));
        assert!(table_out.contains("100"));
        assert!(table_out.contains("1,552,700"));
        assert!(table_out.contains("$0.18"));

        let json_out = JsonFormatter.format(&report);
        assert!(json_out.contains("\"projections\""));
        assert!(json_out.contains("1552700"));
        assert!(json_out.contains("0.18"));

        let md_out = MarkdownFormatter.format(&report);
        assert!(md_out.contains("### Cost Projections"));
        assert!(md_out.contains("| 100 | 1,552,700 | 0.1552700 | $0.18 |"));
    }
}
