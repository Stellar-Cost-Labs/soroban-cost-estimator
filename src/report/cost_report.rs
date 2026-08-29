use comfy_table::Table;

use crate::report::fee_calc::FeeBreakdown;

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
}

/// The delta in resources and fees between target and baseline estimates.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CostReportDelta {
    /// Change in CPU instructions (target - baseline).
    pub cpu_instructions: i64,
    /// Change in memory bytes (target - baseline).
    pub memory_bytes: i64,
    /// Change in transaction size (target - baseline).
    pub tx_size: i32,
    /// Change in read entries (target - baseline).
    pub read_entries: i32,
    /// Change in write entries (target - baseline).
    pub write_entries: i32,
    /// Change in read bytes (target - baseline).
    pub read_bytes: i32,
    /// Change in write bytes (target - baseline).
    pub write_bytes: i32,
    /// Change in total fee in stroops (target - baseline).
    pub fee_total_stroops: i64,
    /// Change in non-refundable fee in stroops (target - baseline).
    pub fee_non_refundable_stroops: i64,
    /// Change in refundable fee in stroops (target - baseline).
    pub fee_refundable_stroops: i64,
}

/// Comparison between two contract estimates (baseline vs target).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CostReportDiff {
    /// Baseline estimate.
    pub baseline: CostReport,
    /// Target estimate.
    pub target: CostReport,
    /// Resource & fee deltas (target - baseline).
    pub delta: CostReportDelta,
}

impl CostReportDiff {
    /// Compute the cost diff between a baseline and target cost report.
    pub fn compute(baseline: &CostReport, target: &CostReport) -> Self {
        let delta = CostReportDelta {
            cpu_instructions: target.cpu_instructions as i64 - baseline.cpu_instructions as i64,
            memory_bytes: target.memory_bytes as i64 - baseline.memory_bytes as i64,
            tx_size: target.tx_size as i32 - baseline.tx_size as i32,
            read_entries: target.read_entries as i32 - baseline.read_entries as i32,
            write_entries: target.write_entries as i32 - baseline.write_entries as i32,
            read_bytes: target.read_bytes as i32 - baseline.read_bytes as i32,
            write_bytes: target.write_bytes as i32 - baseline.write_bytes as i32,
            fee_total_stroops: target.fee.total_stroops - baseline.fee.total_stroops,
            fee_non_refundable_stroops: target.fee.non_refundable_stroops
                - baseline.fee.non_refundable_stroops,
            fee_refundable_stroops: target.fee.refundable_stroops
                - baseline.fee.refundable_stroops,
        };
        Self {
            baseline: baseline.clone(),
            target: target.clone(),
            delta,
        }
    }
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

/// Formats a cost report diff as a human-readable table.
pub fn format_diff_table(diff: &CostReportDiff) -> String {
    let mut output = String::new();

    output.push_str(&format!("Baseline WASM SHA-256: {}\n", diff.baseline.wasm_hash));
    output.push_str(&format!("Target WASM SHA-256:   {}\n", diff.target.wasm_hash));
    output.push_str(&format!("Function: {}\n", diff.target.function));
    output.push_str(&format!(
        "Network: {} (ledger {})\n\n",
        diff.target.network, diff.target.ledger
    ));

    let fmt_delta = |val: i64| -> String {
        if val > 0 {
            format!("+{val}")
        } else {
            val.to_string()
        }
    };

    let mut table = Table::new();
    table.set_header(vec!["Resource", "Baseline", "Target", "Delta"]);

    table.add_row(vec![
        "CPU Instructions",
        &diff.baseline.cpu_instructions.to_string(),
        &diff.target.cpu_instructions.to_string(),
        &fmt_delta(diff.delta.cpu_instructions),
    ]);
    table.add_row(vec![
        "Memory Bytes",
        &diff.baseline.memory_bytes.to_string(),
        &diff.target.memory_bytes.to_string(),
        &fmt_delta(diff.delta.memory_bytes),
    ]);
    table.add_row(vec![
        "Read Entries",
        &diff.baseline.read_entries.to_string(),
        &diff.target.read_entries.to_string(),
        &fmt_delta(diff.delta.read_entries as i64),
    ]);
    table.add_row(vec![
        "Write Entries",
        &diff.baseline.write_entries.to_string(),
        &diff.target.write_entries.to_string(),
        &fmt_delta(diff.delta.write_entries as i64),
    ]);
    table.add_row(vec![
        "Read Bytes",
        &diff.baseline.read_bytes.to_string(),
        &diff.target.read_bytes.to_string(),
        &fmt_delta(diff.delta.read_bytes as i64),
    ]);
    table.add_row(vec![
        "Write Bytes",
        &diff.baseline.write_bytes.to_string(),
        &diff.target.write_bytes.to_string(),
        &fmt_delta(diff.delta.write_bytes as i64),
    ]);
    table.add_row(vec![
        "Transaction Size",
        &diff.baseline.tx_size.to_string(),
        &diff.target.tx_size.to_string(),
        &fmt_delta(diff.delta.tx_size as i64),
    ]);

    output.push_str(&table.to_string());
    output.push('\n');

    output.push_str("\nFee Breakdown Delta:\n");
    output.push_str(&format!(
        "  Non-refundable: {} -> {} stroops ({})\n",
        diff.baseline.fee.non_refundable_stroops,
        diff.target.fee.non_refundable_stroops,
        fmt_delta(diff.delta.fee_non_refundable_stroops),
    ));
    output.push_str(&format!(
        "  Refundable:     {} -> {} stroops ({})\n",
        diff.baseline.fee.refundable_stroops,
        diff.target.fee.refundable_stroops,
        fmt_delta(diff.delta.fee_refundable_stroops),
    ));
    output.push_str(&format!(
        "  Total:          {} -> {} stroops ({})\n",
        diff.baseline.fee.total_stroops,
        diff.target.fee.total_stroops,
        fmt_delta(diff.delta.fee_total_stroops),
    ));

    output
}

/// Formats a cost report diff as pretty-printed JSON.
pub fn format_diff_json(diff: &CostReportDiff) -> String {
    serde_json::to_string_pretty(diff).unwrap_or_else(|_| "{}".to_string())
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

    #[test]
    fn test_cost_report_diff_compute_and_formatting() {
        let baseline = CostReport {
            function: "test".to_string(),
            wasm_hash: "aaa".to_string(),
            cpu_instructions: 100,
            memory_bytes: 10,
            tx_size: 50,
            read_entries: 1,
            write_entries: 1,
            read_bytes: 20,
            write_bytes: 30,
            fee: FeeBreakdown {
                non_refundable_stroops: 100,
                refundable_stroops: 200,
                cpu_fee_stroops: 10,
                storage_fee_stroops: 20,
                bandwidth_fee_stroops: 30,
                total_stroops: 300,
                total_xlm: "0.0000300".to_string(),
            },
            ledger: 1000,
            network: "testnet".to_string(),
        };

        let target = CostReport {
            function: "test".to_string(),
            wasm_hash: "bbb".to_string(),
            cpu_instructions: 150,
            memory_bytes: 10,
            tx_size: 60,
            read_entries: 2,
            write_entries: 1,
            read_bytes: 20,
            write_bytes: 40,
            fee: FeeBreakdown {
                non_refundable_stroops: 120,
                refundable_stroops: 200,
                cpu_fee_stroops: 15,
                storage_fee_stroops: 25,
                bandwidth_fee_stroops: 30,
                total_stroops: 320,
                total_xlm: "0.0000320".to_string(),
            },
            ledger: 1000,
            network: "testnet".to_string(),
        };

        let diff = CostReportDiff::compute(&baseline, &target);
        assert_eq!(diff.delta.cpu_instructions, 50);
        assert_eq!(diff.delta.memory_bytes, 0);
        assert_eq!(diff.delta.tx_size, 10);
        assert_eq!(diff.delta.read_entries, 1);
        assert_eq!(diff.delta.write_entries, 0);
        assert_eq!(diff.delta.fee_total_stroops, 20);

        let table = format_diff_table(&diff);
        assert!(table.contains("Baseline WASM SHA-256: aaa"));
        assert!(table.contains("Target WASM SHA-256:   bbb"));
        assert!(table.contains("+50"));

        let json = format_diff_json(&diff);
        assert!(json.contains("\"cpu_instructions\": 50"));
    }
}
