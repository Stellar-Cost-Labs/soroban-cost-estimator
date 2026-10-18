//! Edge-case tests for `xlm_to_stroops` parsing.
//!
//! Covers empty/whitespace input, malformed fractions, overflow, negative
//! zero, and very large values at and beyond the `i64` stroops boundary.
//! Happy-path conversions are covered by unit tests in
//! `src/report/fee_calc.rs`.

use soroban_cost_estimator::report::fee_calc::xlm_to_stroops;

/// `i64::MAX` stroops expressed as an XLM string: 922337203685 XLM +
/// 4775807 stroops of fraction, exactly representable with 7 digits.
const MAX_XLM: &str = "922337203685.4775807";

#[test]
fn test_empty_string_is_rejected() {
    let err = xlm_to_stroops("").unwrap_err();
    assert!(
        err.to_string().contains("invalid XLM value"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_whitespace_only_is_rejected() {
    assert!(xlm_to_stroops("   ").is_err());
    assert!(xlm_to_stroops("\t").is_err());
}

#[test]
fn test_fraction_without_whole_part_is_rejected() {
    // The whole part is empty before the dot and cannot parse as i64.
    assert!(xlm_to_stroops(".5").is_err());
}

#[test]
fn test_multiple_dots_are_rejected() {
    assert!(xlm_to_stroops("1.2.3").is_err());
    assert!(xlm_to_stroops("1..2").is_err());
}

#[test]
fn test_non_numeric_input_is_rejected() {
    assert!(xlm_to_stroops("abc").is_err());
    assert!(xlm_to_stroops("12x.5").is_err());
    assert!(xlm_to_stroops("nan").is_err());
}

#[test]
fn test_negative_zero_parses_to_zero() {
    // "-0" parses to i64 zero; no negative-zero stroop value exists.
    assert_eq!(xlm_to_stroops("-0").unwrap(), 0);
    assert_eq!(xlm_to_stroops("-0.0").unwrap(), 0);
    assert_eq!(xlm_to_stroops("-0.0000000").unwrap(), 0);
}

#[test]
fn test_max_i64_value_is_accepted() {
    // Exactly i64::MAX stroops: the largest value that must not overflow.
    assert_eq!(xlm_to_stroops(MAX_XLM).unwrap(), i64::MAX);
}

#[test]
fn test_whole_only_overflow_is_rejected() {
    // 922337203686 * 10^7 exceeds i64::MAX → checked_mul overflow.
    let err = xlm_to_stroops("922337203686").unwrap_err();
    assert!(
        err.to_string()
            .contains("XLM value is out of range for stroops"),
        "unexpected error: {err}"
    );
    // Far beyond i64 range: the parse itself fails (reported as invalid).
    assert!(xlm_to_stroops("99999999999999999999").is_err());
}

#[test]
fn test_fractional_overflow_is_rejected() {
    // One stroop past i64::MAX: checked_add overflows on the fraction.
    let err = xlm_to_stroops("922337203685.4775808").unwrap_err();
    assert!(
        err.to_string()
            .contains("XLM value is out of range for stroops"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_very_large_values_round_trip_within_precision() {
    // Large but safely-representable values keep all 7 fractional digits.
    assert_eq!(
        xlm_to_stroops("99999999.9999999").unwrap(),
        999_999_999_999_999
    );
    assert_eq!(
        xlm_to_stroops("123456789.9876543").unwrap(),
        1_234_567_899_876_543
    );
}

#[test]
fn test_extra_fraction_digits_are_truncated() {
    // Only the first 7 fractional digits (stroop precision) are used;
    // anything beyond is silently truncated rather than rounded.
    assert_eq!(xlm_to_stroops("0.12345678").unwrap(), 1_234_567);
    assert_eq!(xlm_to_stroops("1.00000009").unwrap(), 10_000_000);
}

#[test]
#[allow(clippy::too_many_lines)]
fn test_explain_fee_calculation_reconciles_with_compute_fee_breakdown_and_integer_rounding() {
    use soroban_cost_estimator::report::cost_report::CostReport;
    use soroban_cost_estimator::report::fee_calc::{
        DEFAULT_PRECISION, FeeRates, compute_fee_breakdown, explain_fee_calculation,
    };
    use soroban_cost_estimator::report::formatter::{
        JsonFormatter, ReportFormatter, ReportOptions, TableFormatter,
    };
    use soroban_cost_estimator::wasm::parser::ContractMeta;

    let rates = FeeRates {
        fee_per_10k_insns: 25,
        fee_per_read_entry: 6_250,
        fee_per_write_entry: 10_000,
        fee_per_read_1kb: 1_786,
        fee_per_1kb: 1_624,
    };
    // Choose unit counts that exercise integer truncation/rounding across 10,000 and 1,024 scales:
    // CPU: (14_999 * 25) / 10_000 = 374_975 / 10_000 = 37 stroops
    // Read entries: 2 * 6_250 = 12_500 stroops
    // Write entries: 1 * 10_000 = 10_000 stroops
    // Read bytes: (1_500 * 1_786) / 1024 = 2_679_000 / 1024 = 2_616 stroops
    // Tx size: (511 * 1_624) / 1024 = 829_864 / 1024 = 810 stroops
    let cpu_insns = 14_999;
    let read_entries = 2;
    let write_entries = 1;
    let read_bytes = 1_500;
    let tx_size = 511;
    let total_resource_fee = 50_000;

    let breakdown = compute_fee_breakdown(
        total_resource_fee,
        cpu_insns,
        read_entries,
        write_entries,
        read_bytes,
        tx_size,
        rates,
        DEFAULT_PRECISION,
    );
    let explanation = explain_fee_calculation(
        cpu_insns,
        read_entries,
        write_entries,
        read_bytes,
        tx_size,
        rates,
    );

    assert_eq!(explanation.cpu_fee_stroops, 37);
    assert_eq!(explanation.storage_fee_stroops, 12_500 + 10_000 + 2_616);
    assert_eq!(explanation.bandwidth_fee_stroops, 810);
    assert_eq!(explanation.cpu_fee_stroops, breakdown.cpu_fee_stroops);
    assert_eq!(
        explanation.storage_fee_stroops,
        breakdown.storage_fee_stroops
    );
    assert_eq!(
        explanation.bandwidth_fee_stroops,
        breakdown.bandwidth_fee_stroops
    );
    assert_eq!(
        explanation.non_refundable_stroops,
        breakdown.non_refundable_stroops
    );

    let report = CostReport {
        function: "increment".to_string(),
        wasm_hash: "deadbeef".to_string(),
        wasm_size: 1024,
        wasm_sections: Vec::new(),
        cpu_instructions: cpu_insns,
        memory_bytes: 2048,
        tx_size,
        read_entries,
        write_entries,
        read_bytes,
        write_bytes: 128,
        fee: breakdown,
        ledger: 42,
        network: "testnet".to_string(),
        rpc_latency_ms: 10,
        rates: Some(rates),
        warnings: Vec::new(),
        history: None,
        projections: None,
        contract_meta: ContractMeta::default(),
    };

    let opts = ReportOptions::default().with_explain_fee(true);
    let table_out = TableFormatter.format_with(&report, opts);
    assert!(
        table_out.contains("Fee Calculation Explanation:"),
        "got: {table_out}"
    );
    assert!(
        table_out.contains("14999 instructions"),
        "table must label CPU units: {table_out}"
    );
    assert!(
        table_out.contains("25 stroops / 10,000 instructions"),
        "table must label CPU rate: {table_out}"
    );
    assert!(
        table_out.contains("511 bytes"),
        "table must label bandwidth units: {table_out}"
    );

    let json_out = JsonFormatter.format_with(&report, opts);
    let parsed: serde_json::Value = serde_json::from_str(&json_out).expect("valid JSON");
    assert_eq!(parsed["fee_explanation"]["cpu_fee_stroops"], 37);
    assert_eq!(
        parsed["fee_explanation"]["storage_fee_stroops"],
        12_500 + 10_000 + 2_616
    );
    assert_eq!(parsed["fee_explanation"]["bandwidth_fee_stroops"], 810);
    assert_eq!(
        parsed["fee_explanation"]["components"][0]["unit_label"],
        "instructions"
    );
    assert_eq!(parsed["fee_explanation"]["components"][0]["scale"], 10_000);
}
