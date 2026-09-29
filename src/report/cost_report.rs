use comfy_table::{Cell, Color, Table};

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

/// Percentage of a protocol limit at which a [`ResourceWarning`] is raised.
pub const RESOURCE_WARNING_THRESHOLD_PERCENT: u64 = 80;

/// The subset of the network's resource-limit configuration needed to warn
/// when a simulation approaches protocol ceilings.
///
/// Values mirror `ConfigSettingContractComputeV0`,
/// `ConfigSettingContractLedgerCostV0`, and `ConfigSettingContractBandwidthV0`.
/// A field is `None` when the corresponding config setting was unavailable —
/// that check is then skipped rather than comparing against a bogus zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NetworkConfig {
    /// `ConfigSettingContractComputeV0.tx_max_instructions`.
    pub tx_max_instructions: Option<u64>,
    /// `ConfigSettingContractLedgerCostV0.tx_max_disk_read_entries`.
    pub tx_max_read_entries: Option<u64>,
    /// `ConfigSettingContractLedgerCostV0.tx_max_write_ledger_entries`.
    pub tx_max_write_entries: Option<u64>,
    /// `ConfigSettingContractLedgerCostV0.tx_max_disk_read_bytes`.
    pub tx_max_read_bytes: Option<u64>,
    /// `ConfigSettingContractLedgerCostV0.tx_max_write_bytes`.
    pub tx_max_write_bytes: Option<u64>,
    /// `ConfigSettingContractBandwidthV0.tx_max_size_bytes`.
    pub tx_max_size: Option<u64>,
}

impl NetworkConfig {
    /// Build the limit set from a config snapshot.
    ///
    /// Missing sections leave the corresponding fields `None`, which disables
    /// their checks instead of treating an unknown limit as zero. Limits that
    /// are non-positive in the config are likewise treated as unknown.
    #[must_use]
    pub fn from_snapshot(snapshot: &crate::config_snapshot::model::ConfigSnapshot) -> Self {
        let positive_u64 = |value: i64| u64::try_from(value).ok().filter(|v| *v > 0);
        let positive_u32 = |value: u32| Some(u64::from(value)).filter(|v| *v > 0);

        let mut config = Self::default();
        if let Some(compute) = &snapshot.contract_compute {
            config.tx_max_instructions = positive_u64(compute.tx_max_instructions);
        }
        if let Some(cost) = &snapshot.contract_ledger_cost {
            config.tx_max_read_entries = positive_u32(cost.tx_max_disk_read_entries);
            config.tx_max_write_entries = positive_u32(cost.tx_max_write_ledger_entries);
            config.tx_max_read_bytes = positive_u32(cost.tx_max_disk_read_bytes);
            config.tx_max_write_bytes = positive_u32(cost.tx_max_write_bytes);
        }
        if let Some(bandwidth) = &snapshot.contract_bandwidth {
            config.tx_max_size = positive_u32(bandwidth.tx_max_size_bytes);
        }
        config
    }
}

/// A warning that a simulated resource is approaching or exceeding a
/// protocol limit.
///
/// `percent` is computed with integer arithmetic only (floor of
/// `used * 100 / limit`), so it is stable across platforms and never depends
/// on floating-point rounding.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResourceWarning {
    /// Machine-readable resource key (e.g. `"cpu_instructions"`).
    pub resource: String,
    /// Human-readable resource label (e.g. `"CPU instructions"`).
    pub label: String,
    /// Amount consumed by the simulation.
    pub used: u64,
    /// Protocol limit for this resource.
    pub limit: u64,
    /// Percentage of the limit consumed, floored to a whole percent.
    pub percent: u32,
    /// Human-readable explanation suitable for a warning banner.
    pub message: String,
}

/// Compare a cost report against network resource limits and return a warning
/// for every resource at or above
/// [`RESOURCE_WARNING_THRESHOLD_PERCENT`] of its limit.
///
/// Checks run in a fixed order (CPU, entries, bytes, tx size) so output is
/// deterministic. Resources whose limit is unknown (`None`) or non-positive
/// are skipped.
#[must_use]
pub fn check_resource_limits(report: &CostReport, config: &NetworkConfig) -> Vec<ResourceWarning> {
    check_resource_limits_raw(
        report.cpu_instructions,
        report.read_entries,
        report.write_entries,
        report.read_bytes,
        report.write_bytes,
        report.tx_size,
        config,
    )
}

/// Threshold check against raw resource values, for callers that hold the
/// simulator's output before a full [`CostReport`] has been assembled.
#[must_use]
pub fn check_resource_limits_raw(
    cpu_instructions: u64,
    read_entries: u32,
    write_entries: u32,
    read_bytes: u32,
    write_bytes: u32,
    tx_size: u32,
    config: &NetworkConfig,
) -> Vec<ResourceWarning> {
    let mut warnings: Vec<ResourceWarning> = Vec::new();
    push_resource_warning(
        &mut warnings,
        "cpu_instructions",
        "CPU instructions",
        cpu_instructions,
        config.tx_max_instructions,
    );
    push_resource_warning(
        &mut warnings,
        "read_entries",
        "Ledger read entries",
        u64::from(read_entries),
        config.tx_max_read_entries,
    );
    push_resource_warning(
        &mut warnings,
        "write_entries",
        "Ledger write entries",
        u64::from(write_entries),
        config.tx_max_write_entries,
    );
    push_resource_warning(
        &mut warnings,
        "read_bytes",
        "Ledger read bytes",
        u64::from(read_bytes),
        config.tx_max_read_bytes,
    );
    push_resource_warning(
        &mut warnings,
        "write_bytes",
        "Ledger write bytes",
        u64::from(write_bytes),
        config.tx_max_write_bytes,
    );
    push_resource_warning(
        &mut warnings,
        "tx_size",
        "Transaction size",
        u64::from(tx_size),
        config.tx_max_size,
    );
    warnings
}

/// Append a warning for `resource` when `used` is at or above the threshold
/// of `limit`. No-op when the limit is unknown or non-positive.
fn push_resource_warning(
    out: &mut Vec<ResourceWarning>,
    resource: &str,
    label: &str,
    used: u64,
    limit: Option<u64>,
) {
    let Some(limit) = limit else {
        return;
    };
    if limit == 0 {
        return;
    }
    // Integer-only threshold comparison: used / limit >= threshold / 100.
    if used.saturating_mul(100) < limit.saturating_mul(RESOURCE_WARNING_THRESHOLD_PERCENT) {
        return;
    }
    let percent = u32::try_from(used.saturating_mul(100) / limit).unwrap_or(u32::MAX);
    out.push(ResourceWarning {
        resource: resource.to_string(),
        label: label.to_string(),
        used,
        limit,
        percent,
        message: format!("{label} at {percent}% of the network limit ({used} of {limit})"),
    });
}

/// Render resource-limit warnings as a human-readable banner.
///
/// Returns an empty string when there is nothing to warn about, so callers can
/// append the result unconditionally without leaving a stray section header.
#[must_use]
pub fn format_resource_warnings(warnings: &[ResourceWarning]) -> String {
    if warnings.is_empty() {
        return String::new();
    }
    let mut out = String::from("\nResource limit warnings:\n");
    for warning in warnings {
        out.push_str(&format!("  - {}\n", warning.message));
    }
    out
}

/// Direction of a historical fee change relative to the current run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CostTrend {
    /// The historical fee is lower than the current run (cost decreased).
    Improvement,
    /// The historical fee is higher than the current run (cost increased).
    Regression,
    /// The historical fee equals the current run.
    Unchanged,
}

impl CostTrend {
    /// Classify a `delta = historical - current` fee difference.
    #[must_use]
    pub fn for_delta(delta_stroops: i64) -> Self {
        match delta_stroops.cmp(&0) {
            std::cmp::Ordering::Greater => Self::Regression,
            std::cmp::Ordering::Less => Self::Improvement,
            std::cmp::Ordering::Equal => Self::Unchanged,
        }
    }
}

/// One previous run of the same contract function, alongside its fee delta
/// against the current run.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HistoryEntry {
    /// ISO-8601 timestamp of the historical estimate.
    pub timestamp: String,
    /// Ledger sequence at the time of the historical estimate.
    pub ledger: u32,
    /// CPU instructions consumed by the historical estimate.
    pub cpu_instructions: u64,
    /// Total fee of the historical estimate, in stroops.
    pub total_stroops: i64,
    /// Total fee of the historical estimate, in XLM.
    pub total_xlm: String,
    /// `historical - current` in stroops: positive means a regression.
    pub delta_stroops: i64,
    /// Trend classification derived from `delta_stroops`.
    pub trend: CostTrend,
}

/// Build [`HistoryEntry`] records from previous runs, computing each one's
/// delta against the current run's total fee.
///
/// Entries are returned in the order supplied (callers pass them
/// newest-first) with `delta_stroops = entry_fee - current_fee`.
#[must_use]
pub fn build_history_entries(
    current_total_stroops: i64,
    current_xlm_precision: u32,
    runs: &[HistoricalRun],
) -> Vec<HistoryEntry> {
    runs.iter()
        .map(|run| {
            let delta_stroops = run.total_stroops.saturating_sub(current_total_stroops);
            HistoryEntry {
                timestamp: run.timestamp.clone(),
                ledger: run.ledger,
                cpu_instructions: run.cpu_instructions,
                total_stroops: run.total_stroops,
                total_xlm: crate::report::fee_calc::stroops_to_xlm(
                    run.total_stroops,
                    current_xlm_precision,
                ),
                delta_stroops,
                trend: CostTrend::for_delta(delta_stroops),
            }
        })
        .collect()
}

/// A raw previous run used to build [`HistoryEntry`] records.
///
/// Kept separate from [`HistoryEntry`] so callers can supply cache rows
/// without knowing the current run's fee.
#[derive(Debug, Clone)]
pub struct HistoricalRun {
    /// ISO-8601 timestamp of the run.
    pub timestamp: String,
    /// Ledger sequence at the time of the run.
    pub ledger: u32,
    /// CPU instructions consumed.
    pub cpu_instructions: u64,
    /// Total fee in stroops.
    pub total_stroops: i64,
}

/// Render the historical cost trend table.
///
/// Regressions (cost increases vs the current run) are shown in red,
/// improvements (cost reductions) in green, and unchanged runs in yellow.
/// Returns an empty string when there is no history, so callers can append the
/// result unconditionally.
#[must_use]
pub fn format_cost_history(entries: &[HistoryEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let mut table = Table::new();
    table.set_header(vec![
        "Timestamp",
        "Ledger",
        "CPU Instructions",
        "Total Fee",
        "Delta vs current",
    ]);

    for entry in entries {
        let (delta_text, color) = match entry.trend {
            CostTrend::Regression => (format!("+{}", entry.delta_stroops), Color::Red),
            CostTrend::Improvement => (format!("{}", entry.delta_stroops), Color::Green),
            CostTrend::Unchanged => ("0".to_string(), Color::Yellow),
        };
        table.add_row(vec![
            Cell::new(&entry.timestamp),
            Cell::new(entry.ledger),
            Cell::new(entry.cpu_instructions),
            Cell::new(format!("{} ({})", entry.total_stroops, entry.total_xlm)),
            Cell::new(delta_text).fg(color),
        ]);
    }

    let mut out = String::from("\nCost history (previous runs, newest first):\n");
    out.push_str(&table.to_string());
    out.push('\n');
    out
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
    /// Resource-limit warnings for this run (#322). Always serialized so JSON
    /// consumers see a stable `warnings` array (empty when nothing is near a
    /// limit).
    #[serde(default)]
    pub warnings: Vec<ResourceWarning>,
    /// Historical trend entries (#321). `None` unless the caller requested
    /// history (via `--history`); when requested it is always an array in JSON
    /// output, possibly empty when there are no previous runs to compare.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<HistoryEntry>>,
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

    // Resource-limit warnings (#322) — only when something nears a ceiling.
    output.push_str(&format_resource_warnings(&report.warnings));

    // Historical trend table (#321) — only populated with `--history`.
    if let Some(history) = &report.history {
        output.push_str(&format_cost_history(history));
    }

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
            warnings: Vec::new(),
            history: None,
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
            warnings: Vec::new(),
            history: None,
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
