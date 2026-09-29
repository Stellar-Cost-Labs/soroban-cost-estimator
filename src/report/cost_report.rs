use comfy_table::Table;

use crate::report::fee_calc::{FeeBreakdown, FeeRates};

// fee_percentage removed since we now use the precalculated exact percentages

/// Minimum terminal width (in columns) required before the fee bar chart is
/// rendered. Narrower terminals receive the table only.
pub const MIN_CHART_WIDTH: usize = 80;

/// Terminal width assumed when the real width cannot be detected.
pub const DEFAULT_CHART_WIDTH: usize = 80;

/// Narrowest a chart bar may become after width scaling, so tiny shares stay
/// visible instead of collapsing to an empty column.
const MIN_BAR_WIDTH: usize = 10;

/// Widest a chart bar may become after width scaling, keeping labels and bars
/// readable on very wide terminals.
const MAX_BAR_WIDTH: usize = 60;

/// Block glyph used for a fully filled bar cell.
const BLOCK_FULL: char = '█';

/// One labelled fee component rendered as a row in the bar chart.
struct BarRow {
    /// Display label (e.g. `"CPU"`).
    label: &'static str,
    /// Fee amount in stroops.
    stroops: i64,
}

/// Render an ASCII/Unicode horizontal bar chart of the fee distribution.
///
/// Each bar's length is proportional to that component's share of the total
/// fee. The four displayed components are CPU instructions, read/write
/// storage I/O, bandwidth (transaction size) and rent (the refundable
/// portion); the fixed base-inclusion fee is intentionally left out of the
/// distribution.
///
/// Sub-cell precision is expressed with the block glyphs `█` (full), `▓`
/// (three-quarters), `▒` (half) and `░` (quarter); remaining cells are spaces.
/// `width` is the number of terminal columns available, and the bar length is
/// scaled so each rendered line fits within it.
///
/// Returns an empty string when `breakdown.total_stroops` is zero, since there
/// is nothing to visualize.
///
/// # Output format
///
/// ```text
/// Fee Distribution:
///
///   CPU         | ██████████████████████████████████████ |  70.9%
///   Storage I/O | ███████████████▒                       |  26.3%
/// ```
///
/// # Network calls
/// None — pure computation.
#[must_use]
pub fn render_fee_bar_chart(breakdown: &FeeBreakdown, width: usize) -> String {
    let total = breakdown.total_stroops;
    if total <= 0 {
        return String::new();
    }

    let rows = [
        BarRow {
            label: "CPU",
            stroops: breakdown.cpu_fee_stroops,
        },
        BarRow {
            label: "Storage I/O",
            stroops: breakdown.storage_fee_stroops,
        },
        BarRow {
            label: "Bandwidth",
            stroops: breakdown.bandwidth_fee_stroops,
        },
        BarRow {
            label: "Rent",
            stroops: breakdown.refundable_stroops,
        },
    ];

    let label_width = rows.iter().map(|row| row.label.len()).max().unwrap_or(0);
    // 2 leading spaces + label + " | " + bar + " | " + a 6-column percentage.
    let overhead = 2 + label_width + 3 + 3 + 6;
    let bar_width = width
        .saturating_sub(overhead)
        .clamp(MIN_BAR_WIDTH, MAX_BAR_WIDTH);

    let mut output = String::from("\nFee Distribution:\n\n");
    for row in &rows {
        let ratio = row.stroops.max(0) as f64 / total as f64;
        let bar = render_bar(ratio, bar_width);
        let pct = ratio * 100.0;
        output.push_str(&format!(
            "  {label:<label_width$} | {bar} | {pct:>5.1}%\n",
            label = row.label,
        ));
    }
    output
}

/// Render a single bar of `width` cells for `ratio` in `0.0..=1.0`.
///
/// The fractional trailing cell is represented with a partial-block glyph
/// (`▓`, `▒`, `░`); a fraction within an eighth of a full cell rounds up to a
/// full block instead. Unfilled cells are spaces, so every bar is exactly
/// `width` cells wide.
#[must_use]
fn render_bar(ratio: f64, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let ratio = ratio.clamp(0.0, 1.0);
    let exact = ratio * width as f64;
    let whole = exact.floor() as usize;
    let remainder = exact - whole as f64;

    let (full_cells, partial) = if remainder >= 0.875 {
        ((whole + 1).min(width), None)
    } else if remainder >= 0.625 {
        (whole, Some('▓'))
    } else if remainder >= 0.375 {
        (whole, Some('▒'))
    } else if remainder >= 0.125 {
        (whole, Some('░'))
    } else {
        (whole, None)
    };
    let full_cells = full_cells.min(width);

    let mut bar = String::with_capacity(width);
    for _ in 0..full_cells {
        bar.push(BLOCK_FULL);
    }
    if let Some(glyph) = partial {
        if full_cells < width {
            bar.push(glyph);
        }
    }
    while bar.chars().count() < width {
        bar.push(' ');
    }
    bar
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
    if crate::cli::should_colorize() {
        table.enforce_styling();
    } else {
        table.force_no_tty();
    }

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

    output.push_str("\nFee Breakdown:\n\n");
    let pct = &report.fee.fee_percentages;
    let mut fee_table = Table::new();
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

    // ASCII bar chart for visual cost breakdown
    output.push_str(&render_fee_bar_chart(&report.fee, DEFAULT_CHART_WIDTH));

    output
}

/// Formats a cost report as a JSON string.
pub fn format_report_json(report: &CostReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
                base_fee_stroops: 100,
                total_stroops: 15_527,
                total_xlm: "0.0015527".to_string(),
                fee_percentages: std::collections::BTreeMap::new(),
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
                base_fee_stroops: 0,
                total_stroops: 0,
                total_xlm: "0.0000000".to_string(),
                fee_percentages: std::collections::BTreeMap::new(),
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

    /// A breakdown with a clear 65/20/5/10 split so chart output is easy to
    /// reason about in assertions.
    fn chart_breakdown() -> FeeBreakdown {
        FeeBreakdown {
            non_refundable_stroops: 9_000,
            refundable_stroops: 1_000,
            cpu_fee_stroops: 6_500,
            storage_fee_stroops: 2_000,
            bandwidth_fee_stroops: 500,
            base_fee_stroops: 100,
            total_stroops: 10_000,
            total_xlm: "0.0010000".to_string(),
            fee_percentages: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn test_render_fee_bar_chart_zero_total_is_empty() {
        let mut breakdown = chart_breakdown();
        breakdown.total_stroops = 0;
        assert_eq!(render_fee_bar_chart(&breakdown, DEFAULT_CHART_WIDTH), "");
    }

    #[test]
    fn test_render_fee_bar_chart_lists_all_components() {
        let chart = render_fee_bar_chart(&chart_breakdown(), DEFAULT_CHART_WIDTH);
        assert!(chart.starts_with("\nFee Distribution:\n\n"));
        for label in ["CPU", "Storage I/O", "Bandwidth", "Rent"] {
            assert!(chart.contains(label), "missing {label} in:\n{chart}");
        }
        assert!(chart.contains("65.0%"), "missing 65.0% in:\n{chart}");
        assert!(chart.contains("20.0%"), "missing 20.0% in:\n{chart}");
        assert!(chart.contains("5.0%"), "missing 5.0% in:\n{chart}");
        assert!(chart.contains("10.0%"), "missing 10.0% in:\n{chart}");
        assert!(chart.contains('█'), "filled block missing in:\n{chart}");
        assert!(
            chart.contains('▓') || chart.contains('▒') || chart.contains('░'),
            "partial block missing in:\n{chart}"
        );
    }

    #[test]
    fn test_render_fee_bar_chart_scales_to_width() {
        let breakdown = chart_breakdown();
        for width in [40usize, 80, 120] {
            let chart = render_fee_bar_chart(&breakdown, width);
            for line in chart.lines() {
                assert!(
                    line.chars().count() <= width,
                    "line exceeds {width} columns: {line:?}"
                );
            }
        }

        // Wider terminals get longer bars (up to the configured maximum).
        let cpu_bar_width = |width: usize| {
            render_fee_bar_chart(&breakdown, width)
                .lines()
                .find(|line| line.contains("CPU"))
                .and_then(|line| line.split(" | ").nth(1))
                .map(|bar| bar.chars().count())
                .unwrap_or(0)
        };
        assert!(
            cpu_bar_width(120) > cpu_bar_width(40),
            "bar should scale with terminal width"
        );
    }
}
