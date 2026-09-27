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
    /// Size of the contract WASM binary in bytes.
    ///
    /// Carried on the report (rather than looked up again) because WASM size
    /// drives upload bandwidth cost, and
    /// [`generate_optimization_tips`] flags binaries over
    /// [`WASM_SIZE_TIP_THRESHOLD_BYTES`]. `#[serde(default)]` so reports
    /// serialized before this field existed still deserialize.
    #[serde(default)]
    pub wasm_size_bytes: u32,
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

/// Contract WASM size, in bytes, above which a size-optimization tip is
/// emitted (30 KB).
///
/// Uploads are charged bandwidth per byte on every deploy, and a contract
/// above this size is usually carrying debug symbols or unoptimized
/// dependencies rather than logic.
pub const WASM_SIZE_TIP_THRESHOLD_BYTES: u32 = 30 * 1024;

/// Share of the total fee, in percent, at which a cost component is treated
/// as the *dominant* factor and earns a targeted tip.
pub const DOMINANT_COST_PCT: u64 = 40;

/// Ledger-entry count from which batching/merging state becomes worth
/// suggesting. One or two entries is normal for a contract instance; three or
/// more usually means the footprint can be collapsed.
pub const MULTI_ENTRY_THRESHOLD: u32 = 3;

/// Transaction-envelope size, in bytes, above which a large-payload tip is
/// eligible (1 KB).
pub const LARGE_TX_BYTES: u32 = 1024;

/// `part` as a whole percentage of `total`, using integer-only arithmetic.
///
/// Fee math in this crate stays in `stroops`/`i64` — no floats near a value
/// that gets added up and compared. Returns `0` when `total` is not positive,
/// and clamps negative `part` to `0` so a defensive negative component can
/// never produce a nonsensical share.
#[must_use]
pub fn share_pct(part: i64, total: i64) -> u64 {
    if total <= 0 || part <= 0 {
        return 0;
    }
    let pct = i128::from(part) * 100 / i128::from(total);
    u64::try_from(pct).unwrap_or(u64::MAX)
}

/// Generate contextual, actionable cost-optimization tips for a report.
///
/// Where [`CostReport::suggest_optimizations`] quantifies *per-unit* savings
/// from the network's fee rates, this function answers the question a
/// newcomer actually has: **which single factor dominates this fee, and what
/// should I change?** Each tip names the dominant factor, quantifies its share
/// of the total fee, and proposes a concrete change.
///
/// A factor earns a tip when it both has a meaningful footprint and accounts
/// for at least [`DOMINANT_COST_PCT`]% of the total fee:
///
/// * **WASM size** — reported whenever the binary exceeds
///   [`WASM_SIZE_TIP_THRESHOLD_BYTES`], independent of the fee split (the
///   upload cost lands on the bandwidth component).
/// * **Ledger writes** — [`MULTI_ENTRY_THRESHOLD`] or more write entries while
///   storage dominates; suggests merging related state into one entry.
/// * **Ledger reads** — same threshold, suggests caching/batching.
/// * **CPU** — the instruction fee dominates.
/// * **Argument payload** — a large transaction envelope whose bandwidth fee
///   dominates; suggests trimming the payload.
/// * **Refundable fee** — rent bumps and events dominate; suggests reducing
///   state growth and emitted events.
///
/// Tips are returned in a fixed order (WASM, writes, reads, CPU, payload,
/// refundable) so output is deterministic and snapshot-testable. An empty
/// vector means no factor is dominant — callers should render nothing rather
/// than a "nothing to say" block.
///
/// # Network calls
/// None — pure computation over the report.
#[must_use]
pub fn generate_optimization_tips(report: &CostReport) -> Vec<String> {
    let total = report.fee.total_stroops;
    let mut tips: Vec<String> = Vec::new();

    if report.wasm_size_bytes > WASM_SIZE_TIP_THRESHOLD_BYTES {
        tips.push(format!(
            "Tip: the contract WASM is {} bytes, over the {} KB threshold. Uploads are charged bandwidth on every byte, so consider a release build with LTO and `strip = true` to shrink the binary.",
            report.wasm_size_bytes,
            WASM_SIZE_TIP_THRESHOLD_BYTES / 1024
        ));
    }

    let storage_share = share_pct(report.fee.storage_fee_stroops, total);
    if report.write_entries >= MULTI_ENTRY_THRESHOLD && storage_share >= DOMINANT_COST_PCT {
        tips.push(format!(
            "Tip: writing {} ledger entries accounts for {}% of the total fee. Consider combining related state into a single entry.",
            report.write_entries, storage_share
        ));
    }
    if report.read_entries >= MULTI_ENTRY_THRESHOLD && storage_share >= DOMINANT_COST_PCT {
        tips.push(format!(
            "Tip: reading {} ledger entries accounts for {}% of the total fee. Consider caching hot state or batching the reads into one call.",
            report.read_entries, storage_share
        ));
    }

    let cpu_share = share_pct(report.fee.cpu_fee_stroops, total);
    if report.cpu_instructions > 0 && cpu_share >= DOMINANT_COST_PCT {
        tips.push(format!(
            "Tip: CPU instructions account for {}% of the total fee ({} instructions consumed). Consider profiling the hot path and moving work off-chain.",
            cpu_share, report.cpu_instructions
        ));
    }

    let bandwidth_share = share_pct(report.fee.bandwidth_fee_stroops, total);
    if report.tx_size >= LARGE_TX_BYTES && bandwidth_share >= DOMINANT_COST_PCT {
        tips.push(format!(
            "Tip: the transaction envelope is {} bytes and its bandwidth fee accounts for {}% of the total fee. Consider trimming large argument payloads or passing a reference instead of the full value.",
            report.tx_size, bandwidth_share
        ));
    }

    let refundable_share = share_pct(report.fee.refundable_stroops, total);
    if report.fee.refundable_stroops > 0 && refundable_share >= DOMINANT_COST_PCT {
        tips.push(format!(
            "Tip: {}% of the total fee is refundable, which comes from ledger rent bumps and contract events. Consider reducing state growth and the number or size of emitted events.",
            refundable_share
        ));
    }

    tips
}

/// Render cost-optimization tips as a human-readable block.
///
/// Returns an empty string when there is nothing to say, so callers can
/// append the result unconditionally and stay quiet for a report with no
/// dominant cost factor.
#[must_use]
pub fn format_tips(tips: &[String]) -> String {
    if tips.is_empty() {
        return String::new();
    }
    let mut out = String::from("Optimization Tips:\n");
    for tip in tips {
        out.push_str(&format!("  {tip}\n"));
    }
    out
}

/// Aggregated metrics for a whole `estimate-all` batch.
///
/// Every value is a whole-unit aggregate over the successfully estimated
/// functions; `functions_evaluated` is the number of reports the aggregates
/// were computed from, so a caller can tell an empty batch from a single
/// cheap function. All fee values are stroops; the XLM strings are rendered
/// with the caller's chosen precision.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EstimateAllSummary {
    /// Number of functions included in the aggregates.
    pub functions_evaluated: usize,
    /// Lowest per-function total fee, in stroops.
    pub min_fee_stroops: i64,
    /// Highest per-function total fee, in stroops.
    pub max_fee_stroops: i64,
    /// Mean per-function total fee, in stroops (integer division).
    pub avg_fee_stroops: i64,
    /// Sum of every per-function total fee, in stroops.
    pub total_fee_stroops: i64,
    /// Lowest per-function CPU instruction count.
    pub min_cpu_instructions: u64,
    /// Highest per-function CPU instruction count.
    pub max_cpu_instructions: u64,
    /// Sum of every per-function CPU instruction count.
    pub total_cpu_instructions: u64,
    /// Sum of every per-function ledger write entry count.
    pub total_write_entries: u64,
    /// Sum of every per-function ledger read entry count.
    pub total_read_entries: u64,
    /// Lowest XLM total across the batch.
    pub min_total_xlm: String,
    /// Highest XLM total across the batch.
    pub max_total_xlm: String,
    /// Mean XLM total across the batch.
    pub avg_total_xlm: String,
}

/// Aggregate a batch of per-function reports into an [`EstimateAllSummary`].
///
/// Returns `None` for an empty batch — a summary over zero functions is
/// undefined (there is no min, max, or average), so callers can render a
/// "nothing was estimated" state instead of a row full of zeros.
///
/// The fee sum accumulates in `i128` so a large batch cannot overflow before
/// being cast back to `i64`; the cast is lossless because the result is a sum
/// of `i64` fees. The average uses integer division, matching
/// [`crate::report::fee_calc::fee_range`] and keeping fee math float-free.
#[must_use]
pub fn summarize_estimate_all(
    reports: &[CostReport],
    precision: u32,
) -> Option<EstimateAllSummary> {
    let first = reports.first()?;
    let mut min_fee = first.fee.total_stroops;
    let mut max_fee = first.fee.total_stroops;
    let mut min_cpu = first.cpu_instructions;
    let mut max_cpu = first.cpu_instructions;
    let mut total_fee: i128 = 0;
    let mut total_cpu: u128 = 0;
    let mut total_writes: u64 = 0;
    let mut total_reads: u64 = 0;

    for report in reports {
        min_fee = min_fee.min(report.fee.total_stroops);
        max_fee = max_fee.max(report.fee.total_stroops);
        min_cpu = min_cpu.min(report.cpu_instructions);
        max_cpu = max_cpu.max(report.cpu_instructions);
        total_fee += i128::from(report.fee.total_stroops);
        total_cpu += u128::from(report.cpu_instructions);
        total_writes += u64::from(report.write_entries);
        total_reads += u64::from(report.read_entries);
    }

    let count = reports.len();
    let avg_fee = (total_fee / i128::try_from(count).unwrap_or(1)) as i64;

    Some(EstimateAllSummary {
        functions_evaluated: count,
        min_fee_stroops: min_fee,
        max_fee_stroops: max_fee,
        avg_fee_stroops: avg_fee,
        total_fee_stroops: total_fee as i64,
        min_cpu_instructions: min_cpu,
        max_cpu_instructions: max_cpu,
        total_cpu_instructions: total_cpu as u64,
        total_write_entries: total_writes,
        total_read_entries: total_reads,
        min_total_xlm: crate::report::fee_calc::stroops_to_xlm(min_fee, precision),
        max_total_xlm: crate::report::fee_calc::stroops_to_xlm(max_fee, precision),
        avg_total_xlm: crate::report::fee_calc::stroops_to_xlm(avg_fee, precision),
    })
}

/// Narrowest column width comfy-table is given, so a column of short cells
/// still renders a readable box.
const MIN_COLUMN_WIDTH: u16 = 6;

/// Builds the `estimate-all` table: one row per estimated function plus a
/// visually separated summary footer row.
///
/// The footer is separated with real border styling, not a blank line: the
/// table draws a `├───┼───┤` rule above every row (comfy-table's internal
/// horizontal line), so the summary sits in its own band at the bottom of the
/// box. The whole table is rendered in one pass by one `comfy_table::Table`,
/// which sizes every column to its widest cell (footer included) — so nothing
/// is truncated and the rows can never drift out of alignment.
///
/// Returns an empty string for an empty batch, so an empty `estimate-all` run
/// adds nothing to the output.
///
/// # Arguments
/// * `reports` — the successfully estimated functions, in run order.
/// * `summary` — pre-computed aggregates via [`summarize_estimate_all`];
///   `None` omits the footer row (e.g. when `--quiet` suppressed it).
pub fn format_estimate_all_table(
    reports: &[CostReport],
    summary: Option<&EstimateAllSummary>,
) -> String {
    if reports.is_empty() && summary.is_none() {
        return String::new();
    }

    let header = vec![
        "Function".to_string(),
        "CPU insns".to_string(),
        "Fee (stroops)".to_string(),
        "Fee (XLM)".to_string(),
        "Ledger".to_string(),
        "Write entries".to_string(),
    ];
    let mut rows: Vec<Vec<String>> = reports
        .iter()
        .map(|r| {
            vec![
                r.function.clone(),
                r.cpu_instructions.to_string(),
                r.fee.total_stroops.to_string(),
                r.fee.total_xlm.clone(),
                r.ledger.to_string(),
                r.write_entries.to_string(),
            ]
        })
        .collect();

    if let Some(s) = summary {
        // The footer is the last row, so comfy-table draws the separator rule
        // immediately above it.
        rows.push(vec![
            format!("Summary: {} function(s)", s.functions_evaluated),
            format!("{} - {}", s.min_cpu_instructions, s.max_cpu_instructions),
            format!(
                "min {} / max {} / avg {}",
                s.min_fee_stroops, s.max_fee_stroops, s.avg_fee_stroops
            ),
            format!("{} - {}", s.min_total_xlm, s.max_total_xlm),
            String::new(),
            s.total_write_entries.to_string(),
        ]);
    }

    let mut table = comfy_table::Table::new();
    table.load_preset(comfy_table::presets::UTF8_BORDERS_ONLY);
    // Turn on comfy-table's internal horizontal rule so the last row (the
    // summary footer) is visually separated from the per-function rows, and
    // close the header divider with proper intersections.
    table.set_style(comfy_table::TableComponent::HorizontalLines, '─');
    table.set_style(comfy_table::TableComponent::MiddleIntersections, '┼');
    table.set_style(comfy_table::TableComponent::LeftBorderIntersections, '├');
    table.set_style(comfy_table::TableComponent::RightBorderIntersections, '┤');
    table.set_style(comfy_table::TableComponent::BottomBorderIntersections, '┴');
    table.set_style(comfy_table::TableComponent::MiddleHeaderIntersections, '╪');

    table.set_header(header);
    for row in &rows {
        table.add_row(row.clone());
    }
    // Keep very narrow columns (e.g. a single-digit write-entry count) from
    // collapsing to an unreadable box.
    let widths: Vec<u16> = table
        .column_max_content_widths()
        .into_iter()
        .map(|width| width.max(MIN_COLUMN_WIDTH))
        .collect();
    for (index, width) in widths.iter().enumerate() {
        if let Some(column) = table.column_mut(index) {
            // `ColumnConstraint::Boundaries` pins both ends of the column to
            // the same width, so comfy-table neither grows the column beyond
            // its widest cell nor truncates one.
            column.set_constraint(comfy_table::ColumnConstraint::Boundaries {
                lower: comfy_table::Width::Fixed(*width),
                upper: comfy_table::Width::Fixed(*width),
            });
        }
    }

    table.to_string()
}

/// Renders the batch [`EstimateAllSummary`] as a GitHub-flavored Markdown
/// table, for `estimate-all --format markdown`.
///
/// Complements [`format_estimate_all_table`]: the same aggregate in the same
/// shape, but expressed in Markdown so it renders in a PR comment or a
/// GitBook page. Returns an empty string when there is no summary.
#[must_use]
pub fn format_estimate_all_summary_markdown(summary: &EstimateAllSummary) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "### Summary — {} function(s) evaluated\n\n",
        summary.functions_evaluated
    ));
    out.push_str("| Metric | Value |\n| --- | --- |\n");
    out.push_str(&format!(
        "| Min fee | {} stroops ({}) |\n",
        summary.min_fee_stroops, summary.min_total_xlm
    ));
    out.push_str(&format!(
        "| Max fee | {} stroops ({}) |\n",
        summary.max_fee_stroops, summary.max_total_xlm
    ));
    out.push_str(&format!(
        "| Average fee | {} stroops ({}) |\n",
        summary.avg_fee_stroops, summary.avg_total_xlm
    ));
    out.push_str(&format!(
        "| Total fee | {} stroops |\n",
        summary.total_fee_stroops
    ));
    out.push_str(&format!(
        "| CPU instructions | {} - {} (total {}) |\n",
        summary.min_cpu_instructions, summary.max_cpu_instructions, summary.total_cpu_instructions
    ));
    out.push_str(&format!(
        "| Ledger entries | {} read / {} written |\n",
        summary.total_read_entries, summary.total_write_entries
    ));
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
            wasm_size_bytes: 14_432,
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
            wasm_size_bytes: 0,
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

    // ── share_pct ─────────────────────────────────────────────────

    #[test]
    fn test_share_pct_basic() {
        assert_eq!(share_pct(50, 200), 25);
        assert_eq!(share_pct(200, 200), 100);
        assert_eq!(share_pct(1, 3), 33);
    }

    /// A zero, negative, or absent total has no share; a negative component
    /// must never render as a nonsensical percentage.
    #[test]
    fn test_share_pct_degenerate_totals() {
        assert_eq!(share_pct(10, 0), 0);
        assert_eq!(share_pct(10, -5), 0);
        assert_eq!(share_pct(-10, 100), 0);
        assert_eq!(share_pct(0, 100), 0);
    }

    // ── generate_optimization_tips ───────────────────────────────

    /// The acceptance-criteria example: three write entries dominating the fee
    /// must produce a tip that names the count, the share, and the fix.
    #[test]
    fn test_tip_for_dominant_ledger_writes() {
        let mut report = report_with_rates(sample_rates());
        report.write_entries = 3;
        report.fee.storage_fee_stroops = 11_110; // 72% of 15_427
        report.fee.refundable_stroops = 0;

        let tips = generate_optimization_tips(&report);
        let write_tip = tips
            .iter()
            .find(|t| t.contains("writing 3 ledger entries"))
            .expect("a write tip");
        assert!(
            write_tip.starts_with("Tip: "),
            "tips must be prefixed; got: {write_tip}"
        );
        assert!(
            write_tip.contains("72% of the total fee"),
            "got: {write_tip}"
        );
        assert!(
            write_tip.contains("combining related state"),
            "got: {write_tip}"
        );
    }

    #[test]
    fn test_tip_for_dominant_ledger_reads() {
        let mut report = report_with_rates(sample_rates());
        report.read_entries = 4;
        report.fee.storage_fee_stroops = 15_000;
        report.fee.refundable_stroops = 0;

        let tips = generate_optimization_tips(&report);
        assert!(
            tips.iter()
                .any(|t| t.contains("reading 4 ledger entries") && t.contains("batching")),
            "got: {tips:?}"
        );
    }

    #[test]
    fn test_tip_for_dominant_cpu() {
        let mut report = report_with_rates(sample_rates());
        report.cpu_instructions = 4_000_000;
        report.fee.cpu_fee_stroops = 14_000;
        report.fee.storage_fee_stroops = 0;
        report.fee.bandwidth_fee_stroops = 0;
        report.fee.refundable_stroops = 0;

        let tips = generate_optimization_tips(&report);
        assert!(
            tips.iter()
                .any(|t| t.contains("4000000 instructions") && t.contains("90%")),
            "got: {tips:?}"
        );
    }

    #[test]
    fn test_tip_for_large_argument_payload() {
        let mut report = report_with_rates(sample_rates());
        report.tx_size = 4_096;
        report.fee.bandwidth_fee_stroops = 15_000;
        report.fee.storage_fee_stroops = 0;
        report.fee.cpu_fee_stroops = 0;
        report.fee.refundable_stroops = 0;

        let tips = generate_optimization_tips(&report);
        assert!(
            tips.iter()
                .any(|t| t.contains("4096 bytes") && t.contains("argument payloads")),
            "got: {tips:?}"
        );
    }

    /// Below the threshold a small contract earns no WASM tip.
    #[test]
    fn test_no_wasm_size_tip_at_or_below_threshold() {
        let mut report = report_with_rates(sample_rates());
        report.wasm_size_bytes = WASM_SIZE_TIP_THRESHOLD_BYTES;
        assert!(
            !generate_optimization_tips(&report)
                .iter()
                .any(|t| t.contains("contract WASM")),
            "the threshold itself must not trigger the tip"
        );

        report.wasm_size_bytes = WASM_SIZE_TIP_THRESHOLD_BYTES + 1;
        let tips = generate_optimization_tips(&report);
        assert!(
            tips.iter().any(|t| t.contains("over the 30 KB threshold")),
            "got: {tips:?}"
        );
    }

    #[test]
    fn test_wasm_size_tip_is_independent_of_the_fee_split() {
        // A tiny total fee with a huge binary still flags the upload cost.
        let mut report = report_with_rates(sample_rates());
        report.wasm_size_bytes = 400_000;
        report.fee.total_stroops = 1;
        assert!(
            generate_optimization_tips(&report)
                .iter()
                .any(|t| t.contains("400000 bytes")),
            "the WASM tip must not depend on fee dominance"
        );
    }

    #[test]
    fn test_tip_for_dominant_refundable_fee() {
        let mut report = report_with_rates(sample_rates());
        report.fee.storage_fee_stroops = 0;
        report.fee.cpu_fee_stroops = 0;
        report.fee.bandwidth_fee_stroops = 0;
        report.fee.refundable_stroops = 15_000;

        let tips = generate_optimization_tips(&report);
        assert!(
            tips.iter()
                .any(|t| t.contains("refundable") && t.contains("rent bumps")),
            "got: {tips:?}"
        );
    }

    /// A report where no component reaches the dominance threshold produces no
    /// tips at all, so the renderer stays silent instead of printing an empty
    /// section.
    #[test]
    fn test_no_tips_when_nothing_dominates() {
        let mut report = report_with_rates(sample_rates());
        // Spread the fee evenly: 33% / 33% / 6% / 28% — nothing dominant.
        report.fee.total_stroops = 15_000;
        report.fee.cpu_fee_stroops = 5_000;
        report.fee.storage_fee_stroops = 5_000;
        report.fee.bandwidth_fee_stroops = 1_000;
        report.fee.refundable_stroops = 4_000;
        report.write_entries = 1;
        report.read_entries = 1;
        report.tx_size = 156;

        let tips = generate_optimization_tips(&report);
        assert!(tips.is_empty(), "unexpected tips: {tips:?}");
        assert!(
            format_tips(&tips).is_empty(),
            "an empty tip list must render nothing"
        );
    }

    /// Tips come back in a fixed order so output is deterministic.
    #[test]
    fn test_tip_order_is_deterministic() {
        let mut report = report_with_rates(sample_rates());
        report.wasm_size_bytes = 90_000;
        report.write_entries = 5;
        report.read_entries = 5;
        report.fee.storage_fee_stroops = 14_000;
        report.fee.refundable_stroops = 0;

        let first = generate_optimization_tips(&report);
        let second = generate_optimization_tips(&report);
        assert_eq!(first, second);
        assert!(
            first[0].contains("contract WASM"),
            "WASM comes first; got: {first:?}"
        );
        assert!(
            first[1].contains("writing 5 ledger entries"),
            "writes precede reads; got: {first:?}"
        );
        assert!(
            first[2].contains("reading 5 ledger entries"),
            "reads follow writes; got: {first:?}"
        );
    }

    #[test]
    fn test_format_tips_renders_header_and_entries() {
        let out = format_tips(&["Tip: first".to_string(), "Tip: second".to_string()]);
        assert!(out.starts_with("Optimization Tips:\n"));
        assert!(out.contains("  Tip: first\n"));
        assert!(out.contains("  Tip: second\n"));
    }

    // ── summarize_estimate_all / format_estimate_all_table ────────

    fn batch_report(function: &str, cpu: u64, fee: i64, writes: u32) -> CostReport {
        let mut report = report_with_rates(sample_rates());
        report.function = function.to_string();
        report.cpu_instructions = cpu;
        report.write_entries = writes;
        report.fee.total_stroops = fee;
        report.fee.total_xlm = crate::report::fee_calc::stroops_to_xlm(
            fee,
            crate::report::fee_calc::DEFAULT_PRECISION,
        );
        report
    }

    #[test]
    fn test_summarize_estimate_all_empty_is_none() {
        assert!(summarize_estimate_all(&[], crate::report::fee_calc::DEFAULT_PRECISION).is_none());
        assert!(format_estimate_all_table(&[], None).is_empty());
    }

    #[test]
    fn test_summarize_estimate_all_single_function() {
        let reports = vec![batch_report("increment", 500, 1_000, 1)];
        let summary = summarize_estimate_all(&reports, crate::report::fee_calc::DEFAULT_PRECISION)
            .expect("one report yields a summary");
        assert_eq!(summary.functions_evaluated, 1);
        assert_eq!(summary.min_fee_stroops, 1_000);
        assert_eq!(summary.max_fee_stroops, 1_000);
        assert_eq!(summary.avg_fee_stroops, 1_000);
        assert_eq!(summary.total_fee_stroops, 1_000);
        assert_eq!(summary.min_cpu_instructions, 500);
        assert_eq!(summary.max_cpu_instructions, 500);
        assert_eq!(summary.total_cpu_instructions, 500);
        assert_eq!(summary.total_write_entries, 1);
    }

    #[test]
    fn test_summarize_estimate_all_multi_function() {
        let reports = vec![
            batch_report("increment", 500, 1_000, 1),
            batch_report("decrement", 100, 3_000, 2),
            batch_report("reset", 900, 4_000, 3),
        ];
        let summary = summarize_estimate_all(&reports, crate::report::fee_calc::DEFAULT_PRECISION)
            .expect("three reports yield a summary");
        assert_eq!(summary.functions_evaluated, 3);
        assert_eq!(summary.min_fee_stroops, 1_000);
        assert_eq!(summary.max_fee_stroops, 4_000);
        assert_eq!(summary.avg_fee_stroops, 2_666);
        assert_eq!(summary.total_fee_stroops, 8_000);
        assert_eq!(summary.min_cpu_instructions, 100);
        assert_eq!(summary.max_cpu_instructions, 900);
        assert_eq!(summary.total_cpu_instructions, 1_500);
        assert_eq!(summary.total_write_entries, 6);
        assert_eq!(summary.total_read_entries, 3);
    }

    /// A fee sum that would overflow `i64` must saturate through `i128` and
    /// still be reported (clamped) rather than wrapping negative.
    #[test]
    fn test_summarize_estimate_all_saturates_large_sums() {
        let reports = vec![
            batch_report("a", 1, i64::MAX / 2, 0),
            batch_report("b", 1, i64::MAX / 2, 0),
        ];
        let summary = summarize_estimate_all(&reports, crate::report::fee_calc::DEFAULT_PRECISION)
            .expect("summary");
        assert_eq!(summary.total_fee_stroops, i64::MAX - 1);
        assert!(summary.total_fee_stroops > 0, "the sum must not wrap");
    }

    /// The footer must be a real border-separated row, not a blank line, and
    /// both halves of the table must line up column-wise.
    #[test]
    fn test_estimate_all_table_has_a_separated_footer_row() {
        let reports = vec![
            batch_report("increment", 500, 1_000, 1),
            batch_report("decrement", 100, 3_000, 2),
        ];
        let summary = summarize_estimate_all(&reports, crate::report::fee_calc::DEFAULT_PRECISION)
            .expect("summary");
        let table = format_estimate_all_table(&reports, Some(&summary));

        // Required footer content: function count, min/max/avg fee, CPU range.
        assert!(table.contains("Summary: 2 function(s)"));
        assert!(table.contains("min 1000 / max 3000 / avg 2000"));
        assert!(table.contains("100 - 500"));

        // Visual separation is drawn with box characters, not whitespace.
        assert!(table.contains('┌'), "missing top border");
        assert!(table.contains('╞'), "missing header divider");
        assert!(table.contains('├'), "missing footer separator");
        assert!(table.contains('└'), "missing bottom border");

        // Every line of one rendered table has the same width.
        let widths: Vec<usize> = table.lines().map(|l| l.chars().count()).collect();
        assert!(
            widths.windows(2).all(|w| w[0] == w[1]),
            "misaligned columns: {widths:?}"
        );
    }

    #[test]
    fn test_estimate_all_table_without_summary_has_no_footer() {
        let reports = vec![batch_report("increment", 500, 1_000, 1)];
        let table = format_estimate_all_table(&reports, None);
        assert!(table.contains("increment"));
        assert!(!table.contains("Summary:"));
    }

    #[test]
    fn test_estimate_all_summary_markdown_carries_every_metric() {
        let reports = vec![
            batch_report("increment", 500, 1_000, 1),
            batch_report("decrement", 100, 3_000, 2),
        ];
        let summary = summarize_estimate_all(&reports, crate::report::fee_calc::DEFAULT_PRECISION)
            .expect("summary");
        let markdown = format_estimate_all_summary_markdown(&summary);

        assert!(markdown.starts_with("### Summary — 2 function(s) evaluated"));
        assert!(markdown.contains("| Min fee | 1000 stroops (0.0001000) |"));
        assert!(markdown.contains("| Max fee | 3000 stroops (0.0003000) |"));
        assert!(markdown.contains("| Average fee | 2000 stroops (0.0002000) |"));
        assert!(markdown.contains("| Total fee | 4000 stroops |"));
        assert!(markdown.contains("| CPU instructions | 100 - 500 (total 600) |"));
        assert!(markdown.contains("| Ledger entries | 2 read / 3 written |"));
    }

    /// A WASM export name may legally contain non-ASCII characters. The table
    /// must still render the row and its summary footer rather than panicking
    /// or dropping the content. (Exact column alignment is only asserted for
    /// ASCII content, where `chars().count()` equals the display width.)
    #[test]
    fn test_estimate_all_table_handles_non_ascii_function_names() {
        let mut report = batch_report("incrément✨", 500, 1_000, 1);
        report.fee.total_xlm = "0.0001000".to_string();
        let reports = vec![report];
        let summary = summarize_estimate_all(&reports, crate::report::fee_calc::DEFAULT_PRECISION)
            .expect("summary");
        let table = format_estimate_all_table(&reports, Some(&summary));

        assert!(table.contains("incrément✨"));
        assert!(table.contains("Summary: 1 function(s)"));
        assert!(table.contains("min 1000 / max 1000 / avg 1000"));
    }
}
