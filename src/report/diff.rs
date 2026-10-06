//! Side-by-side cost report diff formatting.
//!
//! Renders two [`CostReport`]s (e.g. the same function simulated from two
//! WASM builds) as a single comparative table:
//!
//! ```text
//! +------------------+----------+----------+---------------+
//! | Resource         | Old      | New      | Change        |
//! +==================+==========+==========+===============+
//! | WASM Size        | 4096     | 5120     | +1024 (+25.0%)|
//! | CPU Instructions | 532502   | 480000   | -52502 (-9.9%)|
//! ...
//! ```
//!
//! Increases are rendered in red and decreases in green (a cost increase is
//! bad, a decrease is good), using comfy-table's native cell styling so
//! column widths stay correct even with ANSI escape sequences present.
//!
//! Machine-readable output is available through [`CostReportDiff`], which
//! serializes to a stable JSON structure (`old`, `new`, and a `rows` array
//! with per-resource `delta` / `change_percent` values).

use std::io::IsTerminal;

use comfy_table::Cell;
use comfy_table::Color;
use comfy_table::ContentArrangement;
use comfy_table::Table;
use serde::Deserialize;
use serde::Serialize;

use crate::report::cost_report::CostReport;

/// Direction of a metric change, used for colorization and JSON consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeDirection {
    /// The new build consumes more than the old one.
    Increase,
    /// The new build consumes less than the old one.
    Decrease,
    /// The metric is identical in both builds.
    Unchanged,
}

/// The two reports being compared, reduced to their identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportIdentity {
    /// Contract function the reports describe.
    pub function: String,
    /// Network both simulations ran against.
    pub network: String,
    /// SHA-256 hash of the baseline ("old") WASM.
    pub old_wasm_hash: String,
    /// SHA-256 hash of the comparison ("new") WASM.
    pub new_wasm_hash: String,
    /// Ledger the baseline simulation ran against.
    pub old_ledger: u32,
    /// Ledger the comparison simulation ran against.
    pub new_ledger: u32,
}

/// A single resource row of the side-by-side comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffRow {
    /// Human-readable resource name (e.g. `"CPU Instructions"`).
    pub resource: String,
    /// Baseline value.
    pub old: i64,
    /// Comparison value.
    pub new: i64,
    /// `new - old` (negative when the metric improved).
    pub delta: i64,
    /// Percentage change relative to `old`. `None` when `old` is 0 and `new`
    /// is not (the change is unbounded / not expressible as a percentage).
    pub change_percent: Option<f64>,
    /// Which way the metric moved.
    pub direction: ChangeDirection,
}

/// A complete side-by-side diff between two cost reports.
///
/// This is the JSON structure emitted by `estimate --diff --json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostReportDiff {
    /// Function, network, and WASM identities of both reports.
    pub identity: ReportIdentity,
    /// One row per compared resource, in report order.
    pub rows: Vec<DiffRow>,
}

/// Compute the percentage change from `old` to `new`.
///
/// `None` means "undefined": the baseline was zero and the new value is not,
/// so no finite percentage exists. Zero-to-zero is reported as `0.0`.
fn percent_change(old: i64, new: i64) -> Option<f64> {
    if old == 0 {
        if new == 0 { Some(0.0) } else { None }
    } else {
        Some((new - old) as f64 / old as f64 * 100.0)
    }
}

/// Build one [`DiffRow`].
fn row(resource: &str, old: i64, new: i64) -> DiffRow {
    let delta = new.saturating_sub(old);
    let direction = match delta.cmp(&0) {
        std::cmp::Ordering::Greater => ChangeDirection::Increase,
        std::cmp::Ordering::Less => ChangeDirection::Decrease,
        std::cmp::Ordering::Equal => ChangeDirection::Unchanged,
    };
    DiffRow {
        resource: resource.to_string(),
        old,
        new,
        delta,
        change_percent: percent_change(old, new),
        direction,
    }
}

/// Build the comparison rows for two reports.
///
/// The row set is fixed and ordered: WASM Size, CPU Instructions, RAM Bytes,
/// Read Entries, Write Entries, Read Bytes, Write Bytes, Total Fee. Keeping a
/// fixed row set (rather than diffing arbitrary keys) means the table always
/// has the same shape, which makes it easy to scan and to snapshot.
#[must_use]
pub fn build_cost_report_diff(old: &CostReport, new: &CostReport) -> CostReportDiff {
    let rows = vec![
        row("WASM Size", old.wasm_size as i64, new.wasm_size as i64),
        row(
            "CPU Instructions",
            old.cpu_instructions as i64,
            new.cpu_instructions as i64,
        ),
        row(
            "RAM Bytes",
            old.memory_bytes as i64,
            new.memory_bytes as i64,
        ),
        row(
            "Read Entries",
            old.read_entries as i64,
            new.read_entries as i64,
        ),
        row(
            "Write Entries",
            old.write_entries as i64,
            new.write_entries as i64,
        ),
        row("Read Bytes", old.read_bytes as i64, new.read_bytes as i64),
        row(
            "Write Bytes",
            old.write_bytes as i64,
            new.write_bytes as i64,
        ),
        row("Total Fee", old.fee.total_stroops, new.fee.total_stroops),
    ];

    CostReportDiff {
        identity: ReportIdentity {
            function: old.function.clone(),
            network: old.network.clone(),
            old_wasm_hash: old.wasm_hash.clone(),
            new_wasm_hash: new.wasm_hash.clone(),
            old_ledger: old.ledger,
            new_ledger: new.ledger,
        },
        rows,
    }
}

/// Render the `Change` column text for a row, without color.
fn change_text(row: &DiffRow) -> String {
    match (row.direction, row.change_percent) {
        (ChangeDirection::Unchanged, _) => format!("{} (0.0%)", row.delta),
        (_, None) => format!("{:+} (n/a)", row.delta),
        (_, Some(pct)) => format!("{:+} ({pct:+.1}%)", row.delta),
    }
}

/// ANSI color for a change direction: red for increases (worse), green for
/// decreases (better), uncolored for unchanged.
fn change_color(direction: ChangeDirection) -> Option<Color> {
    match direction {
        ChangeDirection::Increase => Some(Color::Red),
        ChangeDirection::Decrease => Some(Color::Green),
        ChangeDirection::Unchanged => None,
    }
}

/// Format a diff as a side-by-side table string, choosing color based on
/// whether stdout is a terminal.
///
/// This is the entry point used by the CLI; tests and library callers that
/// need deterministic output should use
/// [`format_cost_report_diff_sized`] with an explicit `color` flag.
#[must_use]
pub fn format_cost_report_diff(old: &CostReport, new: &CostReport) -> String {
    format_cost_report_diff_sized(old, new, None, std::io::stdout().is_terminal())
}

/// Format a diff as a table with an optional maximum width and an explicit
/// color flag.
///
/// * `width` — when `Some`, the table switches to dynamic content
///   arrangement and wraps within that terminal width. Useful for snapshot
///   tests across narrow and wide terminals.
/// * `color` — when true, increases are red and decreases green, forced even
///   when stdout is not a terminal (so pipelines can opt into colored output
///   and tests can assert on it).
#[must_use]
pub fn format_cost_report_diff_sized(
    old: &CostReport,
    new: &CostReport,
    width: Option<u16>,
    color: bool,
) -> String {
    let diff = build_cost_report_diff(old, new);

    let mut output = String::new();
    output.push_str(&format!(
        "Cost diff: {} on {}\n",
        diff.identity.function, diff.identity.network
    ));
    output.push_str(&format!(
        "  Old: {} (ledger {})\n",
        diff.identity.old_wasm_hash, diff.identity.old_ledger
    ));
    output.push_str(&format!(
        "  New: {} (ledger {})\n\n",
        diff.identity.new_wasm_hash, diff.identity.new_ledger
    ));

    let mut table = Table::new();
    if let Some(width) = width {
        table.set_content_arrangement(ContentArrangement::Dynamic);
        table.set_width(width);
    }
    if color {
        table.enforce_styling();
    } else {
        table.force_no_tty();
    }
    table.set_header(vec!["Resource", "Old", "New", "Change (+/- %)"]);

    for row in &diff.rows {
        let text = change_text(row);
        let change_cell = match change_color(row.direction) {
            Some(color) => Cell::new(text).fg(color),
            None => Cell::new(text),
        };
        table.add_row(vec![
            Cell::new(&row.resource),
            Cell::new(row.old),
            Cell::new(row.new),
            change_cell,
        ]);
    }

    output.push_str(&table.to_string());
    output.push('\n');
    output.push_str("\nLegend: red = increased cost, green = decreased cost.\n");

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::fee_calc::FeeBreakdown;

    fn report(cpu: u64, writes: u32, total: i64, wasm_size: u64) -> CostReport {
        CostReport {
            function: "increment".to_string(),
            wasm_hash: format!("hash-{wasm_size}"),
            wasm_size,
            section_count: 0,
            custom_sections: Vec::new(),
            cpu_instructions: cpu,
            memory_bytes: 1_024,
            tx_size: 156,
            read_entries: 2,
            write_entries: writes,
            read_bytes: 10,
            write_bytes: 200,
            fee: FeeBreakdown {
                non_refundable_stroops: total / 3,
                refundable_stroops: total - total / 3,
                cpu_fee_stroops: 372,
                storage_fee_stroops: 4_063,
                bandwidth_fee_stroops: 61,
                base_fee_stroops: 100,
                total_stroops: total,
                total_xlm: "0.0015427".to_string(),
                fee_percentages: std::collections::BTreeMap::new(),
            },
            ledger: 100,
            network: "testnet".to_string(),
            rpc_latency_ms: 42,
            rates: None,
            warnings: Vec::new(),
            history: None,
            projections: None,
            contract_meta: crate::wasm::parser::ContractMeta::default(),
        }
    }

    #[test]
    fn diff_rows_cover_the_required_resources_in_order() {
        let diff =
            build_cost_report_diff(&report(100, 1, 1_000, 4_096), &report(120, 2, 1_500, 5_120));
        let names: Vec<&str> = diff.rows.iter().map(|r| r.resource.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "WASM Size",
                "CPU Instructions",
                "RAM Bytes",
                "Read Entries",
                "Write Entries",
                "Read Bytes",
                "Write Bytes",
                "Total Fee",
            ]
        );
    }

    #[test]
    fn increase_is_flagged_and_red() {
        let diff =
            build_cost_report_diff(&report(100, 1, 1_000, 4_096), &report(150, 1, 1_000, 4_096));
        let cpu = diff
            .rows
            .iter()
            .find(|r| r.resource == "CPU Instructions")
            .expect("cpu row");
        assert_eq!(cpu.delta, 50);
        assert_eq!(cpu.direction, ChangeDirection::Increase);
        assert_eq!(cpu.change_percent, Some(50.0));

        let out = format_cost_report_diff_sized(
            &report(100, 1, 1_000, 4_096),
            &report(150, 1, 1_000, 4_096),
            None,
            true,
        );
        assert!(out.contains("+50 (+50.0%)"), "got: {out}");
        // The direction maps to red, and the rendered table carries ANSI
        // escape sequences when color is requested.
        assert_eq!(change_color(cpu.direction), Some(Color::Red));
        assert!(
            out.contains('\u{1b}'),
            "colored output must contain ANSI escapes: {out}"
        );

        let plain = format_cost_report_diff_sized(
            &report(100, 1, 1_000, 4_096),
            &report(150, 1, 1_000, 4_096),
            None,
            false,
        );
        assert!(
            !plain.contains('\u{1b}'),
            "plain output must not contain ANSI escapes: {plain}"
        );
    }

    #[test]
    fn decrease_is_flagged_and_green() {
        let diff =
            build_cost_report_diff(&report(100, 1, 1_000, 4_096), &report(50, 1, 1_000, 4_096));
        let cpu = diff
            .rows
            .iter()
            .find(|r| r.resource == "CPU Instructions")
            .expect("cpu row");
        assert_eq!(cpu.delta, -50);
        assert_eq!(cpu.direction, ChangeDirection::Decrease);

        let out = format_cost_report_diff_sized(
            &report(100, 1, 1_000, 4_096),
            &report(50, 1, 1_000, 4_096),
            None,
            true,
        );
        assert!(out.contains("-50 (-50.0%)"), "got: {out}");
        assert_eq!(change_color(cpu.direction), Some(Color::Green));
        assert!(
            out.contains('\u{1b}'),
            "colored output must contain ANSI escapes: {out}"
        );
    }

    #[test]
    fn unchanged_metric_is_not_colorized() {
        let out = format_cost_report_diff_sized(
            &report(100, 1, 1_000, 4_096),
            &report(100, 1, 1_000, 4_096),
            None,
            true,
        );
        assert!(out.contains("0 (0.0%)"), "got: {out}");
        assert!(
            !out.contains('\u{1b}'),
            "unchanged rows must not be colored: {out}"
        );
    }

    #[test]
    fn zero_baseline_reports_undefined_percentage() {
        let diff = build_cost_report_diff(&report(0, 1, 1_000, 0), &report(100, 1, 1_000, 0));
        let cpu = diff
            .rows
            .iter()
            .find(|r| r.resource == "CPU Instructions")
            .expect("cpu row");
        assert_eq!(cpu.change_percent, None);
        assert_eq!(cpu.direction, ChangeDirection::Increase);

        let out = format_cost_report_diff_sized(
            &report(0, 1, 1_000, 0),
            &report(100, 1, 1_000, 0),
            None,
            false,
        );
        assert!(out.contains("+100 (n/a)"), "got: {out}");
    }

    #[test]
    fn zero_to_zero_is_unchanged() {
        let diff = build_cost_report_diff(&report(0, 0, 0, 0), &report(0, 0, 0, 0));
        assert!(
            diff.rows
                .iter()
                .all(|r| r.direction == ChangeDirection::Unchanged && r.change_percent == Some(0.0))
        );
    }

    #[test]
    fn table_layout_holds_across_terminal_widths() {
        let old = report(500_000, 2, 15_000, 4_096);
        let new = report(420_000, 3, 18_500, 4_800);
        let row_labels = [
            "WASM Size",
            "CPU Instructions",
            "RAM Bytes",
            "Read Entries",
            "Write Entries",
            "Read Bytes",
            "Write Bytes",
            "Total Fee",
        ];

        for width in [40u16, 60, 80, 120, 200] {
            let out = format_cost_report_diff_sized(&old, &new, Some(width), false);

            // The header and legend render at every width.
            assert!(
                out.contains("Resource") && out.contains("Change") && out.contains("Old"),
                "header must be present at width {width}: {out}"
            );
            assert!(
                out.contains("Legend:"),
                "legend must be present at width {width}: {out}"
            );

            // Every resource row renders: one header line plus at least one
            // line per row (narrow widths wrap a row across several lines).
            let content_lines = out.lines().filter(|l| l.starts_with('|')).count();
            assert!(
                content_lines > row_labels.len(),
                "expected a header plus {} content lines at width {width}, got {content_lines}: {out}",
                row_labels.len()
            );

            // Wide-enough terminals keep the full labels on one line.
            if width >= 60 {
                for label in row_labels {
                    assert!(
                        out.contains(label),
                        "row `{label}` must survive width {width}: {out}"
                    );
                }
            }
        }
    }

    #[test]
    fn json_structure_is_stable() {
        let diff =
            build_cost_report_diff(&report(100, 1, 1_000, 4_096), &report(150, 2, 1_200, 4_096));
        let value = serde_json::to_value(&diff).expect("serialize diff");
        assert_eq!(value["identity"]["function"], "increment");
        assert_eq!(value["identity"]["network"], "testnet");
        assert_eq!(value["rows"][0]["resource"], "WASM Size");
        assert_eq!(value["rows"][0]["delta"], 0);
        assert_eq!(value["rows"][1]["resource"], "CPU Instructions");
        assert_eq!(value["rows"][1]["delta"], 50);
        assert_eq!(value["rows"][1]["direction"], "increase");
    }
}
