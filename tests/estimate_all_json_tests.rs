//! Snapshot tests for the `estimate-all --json` schema.
//!
//! The expected JSON below is the contract for automation scripts: field
//! names, status values, sort order, and null-vs-present rules are all
//! asserted by exact string comparison.

use soroban_cost_estimator::report::cost_report::{
    EstimateAllFunction, EstimateAllReport, FunctionResources,
};
use soroban_cost_estimator::report::fee_calc::FeeBreakdown;

fn success_entry(name: &str) -> EstimateAllFunction {
    EstimateAllFunction::success(
        name.to_string(),
        FunctionResources {
            cpu_instructions: 524_389,
            memory_bytes: 0,
            read_bytes: 0,
            write_bytes: 136,
        },
        FeeBreakdown {
            non_refundable_stroops: 4_496,
            refundable_stroops: 10_931,
            total_stroops: 15_427,
            total_xlm: "0.0015427".to_string(),
        },
    )
}

fn failed_entry(name: &str, message: &str) -> EstimateAllFunction {
    EstimateAllFunction::failed(name.to_string(), message.to_string())
}

/// Exact snapshot of the report JSON: schema, sort order, and summary.
#[test]
fn estimate_all_report_json_snapshot() {
    // Deliberately unsorted input — `new` must sort by function name.
    let report = EstimateAllReport::new(
        "ea14bca998e98f0ddb338e8e5cef6e19f07378a3b71e8b4f8868cedc857e4ecd".to_string(),
        "testnet".to_string(),
        vec![
            failed_entry("zeta", "simulation failed: boom"),
            success_entry("alpha"),
            failed_entry("increment", "needs --fn/--arg (1 param(s))"),
        ],
    );

    let json = serde_json::to_string_pretty(&report).expect("serialize report");

    let expected = r#"{
  "contract_wasm_hash": "ea14bca998e98f0ddb338e8e5cef6e19f07378a3b71e8b4f8868cedc857e4ecd",
  "network": "testnet",
  "functions": [
    {
      "function_name": "alpha",
      "status": "success",
      "resources": {
        "cpu_instructions": 524389,
        "memory_bytes": 0,
        "read_bytes": 0,
        "write_bytes": 136
      },
      "fee_breakdown": {
        "non_refundable_stroops": 4496,
        "refundable_stroops": 10931,
        "total_stroops": 15427,
        "total_xlm": "0.0015427"
      }
    },
    {
      "function_name": "increment",
      "status": "failed",
      "resources": null,
      "fee_breakdown": null,
      "error_message": "needs --fn/--arg (1 param(s))"
    },
    {
      "function_name": "zeta",
      "status": "failed",
      "resources": null,
      "fee_breakdown": null,
      "error_message": "simulation failed: boom"
    }
  ],
  "total_summary": {
    "total_functions": 3,
    "successful": 1,
    "failed": 2,
    "total_cpu_instructions": 524389,
    "total_memory_bytes": 0,
    "total_fee_stroops": 15427
  }
}"#;

    assert_eq!(
        json, expected,
        "estimate-all --json schema snapshot changed — update docs/commands/estimate-all.md if intentional"
    );
}

/// The status values must be exactly "success" / "failed" (snake_case).
#[test]
fn status_serializes_as_success_or_failed() {
    assert_eq!(
        serde_json::to_value(
            soroban_cost_estimator::report::cost_report::EstimateAllStatus::Success
        )
        .expect("serialize"),
        serde_json::json!("success")
    );
    assert_eq!(
        serde_json::to_value(
            soroban_cost_estimator::report::cost_report::EstimateAllStatus::Failed
        )
        .expect("serialize"),
        serde_json::json!("failed")
    );
}

/// A failed entry must omit `error_message` on success and carry null
/// resources/fee_breakdown on failure.
#[test]
fn error_message_present_only_on_failure() {
    let success = serde_json::to_value(success_entry("ok_fn")).expect("serialize");
    assert!(
        success.get("error_message").is_none(),
        "success entries must not carry error_message: {success}"
    );
    assert!(success["resources"].is_object());
    assert!(success["fee_breakdown"].is_object());

    let failed = serde_json::to_value(failed_entry("bad_fn", "nope")).expect("serialize");
    assert_eq!(failed["error_message"], serde_json::json!("nope"));
    assert!(failed["resources"].is_null());
    assert!(failed["fee_breakdown"].is_null());
}

/// The summary must only sum resources/fees of successful functions and
/// count failures (including needs-args) separately.
#[test]
fn summary_counts_and_sums() {
    let report = EstimateAllReport::new(
        "hash".to_string(),
        "testnet".to_string(),
        vec![
            success_entry("a"),
            success_entry("b"),
            failed_entry("c", "needs --fn/--arg"),
            failed_entry("d", "simulation failed"),
        ],
    );
    let summary = report.total_summary;
    assert_eq!(summary.total_functions, 4);
    assert_eq!(summary.successful, 2);
    assert_eq!(summary.failed, 2);
    assert_eq!(summary.total_cpu_instructions, 524_389 * 2);
    assert_eq!(summary.total_memory_bytes, 0);
    assert_eq!(summary.total_fee_stroops, 15_427 * 2);
}

/// Sorting is by byte order of the function name, independent of input
/// order, so repeated runs always emit the same array order.
#[test]
fn functions_sorted_deterministically() {
    let report = EstimateAllReport::new(
        "hash".to_string(),
        "testnet".to_string(),
        vec![
            failed_entry("gamma", "x"),
            failed_entry("alpha", "x"),
            failed_entry("beta", "x"),
        ],
    );
    let names: Vec<&str> = report
        .functions
        .iter()
        .map(|f| f.function_name.as_str())
        .collect();
    assert_eq!(names, vec!["alpha", "beta", "gamma"]);
}
