use std::path::Path;

#[test]
fn test_load_minimal_wasm() {
    let path = Path::new("tests/fixtures/minimal.wasm");
    assert!(path.exists(), "test WASM fixture not found");

    let wasm_info =
        soroban_cost_estimator::wasm::parser::load_wasm(path).expect("failed to load test WASM");

    assert!(!wasm_info.bytes.is_empty(), "WASM should have bytes");
    assert_eq!(wasm_info.bytes.len(), 44, "unexpected WASM size");

    // Should find at least one exported function
    assert!(
        !wasm_info.functions.is_empty(),
        "WASM should have exported functions"
    );
    let names: Vec<String> = wasm_info.functions.iter().map(|f| f.name.clone()).collect();
    assert!(
        names.contains(&"add_one".to_string()),
        "should contain 'add_one' function, got: {:?}",
        names
    );
}

/// The real-contract fixture is a compiled Soroban contract (contractspecv0
/// custom section + typed params), structurally identical to what a real
/// submission would use — unlike `minimal.wasm`, which is bare WASM.
#[test]
fn test_load_real_soroban_contract_fixture() {
    let path = Path::new("tests/fixtures/contract.wasm");
    assert!(
        path.exists(),
        "real contract fixture not found; build with tests/fixtures/contract/build.sh"
    );

    let wasm_info = soroban_cost_estimator::wasm::parser::load_wasm(path)
        .expect("failed to load contract fixture");

    assert!(
        wasm_info.has_spec,
        "fixture should carry a contractspecv0 section"
    );

    let inc = wasm_info
        .functions
        .iter()
        .find(|f| f.name == "increment")
        .expect("fixture should export 'increment'");

    // One exported function, one typed argument: the spec must decode real
    // typed params, not bare WASM export signatures.
    assert_eq!(inc.param_count, 1);
    assert_eq!(
        inc.params.len(),
        1,
        "increment should declare one typed param"
    );
    assert_eq!(inc.params[0].name, "step");
    assert_eq!(inc.params[0].type_name, "i64");

    let formatted = soroban_cost_estimator::wasm::parser::format_function(inc);
    assert!(
        formatted.contains("step") && formatted.contains("i64"),
        "got: {formatted}"
    );
}

#[test]
fn test_invalid_wasm_rejected() {
    let invalid_bytes = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]; // magic only, no content
    let temp_dir = std::env::temp_dir();
    let invalid_path = temp_dir.join("invalid.wasm");
    std::fs::write(&invalid_path, &invalid_bytes).unwrap();

    let result = soroban_cost_estimator::wasm::parser::load_wasm(&invalid_path);
    assert!(result.is_err(), "invalid WASM should be rejected");

    let _ = std::fs::remove_file(&invalid_path);
}

#[test]
fn test_nonexistent_wasm() {
    let result = soroban_cost_estimator::wasm::parser::load_wasm(Path::new(
        "tests/fixtures/nonexistent.wasm",
    ));
    assert!(result.is_err(), "nonexistent file should error");
}

/// The bare fixture exports only the `add_one` function; the captured export
/// structure must reflect that, and the module start function is absent.
#[test]
fn test_module_metadata_bare_wasm() {
    let path = Path::new("tests/fixtures/minimal.wasm");
    let wasm_info =
        soroban_cost_estimator::wasm::parser::load_wasm(path).expect("failed to load test WASM");

    let add_one_export = wasm_info
        .exports
        .iter()
        .find(|e| e.name == "add_one")
        .expect("add_one should appear in the export structure");
    assert_eq!(add_one_export.kind, "function");

    assert!(
        wasm_info.start_function.is_none(),
        "bare fixture has no start function"
    );
    assert!(
        wasm_info.memories.is_empty() || wasm_info.memories.len() == 1,
        "bare fixture declares at most one memory"
    );

    let summary = soroban_cost_estimator::wasm::parser::format_module_metadata(&wasm_info);
    assert!(
        summary.contains("WASM module metadata:"),
        "summary should have a header, got: {summary}"
    );
    assert!(
        summary.contains("imports:") && summary.contains("exports:"),
        "summary should list imports and exports, got: {summary}"
    );
}

/// The real-contract fixture must carry its exported functions in the export
/// structure, typed params in the spec, and a complete import/export summary.
#[test]
fn test_module_metadata_real_contract() {
    let path = Path::new("tests/fixtures/contract.wasm");
    let wasm_info = soroban_cost_estimator::wasm::parser::load_wasm(path)
        .expect("failed to load contract fixture");

    assert!(
        wasm_info.has_spec,
        "fixture should carry a contractspecv0 section"
    );
    assert!(
        !wasm_info.functions.is_empty(),
        "fixture should export functions"
    );
    assert!(
        !wasm_info.exports.is_empty(),
        "fixture should populate the export structure"
    );

    for function in &wasm_info.functions {
        let export = wasm_info
            .exports
            .iter()
            .find(|e| e.name == function.name)
            .unwrap_or_else(|| {
                panic!(
                    "exported function {} missing from export structure",
                    function.name
                )
            });
        assert_eq!(export.kind, "function");
    }

    let summary = soroban_cost_estimator::wasm::parser::format_module_metadata(&wasm_info);
    assert!(
        summary.contains("start function") && summary.contains("memories:"),
        "summary should describe entry points, got: {summary}"
    );
}

#[test]
fn test_validate_arg_value_i64() {
    let ty = stellar_xdr::ScSpecTypeDef::I64;

    assert!(soroban_cost_estimator::wasm::parser::validate_arg_value(&ty, "42").is_ok());
    assert!(soroban_cost_estimator::wasm::parser::validate_arg_value(&ty, "step=42").is_ok());
    assert!(soroban_cost_estimator::wasm::parser::validate_arg_value(&ty, "-7").is_ok());
    assert!(
        soroban_cost_estimator::wasm::parser::validate_arg_value(&ty, "abc").is_err(),
        "abc is not an i64"
    );
    assert!(
        soroban_cost_estimator::wasm::parser::validate_arg_value(&ty, "99999999999999999999999999")
            .is_err(),
        "overflow is not an i64"
    );
}

#[test]
fn test_validate_arg_value_bool() {
    let ty = stellar_xdr::ScSpecTypeDef::Bool;
    assert!(soroban_cost_estimator::wasm::parser::validate_arg_value(&ty, "true").is_ok());
    assert!(soroban_cost_estimator::wasm::parser::validate_arg_value(&ty, "false").is_ok());
    assert!(soroban_cost_estimator::wasm::parser::validate_arg_value(&ty, "yes").is_err());
}

#[test]
fn test_validate_arg_value_symbol() {
    let ty = stellar_xdr::ScSpecTypeDef::Symbol;
    assert!(soroban_cost_estimator::wasm::parser::validate_arg_value(&ty, "player_1").is_ok());
    assert!(
        soroban_cost_estimator::wasm::parser::validate_arg_value(&ty, "a b").is_err(),
        "spaces are not symbols"
    );
    assert!(
        soroban_cost_estimator::wasm::parser::validate_arg_value(&ty, "").is_err(),
        "empty symbol is invalid"
    );
    let too_long = "a".repeat(33);
    assert!(soroban_cost_estimator::wasm::parser::validate_arg_value(&ty, &too_long).is_err());
}

#[test]
fn test_validate_arg_value_wide_integers() {
    let ty = stellar_xdr::ScSpecTypeDef::U256;
    assert!(
        soroban_cost_estimator::wasm::parser::validate_arg_value(
            &ty,
            "340282366920938463463374607431768211455"
        )
        .is_ok()
    );
    assert!(soroban_cost_estimator::wasm::parser::validate_arg_value(&ty, "0x1ff").is_ok());
    assert!(soroban_cost_estimator::wasm::parser::validate_arg_value(&ty, "abc").is_err());
    assert!(soroban_cost_estimator::wasm::parser::validate_arg_value(&ty, "").is_err());
}

#[test]
fn test_validate_arg_value_bytes_n() {
    let two = stellar_xdr::ScSpecTypeDef::BytesN(stellar_xdr::ScSpecTypeBytesN { n: 2 });
    assert!(soroban_cost_estimator::wasm::parser::validate_arg_value(&two, "0x00ff").is_ok());
    assert!(soroban_cost_estimator::wasm::parser::validate_arg_value(&two, "00ff").is_ok());
    assert!(
        soroban_cost_estimator::wasm::parser::validate_arg_value(&two, "0x00").is_err(),
        "2-byte type needs 2 bytes"
    );
}
