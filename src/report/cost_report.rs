use comfy_table::Table;

use crate::report::fee_calc::FeeBreakdown;

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
    output.push_str(&format!(
        "  Non-refundable: {} stroops\n",
        report.fee.non_refundable_stroops
    ));
    output.push_str(&format!(
        "  Refundable:     {} stroops\n",
        report.fee.refundable_stroops
    ));
    output.push_str(&format!(
        "  Total:          {} stroops ({})\n",
        report.fee.total_stroops, report.fee.total_xlm,
    ));

    output
}

/// Formats a cost report as a JSON string.
pub fn format_report_json(report: &CostReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|_| "{}".to_string())
}

/// Structured JSON report for `estimate-all --json`.
///
/// Emits one object containing the contract identity, the per-function
/// results (deterministically sorted by function name), and an aggregate
/// summary. Functions that could not be simulated carry
/// `status: "failed"` with an `error_message`; their `resources` and
/// `fee_breakdown` are `null`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EstimateAllReport {
    /// SHA-256 hash of the contract WASM bytes (hex).
    pub contract_wasm_hash: String,
    /// Network the simulations ran against.
    pub network: String,
    /// Per-function results, sorted by `function_name`.
    pub functions: Vec<EstimateAllFunction>,
    /// Aggregate totals across all functions.
    pub total_summary: EstimateAllSummary,
}

impl EstimateAllReport {
    /// Build a report, sorting functions by name for deterministic output
    /// and computing the aggregate summary.
    ///
    /// # Network calls
    /// None — pure computation.
    pub fn new(
        contract_wasm_hash: String,
        network: String,
        mut functions: Vec<EstimateAllFunction>,
    ) -> Self {
        functions.sort_by(|a, b| a.function_name.cmp(&b.function_name));
        let total_summary = EstimateAllSummary::from_functions(&functions);
        Self {
            contract_wasm_hash,
            network,
            functions,
            total_summary,
        }
    }
}

/// Result for a single function in `estimate-all --json`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EstimateAllFunction {
    /// Name of the contract function.
    pub function_name: String,
    /// Outcome: `success` or `failed`.
    pub status: EstimateAllStatus,
    /// Resource usage — `null` when the function failed.
    pub resources: Option<FunctionResources>,
    /// Fee breakdown — `null` when the function failed.
    pub fee_breakdown: Option<crate::report::fee_calc::FeeBreakdown>,
    /// Error detail — present only when `status` is `failed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

impl EstimateAllFunction {
    /// Build a successful entry with resources and a fee breakdown.
    pub fn success(
        function_name: String,
        resources: FunctionResources,
        fee_breakdown: crate::report::fee_calc::FeeBreakdown,
    ) -> Self {
        Self {
            function_name,
            status: EstimateAllStatus::Success,
            resources: Some(resources),
            fee_breakdown: Some(fee_breakdown),
            error_message: None,
        }
    }

    /// Build a failed entry carrying the error detail.
    pub fn failed(function_name: String, error_message: String) -> Self {
        Self {
            function_name,
            status: EstimateAllStatus::Failed,
            resources: None,
            fee_breakdown: None,
            error_message: Some(error_message),
        }
    }
}

/// Status of a single function estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EstimateAllStatus {
    /// The simulation completed and produced resources + fees.
    Success,
    /// The function could not be estimated (needs args, or simulation failed).
    Failed,
}

/// Resource usage for a successful function estimate.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct FunctionResources {
    /// CPU instructions consumed.
    pub cpu_instructions: u64,
    /// Memory bytes consumed.
    pub memory_bytes: u64,
    /// Ledger disk bytes read.
    pub read_bytes: u32,
    /// Ledger disk bytes written.
    pub write_bytes: u32,
}

/// Aggregate totals across all functions in an `estimate-all --json` report.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct EstimateAllSummary {
    /// Total functions considered.
    pub total_functions: u32,
    /// Functions that estimated successfully.
    pub successful: u32,
    /// Functions that failed (including those that needed arguments).
    pub failed: u32,
    /// Sum of CPU instructions across successful functions.
    pub total_cpu_instructions: u64,
    /// Sum of memory bytes across successful functions.
    pub total_memory_bytes: u64,
    /// Sum of total fees across successful functions (stroops).
    pub total_fee_stroops: i64,
}

impl EstimateAllSummary {
    /// Compute the aggregate summary from per-function results.
    ///
    /// Resource and fee sums only include successful functions; failed
    /// entries contribute to the `failed` count only. All arithmetic is
    /// integer (stroops are `i64`).
    fn from_functions(functions: &[EstimateAllFunction]) -> Self {
        let mut summary = Self {
            total_functions: u32::try_from(functions.len()).unwrap_or(u32::MAX),
            ..Self::default()
        };
        for function in functions {
            match function.status {
                EstimateAllStatus::Success => {
                    summary.successful += 1;
                    if let Some(resources) = &function.resources {
                        summary.total_cpu_instructions = summary
                            .total_cpu_instructions
                            .saturating_add(resources.cpu_instructions);
                        summary.total_memory_bytes = summary
                            .total_memory_bytes
                            .saturating_add(resources.memory_bytes);
                    }
                    if let Some(fee) = &function.fee_breakdown {
                        summary.total_fee_stroops =
                            summary.total_fee_stroops.saturating_add(fee.total_stroops);
                    }
                }
                EstimateAllStatus::Failed => summary.failed += 1,
            }
        }
        summary
    }
}
