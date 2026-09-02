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
    /// Aggregate latency/fee statistics when this report is the representative
    /// output of an `estimate --repeat N` benchmark. `None` for a single run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark: Option<BenchmarkSummary>,
}

/// Aggregate benchmark statistics over multiple runs of one simulation.
///
/// Computed from per-run `(rpc_latency_ms, total_fee_stroops)` samples by
/// `estimate --repeat N` so developers can judge RPC latency and fee
/// stability when benchmarking a contract invocation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BenchmarkSummary {
    /// Number of simulation runs the benchmark collected.
    pub runs: u64,
    /// Fastest RPC round-trip across runs (ms).
    pub min_latency_ms: u64,
    /// Slowest RPC round-trip across runs (ms).
    pub max_latency_ms: u64,
    /// Mean RPC round-trip across runs (ms).
    pub avg_latency_ms: u64,
    /// 95th-percentile RPC round-trip across runs (ms), nearest-rank.
    pub p95_latency_ms: u64,
    /// Lowest total fee across runs (stroops).
    pub min_fee_stroops: i64,
    /// Highest total fee across runs (stroops).
    pub max_fee_stroops: i64,
    /// Mean total fee across runs (stroops).
    pub avg_fee_stroops: i64,
    /// Population variance of the total fee across runs (stroops²).
    pub fee_variance: f64,
}

/// Compute a benchmark summary from per-run `(latency_ms, fee_stroops)` samples.
///
/// Returns a zeroed summary for an empty sample set (defensive; callers pass
/// at least one run). Latency percentiles use the nearest-rank method, the
/// fee variance is the population variance in stroops², and the fee average
/// uses integer division exactly like `fee_calc::fee_range`.
///
/// # Network calls
/// None — pure computation.
#[must_use]
pub fn compute_benchmark_summary(samples: &[(u64, i64)]) -> BenchmarkSummary {
    let runs = u64::try_from(samples.len()).unwrap_or(u64::MAX);
    if samples.is_empty() {
        return BenchmarkSummary {
            runs: 0,
            min_latency_ms: 0,
            max_latency_ms: 0,
            avg_latency_ms: 0,
            p95_latency_ms: 0,
            min_fee_stroops: 0,
            max_fee_stroops: 0,
            avg_fee_stroops: 0,
            fee_variance: 0.0,
        };
    }

    let mut latencies: Vec<u64> = samples.iter().map(|(latency, _)| *latency).collect();
    latencies.sort_unstable();
    let mut fees: Vec<i64> = samples.iter().map(|(_, fee)| *fee).collect();
    fees.sort_unstable();

    let latency_sum: u128 = latencies.iter().map(|l| u128::from(*l)).sum();
    let fee_sum: i128 = fees.iter().map(|f| i128::from(*f)).sum();
    let count = samples.len();

    // Nearest-rank 95th percentile: the ceiling of 0.95 * N, 1-indexed.
    let rank = (count as f64 * 0.95).ceil() as usize;
    let p95_latency_ms = latencies[rank.saturating_sub(1).min(count - 1)];

    let mean_fee = (fee_sum / i128::from(count as u64)) as i64;
    let mean_fee_f64 = mean_fee as f64;
    let fee_variance = if count > 1 {
        let sum_sq: f64 = fees
            .iter()
            .map(|f| {
                let diff = *f as f64 - mean_fee_f64;
                diff * diff
            })
            .sum();
        sum_sq / count as f64
    } else {
        0.0
    };

    BenchmarkSummary {
        runs,
        min_latency_ms: latencies[0],
        max_latency_ms: latencies[count - 1],
        avg_latency_ms: (latency_sum / u128::from(count as u64)) as u64,
        p95_latency_ms,
        min_fee_stroops: fees[0],
        max_fee_stroops: fees[count - 1],
        avg_fee_stroops: mean_fee,
        fee_variance,
    }
}

/// Render a benchmark summary as a human-readable block.
///
/// Always emits a header followed by one latency line and one fee line.
#[must_use]
pub fn format_benchmark_summary(benchmark: &BenchmarkSummary) -> String {
    let mut out = String::new();
    out.push_str("\nBenchmark Summary:\n");
    out.push_str(&format!("  Runs:                    {}\n", benchmark.runs));
    out.push_str(&format!(
        "  RPC latency (ms):        min {} | max {} | avg {} | p95 {}\n",
        benchmark.min_latency_ms,
        benchmark.max_latency_ms,
        benchmark.avg_latency_ms,
        benchmark.p95_latency_ms,
    ));
    out.push_str(&format!(
        "  Total fee (stroops):     min {} | max {} | avg {} | variance {:.2}\n",
        benchmark.min_fee_stroops,
        benchmark.max_fee_stroops,
        benchmark.avg_fee_stroops,
        benchmark.fee_variance,
    ));
    out
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

    // ── Benchmark summary ─────────────────────────────────────────────

    #[test]
    fn test_benchmark_summary_computes_latency_stats() {
        let summary = compute_benchmark_summary(&[
            (500, 5_000),
            (100, 1_000),
            (300, 3_000),
            (400, 4_000),
            (200, 2_000),
        ]);

        assert_eq!(summary.runs, 5);
        assert_eq!(summary.min_latency_ms, 100);
        assert_eq!(summary.max_latency_ms, 500);
        assert_eq!(summary.avg_latency_ms, 300);
        // nearest-rank: ceil(0.95 * 5) = 5 → the 5th (max) latency.
        assert_eq!(summary.p95_latency_ms, 500);
    }

    #[test]
    fn test_benchmark_summary_computes_fee_stats() {
        let summary = compute_benchmark_summary(&[
            (100, 1_000),
            (200, 2_000),
            (300, 3_000),
            (400, 4_000),
            (500, 5_000),
        ]);

        assert_eq!(summary.min_fee_stroops, 1_000);
        assert_eq!(summary.max_fee_stroops, 5_000);
        assert_eq!(summary.avg_fee_stroops, 3_000);
        // population variance of [1000..5000] around mean 3000:
        // (4e6 + 1e6 + 0 + 1e6 + 4e6) / 5 = 2e6.
        assert!((summary.fee_variance - 2_000_000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_benchmark_summary_p95_nearest_rank() {
        // 20 samples 1..=20 → 95th percentile is the 19th value.
        let samples: Vec<(u64, i64)> = (1..=20).map(|i| (i, i as i64)).collect();
        let summary = compute_benchmark_summary(&samples);
        assert_eq!(summary.p95_latency_ms, 19);

        // 1 sample → p95 equals the only value.
        let single = compute_benchmark_summary(&[(77, 42)]);
        assert_eq!(single.p95_latency_ms, 77);
        assert!(single.fee_variance.abs() < f64::EPSILON);
        assert_eq!(single.min_fee_stroops, 42);
        assert_eq!(single.max_fee_stroops, 42);
    }

    #[test]
    fn test_benchmark_summary_empty_is_zeroed() {
        let summary = compute_benchmark_summary(&[]);
        assert_eq!(summary.runs, 0);
        assert_eq!(summary.min_latency_ms, 0);
        assert_eq!(summary.max_fee_stroops, 0);
        assert!(summary.fee_variance.abs() < f64::EPSILON);
    }

    #[test]
    fn test_format_benchmark_summary_renders_lines() {
        let summary = compute_benchmark_summary(&[(100, 1_000), (300, 3_000)]);
        let out = format_benchmark_summary(&summary);
        assert!(out.contains("Benchmark Summary:"));
        assert!(out.contains("Runs:                    2"));
        assert!(out.contains("RPC latency (ms):        min 100 | max 300 | avg 200 | p95 300"));
        assert!(out.contains(
            "Total fee (stroops):     min 1000 | max 3000 | avg 2000 | variance 1000000.00"
        ));
    }
}
