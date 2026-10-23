//! Schema-contract tests for every documented JSON report family (#531).
//!
//! These tests protect the shapes downstream integrators parse. They assert
//! structure — required keys and value types — rather than whole-output
//! snapshots, because JSON object key order is not part of the contract.
//! Optional-field behavior follows the documented compatibility policy in
//! `docs/commands/estimate.md` ("JSON output compatibility"): an unset
//! optional field is either omitted entirely (fields annotated
//! `skip_serializing_if`) or emitted as `null` (fields serialized from the
//! `json!` macro or without a skip predicate), and consumers must tolerate
//! new keys without failing.
//!
//! Families covered here (lib-side):
//!
//! * single estimate — `JsonFormatter` output for a `CostReport`
//! * multi-contract batch — `format_batch_report_json` / `BatchCostReport`
//! * side-by-side diff — `CostReportDiff` from `report::diff`
//! * WASM structure summary — `WasmStructureSummary`
//!
//! The `estimate-all --json` document and `wasm info --json` are produced by
//! private types in `src/main.rs`; their schema tests live in that file's
//! test module.

use serde_json::Value;
use soroban_cost_estimator::report::cost_report::{
    BatchCostReport, ContractCostSummary, CostProjection, CostReport, CostTrend, HistoryEntry,
    ResourceWarning, format_batch_report_json,
};
use soroban_cost_estimator::report::diff::{
    ContractAbiDiff, FunctionAbiChange, ParamTypeChange, build_cost_report_diff,
    build_cost_report_diff_with_abi,
};
use soroban_cost_estimator::report::fee_calc::{DEFAULT_PRECISION, FeeBreakdown, stroops_to_xlm};
use soroban_cost_estimator::report::formatter::{JsonFormatter, ReportFormatter, ReportOptions};
use soroban_cost_estimator::wasm::parser::{
    ContractMeta, WasmStructureSummary, section_size_breakdown,
};

// ── Assertion helpers ────────────────────────────────────────────────
//
// Every helper fails with the offending path and the keys actually present,
// so a schema break points at the consumer-visible problem immediately.

type JsonObject = serde_json::Map<String, Value>;

/// `value` must be a JSON object; returns its keys.
fn obj<'a>(value: &'a Value, path: &str) -> &'a JsonObject {
    value
        .as_object()
        .unwrap_or_else(|| panic!("`{path}` must be a JSON object, got {value}"))
}

/// The documented key `key` must be present in `object`.
fn require<'a>(object: &'a JsonObject, key: &str, path: &str) -> &'a Value {
    object.get(key).unwrap_or_else(|| {
        let mut present: Vec<&str> = object.keys().map(String::as_str).collect();
        present.sort_unstable();
        panic!(
            "`{path}.{key}` is a required key of the documented schema; present keys: {present:?}"
        );
    })
}

/// A documented optional key must be absent.
fn assert_absent(object: &JsonObject, key: &str, path: &str) {
    assert!(
        object.get(key).is_none(),
        "`{path}.{key}` is contractually omitted when unset, got {:?}",
        object.get(key)
    );
}

fn need_str(value: &Value, path: &str) {
    assert!(
        value.is_string(),
        "`{path}` must be a JSON string, got {value}"
    );
}

fn need_u64(value: &Value, path: &str) {
    assert!(
        value.as_u64().is_some(),
        "`{path}` must be a non-negative integer, got {value}"
    );
}

fn need_i64(value: &Value, path: &str) {
    assert!(
        value.is_i64() || value.is_u64(),
        "`{path}` must be a signed or unsigned integer, got {value}"
    );
}

fn need_number(value: &Value, path: &str) {
    assert!(
        value.is_number(),
        "`{path}` must be a JSON number, got {value}"
    );
}

fn need_bool(value: &Value, path: &str) {
    assert!(
        value.is_boolean(),
        "`{path}` must be a JSON boolean, got {value}"
    );
}

fn need_array<'a>(value: &'a Value, path: &str) -> &'a [Value] {
    value
        .as_array()
        .unwrap_or_else(|| panic!("`{path}` must be a JSON array, got {value}"))
}

/// Parses formatter output as JSON and guarantees it is an object.
fn parse_report(json: &str) -> Value {
    let value: Value = serde_json::from_str(json).expect("formatter output must be valid JSON");
    assert!(value.is_object(), "report JSON must be a top-level object");
    value
}

/// Shared validation for the documented `fee` object shape.
fn check_fee_breakdown(fee: &Value, path: &str) {
    let fee = obj(fee, path);
    for key in [
        "non_refundable_stroops",
        "refundable_stroops",
        "cpu_fee_stroops",
        "storage_fee_stroops",
        "bandwidth_fee_stroops",
        "base_fee_stroops",
        "total_stroops",
    ] {
        need_i64(require(fee, key, path), &format!("{path}.{key}"));
    }
    need_str(
        require(fee, "total_xlm", path),
        &format!("{path}.total_xlm"),
    );
    let percentages = obj(
        require(fee, "fee_percentages", path),
        &format!("{path}.fee_percentages"),
    );
    for (key, value) in percentages {
        need_str(value, &format!("{path}.fee_percentages.{key}"));
    }
}

/// Shared validation for one `warnings` array entry.
fn check_resource_warning(entry: &Value, path: &str) {
    let warning = obj(entry, path);
    need_str(
        require(warning, "resource", path),
        &format!("{path}.resource"),
    );
    need_str(require(warning, "label", path), &format!("{path}.label"));
    need_u64(require(warning, "used", path), &format!("{path}.used"));
    need_u64(require(warning, "limit", path), &format!("{path}.limit"));
    need_u64(
        require(warning, "percent", path),
        &format!("{path}.percent"),
    );
    need_str(
        require(warning, "message", path),
        &format!("{path}.message"),
    );
}

// ── Fixtures ─────────────────────────────────────────────────────────

fn sample_fee(total: i64) -> FeeBreakdown {
    FeeBreakdown {
        non_refundable_stroops: 4_496,
        refundable_stroops: total - 4_496,
        cpu_fee_stroops: 372,
        storage_fee_stroops: 4_063,
        bandwidth_fee_stroops: 61,
        base_fee_stroops: 100,
        total_stroops: total,
        total_xlm: stroops_to_xlm(total, DEFAULT_PRECISION),
        fee_percentages: std::collections::BTreeMap::new(),
    }
}

/// A fully-populated single-function report: everything the JSON contract
/// promises when all optional data is present.
fn populated_report() -> CostReport {
    CostReport {
        function: "increment".to_string(),
        wasm_hash: "abc123def456".to_string(),
        wasm_size: 14_432,
        wasm_sections: Vec::new(),
        cpu_instructions: 532_502,
        memory_bytes: 0,
        tx_size: 156,
        read_entries: 1,
        write_entries: 1,
        read_bytes: 0,
        write_bytes: 136,
        fee: sample_fee(15_427),
        ledger: 3_894_195,
        network: "testnet".to_string(),
        rpc_latency_ms: 87,
        rates: None,
        warnings: vec![ResourceWarning {
            resource: "cpu_instructions".to_string(),
            label: "CPU instructions".to_string(),
            used: 900,
            limit: 1_000,
            percent: 90,
            message: "CPU instructions at 90% of the network limit".to_string(),
        }],
        history: Some(vec![HistoryEntry {
            timestamp: "2026-01-02T00:00:00Z".to_string(),
            ledger: 3_894_000,
            cpu_instructions: 520_000,
            total_stroops: 15_000,
            total_xlm: "0.0015000".to_string(),
            delta_stroops: -427,
            trend: CostTrend::Improvement,
        }]),
        projections: Some(vec![CostProjection {
            invocations: 100,
            total_stroops: 1_542_700,
            total_xlm: "0.1542700".to_string(),
            usd: Some(0.42),
        }]),
        contract_meta: ContractMeta {
            name: Some("MetaContract".to_string()),
            version: Some("1.0.0".to_string()),
            description: None,
            author: None,
            sdk_version: Some("25.3.2".to_string()),
            entries: vec![("name".to_string(), "MetaContract".to_string())],
        },
    }
}

/// The all-zero report used by the snapshot suite: every optional field
/// unset, so it exercises the "absent" side of the contract.
fn minimal_report() -> CostReport {
    CostReport {
        function: "(wasm upload)".to_string(),
        wasm_hash: "0000000000000000".to_string(),
        wasm_size: 0,
        wasm_sections: Vec::new(),
        cpu_instructions: 0,
        memory_bytes: 0,
        tx_size: 0,
        read_entries: 0,
        write_entries: 0,
        read_bytes: 0,
        write_bytes: 0,
        fee: sample_fee(0),
        ledger: 0,
        network: "mainnet".to_string(),
        rpc_latency_ms: 0,
        rates: None,
        warnings: Vec::new(),
        history: None,
        projections: None,
        contract_meta: ContractMeta::default(),
    }
}

// ── Family 1: single estimate (`estimate --json`) ────────────────────

#[test]
#[allow(clippy::too_many_lines)]
fn estimate_json_required_keys_have_documented_types() {
    let parsed = parse_report(&JsonFormatter.format(&populated_report()));
    let root = obj(&parsed, "estimate");

    need_str(require(root, "function", "estimate"), "estimate.function");
    need_str(require(root, "wasm_hash", "estimate"), "estimate.wasm_hash");
    need_u64(require(root, "wasm_size", "estimate"), "estimate.wasm_size");
    need_u64(
        require(root, "cpu_instructions", "estimate"),
        "estimate.cpu_instructions",
    );
    need_u64(
        require(root, "memory_bytes", "estimate"),
        "estimate.memory_bytes",
    );
    need_u64(require(root, "tx_size", "estimate"), "estimate.tx_size");
    need_u64(
        require(root, "read_entries", "estimate"),
        "estimate.read_entries",
    );
    need_u64(
        require(root, "write_entries", "estimate"),
        "estimate.write_entries",
    );
    need_u64(
        require(root, "read_bytes", "estimate"),
        "estimate.read_bytes",
    );
    need_u64(
        require(root, "write_bytes", "estimate"),
        "estimate.write_bytes",
    );
    need_u64(
        require(root, "rpc_latency_ms", "estimate"),
        "estimate.rpc_latency_ms",
    );
    // The ledger sequence is serialized under its documented name.
    need_u64(
        require(root, "ledger_sequence", "estimate"),
        "estimate.ledger_sequence",
    );
    assert_absent(root, "ledger", "estimate");
    need_str(require(root, "network", "estimate"), "estimate.network");

    check_fee_breakdown(require(root, "fee", "estimate"), "estimate.fee");

    // `warnings` is always an array; entries carry the documented fields.
    let warnings = need_array(require(root, "warnings", "estimate"), "estimate.warnings");
    assert!(!warnings.is_empty(), "fixture carries one warning");
    for (index, entry) in warnings.iter().enumerate() {
        check_resource_warning(entry, &format!("estimate.warnings[{index}]"));
    }

    // `contract_meta` is always present on an estimate; populated keys are
    // strings and `entries` is an array of [key, value] pairs.
    let meta = obj(
        require(root, "contract_meta", "estimate"),
        "estimate.contract_meta",
    );
    need_str(
        require(meta, "name", "estimate.contract_meta"),
        "estimate.contract_meta.name",
    );
    need_str(
        require(meta, "sdk_version", "estimate.contract_meta"),
        "estimate.contract_meta.sdk_version",
    );
    let entries = need_array(
        require(meta, "entries", "estimate.contract_meta"),
        "estimate.contract_meta.entries",
    );
    for pair in entries {
        let pair = need_array(pair, "estimate.contract_meta.entries[]");
        assert_eq!(pair.len(), 2, "entry pairs are [key, value]");
        need_str(&pair[0], "entry key");
        need_str(&pair[1], "entry value");
    }

    // Derived advisory keys are part of the default (non-quiet) document.
    let suggestions = need_array(
        require(root, "suggestions", "estimate"),
        "estimate.suggestions",
    );
    for tip in suggestions {
        need_str(tip, "estimate.suggestions[]");
    }
    for (index, entry) in need_array(
        require(root, "optimization_suggestions", "estimate"),
        "estimate.optimization_suggestions",
    )
    .iter()
    .enumerate()
    {
        let path = format!("estimate.optimization_suggestions[{index}]");
        let entry = obj(entry, &path);
        need_str(require(entry, "title", &path), &format!("{path}.title"));
        need_str(require(entry, "detail", &path), &format!("{path}.detail"));
        need_i64(
            require(entry, "potential_savings_stroops", &path),
            &format!("{path}.potential_savings_stroops"),
        );
    }

    // Present optional blocks keep their documented element shape.
    let history = need_array(require(root, "history", "estimate"), "estimate.history");
    for (index, entry) in history.iter().enumerate() {
        let path = format!("estimate.history[{index}]");
        let entry = obj(entry, &path);
        need_str(
            require(entry, "timestamp", &path),
            &format!("{path}.timestamp"),
        );
        need_u64(require(entry, "ledger", &path), &format!("{path}.ledger"));
        need_u64(
            require(entry, "cpu_instructions", &path),
            &format!("{path}.cpu_instructions"),
        );
        need_i64(
            require(entry, "total_stroops", &path),
            &format!("{path}.total_stroops"),
        );
        need_str(
            require(entry, "total_xlm", &path),
            &format!("{path}.total_xlm"),
        );
        need_i64(
            require(entry, "delta_stroops", &path),
            &format!("{path}.delta_stroops"),
        );
        let trend = need_str_get(require(entry, "trend", &path), &format!("{path}.trend"));
        assert!(
            matches!(trend, "improvement" | "regression" | "unchanged"),
            "trend must be one of the lowercase documented variants, got {trend:?}"
        );
    }
    let projections = need_array(
        require(root, "projections", "estimate"),
        "estimate.projections",
    );
    for (index, entry) in projections.iter().enumerate() {
        let path = format!("estimate.projections[{index}]");
        let entry = obj(entry, &path);
        need_u64(
            require(entry, "invocations", &path),
            &format!("{path}.invocations"),
        );
        need_i64(
            require(entry, "total_stroops", &path),
            &format!("{path}.total_stroops"),
        );
        need_str(
            require(entry, "total_xlm", &path),
            &format!("{path}.total_xlm"),
        );
        // `usd` is present here because the fixture carries a price.
        need_number(require(entry, "usd", &path), &format!("{path}.usd"));
    }
}

#[test]
fn estimate_json_optional_fields_are_absent_when_unset() {
    let parsed = parse_report(&JsonFormatter.format(&minimal_report()));
    let root = obj(&parsed, "estimate");

    // Required keys are still there on an all-zero report.
    for key in [
        "function",
        "wasm_hash",
        "cpu_instructions",
        "memory_bytes",
        "tx_size",
        "read_entries",
        "write_entries",
        "read_bytes",
        "write_bytes",
        "fee",
        "ledger_sequence",
        "network",
        "rpc_latency_ms",
        "warnings",
        "contract_meta",
        "suggestions",
        "optimization_suggestions",
    ] {
        require(root, key, "estimate");
    }

    // Unset optionals are omitted, never emitted as zero or null.
    assert_absent(root, "wasm_size", "estimate");
    assert_absent(root, "wasm_sections", "estimate");
    assert_absent(root, "section_sizes", "estimate");
    assert_absent(root, "history", "estimate");
    assert_absent(root, "projections", "estimate");

    // Internal-only state never reaches the document.
    assert_absent(root, "rates", "estimate");

    // `contract_meta` is always serialized but renders empty when absent,
    // and `warnings` stays an (empty) array.
    assert!(
        obj(
            require(root, "contract_meta", "estimate"),
            "estimate.contract_meta",
        )
        .is_empty(),
        "absent contract metadata must render as an empty object"
    );
    assert!(need_array(require(root, "warnings", "estimate"), "estimate.warnings",).is_empty());
}

#[test]
fn estimate_json_quiet_option_drops_advisory_keys() {
    let parsed =
        parse_report(&JsonFormatter.format_with(&populated_report(), ReportOptions::quiet()));
    let root = obj(&parsed, "estimate");

    assert_absent(root, "suggestions", "estimate");
    assert_absent(root, "optimization_suggestions", "estimate");
    // The cost data itself is unaffected by `--quiet`.
    check_fee_breakdown(require(root, "fee", "estimate"), "estimate.fee");
}

#[test]
fn estimate_json_section_breakdown_adds_sections_and_section_sizes() {
    let bytes = std::fs::read("tests/fixtures/contract.wasm").expect("fixture readable");
    let breakdown = section_size_breakdown(&bytes).expect("fixture is valid WASM");
    let mut report = populated_report();
    report.wasm_size = bytes.len() as u64;
    report.wasm_sections = breakdown.sections;

    let parsed = parse_report(&JsonFormatter.format(&report));
    let root = obj(&parsed, "estimate");

    let sections = need_array(
        require(root, "wasm_sections", "estimate"),
        "estimate.wasm_sections",
    );
    assert!(!sections.is_empty());
    for (index, entry) in sections.iter().enumerate() {
        let path = format!("estimate.wasm_sections[{index}]");
        let entry = obj(entry, &path);
        need_str(require(entry, "name", &path), &format!("{path}.name"));
        need_u64(require(entry, "size", &path), &format!("{path}.size"));
        need_u64(
            require(entry, "header_size", &path),
            &format!("{path}.header_size"),
        );
        need_u64(
            require(entry, "total_size", &path),
            &format!("{path}.total_size"),
        );
        need_number(require(entry, "percent", &path), &format!("{path}.percent"));
    }

    let sizes = obj(
        require(root, "section_sizes", "estimate"),
        "estimate.section_sizes",
    );
    for (name, value) in sizes {
        need_u64(value, &format!("estimate.section_sizes.{name}"));
    }
}

#[test]
fn estimate_json_round_trips_back_into_cost_report() {
    // A consumer can deserialize the document with the same struct the tool
    // uses internally, proving the emitted keys match the documented model.
    let json = JsonFormatter.format(&populated_report());
    let reparsed: CostReport = serde_json::from_str(&json).expect("document must deserialize");
    assert_eq!(reparsed.function, "increment");
    assert_eq!(reparsed.fee.total_stroops, 15_427);
    assert_eq!(reparsed.ledger, 3_894_195);
    assert_eq!(reparsed.warnings.len(), 1);
    assert!(reparsed.rates.is_none(), "`rates` is never serialized");
}

// ── Family 2: multi-contract batch (`--wasm-dir` runs) ───────────────

#[test]
fn batch_report_json_top_level_schema() {
    let batch = BatchCostReport::new(
        "testnet".to_string(),
        vec![
            ContractCostSummary {
                path: "a.wasm".to_string(),
                wasm_hash: Some("aaaa".to_string()),
                function_count: Some(3),
                upload_cost_stroops: Some(100_000),
                min_invocation_fee_stroops: Some(120),
                max_invocation_fee_stroops: Some(480),
                functions_simulated: Some(2),
                error: None,
            },
            ContractCostSummary::unloadable("broken.wasm", "unreadable file"),
        ],
    );
    let parsed: Value =
        serde_json::from_str(&format_batch_report_json(&batch)).expect("valid JSON");
    let root = obj(&parsed, "batch");

    need_str(require(root, "network", "batch"), "batch.network");
    need_u64(
        require(root, "total_contracts", "batch"),
        "batch.total_contracts",
    );
    assert_eq!(
        root["total_contracts"].as_u64(),
        Some(2),
        "batch totals are derived from the rows"
    );
    assert_eq!(root["succeeded"].as_u64(), Some(1));
    assert_eq!(root["failed"].as_u64(), Some(1));
    let contracts = need_array(require(root, "contracts", "batch"), "batch.contracts");
    assert_eq!(contracts.len(), 2, "one row per contract, in command order");

    // Fully evaluated row: every documented key present with its type.
    let row = obj(&contracts[0], "batch.contracts[0]");
    need_str(require(row, "path", "batch.contracts[0]"), "…path");
    need_str(
        require(row, "wasm_hash", "batch.contracts[0]"),
        "…wasm_hash",
    );
    need_u64(
        require(row, "function_count", "batch.contracts[0]"),
        "…function_count",
    );
    need_i64(
        require(row, "upload_cost_stroops", "batch.contracts[0]"),
        "…upload_cost_stroops",
    );
    need_i64(
        require(row, "min_invocation_fee_stroops", "batch.contracts[0]"),
        "…min_invocation_fee_stroops",
    );
    need_i64(
        require(row, "max_invocation_fee_stroops", "batch.contracts[0]"),
        "…max_invocation_fee_stroops",
    );
    need_u64(
        require(row, "functions_simulated", "batch.contracts[0]"),
        "…functions_simulated",
    );
    assert_absent(row, "error", "batch.contracts[0]");

    // Failed row: unknown values are omitted (not zeroed); `error` carries
    // the reason.
    let failed = obj(&contracts[1], "batch.contracts[1]");
    need_str(require(failed, "path", "batch.contracts[1]"), "…path");
    need_str(require(failed, "error", "batch.contracts[1]"), "…error");
    for key in [
        "wasm_hash",
        "function_count",
        "upload_cost_stroops",
        "min_invocation_fee_stroops",
        "max_invocation_fee_stroops",
        "functions_simulated",
    ] {
        assert_absent(failed, key, "batch.contracts[1]");
    }
}

// ── Family 3: side-by-side diff (`estimate --diff --json`) ───────────

#[test]
fn diff_json_required_keys_have_documented_types() {
    let old = populated_report();
    let mut new = populated_report();
    new.wasm_hash = "ffff0000".to_string();
    new.ledger = 3_894_200;
    new.memory_bytes = 5; // old is 0: the percentage change is unbounded
    new.cpu_instructions = 600_000;

    let diff = build_cost_report_diff(&old, &new);
    let value = serde_json::to_value(&diff).expect("diff serializes");
    let root = obj(&value, "diff");

    let identity = obj(require(root, "identity", "diff"), "diff.identity");
    need_str(require(identity, "function", "diff.identity"), "…function");
    need_str(require(identity, "network", "diff.identity"), "…network");
    need_str(
        require(identity, "old_wasm_hash", "diff.identity"),
        "…old_wasm_hash",
    );
    need_str(
        require(identity, "new_wasm_hash", "diff.identity"),
        "…new_wasm_hash",
    );
    need_u64(
        require(identity, "old_ledger", "diff.identity"),
        "…old_ledger",
    );
    need_u64(
        require(identity, "new_ledger", "diff.identity"),
        "…new_ledger",
    );

    // ABI diff is contractually omitted when no ABI comparison ran.
    assert_absent(root, "abi_diff", "diff");

    let rows = need_array(require(root, "rows", "diff"), "diff.rows");
    assert_eq!(rows.len(), 8, "the diff row set is fixed and ordered");
    let mut saw_unbounded_percent = false;
    for (index, entry) in rows.iter().enumerate() {
        let path = format!("diff.rows[{index}]");
        let row = obj(entry, &path);
        need_str(require(row, "resource", &path), &format!("{path}.resource"));
        let old_value = row["old"]
            .as_i64()
            .unwrap_or_else(|| panic!("`{path}.old` must be a signed integer, got {}", row["old"]));
        let new_value = row["new"]
            .as_i64()
            .unwrap_or_else(|| panic!("`{path}.new` must be a signed integer, got {}", row["new"]));
        need_i64(require(row, "delta", &path), &format!("{path}.delta"));
        assert_eq!(
            row["delta"].as_i64(),
            Some(new_value - old_value),
            "{path}: delta must equal new - old"
        );
        let direction = need_str_get(
            require(row, "direction", &path),
            &format!("{path}.direction"),
        );
        assert!(
            matches!(direction, "increase" | "decrease" | "unchanged"),
            "direction must be a lowercase documented variant, got {direction:?}"
        );
        // `change_percent` is the null-when-undefined case: unlike the
        // skip-serialized optionals, the key is always present.
        let percent = require(row, "change_percent", &path);
        if old_value == 0 && new_value != 0 {
            assert!(percent.is_null(), "{path}: unbounded change must be null");
            saw_unbounded_percent = true;
        } else {
            need_number(percent, &format!("{path}.change_percent"));
        }
    }
    assert!(
        saw_unbounded_percent,
        "fixture must exercise the null change_percent contract"
    );
}

#[test]
fn diff_json_abi_diff_block_keeps_documented_shape() {
    let old = populated_report();
    let new = populated_report();
    let diff = build_cost_report_diff_with_abi(
        &old,
        &new,
        Some(ContractAbiDiff {
            added_functions: vec!["transfer".to_string()],
            removed_functions: Vec::new(),
            modified_functions: vec![FunctionAbiChange {
                name: "increment".to_string(),
                old_signature: "increment(step: I64) -> i64".to_string(),
                new_signature: "increment(step: U32) -> u32".to_string(),
                param_changes: vec![ParamTypeChange {
                    name: "step".to_string(),
                    old_type: Some("I64".to_string()),
                    new_type: Some("U32".to_string()),
                }],
                old_return: Some("i64".to_string()),
                new_return: Some("u32".to_string()),
            }],
        }),
    );
    let value = serde_json::to_value(&diff).expect("diff serializes");
    let root = obj(&value, "diff");

    let abi = obj(require(root, "abi_diff", "diff"), "diff.abi_diff");
    let added = need_array(
        require(abi, "added_functions", "diff.abi_diff"),
        "diff.abi_diff.added_functions",
    );
    assert_eq!(added.len(), 1);
    need_str(&added[0], "diff.abi_diff.added_functions[0]");
    // Empty collections are skipped, so `removed_functions` is absent.
    assert_absent(abi, "removed_functions", "diff.abi_diff");

    let modified = need_array(
        require(abi, "modified_functions", "diff.abi_diff"),
        "diff.abi_diff.modified_functions",
    );
    let change = obj(&modified[0], "diff.abi_diff.modified_functions[0]");
    need_str(require(change, "name", "…change"), "…name");
    need_str(
        require(change, "old_signature", "…change"),
        "…old_signature",
    );
    need_str(
        require(change, "new_signature", "…change"),
        "…new_signature",
    );
    let params = need_array(
        require(change, "param_changes", "…change"),
        "…param_changes",
    );
    let param = obj(&params[0], "param_changes[0]");
    need_str(require(param, "name", "…param"), "…name");
    need_str(require(param, "old_type", "…param"), "…old_type");
    need_str(require(param, "new_type", "…param"), "…new_type");
    // A parameter present in both builds carries both type strings; the
    // absent-side variant is covered by the skip predicate below.
    assert!(
        change["param_changes"][0]
            .get("old_type")
            .is_some_and(|v| v.is_string()),
    );

    let returns_present = |obj_: &JsonObject| {
        obj_.get("old_return").is_some_and(Value::is_string)
            && obj_.get("new_return").is_some_and(Value::is_string)
    };
    assert!(returns_present(change));
}

#[test]
fn diff_json_abi_optional_signature_fields_omit_absent_sides() {
    // A parameter that exists only in the new build omits `old_type`
    // entirely rather than emitting null — the skip-serialized half of the
    // optional-field contract inside the diff family.
    let old = populated_report();
    let new = populated_report();
    let diff = build_cost_report_diff_with_abi(
        &old,
        &new,
        Some(ContractAbiDiff {
            added_functions: Vec::new(),
            removed_functions: Vec::new(),
            modified_functions: vec![FunctionAbiChange {
                name: "increment".to_string(),
                old_signature: "increment() -> i64".to_string(),
                new_signature: "increment(step: U32) -> i64".to_string(),
                param_changes: vec![ParamTypeChange {
                    name: "step".to_string(),
                    old_type: None,
                    new_type: Some("U32".to_string()),
                }],
                old_return: None,
                new_return: None,
            }],
        }),
    );
    let value = serde_json::to_value(&diff).expect("diff serializes");
    let root = obj(&value, "diff");
    let abi = obj(require(root, "abi_diff", "diff"), "diff.abi_diff");
    // Both other collections are empty and therefore omitted.
    assert_absent(abi, "added_functions", "diff.abi_diff");
    assert_absent(abi, "removed_functions", "diff.abi_diff");
    let change = obj(
        &abi["modified_functions"].as_array().expect("array")[0],
        "…modified_functions[0]",
    );
    assert_absent(change, "old_return", "…modified_functions[0]");
    assert_absent(change, "new_return", "…modified_functions[0]");
    let param = obj(
        &change["param_changes"].as_array().expect("array")[0],
        "…param_changes[0]",
    );
    assert_absent(param, "old_type", "…param_changes[0]");
    need_str(require(param, "new_type", "…param_changes[0]"), "…new_type");
}

// ── Family 4: WASM structure summary (`--wasm-info`) ─────────────────

#[test]
fn wasm_structure_summary_json_uses_null_for_unset_option() {
    // This summary is emitted verbatim as JSON by the `--wasm-info` flag, so
    // its key set is part of the contract. Unset `max_pages` serializes as
    // `null` (no skip predicate) — consumers must treat null as "no limit".
    let value = serde_json::to_value(WasmStructureSummary {
        initial_pages: 2,
        max_pages: None,
        imports_count: 7,
        exports_count: 3,
        has_start_function: false,
        tables_count: 1,
    })
    .expect("summary serializes");
    let root = obj(&value, "wasm_summary");

    need_u64(
        require(root, "initial_pages", "wasm_summary"),
        "…initial_pages",
    );
    assert!(
        require(root, "max_pages", "wasm_summary").is_null(),
        "max_pages must be present and null when the memory has no limit"
    );
    need_u64(
        require(root, "imports_count", "wasm_summary"),
        "…imports_count",
    );
    need_u64(
        require(root, "exports_count", "wasm_summary"),
        "…exports_count",
    );
    need_bool(
        require(root, "has_start_function", "wasm_summary"),
        "…has_start_function",
    );
    need_u64(
        require(root, "tables_count", "wasm_summary"),
        "…tables_count",
    );
}

/// Returns the value as `&str` with the same failure message style as the
/// other helpers.
fn need_str_get<'a>(value: &'a Value, path: &str) -> &'a str {
    value
        .as_str()
        .unwrap_or_else(|| panic!("`{path}` must be a JSON string, got {value}"))
}
