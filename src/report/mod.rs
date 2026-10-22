/// The end-to-end cost report model assembled after a simulation.
pub mod cost_report;
/// Comparison of two cost reports (per-resource deltas and regressions).
pub mod diff;
/// Soroban fee and resource-fee computation using integer stroop arithmetic.
pub mod fee_calc;
/// Output formatters (table, JSON, CSV, Markdown) for cost reports.
pub mod formatter;
