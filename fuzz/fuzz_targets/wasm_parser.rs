#![no_main]

use libfuzzer_sys::fuzz_target;
use soroban_cost_estimator::wasm::parser::fuzz_wasm_parser_and_spec_boundaries;
use soroban_cost_estimator::xdr_helper::fuzz_xdr_config_decode_boundary;

fuzz_target!(|data: &[u8]| {
    fuzz_wasm_parser_and_spec_boundaries(data);
    if let Ok(text) = std::str::from_utf8(data) {
        fuzz_xdr_config_decode_boundary(text);
    }
});
