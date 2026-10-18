# CLI Command Reference

This page provides a unified reference of all `soroban-cost-estimator` CLI commands, subcommands, global flags, environment variables, and process exit codes.

---

## 1. Global Flags

Global flags can be passed before or after any subcommand:

| Flag | Short | Default | Description | Example |
|---|---|---|---|---|
| `--config <PATH>` | `-c` | `.soroban-cost-estimator.toml` | Path to a custom TOML configuration file | `-c ./ci-config.toml` |
| `--verbose` | `-v` | `false` | Enable verbose diagnostic output on `stderr` | `--verbose` |
| `--quiet` | `-q` | `false` | Suppress non-essential advisory output and progress bars | `-q` |
| `--header <KEY=VALUE>` | `-H` | None | Attach custom HTTP header to every RPC request (repeatable) | `-H "X-API-Key=secret"` |
| `--timeout <SECONDS>` | — | `30` | HTTP request timeout in seconds (`> 0`) | `--timeout 60` |
| `--connect-timeout <SECONDS>` | — | `5` | TCP connection establishment timeout in seconds (`> 0`) | `--connect-timeout 10` |
| `--max-retries <COUNT>` | — | `3` | Maximum retry attempts for transient RPC failures (`0` disables) | `--max-retries 5` |
| `--rate-limit <RPS>` | — | None | Client-side token-bucket limit on outbound RPC requests per second | `--rate-limit 10` |
| `--fallback-rpc-url <URL>` | — | None | Fallback RPC URL(s) tried in order when the primary endpoint fails | `--fallback-rpc-url https://...` |
| `--help` | `-h` | — | Print help information | `--help` |
| `--version` | `-V` | — | Print version information | `--version` |

---

## 2. Subcommands

### 2.1 `estimate`

Simulate a single contract invocation (or WASM upload when `--fn` is omitted) and render a cost report.

| Flag | Short | Default | Description | Example |
|---|---|---|---|---|
| `--wasm <PATH>` | `-w` | *(required)* | Path to compiled `.wasm` contract binary | `--wasm contract.wasm` |
| `--network <NAME>` | `-n` | `testnet` | Target Stellar network (`testnet`, `mainnet`, `futurenet`) | `--network mainnet` |
| `--rpc-url <URL>` | — | Network default | Custom Soroban JSON-RPC endpoint URL | `--rpc-url http://localhost:8000/soroban/rpc` |
| `--id <CONTRACT_ID>` | — | None | Deployed contract ID (`C...` strkey or 64-char hex; required with `--fn`) | `--id CDLZ...` |
| `--fn <NAME>` | — | None | Contract function name to simulate | `--fn increment` |
| `--arg <KEY=VAL>` | `-a` | `[]` | Function argument(s), repeatable | `--arg step=5` |
| `--args-json <JSON>` | — | None | JSON array of function arguments (mutually exclusive with `--arg`) | `--args-json '[5]'` |
| `--format <FMT>` | — | `table` | Output format (`table`, `json`, `csv`, `markdown`, `html`, `flamegraph`) | `--format markdown` |
| `--json` | — | `false` | Shorthand for `--format json` | `--json` |
| `--diff` | — | `false` | Compare two WASM files side-by-side (requires `--wasm-new`) | `--diff --wasm-new v2.wasm` |
| `--wasm-new <PATH>` | — | None | Second WASM binary for `--diff` comparison | `--wasm-new v2.wasm` |
| `--compare` | — | `false` | Compare simulation result against the previous cached estimate | `--compare` |
| `--history` | — | `false` | Compare current run against up to 5 previous cached estimates | `--history` |
| `--clear-cache` | — | `false` | Clear all cached estimates for `--network` before simulating | `--clear-cache` |
| `--cache-ttl <DURATION>` | — | `7d` | Maximum age of cached estimates before re-simulating (`30m`, `24h`, `7d`) | `--cache-ttl 24h` |
| `--precision <DIGITS>` | — | `7` | Number of decimal places for XLM formatting (`0..=7`) | `--precision 4` |

```bash
soroban-cost-estimator estimate \
  --wasm contract.wasm \
  --id CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC \
  --fn increment \
  --arg step=5 \
  --network testnet
```

---

### 2.2 `estimate-all`

Enumerate all exported contract functions from the WASM binary and `contractspecv0` section and estimate every zero-argument function.

| Flag | Short | Default | Description | Example |
|---|---|---|---|---|
| `--wasm <PATH>` | `-w` | *(required)* | Path to compiled `.wasm` contract binary | `--wasm contract.wasm` |
| `--id <CONTRACT_ID>` | — | *(required)* | Deployed contract ID (`C...` strkey or 64-char hex) | `--id CDLZ...` |
| `--network <NAME>` | `-n` | `testnet` | Target Stellar network (`testnet`, `mainnet`, `futurenet`) | `--network testnet` |
| `--rpc-url <URL>` | — | Network default | Custom Soroban JSON-RPC endpoint URL | `--rpc-url https://...` |
| `--format <FMT>` | — | `table` | Output format (`table`, `json`, `csv`, `markdown`) | `--format csv` |
| `--json` | — | `false` | Emit structured JSON report with `functions` and `fee_distribution` | `--json` |
| `--concurrency <N>` | `-j` | `4` | Maximum concurrent `simulateTransaction` RPC requests | `-j 8` |

```bash
soroban-cost-estimator estimate-all \
  --wasm contract.wasm \
  --id CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC \
  --network testnet \
  --format markdown
```

---

### 2.3 `config` (`snapshot`, `diff`, `history`, `last-changed`, `validate`, `export`, `import`)

Manage and diff on-chain Soroban `CONFIG_SETTING` resource-pricing snapshots.

#### `config snapshot`
| Flag | Short | Default | Description | Example |
|---|---|---|---|---|
| `--network <NAME>` | `-n` | `testnet` | Target network (`testnet`, `mainnet`, `futurenet`) | `--network mainnet` |
| `--rpc-url <URL>` | — | Network default | Custom RPC endpoint URL | `--rpc-url https://...` |
| `--out <PATH>` | `-o` | Auto-timestamped | Custom output file path for the JSON snapshot | `-o snapshot.json` |
| `--retain <N>` | — | Unlimited | Retain only the `N` newest snapshots for the network | `--retain 10` |
| `--json` | — | `false` | Print the snapshot JSON to `stdout` | `--json` |

#### `config diff`
| Flag | Short | Default | Description | Example |
|---|---|---|---|---|
| `--network <NAME>` | `-n` | `testnet` | Target network (`testnet`, `mainnet`, `futurenet`) | `--network testnet` |
| `--against <PATH>` | — | Latest stored | Compare live network config against a specific snapshot file | `--against baseline.json` |
| `--against-previous` | — | `false` | Compare the two most recent stored snapshots offline | `--against-previous` |
| `--threshold <PCT>` | — | None | Only fail with exit code `1` if a pricing change is `>= PCT%` | `--threshold 5.0` |
| `--pricing-only` | — | `false` | Show only pricing-related changes | `--pricing-only` |
| `--fail-on-any-change` | — | `false` | Exit with code `1` on any config change (pricing or limit) | `--fail-on-any-change` |
| `--ignore-pricing-exit` | — | `false` | Always exit `0` even when pricing changes are detected | `--ignore-pricing-exit` |
| `--summary` | — | `false` | Print a single-line change count summary | `--summary` |
| `--json` | — | `false` | Output diff as JSON | `--json` |

```bash
# Capture a snapshot and diff later
soroban-cost-estimator config snapshot --network testnet --retain 5
soroban-cost-estimator config diff --network testnet --threshold 2.5
```

---

### 2.4 `watch`

Continuously poll network configuration (or subscribe via WebSocket) and report pricing changes in real time.

| Flag | Short | Default | Description | Example |
|---|---|---|---|---|
| `--network <NAME>` | `-n` | `testnet` | Target network (`testnet`, `mainnet`, `futurenet`) | `--network testnet` |
| `--interval <DURATION>` | `-i` | `60s` | Poll interval (`30s`, `5m`, `1h`, `1d`, or seconds) | `--interval 5m` |
| `--rpc-url <URL>` | — | Network default | Custom HTTP/WebSocket RPC URL | `--rpc-url https://...` |
| `--max-polls <N>` | — | Unlimited | Stop automatically after `N` polls | `--max-polls 10` |
| `--webhook-url <URL>` | — | None | POST JSON alert payload to a webhook when pricing changes | `--webhook-url https://...` |

```bash
soroban-cost-estimator watch --network mainnet --interval 15m
```

---

### 2.5 `wasm info`

Inspect a compiled `.wasm` file offline without making network calls.

| Flag | Short | Default | Description | Example |
|---|---|---|---|---|
| `--wasm <PATH>` | `-w` | *(required)* | Path to `.wasm` binary | `--wasm contract.wasm` |
| `--json` | — | `false` | Output structural breakdown and spec entries as JSON | `--json` |

---

### 2.6 `cache` (`stats`, `verify`, `warm`, `clear`)

Inspect, verify, pre-warm, or clear the local simulation cache (`~/.soroban-cost-estimator/cache/`).

```bash
soroban-cost-estimator cache stats
soroban-cost-estimator cache verify
soroban-cost-estimator cache clear --confirm
```

---

### 2.7 `completions`

Generate shell completion scripts for `bash`, `zsh`, `fish`, `powershell`, or `elvish`.

| Flag / Arg | Default | Description | Example |
|---|---|---|---|
| `<SHELL>` | *(required)* | Target shell (`bash`, `zsh`, `fish`, `powershell`, `elvish`) | `completions bash` |

```bash
soroban-cost-estimator completions bash > ~/.local/share/bash-completion/completions/soroban-cost-estimator
```

---

### 2.8 `rpc` / Network Diagnostics

All RPC-enabled commands (`estimate`, `estimate-all`, `config snapshot`, `config diff`, `watch`) share the global RPC transport flags (`--rpc-url`, `--fallback-rpc-url`, `--header`, `--timeout`, `--connect-timeout`, `--max-retries`, `--rate-limit`).

---

## 3. Environment Variables

| Variable | Description | Example |
|---|---|---|
| `RUST_LOG` | Controls `tracing` log verbosity on `stderr` | `RUST_LOG=soroban_cost_estimator=debug` |
| `SOROBAN_COST_ESTIMATOR_HOME` | Override default `~/.soroban-cost-estimator` data directory | `/tmp/sce-data` |
| `NO_COLOR` | Disable ANSI terminal colors in table and diff output | `NO_COLOR=1` |

---

## 4. Process Exit Codes

| Exit Code | Meaning | When Returned |
|---|---|---|
| `0` | **Success** | Simulation succeeded within budgets, or `config diff` detected no pricing drift |
| `1` | **Runtime Error / Drift / Budget Gate Violated** | RPC/WASM/XDR failure, `config diff` detected pricing drift (`>= threshold`), `cache verify` found corruption, or an `estimate` budget gate was exceeded |
| `2` | **CLI Usage / Argument Error** | Invalid CLI flag, missing required argument, or mutually exclusive flags (`clap` parse error) |
