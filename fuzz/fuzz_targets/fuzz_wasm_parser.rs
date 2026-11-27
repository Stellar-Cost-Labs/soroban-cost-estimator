#![no_main]

use libfuzzer_sys::fuzz_target;
use soroban_cost_estimator::wasm::parser::{
    enumerate_module_metadata, parse_contract_spec_entries, validate_wasm,
    validate_wasm_memory_limits,
};

/// Maximum bounded input size for fuzzing (64 KiB).
///
/// Bounding input size ensures the fuzzer spends time exploring semantic structure
/// and boundary conditions without triggering pathological memory allocations.
const MAX_FUZZ_INPUT_SIZE: usize = 64 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_FUZZ_INPUT_SIZE {
        return;
    }

    // 1. Basic structural WASM validation
    let _ = validate_wasm(data);

    // 2. Memory limits inspection
    let _ = validate_wasm_memory_limits(data);

    // 3. Module metadata enumeration (sections, imports, exports, summary)
    let _ = enumerate_module_metadata(data);

    // 4. Contract-spec XDR decoding boundary parsing
    let _ = parse_contract_spec_entries(data);
});
