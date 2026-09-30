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
    /// Size of the compiled WASM artifact in bytes. Reported in the
    /// side-by-side diff so a build-size regression is visible next to the
    /// fee delta. `0` when the caller did not supply it.
    #[serde(default)]
    pub wasm_size: u64,
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
    ///
    /// Serialized as `ledger_sequence` in JSON output (issue #329) while the
    /// Rust field keeps its shorter name for source compatibility.
    #[serde(rename = "ledger_sequence")]
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

/// Aggregate fee and CPU distribution across a batch of simulations.
///
/// Computed over the **successfully estimated** functions only; skipped and
/// errored functions carry no fee or CPU figure and are excluded by the
/// caller. Every field is an integer: fees in stroops, CPU in instructions.
/// The standard deviation is derived from an integer population variance and
/// an integer square root, so no floating point is used anywhere (issue #328).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FeeDistribution {
    /// Number of simulations the statistics were computed over.
    pub function_count: usize,
    /// Lowest fee, in stroops.
    pub min_fee_stroops: i64,
    /// Highest fee, in stroops.
    pub max_fee_stroops: i64,
    /// Arithmetic mean fee, in stroops (integer division, truncated).
    pub mean_fee_stroops: i64,
    /// Median fee, in stroops. For an even count this is the truncated mean of
    /// the two central values.
    pub median_fee_stroops: i64,
    /// Population standard deviation of the fees, in stroops (floored).
    pub std_dev_fee_stroops: i64,
    /// Lowest CPU instruction count.
    pub min_cpu_instructions: u64,
    /// Highest CPU instruction count.
    pub max_cpu_instructions: u64,
    /// Arithmetic mean CPU instruction count (integer division, truncated).
    pub mean_cpu_instructions: u64,
}

impl FeeDistribution {
    /// Compute the distribution from parallel slices of fee (stroops) and CPU
    /// instruction samples.
    ///
    /// This is the shared core behind [`calculate_distribution_stats`]. It is
    /// public so the `estimate-all` CLI can derive the distribution directly
    /// from its per-function records, without first rebuilding full
    /// [`CostReport`]s.
    ///
    /// When either slice is empty — or the slices differ in length, in which
    /// case the shorter length wins — every statistic is zero and
    /// `function_count` is `0`.
    ///
    /// # Network calls
    /// None — pure computation.
    #[must_use]
    pub fn from_samples(fees: &[i64], cpu: &[u64]) -> Self {
        let count = fees.len().min(cpu.len());
        if count == 0 {
            return Self {
                function_count: 0,
                min_fee_stroops: 0,
                max_fee_stroops: 0,
                mean_fee_stroops: 0,
                median_fee_stroops: 0,
                std_dev_fee_stroops: 0,
                min_cpu_instructions: 0,
                max_cpu_instructions: 0,
                mean_cpu_instructions: 0,
            };
        }

        let mut min_fee = i64::MAX;
        let mut max_fee = i64::MIN;
        let mut min_cpu = u64::MAX;
        let mut max_cpu = 0u64;
        // Accumulate in wider integers so a large batch cannot overflow.
        let mut sum_fee: i128 = 0;
        let mut sum_fee_sq: i128 = 0;
        let mut sum_cpu: u128 = 0;
        let mut sorted_fees: Vec<i64> = Vec::with_capacity(count);

        for index in 0..count {
            let fee = fees[index];
            min_fee = min_fee.min(fee);
            max_fee = max_fee.max(fee);
            sum_fee += i128::from(fee);
            sum_fee_sq += i128::from(fee) * i128::from(fee);
            sorted_fees.push(fee);

            let instructions = cpu[index];
            min_cpu = min_cpu.min(instructions);
            max_cpu = max_cpu.max(instructions);
            sum_cpu += u128::from(instructions);
        }

        let n = count as i128;
        let mean_fee = (sum_fee / n) as i64;
        let mean_cpu = (sum_cpu / count as u128) as u64;

        sorted_fees.sort_unstable();
        let median_fee = if count % 2 == 1 {
            sorted_fees[count / 2]
        } else {
            // Sum in `i128` so two near-`i64::MAX` values cannot overflow.
            ((i128::from(sorted_fees[count / 2 - 1]) + i128::from(sorted_fees[count / 2])) / 2)
                as i64
        };

        // Population variance = (n·Σx² − (Σx)²) / n², computed with exact
        // integer arithmetic (the numerator is non-negative by Cauchy–Schwarz).
        let variance_numerator = (n * sum_fee_sq - sum_fee * sum_fee).max(0);
        let variance = variance_numerator / (n * n);
        let std_dev_fee = isqrt_u128(variance as u128) as i64;

        Self {
            function_count: count,
            min_fee_stroops: min_fee,
            max_fee_stroops: max_fee,
            mean_fee_stroops: mean_fee,
            median_fee_stroops: median_fee,
            std_dev_fee_stroops: std_dev_fee,
            min_cpu_instructions: min_cpu,
            max_cpu_instructions: max_cpu,
            mean_cpu_instructions: mean_cpu,
        }
    }
}

/// Integer (floor) square root of a `u128`.
///
/// Uses Newton's method; returns `0` for `0` and `1` for `1`. Keeps the
/// standard-deviation computation free of floating point.
fn isqrt_u128(value: u128) -> u128 {
    if value < 2 {
        return value;
    }
    let mut x = value;
    let mut y = x / 2 + 1;
    while y < x {
        x = y;
        y = u128::midpoint(x, value / x);
    }
    x
}

/// Compute the distribution statistics for a batch of cost reports (issue
/// #328).
///
/// Callers should pass only the successfully simulated reports so that
/// skipped/errored functions do not skew the statistics. Returns an all-zero
/// distribution when `reports` is empty.
///
/// # Network calls
/// None — pure computation.
#[must_use]
pub fn calculate_distribution_stats(reports: &[CostReport]) -> FeeDistribution {
    let fees: Vec<i64> = reports.iter().map(|r| r.fee.total_stroops).collect();
    let cpu: Vec<u64> = reports.iter().map(|r| r.cpu_instructions).collect();
    FeeDistribution::from_samples(&fees, &cpu)
}

/// Insert `,` thousands separators into a run of ASCII digits.
fn group_digits(digits: &str) -> String {
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Format a signed integer with `,` thousands separators.
///
/// Example: `format_thousands(1_234_567)` → `"1,234,567"`.
#[must_use]
pub fn format_thousands(value: i64) -> String {
    let grouped = group_digits(&value.unsigned_abs().to_string());
    if value < 0 {
        format!("-{grouped}")
    } else {
        grouped
    }
}

/// Format an unsigned integer with `,` thousands separators.
#[must_use]
pub fn format_thousands_u64(value: u64) -> String {
    group_digits(&value.to_string())
}

/// Format a ledger sequence number with `,` thousands separators.
///
/// Example: `format_ledger_sequence(1_234_567)` → `"1,234,567"` (issue #329).
#[must_use]
pub fn format_ledger_sequence(ledger: u32) -> String {
    group_digits(&ledger.to_string())
}

/// Render the distribution statistics as a human-readable box for the
/// `estimate-all` summary (issue #328).
///
/// Returns an explanatory line instead of a zeroed box when nothing was
/// successfully estimated, so the section is never silently empty.
#[must_use]
pub fn format_distribution_box(distribution: &FeeDistribution, precision: u32) -> String {
    if distribution.function_count == 0 {
        return "No functions estimated; no fee distribution to report.".to_string();
    }

    let mut out = String::new();
    out.push_str(&format!(
        "\nFee distribution across {} function(s):\n\n",
        distribution.function_count
    ));
    out.push_str("  Fees (stroops):\n");
    out.push_str(&format!(
        "    min     : {}\n",
        format_thousands(distribution.min_fee_stroops)
    ));
    out.push_str(&format!(
        "    max     : {}\n",
        format_thousands(distribution.max_fee_stroops)
    ));
    out.push_str(&format!(
        "    mean    : {} ({})\n",
        format_thousands(distribution.mean_fee_stroops),
        crate::report::fee_calc::stroops_to_xlm(distribution.mean_fee_stroops, precision)
    ));
    out.push_str(&format!(
        "    median  : {}\n",
        format_thousands(distribution.median_fee_stroops)
    ));
    out.push_str(&format!(
        "    std dev : {}\n",
        format_thousands(distribution.std_dev_fee_stroops)
    ));
    out.push_str("\n  CPU instructions:\n");
    out.push_str(&format!(
        "    min     : {}\n",
        format_thousands_u64(distribution.min_cpu_instructions)
    ));
    out.push_str(&format!(
        "    max     : {}\n",
        format_thousands_u64(distribution.max_cpu_instructions)
    ));
    out.push_str(&format!(
        "    mean    : {}\n",
        format_thousands_u64(distribution.mean_cpu_instructions)
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
    output.push_str(&format!(
        "Simulated at ledger sequence: {}\n",
        format_ledger_sequence(report.ledger)
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
            wasm_size: 4_096,
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

    // ── #328: fee / CPU distribution statistics ─────────────────────

    /// A report with the given fee and CPU figures, reusing a fully populated
    /// base report so only the metrics under test vary.
    fn report_with_metrics(fee_stroops: i64, cpu_instructions: u64) -> CostReport {
        let mut report = report_with_rates(sample_rates());
        report.fee.total_stroops = fee_stroops;
        report.cpu_instructions = cpu_instructions;
        report
    }

    #[test]
    fn test_distribution_stats_single_function() {
        let dist = FeeDistribution::from_samples(&[42], &[7]);
        assert_eq!(dist.function_count, 1);
        assert_eq!(dist.min_fee_stroops, 42);
        assert_eq!(dist.max_fee_stroops, 42);
        assert_eq!(dist.mean_fee_stroops, 42);
        assert_eq!(dist.median_fee_stroops, 42);
        assert_eq!(dist.std_dev_fee_stroops, 0);
        assert_eq!(dist.min_cpu_instructions, 7);
        assert_eq!(dist.max_cpu_instructions, 7);
        assert_eq!(dist.mean_cpu_instructions, 7);
    }

    #[test]
    fn test_distribution_stats_identical_fees_have_zero_spread() {
        let dist = FeeDistribution::from_samples(&[500, 500, 500], &[10, 10, 10]);
        assert_eq!(dist.function_count, 3);
        assert_eq!(dist.min_fee_stroops, 500);
        assert_eq!(dist.max_fee_stroops, 500);
        assert_eq!(dist.mean_fee_stroops, 500);
        assert_eq!(dist.median_fee_stroops, 500);
        assert_eq!(dist.std_dev_fee_stroops, 0);
    }

    #[test]
    fn test_distribution_stats_known_values() {
        let dist = FeeDistribution::from_samples(&[100, 200, 300], &[10, 20, 30]);
        assert_eq!(dist.min_fee_stroops, 100);
        assert_eq!(dist.max_fee_stroops, 300);
        assert_eq!(dist.mean_fee_stroops, 200);
        assert_eq!(dist.median_fee_stroops, 200);
        // Population variance = 60000 / 9 = 6666; floor(sqrt(6666)) = 81.
        assert_eq!(dist.std_dev_fee_stroops, 81);
        assert_eq!(dist.min_cpu_instructions, 10);
        assert_eq!(dist.max_cpu_instructions, 30);
        assert_eq!(dist.mean_cpu_instructions, 20);
    }

    #[test]
    fn test_distribution_stats_even_count_median_is_mean_of_middles() {
        let dist = FeeDistribution::from_samples(&[300, 100, 200, 400], &[1, 2, 3, 4]);
        assert_eq!(dist.median_fee_stroops, 250); // (200 + 300) / 2

        let pair = FeeDistribution::from_samples(&[100, 200], &[1, 2]);
        assert_eq!(pair.median_fee_stroops, 150);
        assert_eq!(pair.mean_fee_stroops, 150);
        // variance = (2*50000 - 300^2) / 4 = 2500; sqrt = 50
        assert_eq!(pair.std_dev_fee_stroops, 50);
    }

    #[test]
    fn test_distribution_stats_empty_is_zeroed() {
        let dist = FeeDistribution::from_samples(&[], &[]);
        assert_eq!(dist, FeeDistribution::from_samples(&[], &[]));
        assert_eq!(dist.function_count, 0);
        assert_eq!(dist.min_fee_stroops, 0);
        assert_eq!(dist.max_fee_stroops, 0);
        assert_eq!(dist.mean_fee_stroops, 0);
        assert_eq!(dist.median_fee_stroops, 0);
        assert_eq!(dist.std_dev_fee_stroops, 0);
        assert_eq!(dist.mean_cpu_instructions, 0);
    }

    #[test]
    fn test_calculate_distribution_stats_from_reports() {
        let reports = vec![
            report_with_metrics(5_000, 100),
            report_with_metrics(500_000, 900),
        ];
        let dist = calculate_distribution_stats(&reports);
        assert_eq!(dist.function_count, 2);
        assert_eq!(dist.min_fee_stroops, 5_000);
        assert_eq!(dist.max_fee_stroops, 500_000);
        assert_eq!(dist.mean_fee_stroops, 252_500);
        assert_eq!(dist.median_fee_stroops, 252_500);
        assert_eq!(dist.min_cpu_instructions, 100);
        assert_eq!(dist.max_cpu_instructions, 900);
        assert_eq!(dist.mean_cpu_instructions, 500);
    }

    #[test]
    fn test_calculate_distribution_stats_empty_reports() {
        let dist = calculate_distribution_stats(&[]);
        assert_eq!(dist.function_count, 0);
    }

    #[test]
    fn test_format_distribution_box_empty() {
        let out = format_distribution_box(&FeeDistribution::from_samples(&[], &[]), 7);
        assert_eq!(
            out,
            "No functions estimated; no fee distribution to report."
        );
    }

    #[test]
    fn test_format_distribution_box_contains_all_statistics() {
        let dist = FeeDistribution::from_samples(&[5_000, 500_000], &[100_000, 5_000_000]);
        let out = format_distribution_box(&dist, 7);
        assert!(
            out.contains("Fee distribution across 2 function(s):"),
            "got: {out}"
        );
        assert!(out.contains("min     : 5,000"), "got: {out}");
        assert!(out.contains("max     : 500,000"), "got: {out}");
        assert!(out.contains("mean    : 252,500 (0.0252500)"), "got: {out}");
        assert!(out.contains("median  : 252,500"), "got: {out}");
        assert!(out.contains("std dev : 247,500"), "got: {out}");
        assert!(out.contains("CPU instructions:"), "got: {out}");
        assert!(out.contains("min     : 100,000"), "got: {out}");
        assert!(out.contains("max     : 5,000,000"), "got: {out}");
        assert!(out.contains("mean    : 2,550,000"), "got: {out}");
    }

    #[test]
    fn test_format_thousands_groups_digits() {
        assert_eq!(format_thousands(0), "0");
        assert_eq!(format_thousands(999), "999");
        assert_eq!(format_thousands(1_000), "1,000");
        assert_eq!(format_thousands(1_234_567), "1,234,567");
        assert_eq!(format_thousands(-1_234_567), "-1,234,567");
        assert_eq!(format_thousands_u64(3_894_195), "3,894,195");
        assert_eq!(format_ledger_sequence(1_234_567), "1,234,567");
        assert_eq!(format_ledger_sequence(0), "0");
    }

    // ── #329: ledger sequence in output ────────────────────────────

    #[test]
    fn test_format_report_json_uses_ledger_sequence_field() {
        let report = report_with_rates(sample_rates());
        let parsed: serde_json::Value =
            serde_json::from_str(&format_report_json(&report)).expect("valid json");
        assert_eq!(parsed["ledger_sequence"], 3_894_195);
        assert!(
            parsed.get("ledger").is_none(),
            "legacy `ledger` key should be renamed to `ledger_sequence`: {parsed}"
        );
    }

    #[test]
    fn test_format_report_table_shows_grouped_ledger_sequence() {
        let report = report_with_rates(sample_rates());
        let out = format_report_table(&report);
        assert!(
            out.contains("Simulated at ledger sequence: 3,894,195"),
            "header should show the grouped ledger sequence, got:\n{out}"
        );
    }
}
