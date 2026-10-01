use comfy_table::{Cell, CellAlignment, Table};

use crate::report::fee_calc::{FeeBreakdown, FeeRates};
use crate::wasm::parser::{ContractMeta, format_contract_meta};

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
    /// Optional batch invocation cost projections.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub projections: Option<Vec<CostProjection>>,
    /// Contract metadata parsed from the WASM `contractmeta` custom
    /// section. Empty (and rendered as absent) when the binary carries no
    /// decodable section; deserialized as empty from older payloads.
    #[serde(default)]
    pub contract_meta: ContractMeta,
    /// Aggregated statistics from `estimate --repeat N`. Absent (`None`) for a
    /// normal single-run estimate, so the default report serializes exactly as
    /// before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub benchmark: Option<BenchmarkSummary>,
}

/// A cost projection for a specific batch invocation count.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CostProjection {
    /// Number of simulated contract invocations.
    pub invocations: u64,
    /// Projected total fee in stroops (integer arithmetic).
    pub total_stroops: i64,
    /// Projected total fee in XLM.
    pub total_xlm: String,
    /// Projected fee in USD, if an XLM price was available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usd: Option<f64>,
}

/// Aggregated statistics across the repeated simulations of `estimate --repeat
/// N` (#48).
///
/// Latency figures are the RPC round-trip of each `simulateTransaction` call,
/// in milliseconds; fee figures are the total fee each run reported, in
/// stroops. The percentile is a **nearest-rank** p95, and the variance is a
/// population variance computed with integer arithmetic (no floating point).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BenchmarkSummary {
    /// Number of runs the statistics were computed over.
    pub runs: usize,
    /// Lowest RPC latency, in milliseconds.
    pub min_latency_ms: u64,
    /// Highest RPC latency, in milliseconds.
    pub max_latency_ms: u64,
    /// Arithmetic mean RPC latency, in milliseconds (integer division).
    pub avg_latency_ms: u64,
    /// Nearest-rank 95th-percentile RPC latency, in milliseconds.
    pub p95_latency_ms: u64,
    /// Lowest total fee, in stroops.
    pub min_fee_stroops: i64,
    /// Highest total fee, in stroops.
    pub max_fee_stroops: i64,
    /// Arithmetic mean total fee, in stroops (integer division, truncated).
    pub avg_fee_stroops: i64,
    /// Population variance of the total fees, in squared stroops (floored).
    pub fee_variance_stroops: i64,
}

/// Compute [`BenchmarkSummary`] from parallel latency (ms) and fee (stroops)
/// samples collected by `estimate --repeat N`.
///
/// The `runs` count is the shorter of the two slices; when either is empty,
/// every statistic is zero and `runs` is `0`. The p95 uses the nearest-rank
/// method over the ascending latency samples.
///
/// # Network calls
/// None — pure computation.
#[must_use]
pub fn compute_benchmark_summary(latencies_ms: &[u64], fees_stroops: &[i64]) -> BenchmarkSummary {
    let runs = latencies_ms.len().min(fees_stroops.len());
    if runs == 0 {
        return BenchmarkSummary {
            runs: 0,
            min_latency_ms: 0,
            max_latency_ms: 0,
            avg_latency_ms: 0,
            p95_latency_ms: 0,
            min_fee_stroops: 0,
            max_fee_stroops: 0,
            avg_fee_stroops: 0,
            fee_variance_stroops: 0,
        };
    }

    let mut sorted_latencies: Vec<u64> = latencies_ms[..runs].to_vec();
    sorted_latencies.sort_unstable();
    let min_latency_ms = sorted_latencies[0];
    let max_latency_ms = sorted_latencies[runs - 1];
    let sum_latency: u128 = sorted_latencies.iter().map(|&v| u128::from(v)).sum();
    let avg_latency_ms = (sum_latency / runs as u128) as u64;

    // Nearest-rank percentile: rank = ceil(0.95 * n), index = rank - 1.
    // Computed with integer arithmetic so there is no rounding ambiguity.
    let rank = (95 * runs).div_ceil(100).max(1);
    let p95_latency_ms = sorted_latencies[(rank - 1).min(runs - 1)];

    let mut min_fee = i64::MAX;
    let mut max_fee = i64::MIN;
    let mut sum_fee: i128 = 0;
    let mut sum_fee_sq: i128 = 0;
    for &fee in &fees_stroops[..runs] {
        min_fee = min_fee.min(fee);
        max_fee = max_fee.max(fee);
        sum_fee += i128::from(fee);
        sum_fee_sq += i128::from(fee) * i128::from(fee);
    }

    let n = runs as i128;
    let avg_fee_stroops = (sum_fee / n) as i64;
    // Population variance = (n·Σx² − (Σx)²) / n², exact integer arithmetic.
    let variance_numerator = (n * sum_fee_sq - sum_fee * sum_fee).max(0);
    let fee_variance_stroops = (variance_numerator / (n * n)) as i64;

    BenchmarkSummary {
        runs,
        min_latency_ms,
        max_latency_ms,
        avg_latency_ms,
        p95_latency_ms,
        min_fee_stroops: min_fee,
        max_fee_stroops: max_fee,
        avg_fee_stroops,
        fee_variance_stroops,
    }
}

/// Render a [`BenchmarkSummary`] as a human-readable block, matching the style
/// of the projection and distribution boxes.
#[must_use]
pub fn format_benchmark_table(benchmark: &BenchmarkSummary) -> String {
    let mut output = String::from("\nBenchmark Summary:\n\n");
    let mut table = Table::new();
    if crate::cli::should_colorize() {
        table.enforce_styling();
    } else {
        table.force_no_tty();
    }

    table.set_header(vec!["Metric", "Value"]);
    table.add_row(vec!["Runs", &benchmark.runs.to_string()]);
    table.add_row(vec![
        "Latency min (ms)",
        &benchmark.min_latency_ms.to_string(),
    ]);
    table.add_row(vec![
        "Latency max (ms)",
        &benchmark.max_latency_ms.to_string(),
    ]);
    table.add_row(vec![
        "Latency avg (ms)",
        &benchmark.avg_latency_ms.to_string(),
    ]);
    table.add_row(vec![
        "Latency p95 (ms)",
        &benchmark.p95_latency_ms.to_string(),
    ]);
    table.add_row(vec![
        "Fee min (stroops)",
        &format_thousands(benchmark.min_fee_stroops),
    ]);
    table.add_row(vec![
        "Fee max (stroops)",
        &format_thousands(benchmark.max_fee_stroops),
    ]);
    table.add_row(vec![
        "Fee avg (stroops)",
        &format_thousands(benchmark.avg_fee_stroops),
    ]);
    table.add_row(vec![
        "Fee variance (stroops²)",
        &format_thousands(benchmark.fee_variance_stroops),
    ]);

    output.push_str(&table.to_string());
    output.push('\n');
    output
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
#[allow(clippy::too_many_lines)] // benchmark block pushed this just over 100
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

    // Contract metadata from the WASM `contractmeta` section: present or
    // absent, always rendered so the reader knows the section was checked.
    output.push_str(&format_contract_meta(&report.contract_meta));
    output.push_str("\n\n");

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

    if let Some(ref projections) = report.projections {
        output.push_str(&format_projections_table(projections));
    }

    if let Some(ref benchmark) = report.benchmark {
        output.push_str(&format_benchmark_table(benchmark));
    }

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

/// Parse a comma-separated list of invocation counts for cost projections.
///
/// Ensures each count is non-zero, unique, fits within `i64::MAX` so it can be
/// multiplied by stroop fees safely, and maintains the order supplied by the user.
pub fn parse_projection_counts(input: &str) -> crate::error::AppResult<Vec<u64>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(crate::error::AppError::FeeCalc(
            "projection counts cannot be empty".to_string(),
        ));
    }

    let mut counts = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for raw_part in trimmed.split(',') {
        let part = raw_part.trim();
        if part.is_empty() {
            return Err(crate::error::AppError::FeeCalc(
                "projection counts contain an empty value".to_string(),
            ));
        }

        if part.starts_with('-') {
            return Err(crate::error::AppError::FeeCalc(format!(
                "invalid projection count '{part}': count cannot be negative"
            )));
        }

        let count: u64 = part.parse().map_err(|_| {
            crate::error::AppError::FeeCalc(format!(
                "invalid projection count '{part}': must be a positive integer"
            ))
        })?;

        if count == 0 {
            return Err(crate::error::AppError::FeeCalc(
                "invalid projection count '0': count must be greater than zero".to_string(),
            ));
        }

        if count > i64::MAX as u64 {
            return Err(crate::error::AppError::FeeCalc(format!(
                "invalid projection count '{part}': value exceeds maximum supported limit ({})",
                i64::MAX
            )));
        }

        if !seen.insert(count) {
            return Err(crate::error::AppError::FeeCalc(format!(
                "duplicate projection count: {count}"
            )));
        }

        counts.push(count);
    }

    Ok(counts)
}

/// Calculate cost projections for given invocation counts.
///
/// Performs checked multiplication (`total_fee_stroops * count`) to prevent
/// integer overflow. Stroop fees remain pure integers (`i64`), and XLM is
/// formatted using the standard [`crate::report::fee_calc::stroops_to_xlm`] conversion.
pub fn calculate_projections(
    total_fee_stroops: i64,
    counts: &[u64],
    precision: u32,
    xlm_usd_price: Option<f64>,
) -> crate::error::AppResult<Vec<CostProjection>> {
    let mut projections = Vec::with_capacity(counts.len());

    for &count in counts {
        let count_i64 = i64::try_from(count).map_err(|_| {
            crate::error::AppError::FeeCalc(format!(
                "projection count {count} exceeds maximum supported integer value"
            ))
        })?;

        let projected_stroops = total_fee_stroops
            .checked_mul(count_i64)
            .ok_or_else(|| {
                crate::error::AppError::FeeCalc(format!(
                    "cost projection overflow: {total_fee_stroops} stroops * {count} invocations exceeds integer bounds"
                ))
            })?;

        let total_xlm = crate::report::fee_calc::stroops_to_xlm(projected_stroops, precision);

        let usd = xlm_usd_price.map(|price| (projected_stroops as f64 / 10_000_000.0) * price);

        projections.push(CostProjection {
            invocations: count,
            total_stroops: projected_stroops,
            total_xlm,
            usd,
        });
    }

    Ok(projections)
}

/// Formats a list of cost projections into a human-readable table.
#[must_use]
pub fn format_projections_table(projections: &[CostProjection]) -> String {
    if projections.is_empty() {
        return String::new();
    }

    let mut output = String::from("\nCost Projections:\n\n");
    let mut table = Table::new();
    if crate::cli::should_colorize() {
        table.enforce_styling();
    } else {
        table.force_no_tty();
    }

    table.set_header(vec!["Invocations", "Total Stroops", "Total XLM", "USD"]);
    for p in projections {
        let usd_str = p
            .usd
            .map(|u| format!("${u:.2}"))
            .unwrap_or_else(|| "-".to_string());
        table.add_row(vec![
            Cell::new(format_thousands_u64(p.invocations)).set_alignment(CellAlignment::Right),
            Cell::new(format_thousands(p.total_stroops)).set_alignment(CellAlignment::Right),
            Cell::new(&p.total_xlm).set_alignment(CellAlignment::Right),
            Cell::new(usd_str).set_alignment(CellAlignment::Right),
        ]);
    }

    output.push_str(&table.to_string());
    output.push('\n');
    output
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
            projections: None,
            contract_meta: ContractMeta::default(),
            benchmark: None,
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
            projections: None,
            contract_meta: ContractMeta::default(),
            benchmark: None,
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

    #[test]
    fn test_parse_projection_counts_valid_and_ordered() {
        let counts = parse_projection_counts("100, 1000, 10000").expect("valid counts");
        assert_eq!(counts, vec![100, 1000, 10000]);

        // Order preserved
        let reversed = parse_projection_counts("10000,100,1000").expect("valid counts");
        assert_eq!(reversed, vec![10000, 100, 1000]);
    }

    #[test]
    fn test_parse_projection_counts_invalid() {
        assert!(parse_projection_counts("").is_err());
        assert!(parse_projection_counts("   ").is_err());
        assert!(parse_projection_counts("100,,1000").is_err());
        assert!(parse_projection_counts("abc").is_err());
        assert!(parse_projection_counts("100,abc,1000").is_err());
        assert!(parse_projection_counts("0").is_err());
        assert!(parse_projection_counts("100,0").is_err());
        assert!(parse_projection_counts("-5").is_err());
        assert!(parse_projection_counts("100,-5").is_err());
        assert!(parse_projection_counts("100,1000,100").is_err());
        let overflow_count = format!("{}0", u64::MAX);
        assert!(parse_projection_counts(&overflow_count).is_err());
    }

    #[test]
    fn test_calculate_projections_multiplication() {
        let per_invocation_fee: i64 = 15_527;
        let counts = [100, 1_000, 10_000];
        let projections = calculate_projections(per_invocation_fee, &counts, 7, None)
            .expect("calculation should succeed");

        assert_eq!(projections.len(), 3);
        assert_eq!(projections[0].invocations, 100);
        assert_eq!(projections[0].total_stroops, 1_552_700);
        assert_eq!(projections[0].total_xlm, "0.1552700");
        assert_eq!(projections[0].usd, None);

        assert_eq!(projections[1].invocations, 1_000);
        assert_eq!(projections[1].total_stroops, 15_527_000);
        assert_eq!(projections[1].total_xlm, "1.5527000");

        assert_eq!(projections[2].invocations, 10_000);
        assert_eq!(projections[2].total_stroops, 155_270_000);
        assert_eq!(projections[2].total_xlm, "15.5270000");
    }

    #[test]
    fn test_calculate_projections_large_no_overflow() {
        let fee: i64 = 10_000;
        let counts = [10_000_000_000u64];
        let projections =
            calculate_projections(fee, &counts, 7, None).expect("large multiplication succeeds");
        assert_eq!(projections[0].total_stroops, 100_000_000_000_000);
    }

    #[test]
    fn test_calculate_projections_overflow_error() {
        let fee: i64 = i64::MAX / 2;
        let counts = [3u64];
        let err = calculate_projections(fee, &counts, 7, None).unwrap_err();
        match err {
            crate::error::AppError::FeeCalc(msg) => {
                assert!(msg.contains('3'), "error must identify count: {msg}");
                assert!(
                    msg.contains("overflow"),
                    "error must explain overflow: {msg}"
                );
            }
            other => panic!("expected FeeCalc error, got: {other:?}"),
        }
    }

    #[test]
    fn test_calculate_projections_usd_available() {
        let fee: i64 = 10_000_000; // 1 XLM
        let counts = [100];
        let projections =
            calculate_projections(fee, &counts, 7, Some(0.12)).expect("calculation succeeds");
        assert_eq!(projections[0].usd, Some(12.0));
    }

    #[test]
    fn test_calculate_projections_usd_unavailable() {
        let fee: i64 = 10_000_000;
        let counts = [100];
        let projections =
            calculate_projections(fee, &counts, 7, None).expect("calculation succeeds");
        assert_eq!(projections[0].usd, None);
    }

    #[test]
    fn test_format_thousands() {
        assert_eq!(format_thousands(0), "0");
        assert_eq!(format_thousands(999), "999");
        assert_eq!(format_thousands(1_000), "1,000");
        assert_eq!(format_thousands(10_000), "10,000");
        assert_eq!(format_thousands(1_234_567), "1,234,567");

        assert_eq!(format_thousands(-1_234_567), "-1,234,567");
        assert_eq!(format_thousands(500), "500");
    }

    #[test]
    fn test_format_projections_table() {
        let projections = vec![
            CostProjection {
                invocations: 100,
                total_stroops: 1_552_700,
                total_xlm: "0.1552700".to_string(),
                usd: None,
            },
            CostProjection {
                invocations: 1_000,
                total_stroops: 15_527_000,
                total_xlm: "1.5527000".to_string(),
                usd: Some(1.86),
            },
        ];

        let table = format_projections_table(&projections);
        assert!(table.contains("Cost Projections:"));
        assert!(table.contains("Invocations"));
        assert!(table.contains("Total Stroops"));
        assert!(table.contains("Total XLM"));
        assert!(table.contains("USD"));
        assert!(table.contains("100"));
        assert!(table.contains("1,552,700"));
        assert!(table.contains("0.1552700"));
        assert!(table.contains('-'));
        assert!(table.contains("1,000"));
        assert!(table.contains("15,527,000"));
        assert!(table.contains("1.5527000"));
        assert!(table.contains("$1.86"));
    }

    #[test]
    fn test_format_report_table_and_json_projections() {
        let mut report = report_with_rates(sample_rates());
        assert!(!format_report_table(&report).contains("Cost Projections:"));
        assert!(!format_report_json(&report).contains("\"projections\""));

        report.projections = Some(vec![CostProjection {
            invocations: 100,
            total_stroops: 1_552_700,
            total_xlm: "0.1552700".to_string(),
            usd: None,
        }]);

        let table_out = format_report_table(&report);
        assert!(table_out.contains("Cost Projections:"));
        assert!(table_out.contains("100"));
        assert!(table_out.contains("1,552,700"));

        let json_out = format_report_json(&report);
        let parsed: serde_json::Value = serde_json::from_str(&json_out).expect("valid json");
        assert!(parsed["projections"].is_array());
        assert_eq!(parsed["projections"][0]["invocations"], 100);
        assert_eq!(parsed["projections"][0]["total_stroops"], 1_552_700);
        assert_eq!(parsed["projections"][0]["total_xlm"], "0.1552700");
        assert!(parsed["projections"][0].get("usd").is_none());
    }

    // ── Benchmark summary (#48) ─────────────────────────────────────────

    #[test]
    fn test_compute_benchmark_summary_empty() {
        let s = compute_benchmark_summary(&[], &[]);
        assert_eq!(s.runs, 0);
        assert_eq!(s.min_latency_ms, 0);
        assert_eq!(s.p95_latency_ms, 0);
        assert_eq!(s.fee_variance_stroops, 0);
    }

    #[test]
    fn test_compute_benchmark_summary_single() {
        let s = compute_benchmark_summary(&[120], &[15_527]);
        assert_eq!(s.runs, 1);
        assert_eq!(s.min_latency_ms, 120);
        assert_eq!(s.max_latency_ms, 120);
        assert_eq!(s.avg_latency_ms, 120);
        assert_eq!(s.p95_latency_ms, 120);
        assert_eq!(s.min_fee_stroops, 15_527);
        assert_eq!(s.max_fee_stroops, 15_527);
        assert_eq!(s.avg_fee_stroops, 15_527);
        assert_eq!(s.fee_variance_stroops, 0);
    }

    #[test]
    fn test_compute_benchmark_summary_latency_stats_and_p95() {
        // 20 samples 1..=20 -> nearest-rank p95 = ceil(0.95*20) = 19 -> 19.
        let latencies: Vec<u64> = (1..=20).collect();
        let fees: Vec<i64> = vec![100; 20];
        let s = compute_benchmark_summary(&latencies, &fees);
        assert_eq!(s.runs, 20);
        assert_eq!(s.min_latency_ms, 1);
        assert_eq!(s.max_latency_ms, 20);
        assert_eq!(s.avg_latency_ms, 10); // 210 / 20
        assert_eq!(s.p95_latency_ms, 19);
    }

    #[test]
    fn test_compute_benchmark_summary_fee_population_variance() {
        // Fees 100 and 200: mean 150; population variance (50² + 50²)/2 = 2500.
        let s = compute_benchmark_summary(&[10, 20], &[100, 200]);
        assert_eq!(s.min_fee_stroops, 100);
        assert_eq!(s.max_fee_stroops, 200);
        assert_eq!(s.avg_fee_stroops, 150);
        assert_eq!(s.fee_variance_stroops, 2_500);
    }

    #[test]
    fn test_compute_benchmark_summary_mismatched_slices() {
        let s = compute_benchmark_summary(&[1, 2, 3], &[10, 20]);
        assert_eq!(s.runs, 2);
        assert_eq!(s.max_latency_ms, 2);
    }

    #[test]
    fn test_format_benchmark_table_contents() {
        let s = compute_benchmark_summary(&[100, 120, 140], &[10, 20, 30]);
        let out = format_benchmark_table(&s);
        assert!(out.contains("Benchmark Summary:"));
        assert!(out.contains("Runs"));
        assert!(out.contains("Latency p95 (ms)"));
        assert!(out.contains("Fee variance (stroops²)"));
        assert!(out.contains("120")); // avg latency
    }

    #[test]
    fn test_report_table_includes_benchmark_only_when_present() {
        let mut report = report_with_rates(sample_rates());
        assert!(!format_report_table(&report).contains("Benchmark Summary:"));
        report.benchmark = Some(compute_benchmark_summary(&[100, 200], &[10, 20]));
        assert!(format_report_table(&report).contains("Benchmark Summary:"));
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
        assert_eq!(delta.fee_stroops.absolute, 5_527);
        assert!(delta.read_entries.is_none());
        assert!(delta.write_entries.is_none());
        assert!(!delta.format_markdown().contains("Read Entries"));
    }

    #[test]
    fn test_cost_delta_with_previous_io_includes_entry_deltas() {
        let report = report_with_rates(sample_rates());
        let delta = CostDelta::compute(532_502, 0, 15_527, Some((3, 0)), &report);

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
        assert_eq!(value["fee_stroops"]["absolute"], 15_524);
    }

    #[test]
    fn test_cost_delta_csv_has_header_and_one_row_per_metric() {
        let report = report_with_rates(sample_rates());
        let delta = CostDelta::compute(532_502, 0, 15_527, Some((1, 1)), &report);
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
