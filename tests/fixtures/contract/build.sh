#!/usr/bin/env bash
# Rebuild or verify the real Soroban contract fixture (`../contract.wasm`).
#
# The fixture must be a structurally real Soroban contract (contractspecv0
# custom section + typed params), not just valid WASM, so the parser tests
# exercise the same input shape a real submission would use.
#
# Pass `--check` or `--verify` to compare a freshly built binary against the
# committed `../contract.wasm` without overwriting it.
set -euo pipefail
cd "$(dirname "$0")"

CHECK_ONLY=0
if [[ "${1:-}" == "--check" || "${1:-}" == "--verify" ]]; then
    CHECK_ONLY=1
fi

rustup target add wasm32v1-none >/dev/null 2>&1 || true

# Deterministic compiler flags so identical source + toolchain inputs produce
# byte-identical WASM regardless of the caller's checkout path.
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTFLAGS="--remap-path-prefix=${PWD}=/workspace/contract --remap-path-prefix=${CARGO_HOME}=/cargo-home -C codegen-units=1"

# Release build: a real fixture must be deployable, so it must fit under
# Soroban's wasm/transaction size limits (~129 KB tx max on testnet). The
# debug build is ~3.7 MB and gets rejected on upload; release is ~5 KB.
# Release needs ~2-3 GiB of RAM to link the soroban-sdk tree — do not run
# concurrently with other heavy builds.
cargo build --locked --release --target wasm32v1-none

BUILT_WASM="target/wasm32v1-none/release/increment_fixture.wasm"
if [[ "$CHECK_ONLY" -eq 1 ]]; then
    if ! cmp -s "$BUILT_WASM" ../contract.wasm; then
        echo "Error: ../contract.wasm differs from freshly built $BUILT_WASM" >&2
        exit 1
    fi
    echo "Verified ../contract.wasm is byte-identical ($(wc -c < ../contract.wasm) bytes)"
    exit 0
fi

cp "$BUILT_WASM" ../contract.wasm
echo "Fixture written to ../contract.wasm ($(wc -c < ../contract.wasm) bytes)"

