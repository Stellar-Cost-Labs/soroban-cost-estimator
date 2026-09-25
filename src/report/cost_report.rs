use comfy_table::{Cell, Color, Table};

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
                total_stroops: 0,
                total_xlm: "0.0000000".to_string(),
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

    // ── Resource-limit warnings (#322) ───────────────────────────────

    fn limits_all(value: u64) -> NetworkConfig {
        NetworkConfig {
            tx_max_instructions: Some(value),
            tx_max_read_entries: Some(value),
            tx_max_write_entries: Some(value),
            tx_max_read_bytes: Some(value),
            tx_max_write_bytes: Some(value),
            tx_max_size: Some(value),
        }
    }

    fn report_with_resources(
        cpu_instructions: u64,
        read_entries: u32,
        write_entries: u32,
        read_bytes: u32,
        write_bytes: u32,
        tx_size: u32,
    ) -> CostReport {
        CostReport {
            cpu_instructions,
            tx_size,
            read_entries,
            write_entries,
            read_bytes,
            write_bytes,
            ..report_with_rates(sample_rates())
        }
    }

    #[test]
    fn test_check_resource_limits_triggers_at_threshold() {
        // Exactly 80% must warn (>= comparison, integer math).
        let report = report_with_resources(800, 800, 800, 800, 800, 800);
        let warnings = check_resource_limits(&report, &limits_all(1_000));
        // All six resources sit at exactly 80% of their 1000-unit limit.
        assert_eq!(warnings.len(), 6);
        assert!(warnings.iter().all(|w| w.percent == 80));
        assert_eq!(warnings[0].resource, "cpu_instructions");
    }

    #[test]
    fn test_check_resource_limits_below_threshold_is_empty() {
        // 79% is below the 80% threshold on every axis.
        let report = report_with_resources(790_000, 79, 79, 79, 79, 790);
        let warnings = check_resource_limits(&report, &limits_all(1_000_000));
        assert!(warnings.is_empty(), "79% must not warn: {warnings:?}");
    }

    #[test]
    fn test_check_resource_limits_over_limit_reports_over_100() {
        let report = report_with_resources(1_500, 0, 0, 0, 0, 0);
        let config = NetworkConfig {
            tx_max_instructions: Some(1_000),
            ..NetworkConfig::default()
        };
        let warnings = check_resource_limits(&report, &config);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].percent, 150);
    }

    #[test]
    fn test_check_resource_limits_skips_unknown_limit() {
        let report = report_with_resources(u64::MAX, 100, 100, 100, 100, 1_000);
        // Only the tx_size limit is known; the others are unknown and skipped.
        let config = NetworkConfig {
            tx_max_size: Some(1_000),
            ..NetworkConfig::default()
        };
        let warnings = check_resource_limits(&report, &config);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].resource, "tx_size");
    }

    #[test]
    fn test_network_config_from_snapshot_reads_all_sections() {
        use crate::config_snapshot::model::{
            ConfigSnapshot, ContractBandwidthV0, ContractComputeV0, ContractLedgerCostV0,
        };
        let snapshot = ConfigSnapshot {
            network: "testnet".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            ledger: 1,
            contract_compute: Some(ContractComputeV0 {
                ledger_max_instructions: 10,
                tx_max_instructions: 400_000_000,
                fee_rate_per_instructions_increment: 7,
                tx_memory_limit: 41_943_040,
            }),
            contract_ledger_cost: Some(ContractLedgerCostV0 {
                ledger_max_disk_read_entries: 100,
                ledger_max_disk_read_bytes: 200,
                ledger_max_write_ledger_entries: 300,
                ledger_max_write_bytes: 400,
                tx_max_disk_read_entries: 100,
                tx_max_disk_read_bytes: 200,
                tx_max_write_ledger_entries: 300,
                tx_max_write_bytes: 400,
                fee_disk_read_ledger_entry: 1,
                fee_write_ledger_entry: 1,
                fee_disk_read1_kb: 1,
                soroban_state_target_size_bytes: 1,
                rent_fee1_kb_soroban_state_size_low: 1,
                rent_fee1_kb_soroban_state_size_high: 1,
                soroban_state_rent_fee_growth_factor: 1,
            }),
            contract_historical_data: None,
            contract_events: None,
            contract_bandwidth: Some(ContractBandwidthV0 {
                ledger_max_txs_size_bytes: 1,
                tx_max_size_bytes: 132_096,
                fee_tx_size1_kb: 1,
            }),
            state_archival: None,
        };
        let config = NetworkConfig::from_snapshot(&snapshot);
        assert_eq!(config.tx_max_instructions, Some(400_000_000));
        assert_eq!(config.tx_max_read_entries, Some(100));
        assert_eq!(config.tx_max_write_entries, Some(300));
        assert_eq!(config.tx_max_read_bytes, Some(200));
        assert_eq!(config.tx_max_write_bytes, Some(400));
        assert_eq!(config.tx_max_size, Some(132_096));
    }

    #[test]
    fn test_network_config_from_empty_snapshot_is_all_none() {
        use crate::config_snapshot::model::ConfigSnapshot;
        let snapshot = ConfigSnapshot {
            network: "testnet".to_string(),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            ledger: 1,
            contract_compute: None,
            contract_ledger_cost: None,
            contract_historical_data: None,
            contract_events: None,
            contract_bandwidth: None,
            state_archival: None,
        };
        let config = NetworkConfig::from_snapshot(&snapshot);
        assert_eq!(config, NetworkConfig::default());
    }

    #[test]
    fn test_format_resource_warnings_empty_and_nonempty() {
        assert_eq!(format_resource_warnings(&[]), "");
        let report = report_with_resources(900, 0, 0, 0, 0, 0);
        let config = NetworkConfig {
            tx_max_instructions: Some(1_000),
            ..NetworkConfig::default()
        };
        let warnings = check_resource_limits(&report, &config);
        let out = format_resource_warnings(&warnings);
        assert!(out.contains("Resource limit warnings:"));
        assert!(out.contains("CPU instructions at 90%"));
    }

    // ── Historical trend (#321) ─────────────────────────────────────

    #[test]
    fn test_cost_trend_for_delta() {
        assert_eq!(CostTrend::for_delta(5), CostTrend::Regression);
        assert_eq!(CostTrend::for_delta(-5), CostTrend::Improvement);
        assert_eq!(CostTrend::for_delta(0), CostTrend::Unchanged);
    }

    #[test]
    fn test_build_history_entries_computes_delta_vs_current() {
        let runs = vec![
            HistoricalRun {
                timestamp: "2026-01-02T00:00:00Z".to_string(),
                ledger: 200,
                cpu_instructions: 500,
                total_stroops: 20_000,
            },
            HistoricalRun {
                timestamp: "2026-01-01T00:00:00Z".to_string(),
                ledger: 100,
                cpu_instructions: 400,
                total_stroops: 10_000,
            },
        ];
        let entries = build_history_entries(15_000, 7, &runs);
        assert_eq!(entries.len(), 2);
        // newest first, higher than current -> regression
        assert_eq!(entries[0].delta_stroops, 5_000);
        assert_eq!(entries[0].trend, CostTrend::Regression);
        assert_eq!(entries[0].total_xlm, "0.0020000");
        // older, lower than current -> improvement
        assert_eq!(entries[1].delta_stroops, -5_000);
        assert_eq!(entries[1].trend, CostTrend::Improvement);
    }

    #[test]
    fn test_format_cost_history_empty_and_nonempty() {
        assert_eq!(format_cost_history(&[]), "");
        let entries = build_history_entries(
            15_000,
            7,
            &[HistoricalRun {
                timestamp: "2026-01-02T00:00:00Z".to_string(),
                ledger: 200,
                cpu_instructions: 500,
                total_stroops: 20_000,
            }],
        );
        let out = format_cost_history(&entries);
        assert!(out.contains("Cost history"));
        assert!(out.contains("200"));
        assert!(out.contains("+5000"));
    }
}
