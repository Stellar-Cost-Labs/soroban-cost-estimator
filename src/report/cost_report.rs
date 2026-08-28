use comfy_table::Table;

use crate::report::fee_calc::FeeBreakdown;

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
            format!(" ({:.1}%)", non_refundable as f64 / total_stroops as f64 * 100.0)
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
            format!(" ({:.1}%)", refundable as f64 / total_stroops as f64 * 100.0)
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
    entries.sort_by(|a, b| b.stroops.cmp(&a.stroops));
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
    format!("{}{}", "#".repeat(filled), " ".repeat(CHART_BAR_WIDTH - filled))
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
}

/// Formats a cost report as a human-readable table.
pub fn format_report_table(report: &CostReport) -> String {
    let mut output = String::new();

    output.push_str(&format!("Function: {}\n", report.function));
    output.push_str(&format!(
        "Network: {} (ledger {})\n",
        report.network, report.ledger
    ));
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
    output.push_str(&format!(
        "  Non-refundable: {} stroops\n",
        report.fee.non_refundable_stroops
    ));
    output.push_str(&format!(
        "  Refundable:     {} stroops\n",
        report.fee.refundable_stroops
    ));
    output.push_str(&format!(
        "  Total:          {} stroops ({})\n",
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_fee() -> FeeBreakdown {
        FeeBreakdown {
            non_refundable_stroops: 4_496,
            refundable_stroops: 10_931,
            total_stroops: 15_427,
            total_xlm: "0.0015427".to_string(),
        }
    }

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
            fee: sample_fee(),
            ledger: 3_894_195,
            network: "testnet".to_string(),
        }
    }

    // ── format_cost_breakdown_chart ──────────────────────────────────

    #[test]
    fn test_chart_contains_header() {
        let output = format_cost_breakdown_chart(15_427, 4_496, 10_931);
        assert!(output.contains("Fee Breakdown Chart:"));
    }

    #[test]
    fn test_chart_contains_both_components() {
        let output = format_cost_breakdown_chart(15_427, 4_496, 10_931);
        assert!(output.contains("Non-refundable"));
        assert!(output.contains("Refundable"));
    }

    #[test]
    fn test_chart_contains_stroops_values() {
        let output = format_cost_breakdown_chart(15_427, 4_496, 10_931);
        assert!(output.contains("4496"));
        assert!(output.contains("10931"));
    }

    #[test]
    fn test_chart_contains_percentages() {
        let output = format_cost_breakdown_chart(15_427, 4_496, 10_931);
        // 4496/15427 = 29.1%, 10931/15427 = 70.9%
        assert!(output.contains("29.1%"));
        assert!(output.contains("70.9%"));
    }

    #[test]
    fn test_chart_contains_bars() {
        let output = format_cost_breakdown_chart(15_427, 4_496, 10_931);
        // Both bars should contain # characters
        let lines: Vec<&str> = output.lines().collect();
        let bar_lines: Vec<&str> = lines
            .iter()
            .filter(|l| l.contains("Non-refundable") || l.contains("Refundable"))
            .copied()
            .collect();
        assert_eq!(bar_lines.len(), 2);
        for line in &bar_lines {
            assert!(line.contains('#'), "bar line should contain '#': {line}");
        }
    }

    #[test]
    fn test_chart_larger_component_has_longer_bar() {
        let output = format_cost_breakdown_chart(15_427, 4_496, 10_931);
        let lines: Vec<&str> = output.lines().collect();
        let non_ref_line = lines
            .iter()
            .find(|l| l.contains("Non-refundable"))
            .unwrap();
        let ref_line = lines
            .iter()
            .find(|l| l.contains("Refundable"))
            .unwrap();

        let non_ref_hash_count = non_ref_line.matches('#').count();
        let ref_hash_count = ref_line.matches('#').count();

        // Refundable (10931) > Non-refundable (4496), so refundable bar should be longer
        assert!(
            ref_hash_count > non_ref_hash_count,
            "refundable bar ({ref_hash_count} #) should be longer than non-refundable ({non_ref_hash_count} #)"
        );
    }

    #[test]
    fn test_chart_full_width_for_equal_values() {
        // When both values are equal, both bars should be full width
        let output = format_cost_breakdown_chart(200, 100, 100);
        let lines: Vec<&str> = output.lines().collect();
        let bar_lines: Vec<&str> = lines
            .iter()
            .filter(|l| l.contains("Non-refundable") || l.contains("Refundable"))
            .copied()
            .collect();
        for line in &bar_lines {
            let hash_count = line.matches('#').count();
            assert_eq!(
                hash_count, CHART_BAR_WIDTH,
                "equal-value bars should both be full width"
            );
        }
    }

    #[test]
    fn test_chart_empty_when_all_zero() {
        let output = format_cost_breakdown_chart(0, 0, 0);
        assert!(output.is_empty());
    }

    #[test]
    fn test_chart_only_non_refundable() {
        let output = format_cost_breakdown_chart(4_496, 4_496, 0);
        assert!(output.contains("Non-refundable"));
        assert!(!output.contains("Refundable"));
    }

    #[test]
    fn test_chart_only_refundable() {
        let output = format_cost_breakdown_chart(10_931, 0, 10_931);
        assert!(!output.contains("Non-refundable"));
        assert!(output.contains("Refundable"));
    }

    #[test]
    fn test_chart_no_percentages_when_total_zero() {
        // total=0 but components non-zero (defensive case)
        let output = format_cost_breakdown_chart(0, 100, 200);
        assert!(output.contains("Non-refundable"));
        assert!(output.contains("Refundable"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn test_chart_bar_width_constant() {
        let output = format_cost_breakdown_chart(1000, 500, 500);
        let lines: Vec<&str> = output.lines().collect();
        let bar_lines: Vec<&str> = lines
            .iter()
            .filter(|l| l.contains("Non-refundable") || l.contains("Refundable"))
            .copied()
            .collect();
        for line in &bar_lines {
            // Each bar line should have exactly CHART_BAR_WIDTH '#' characters
            let hash_count = line.matches('#').count();
            assert_eq!(
                hash_count, CHART_BAR_WIDTH,
                "bar should be exactly {CHART_BAR_WIDTH} characters wide"
            );
        }
    }

    // ── build_chart_entries ───────────────────────────────────────────

    #[test]
    fn test_build_chart_entries_sorted_descending() {
        let entries = build_chart_entries(15_427, 4_496, 10_931);
        assert_eq!(entries.len(), 2);
        // Larger value first
        assert_eq!(entries[0].label, "Refundable");
        assert_eq!(entries[0].stroops, 10_931);
        assert_eq!(entries[1].label, "Non-refundable");
        assert_eq!(entries[1].stroops, 4_496);
    }

    #[test]
    fn test_build_chart_entries_excludes_zero() {
        let entries = build_chart_entries(100, 100, 0);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].label, "Non-refundable");
    }

    #[test]
    fn test_build_chart_entries_empty_when_all_zero() {
        let entries = build_chart_entries(0, 0, 0);
        assert!(entries.is_empty());
    }

    // ── render_bar ───────────────────────────────────────────────────

    #[test]
    fn test_render_bar_full_width() {
        let bar = render_bar(100, 100);
        assert_eq!(bar.len(), CHART_BAR_WIDTH);
        assert!(bar.chars().all(|c| c == '#'));
    }

    #[test]
    fn test_render_bar_empty() {
        let bar = render_bar(0, 100);
        assert_eq!(bar.len(), CHART_BAR_WIDTH);
        assert!(!bar.contains('#'));
    }

    #[test]
    fn test_render_bar_half_width() {
        let bar = render_bar(50, 100);
        let hash_count = bar.matches('#').count();
        assert_eq!(hash_count, CHART_BAR_WIDTH / 2);
    }

    #[test]
    fn test_render_bar_max_zero() {
        let bar = render_bar(0, 0);
        assert_eq!(bar.len(), CHART_BAR_WIDTH);
        assert!(!bar.contains('#'));
    }

    // ── format_stroops_aligned ────────────────────────────────────────

    #[test]
    fn test_format_stroops_aligned() {
        assert_eq!(format_stroops_aligned(0), "     0");
        assert_eq!(format_stroops_aligned(4_496), "  4496");
        assert_eq!(format_stroops_aligned(10_931), " 10931");
    }

    // ── Integration: format_report_table includes chart ───────────────

    #[test]
    fn test_format_report_table_includes_chart() {
        let output = format_report_table(&sample_report());
        assert!(output.contains("Fee Breakdown Chart:"));
        assert!(output.contains("Non-refundable"));
        assert!(output.contains("Refundable"));
    }

    #[test]
    fn test_format_report_table_chart_after_fee_breakdown() {
        let output = format_report_table(&sample_report());
        let breakdown_pos = output.find("Fee Breakdown:").unwrap();
        let chart_pos = output.find("Fee Breakdown Chart:").unwrap();
        assert!(
            chart_pos > breakdown_pos,
            "chart should appear after fee breakdown"
        );
    }

    #[test]
    fn test_format_report_table_empty_report_no_chart() {
        let report = CostReport {
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
                total_stroops: 0,
                total_xlm: "0.0000000".to_string(),
            },
            ledger: 0,
            network: "mainnet".to_string(),
        };
        let output = format_report_table(&report);
        // Chart section should not be present when all fees are zero
        assert!(!output.contains("Fee Breakdown Chart:"));
    }
}
