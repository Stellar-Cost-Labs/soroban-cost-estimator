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

/// Format a signed integer delta with a sign prefix and absolute percentage.
///
/// Returns a string like `"+1234"`, `"-567"`, or `"0"`.
pub fn format_delta_i64(delta: i64) -> String {
    if delta > 0 {
        format!("+{delta}")
    } else {
        delta.to_string()
    }
}

/// Format a percentage change from an old value to a new value.
///
/// Returns a formatted string like `"+12.5%"`, `"-3.2%"`, or `"0.0%"`.
/// Returns `"0.0%"` when the old value is zero (no meaningful baseline).
pub fn delta_percentage(old: u64, new: u64) -> String {
    if old == 0 {
        "0.0%".to_string()
    } else {
        let pct = ((new as f64 - old as f64) / old as f64) * 100.0;
        if pct > 0.0 {
            format!("+{pct:.1}%")
        } else {
            format!("{pct:.1}%")
        }
    }
}

/// Cost delta between two estimates of the same function.
///
/// Each field is `(new - old)` — positive means the new estimate is higher.
/// A `None` value means the old estimate did not contain that metric.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CostDelta {
    /// Previous ledger sequence (from the cached estimate).
    pub prev_ledger: u32,
    /// Previous total fee in stroops.
    pub prev_fee_stroops: i64,
    /// Previous CPU instructions (from the cached estimate).
    pub prev_cpu_instructions: u64,
    /// Previous memory bytes (from the cached estimate).
    pub prev_memory_bytes: u64,
    /// CPU instructions delta (new - old).
    pub cpu_delta: i64,
    /// Memory bytes delta (new - old).
    pub memory_delta: i64,
    /// Total fee delta in stroops (new - old).
    pub fee_delta: i64,
    /// Read entries delta.
    pub read_entries_delta: i64,
    /// Write entries delta.
    pub write_entries_delta: i64,
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
    /// Cost delta compared to a previous cached estimate, if one existed
    /// for the same function. `None` when this is the first estimate or
    /// no cached entry was found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<CostDelta>,
}

/// Compute a [`CostDelta`] between a previous cached estimate and the
/// current report values.
///
/// Returns `None` when there is no previous estimate to compare against.
pub fn compute_delta(
    prev_fee_stroops: i64,
    prev_cpu: u64,
    prev_memory: u64,
    prev_ledger: u32,
    prev_read_entries: u32,
    prev_write_entries: u32,
    current: &CostReport,
) -> CostDelta {
    CostDelta {
        prev_ledger,
        prev_fee_stroops,
        prev_cpu_instructions: prev_cpu,
        prev_memory_bytes: prev_memory,
        cpu_delta: current.cpu_instructions as i64 - prev_cpu as i64,
        memory_delta: current.memory_bytes as i64 - prev_memory as i64,
        fee_delta: current.fee.total_stroops - prev_fee_stroops,
        read_entries_delta: current.read_entries as i64 - prev_read_entries as i64,
        write_entries_delta: current.write_entries as i64 - prev_write_entries as i64,
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

    #[test]
    fn test_format_delta_positive() {
        assert_eq!(format_delta_i64(100), "+100");
        assert_eq!(format_delta_i64(1), "+1");
    }

    #[test]
    fn test_format_delta_negative() {
        assert_eq!(format_delta_i64(-100), "-100");
        assert_eq!(format_delta_i64(-1), "-1");
    }

    #[test]
    fn test_format_delta_zero() {
        assert_eq!(format_delta_i64(0), "0");
    }

    #[test]
    fn test_delta_percentage_increase() {
        assert_eq!(delta_percentage(100, 150), "+50.0%");
        assert_eq!(delta_percentage(1000, 1100), "+10.0%");
    }

    #[test]
    fn test_delta_percentage_decrease() {
        assert_eq!(delta_percentage(100, 50), "-50.0%");
        assert_eq!(delta_percentage(1000, 900), "-10.0%");
    }

    #[test]
    fn test_delta_percentage_zero_old() {
        assert_eq!(delta_percentage(0, 100), "0.0%");
        assert_eq!(delta_percentage(0, 0), "0.0%");
    }

    #[test]
    fn test_delta_percentage_no_change() {
        assert_eq!(delta_percentage(100, 100), "0.0%");
    }

    #[test]
    fn test_compute_delta_basic() {
        let report = CostReport {
            function: "increment".to_string(),
            wasm_hash: "abc123".to_string(),
            cpu_instructions: 600,
            memory_bytes: 200,
            tx_size: 100,
            read_entries: 2,
            write_entries: 3,
            read_bytes: 0,
            write_bytes: 0,
            fee: crate::report::fee_calc::FeeBreakdown {
                non_refundable_stroops: 0,
                refundable_stroops: 0,
                cpu_fee_stroops: 0,
                storage_fee_stroops: 0,
                bandwidth_fee_stroops: 0,
                total_stroops: 20_000,
                total_xlm: "0.002".to_string(),
            },
            ledger: 1000,
            network: "testnet".to_string(),
            rpc_latency_ms: 0,
            delta: None,
        };

        let delta = compute_delta(15_000, 500, 100, 900, 1, 2, &report);

        assert_eq!(delta.prev_ledger, 900);
        assert_eq!(delta.prev_fee_stroops, 15_000);
        assert_eq!(delta.prev_cpu_instructions, 500);
        assert_eq!(delta.prev_memory_bytes, 100);
        assert_eq!(delta.cpu_delta, 100); // 600 - 500
        assert_eq!(delta.memory_delta, 100); // 200 - 100
        assert_eq!(delta.fee_delta, 5_000); // 20_000 - 15_000
        assert_eq!(delta.read_entries_delta, 1); // 2 - 1
        assert_eq!(delta.write_entries_delta, 1); // 3 - 2
    }

    #[test]
    fn test_compute_delta_decrease() {
        let report = CostReport {
            function: "transfer".to_string(),
            wasm_hash: "def456".to_string(),
            cpu_instructions: 300,
            memory_bytes: 50,
            tx_size: 100,
            read_entries: 0,
            write_entries: 1,
            read_bytes: 0,
            write_bytes: 0,
            fee: crate::report::fee_calc::FeeBreakdown {
                non_refundable_stroops: 0,
                refundable_stroops: 0,
                cpu_fee_stroops: 0,
                storage_fee_stroops: 0,
                bandwidth_fee_stroops: 0,
                total_stroops: 8_000,
                total_xlm: "0.0008".to_string(),
            },
            ledger: 2000,
            network: "testnet".to_string(),
            rpc_latency_ms: 0,
            delta: None,
        };

        let delta = compute_delta(10_000, 500, 200, 1500, 3, 2, &report);

        assert_eq!(delta.cpu_delta, -200); // 300 - 500
        assert_eq!(delta.memory_delta, -150); // 50 - 200
        assert_eq!(delta.fee_delta, -2_000); // 8_000 - 10_000
        assert_eq!(delta.read_entries_delta, -3); // 0 - 3
        assert_eq!(delta.write_entries_delta, -1); // 1 - 2
    }
}
