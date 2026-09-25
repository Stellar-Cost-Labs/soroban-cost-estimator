use comfy_table::Table;

use crate::report::fee_calc::{FeeBreakdown, FeeRates};

/// Compute what percentage `part` is of `total`.
///
/// Returns a formatted string like `"29.3%"`. Returns `"0.0%"` when the
/// total is zero to avoid division by zero.
pub fn fee_percentage(part: i64, total: i64) -> String {
    if total == 0 {
        "0.0%".to_string()
    } else {
        let pct = (part as f64 / total as f64) * 100.0;
        format!("{pct:.1}%")
    }
}

/// Maximum width of the bar in the ASCII cost breakdown chart (characters).
const CHART_BAR_WIDTH: usize = 40;

/// A single row in the ASCII cost breakdown chart.
#[derive(Debug, Clone)]
pub struct ChartEntry {
    /// Display label for the fee component.
    pub label: String,
    /// Fee amount in stroops.
    pub stroops: i64,
    /// The rendered ASCII bar (e.g. `"########################"`).
    pub bar: String,
    /// Percentage of total (e.g. `" (29.1%)"`), empty when total is 0.
    pub pct: String,
}

/// Render an ASCII bar chart showing the relative cost of each fee component.
///
/// The chart is appended to the cost report output to give a quick visual
/// summary of where the fee is going. Only non-zero components are shown.
///
/// # Output format
///
/// ```text
/// Fee Breakdown Chart:
///
///   Non-refundable | ########################              |  4496 (29.1%)
///   Refundable     | ###################################### | 10931 (70.9%)
/// ```
///
/// # Arguments
/// * `total_stroops` — total fee in stroops (used for percentage calculation;
///   if 0, percentages are omitted).
/// * `non_refundable` — non-refundable fee in stroops.
/// * `refundable` — refundable fee in stroops.
#[must_use]
pub fn format_cost_breakdown_chart(
    total_stroops: i64,
    non_refundable: i64,
    refundable: i64,
) -> String {
    let entries = build_chart_entries(total_stroops, non_refundable, refundable);
    render_chart(&entries)
}

/// Build the chart entries from fee values.
///
/// Returns a `Vec<ChartEntry>` sorted by descending stroops value. Zero-value
/// components are excluded.
#[must_use]
pub fn build_chart_entries(
    total_stroops: i64,
    non_refundable: i64,
    refundable: i64,
) -> Vec<ChartEntry> {
    let max_stroops = non_refundable.max(refundable);
    let has_total = total_stroops > 0;

    let mut entries: Vec<ChartEntry> = Vec::new();

    if non_refundable > 0 {
        let bar = render_bar(non_refundable, max_stroops);
        let pct = if has_total {
            format!(
                " ({:.1}%)",
                non_refundable as f64 / total_stroops as f64 * 100.0
            )
        } else {
            String::new()
        };
        entries.push(ChartEntry {
            label: "Non-refundable".to_string(),
            stroops: non_refundable,
            bar,
            pct,
        });
    }

    if refundable > 0 {
        let bar = render_bar(refundable, max_stroops);
        let pct = if has_total {
            format!(
                " ({:.1}%)",
                refundable as f64 / total_stroops as f64 * 100.0
            )
        } else {
            String::new()
        };
        entries.push(ChartEntry {
            label: "Refundable".to_string(),
            stroops: refundable,
            bar,
            pct,
        });
    }

    // Sort by descending stroops so the largest component is first.
    entries.sort_by_key(|a| std::cmp::Reverse(a.stroops));
    entries
}

/// Render the chart entries into a formatted string.
#[must_use]
fn render_chart(entries: &[ChartEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }

    // Find the longest label to align the bars.
    let label_width = entries.iter().map(|e| e.label.len()).max().unwrap_or(0);
    let mut output = String::from("\nFee Breakdown Chart:\n\n");

    for entry in entries {
        let padded_label = format!("{:<width$}", entry.label, width = label_width);
        let stroops_str = format_stroops_aligned(entry.stroops);
        output.push_str(&format!(
            "  {} | {} | {}{}\n",
            padded_label, entry.bar, stroops_str, entry.pct
        ));
    }

    output
}

/// Render a single ASCII bar proportional to `value` relative to `max`.
///
/// The bar uses `#` characters and is right-padded with spaces to
/// `CHART_BAR_WIDTH`. When `value` equals `max`, the bar is full width.
/// When `value` is 0, the bar is empty.
#[must_use]
fn render_bar(value: i64, max: i64) -> String {
    if max <= 0 {
        return " ".repeat(CHART_BAR_WIDTH);
    }
    let filled = ((value as f64 / max as f64) * CHART_BAR_WIDTH as f64).round() as usize;
    let filled = filled.min(CHART_BAR_WIDTH);
    format!(
        "{}{}",
        "#".repeat(filled),
        " ".repeat(CHART_BAR_WIDTH - filled)
    )
}

/// Format a stroops value with right-alignment for column display.
#[must_use]
fn format_stroops_aligned(stroops: i64) -> String {
    format!("{:>6}", stroops)
}

/// A complete cost report for a single contract invocation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CostReport {
    /// Name of the contract function that was simulated.
    pub function: String,
    /// WASM bytes SHA-256 hash (hex).
    pub wasm_hash: String,
    /// CPU instructions consumed.
    pub cpu_instructions: u64,
    /// Memory bytes used.
    pub memory_bytes: u64,
    /// Transaction size in bytes.
    pub tx_size: u32,
    /// Number of ledger read entries.
    pub read_entries: u32,
    /// Number of ledger write entries.
    pub write_entries: u32,
    /// Number of ledger read bytes.
    pub read_bytes: u32,
    /// Number of ledger write bytes.
    pub write_bytes: u32,
    /// Fee breakdown.
    pub fee: FeeBreakdown,
    /// The ledger sequence the simulation ran against.
    pub ledger: u32,
    /// Network the simulation ran on.
    pub network: String,
    /// RPC round-trip time of the `simulateTransaction` call, in
    /// milliseconds. Helps identify slow or overloaded RPC endpoints.
    pub rpc_latency_ms: u64,
    /// Fee rates used to compute the breakdown (carried so optimization
    /// suggestions can quantify per-resource savings). Excluded from
    /// serialized output; `None` when the rates were unavailable.
    #[serde(skip)]
    pub rates: Option<FeeRates>,
}

/// A concrete, actionable cost-optimization suggestion derived from a report.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct OptimizationSuggestion {
    /// Short headline (e.g. "Reduce ledger write entries").
    pub title: String,
    /// Human-readable explanation including the quantified saving.
    pub detail: String,
    /// Approximate stroops saved by applying this single suggestion. `0` when
    /// the saving cannot be expressed as a single per-unit amount.
    pub potential_savings_stroops: i64,
}

impl CostReport {
    /// Derive actionable cost-optimization suggestions from this report.
    ///
    /// Each suggestion quantifies how much a single reducible resource costs
    /// per unit, using the network fee rates captured at simulation time
    /// (`rates`). Returns an empty list when rates are unavailable or no
    /// reducible resource is present, so callers can render a "no suggestions"
    /// state. Suggestions are ordered by descending potential saving.
    ///
    /// Generic/contract-specific advice is intentionally out of scope; only
    /// per-resource unit savings backed by the report's own rate data are
    /// reported.
    #[must_use]
    pub fn suggest_optimizations(&self) -> Vec<OptimizationSuggestion> {
        let Some(rates) = self.rates else {
            return Vec::new();
        };

        let mut suggestions: Vec<OptimizationSuggestion> = Vec::new();

        if self.write_entries > 0 && rates.fee_per_write_entry > 0 {
            let saving = rates.fee_per_write_entry;
            suggestions.push(OptimizationSuggestion {
                title: "Reduce ledger write entries".to_string(),
                detail: format!(
                    "Removing one write entry saves ~{saving} stroops (current: {} write entries)",
                    self.write_entries
                ),
                potential_savings_stroops: saving,
            });
        }

        if self.read_entries > 0 && rates.fee_per_read_entry > 0 {
            let saving = rates.fee_per_read_entry;
            suggestions.push(OptimizationSuggestion {
                title: "Reduce ledger read entries".to_string(),
                detail: format!(
                    "Removing one read entry saves ~{saving} stroops (current: {} read entries)",
                    self.read_entries
                ),
                potential_savings_stroops: saving,
            });
        }

        if self.read_bytes > 0 && rates.fee_per_read_1kb > 0 {
            let saving = rates.fee_per_read_1kb;
            suggestions.push(OptimizationSuggestion {
                title: "Reduce disk read bytes".to_string(),
                detail: format!(
                    "Reducing disk reads by 1 KB saves ~{saving} stroops (current: {} read bytes)",
                    self.read_bytes
                ),
                potential_savings_stroops: saving,
            });
        }

        if self.cpu_instructions > 0 && rates.fee_per_10k_insns > 0 {
            let saving = rates.fee_per_10k_insns;
            suggestions.push(OptimizationSuggestion {
                title: "Optimize CPU hot path".to_string(),
                detail: format!(
                    "Cutting 10,000 CPU instructions saves ~{saving} stroops (current: {} instructions)",
                    self.cpu_instructions
                ),
                potential_savings_stroops: saving,
            });
        }

        suggestions.sort_by(|a, b| {
            b.potential_savings_stroops
                .cmp(&a.potential_savings_stroops)
        });
        suggestions
    }
}

/// Render optimization suggestions as a human-readable block.
///
/// Always emits a header; when there are no suggestions it explains why, so
/// the section is never silently empty in report output.
#[must_use]
pub fn format_suggestions(suggestions: &[OptimizationSuggestion]) -> String {
    let mut out = String::new();
    out.push_str("Optimization Suggestions:\n");
    if suggestions.is_empty() {
        out.push_str(
            "  No cost optimizations identified (fee rates unavailable or no reducible resources).\n",
        );
    } else {
        for s in suggestions {
            out.push_str(&format!(
                "  - {}: {} (potential saving: {} stroops)\n",
                s.title, s.detail, s.potential_savings_stroops
            ));
        }
    }
    out
}

/// Formats a cost report as a human-readable table.
pub fn format_report_table(report: &CostReport) -> String {
    let mut output = String::new();

    output.push_str(&format!("Function: {}\n", report.function));
    output.push_str(&format!(
        "Network: {} (ledger {})\n",
        report.network, report.ledger
    ));
    output.push_str(&format!("RPC round-trip: {} ms\n", report.rpc_latency_ms));
    output.push_str(&format!("WASM hash: {}\n\n", report.wasm_hash));

    let mut table = Table::new();

    table.set_header(vec!["Resource", "Consumed", "Fee (stroops)"]);

    table.add_row(vec![
        "CPU Instructions",
        &report.cpu_instructions.to_string(),
        "", // fee is itemized in the breakdown below
    ]);
    table.add_row(vec!["Memory Bytes", &report.memory_bytes.to_string(), ""]);
    table.add_row(vec!["Read Entries", &report.read_entries.to_string(), ""]);
    table.add_row(vec!["Write Entries", &report.write_entries.to_string(), ""]);
    table.add_row(vec!["Read Bytes", &report.read_bytes.to_string(), ""]);
    table.add_row(vec!["Write Bytes", &report.write_bytes.to_string(), ""]);
    table.add_row(vec!["Transaction Size", &report.tx_size.to_string(), ""]);

    output.push_str(&table.to_string());
    output.push('\n');

    output.push_str(&format!("\nFee Breakdown:\n"));
    let total = report.fee.total_stroops;
    output.push_str(&format!(
        "  Non-refundable: {} stroops ({})\n",
        report.fee.non_refundable_stroops,
        fee_percentage(report.fee.non_refundable_stroops, total),
    ));
    output.push_str(&format!(
        "  Refundable:     {} stroops ({})\n",
        report.fee.refundable_stroops,
        fee_percentage(report.fee.refundable_stroops, total),
    ));
    output.push_str(&format!("\n  Components (of non-refundable):\n"));
    output.push_str(&format!(
        "    CPU:        {} stroops ({})\n",
        report.fee.cpu_fee_stroops,
        fee_percentage(report.fee.cpu_fee_stroops, total),
    ));
    output.push_str(&format!(
        "    Storage:    {} stroops ({})\n",
        report.fee.storage_fee_stroops,
        fee_percentage(report.fee.storage_fee_stroops, total),
    ));
    output.push_str(&format!(
        "    Bandwidth:  {} stroops ({})\n",
        report.fee.bandwidth_fee_stroops,
        fee_percentage(report.fee.bandwidth_fee_stroops, total),
    ));
    output.push_str(&format!(
        "\n  Total:          {} stroops ({})\n",
        report.fee.total_stroops, report.fee.total_xlm,
    ));

    // ASCII bar chart for visual cost breakdown
    output.push_str(&format_cost_breakdown_chart(
        report.fee.total_stroops,
        report.fee.non_refundable_stroops,
        report.fee.refundable_stroops,
    ));

    output
}

/// Formats a cost report as a JSON string.
pub fn format_report_json(report: &CostReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string())
}

/// Signed change of a single metric between two estimate runs.
///
/// Fee arithmetic stays in integer stroops (`previous`, `current`,
/// `absolute`); `percent` is derived for display only and is `None` when the
/// baseline is zero, because a percentage change from zero is undefined.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct MetricDelta {
    /// Baseline value from the previous estimate.
    pub previous: i64,
    /// Value from the current estimate.
    pub current: i64,
    /// `current - previous`, saturating rather than wrapping.
    pub absolute: i64,
    /// Percentage change, or `None` when `previous == 0`.
    pub percent: Option<f64>,
}

impl MetricDelta {
    /// Compute the delta between two values.
    #[must_use]
    pub fn new(previous: i64, current: i64) -> Self {
        let absolute = current.saturating_sub(previous);
        let percent = if previous == 0 {
            None
        } else {
            Some(absolute as f64 / previous as f64 * 100.0)
        };
        Self {
            previous,
            current,
            absolute,
            percent,
        }
    }

    /// Renders the signed change, e.g. `+12400 (+5.2%)`, `-100 (-1.0%)`,
    /// `0 (0.0%)`, or just `+5` when the percentage is undefined.
    #[must_use]
    pub fn format_signed(&self) -> String {
        let sign = if self.absolute > 0 { "+" } else { "" };
        match self.percent {
            Some(pct) => format!("{sign}{} ({sign}{pct:.1}%)", self.absolute),
            None => format!("{sign}{}", self.absolute),
        }
    }
}

/// Cost deltas between a previous estimate and the current report.
///
/// CPU, memory, and fee are always present because every cached entry stores
/// them. The ledger entry-count deltas are `None` when the previous estimate
/// predates I/O recording.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct CostDelta {
    /// CPU instruction delta.
    pub cpu_instructions: MetricDelta,
    /// Memory byte delta.
    pub memory_bytes: MetricDelta,
    /// Total fee delta, in stroops.
    pub fee_stroops: MetricDelta,
    /// Ledger read-entry delta, when the previous estimate recorded I/O.
    pub read_entries: Option<MetricDelta>,
    /// Ledger write-entry delta, when the previous estimate recorded I/O.
    pub write_entries: Option<MetricDelta>,
}

impl CostDelta {
    /// Computes the delta of `current` against a previous estimate's metrics.
    ///
    /// `previous_io` is `(read_entries, write_entries)` when the previous
    /// estimate recorded its ledger I/O footprint.
    #[must_use]
    pub fn compute(
        previous_cpu: u64,
        previous_memory: u64,
        previous_fee_stroops: i64,
        previous_io: Option<(u32, u32)>,
        current: &CostReport,
    ) -> Self {
        Self {
            cpu_instructions: MetricDelta::new(
                previous_cpu as i64,
                current.cpu_instructions as i64,
            ),
            memory_bytes: MetricDelta::new(previous_memory as i64, current.memory_bytes as i64),
            fee_stroops: MetricDelta::new(previous_fee_stroops, current.fee.total_stroops),
            read_entries: previous_io.map(|(read, _)| {
                MetricDelta::new(i64::from(read), i64::from(current.read_entries))
            }),
            write_entries: previous_io.map(|(_, write)| {
                MetricDelta::new(i64::from(write), i64::from(current.write_entries))
            }),
        }
    }

    /// `(label, delta)` rows for every metric with a baseline to compare to.
    fn rows(&self) -> Vec<(&'static str, MetricDelta)> {
        let mut rows = vec![
            ("CPU Instructions", self.cpu_instructions),
            ("Memory Bytes", self.memory_bytes),
        ];
        if let Some(delta) = self.read_entries {
            rows.push(("Read Entries", delta));
        }
        if let Some(delta) = self.write_entries {
            rows.push(("Write Entries", delta));
        }
        rows.push(("Fee (stroops)", self.fee_stroops));
        rows
    }

    /// Renders the comparison as a terminal table.
    #[must_use]
    pub fn format_text(&self) -> String {
        let mut output = String::from("\nCost delta vs previous estimate:\n");
        let mut table = Table::new();
        table.set_header(vec!["Metric", "Previous", "Current", "Change"]);
        for (label, delta) in self.rows() {
            table.add_row(vec![
                label.to_string(),
                delta.previous.to_string(),
                delta.current.to_string(),
                delta.format_signed(),
            ]);
        }
        output.push_str(&table.to_string());
        output.push('\n');
        output
    }

    /// Renders the comparison as a GitHub-flavored markdown table.
    #[must_use]
    pub fn format_markdown(&self) -> String {
        let mut output = String::from("\n### Cost delta vs previous estimate\n\n");
        output.push_str("| Metric | Previous | Current | Change |\n");
        output.push_str("| --- | --- | --- | --- |\n");
        for (label, delta) in self.rows() {
            output.push_str(&format!(
                "| {label} | {} | {} | {} |\n",
                delta.previous,
                delta.current,
                delta.format_signed(),
            ));
        }
        output
    }

    /// Renders the comparison as RFC 4180 CSV records.
    ///
    /// The percent column is empty when there is no baseline to divide by.
    #[must_use]
    pub fn format_csv(&self) -> String {
        let mut output = String::from("metric,previous,current,absolute,percent\n");
        for (label, delta) in self.rows() {
            let percent = delta.percent.map(|p| format!("{p:.1}")).unwrap_or_default();
            output.push_str(&format!(
                "{label},{},{},{},{percent}\n",
                delta.previous, delta.current, delta.absolute
            ));
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fee_percentage_normal() {
        assert_eq!(fee_percentage(50, 100), "50.0%");
        assert_eq!(fee_percentage(1, 3), "33.3%");
        assert_eq!(fee_percentage(0, 100), "0.0%");
    }

    #[test]
    fn test_fee_percentage_zero_total() {
        assert_eq!(fee_percentage(0, 0), "0.0%");
        assert_eq!(fee_percentage(100, 0), "0.0%");
    }

    #[test]
    fn test_fee_percentage_rounding() {
        assert_eq!(fee_percentage(1, 10), "10.0%");
        assert_eq!(fee_percentage(1, 3), "33.3%");
        assert_eq!(fee_percentage(2, 3), "66.7%");
    }

    fn report_with_rates(rates: FeeRates) -> CostReport {
        CostReport {
            function: "increment".to_string(),
            wasm_hash: "abc".to_string(),
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
            rates: Some(rates),
        }
    }

    fn sample_rates() -> FeeRates {
        FeeRates {
            fee_per_10k_insns: 7,
            fee_per_read_entry: 1_563,
            fee_per_write_entry: 2_500,
            fee_per_read_1kb: 447,
            fee_per_1kb: 406,
        }
    }

    #[test]
    fn test_suggest_optimizations_with_rates() {
        let report = report_with_rates(sample_rates());
        let suggestions = report.suggest_optimizations();

        // write entries (2_500) + read entries (1_563) + cpu 10k (7) expected.
        assert_eq!(suggestions.len(), 3);
        // Ordered by descending potential saving: write entry first.
        assert_eq!(suggestions[0].title, "Reduce ledger write entries");
        assert_eq!(suggestions[0].potential_savings_stroops, 2_500);
        assert_eq!(suggestions[1].title, "Reduce ledger read entries");
        assert_eq!(suggestions[1].potential_savings_stroops, 1_563);
        assert_eq!(suggestions[2].title, "Optimize CPU hot path");
        assert_eq!(suggestions[2].potential_savings_stroops, 7);
    }

    #[test]
    fn test_suggest_optimizations_without_rates_is_empty() {
        let mut report = report_with_rates(sample_rates());
        report.rates = None;
        assert!(report.suggest_optimizations().is_empty());
    }

    #[test]
    fn test_suggest_optimizations_read_bytes_rates() {
        let report = report_with_rates(FeeRates {
            fee_per_10k_insns: 0,
            fee_per_read_entry: 0,
            fee_per_write_entry: 0,
            fee_per_read_1kb: 447,
            fee_per_1kb: 0,
        });
        // No reducible resource with a positive rate, so no suggestions.
        assert!(report.suggest_optimizations().is_empty());
    }

    #[test]
    fn test_format_suggestions_empty() {
        let out = format_suggestions(&[]);
        assert!(out.contains("Optimization Suggestions:"));
        assert!(out.contains("No cost optimizations identified"));
    }

    #[test]
    fn test_format_suggestions_nonempty() {
        let out = format_suggestions(&[OptimizationSuggestion {
            title: "Reduce ledger write entries".to_string(),
            detail: "Removing one write entry saves ~2500 stroops".to_string(),
            potential_savings_stroops: 2_500,
        }]);
        assert!(out.contains("- Reduce ledger write entries:"));
        assert!(out.contains("2500 stroops"));
    }

    #[test]
    fn test_format_report_table_and_json_populated_footprint() {
        let report = report_with_rates(sample_rates());

        // Table verification
        let table_out = format_report_table(&report);
        assert!(table_out.contains("Read Entries"));
        assert!(table_out.contains("Write Entries"));
        assert!(table_out.contains("Read Bytes"));
        assert!(table_out.contains("Write Bytes"));
        assert!(table_out.contains("136")); // write_bytes

        // JSON verification
        let json_out = format_report_json(&report);
        let parsed: serde_json::Value = serde_json::from_str(&json_out).expect("valid json");
        assert_eq!(parsed["read_entries"], 1);
        assert_eq!(parsed["write_entries"], 1);
        assert_eq!(parsed["read_bytes"], 0);
        assert_eq!(parsed["write_bytes"], 136);
    }

    #[test]
    fn test_format_report_table_and_json_zero_footprint() {
        let report = CostReport {
            function: "(wasm upload)".to_string(),
            wasm_hash: "0000".to_string(),
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
            network: "testnet".to_string(),
            rpc_latency_ms: 0,
            rates: None,
        };

        let table_out = format_report_table(&report);
        assert!(table_out.contains("Read Entries"));
        assert!(table_out.contains("Write Entries"));

        let json_out = format_report_json(&report);
        let parsed: serde_json::Value = serde_json::from_str(&json_out).expect("valid json");
        assert_eq!(parsed["read_entries"], 0);
        assert_eq!(parsed["write_entries"], 0);
        assert_eq!(parsed["read_bytes"], 0);
        assert_eq!(parsed["write_bytes"], 0);
    }

    // ── Cost delta math (#278) ──────────────────────────────────────────

    #[test]
    fn test_metric_delta_positive() {
        let delta = MetricDelta::new(100, 200);
        assert_eq!(delta.absolute, 100);
        assert_eq!(delta.percent, Some(100.0));
        assert_eq!(delta.format_signed(), "+100 (+100.0%)");
    }

    #[test]
    fn test_metric_delta_negative() {
        let delta = MetricDelta::new(200, 100);
        assert_eq!(delta.absolute, -100);
        assert_eq!(delta.percent, Some(-50.0));
        assert_eq!(delta.format_signed(), "-100 (-50.0%)");
    }

    #[test]
    fn test_metric_delta_zero() {
        let delta = MetricDelta::new(200, 200);
        assert_eq!(delta.absolute, 0);
        assert_eq!(delta.percent, Some(0.0));
        assert_eq!(delta.format_signed(), "0 (0.0%)");
    }

    #[test]
    fn test_metric_delta_zero_baseline_has_no_percentage() {
        let delta = MetricDelta::new(0, 500);
        assert_eq!(delta.absolute, 500);
        assert_eq!(delta.percent, None);
        assert_eq!(delta.format_signed(), "+500");
    }

    #[test]
    fn test_metric_delta_saturates_instead_of_wrapping() {
        assert_eq!(MetricDelta::new(i64::MIN, i64::MAX).absolute, i64::MAX);
    }

    #[test]
    fn test_cost_delta_without_previous_io_omits_entry_deltas() {
        let report = report_with_rates(sample_rates());
        let delta = CostDelta::compute(500_000, 10, 10_000, None, &report);

        assert_eq!(delta.cpu_instructions.absolute, 32_502);
        assert_eq!(delta.memory_bytes.absolute, -10);
        assert_eq!(delta.fee_stroops.absolute, 5_427);
        assert!(delta.read_entries.is_none());
        assert!(delta.write_entries.is_none());
        assert!(!delta.format_markdown().contains("Read Entries"));
    }

    #[test]
    fn test_cost_delta_with_previous_io_includes_entry_deltas() {
        let report = report_with_rates(sample_rates());
        let delta = CostDelta::compute(532_502, 0, 15_427, Some((3, 0)), &report);

        assert_eq!(delta.read_entries.map(|d| d.absolute), Some(-2));
        assert_eq!(delta.write_entries.map(|d| d.absolute), Some(1));
        assert!(delta.format_text().contains("Read Entries"));
        assert!(delta.format_markdown().contains("Write Entries"));
    }

    #[test]
    fn test_cost_delta_is_json_serializable() {
        let report = report_with_rates(sample_rates());
        let delta = CostDelta::compute(1, 2, 3, Some((1, 1)), &report);
        let value = serde_json::to_value(delta).expect("serialize");

        assert_eq!(value["cpu_instructions"]["previous"], 1);
        assert_eq!(value["cpu_instructions"]["current"], 532_502);
        assert_eq!(value["memory_bytes"]["absolute"], -2);
        assert_eq!(value["fee_stroops"]["absolute"], 15_424);
    }

    #[test]
    fn test_cost_delta_csv_has_header_and_one_row_per_metric() {
        let report = report_with_rates(sample_rates());
        let delta = CostDelta::compute(532_502, 0, 15_427, Some((1, 1)), &report);
        let csv = delta.format_csv();
        let lines: Vec<&str> = csv.lines().collect();

        assert_eq!(lines[0], "metric,previous,current,absolute,percent");
        // cpu, memory, read entries, write entries, fee
        assert_eq!(lines.len(), 6);
        assert!(lines[1].starts_with("CPU Instructions,532502,532502,0,0.0"));
    }

    #[test]
    fn test_cost_delta_csv_leaves_percent_empty_without_baseline() {
        let report = report_with_rates(sample_rates());
        let delta = CostDelta::compute(0, 0, 0, None, &report);
        let csv = delta.format_csv();
        let cpu_row = csv.lines().nth(1).expect("cpu row");

        assert!(
            cpu_row.ends_with(','),
            "percent column should be empty; got: {cpu_row}"
        );
    }
}
