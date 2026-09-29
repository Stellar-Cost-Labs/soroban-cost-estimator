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
