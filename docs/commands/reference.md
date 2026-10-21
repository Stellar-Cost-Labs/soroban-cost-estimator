# CLI command reference

This page documents the commands and options exposed by the current `soroban-cost-estimator` CLI. Use `soroban-cost-estimator <command> --help` for help from the binary you have installed.

## Global options

These options are available before or after a subcommand.

| Flag | Short | Default | Description | Example |
|---|---|---:|---|---|
| `--config <PATH>` | — | automatic | Read this configuration file instead of the default locations. Explicit CLI options override file values. | `--config ./estimator.toml` |
| `--format <FORMAT>` | — | `SOROBAN_JSON`, then config, else `table` | Output format where supported: `table`, `json`, `csv`, or `markdown`. Config and `SOROBAN_JSON` can change the default. | `--format json` |
| `--rps <N>` | — | unset | Maximum RPC requests per second; `0` disables the limit. | `--rps 5` |
| `--timeout <SECS>` | — | `30` | HTTP request timeout in seconds. | `--timeout 60` |
| `--connect-timeout <SECS>` | — | `5` | TCP connection timeout in seconds; `0` disables this separate limit. | `--connect-timeout 10` |
| `--verbose` | `-v` | `false` | Enable debug logging. | `-v` |
| `--quiet` | `-q` | `false` | Suppress progress indicators and informational notices. | `-q` |
| `--header <KEY=VALUE>` | `-H` | none | Add an HTTP header to RPC requests. Repeat for multiple headers; `KEY: VALUE` is also accepted. | `-H 'Authorization=Bearer TOKEN'` |
| `--rpc-fallback-url <URL>` | — | none | Retry transient failures through this fallback RPC endpoint. | `--rpc-fallback-url https://backup.example/rpc` |
| `--max-retries <N>` | — | `3` | Retry transient RPC failures up to this many times; `0` disables retries. | `--max-retries 5` |
| `--color <WHEN>` | — | `auto` | Color policy (`auto`, `always`, `never`). | `--color never` |
| `--wasm-info` | — | `false` | Print WASM structure diagnostics to standard error. | `--wasm-info` |
| `--max-cache-size-mb <MB>` | — | `50` | Cache size quota; `0` disables the byte quota. | `--max-cache-size-mb 100` |
| `--max-cache-entries <N>` | — | `10000` | Cache entry quota; `0` disables the entry quota. | `--max-cache-entries 5000` |
| `--precision <N>` | — | `7` | XLM decimal places, from `0` through `7`. | `--precision 4` |
| `--max-wasm-size <BYTES>` | — | `16 MiB` | Maximum WASM file size accepted by commands that read WASM files. | `--max-wasm-size 8388608` |
| `--help` | `-h` | — | Print help for the selected command. | `estimate --help` |
| `--version` | `-V` | — | Print version and build metadata. | `--version` |

Global settings can also come from the TOML configuration file where supported. See [configuration](../../README.md#configuration-file) for the file format.

## `estimate`

Estimate a single invocation, several WASM files, or every WASM file in a directory.

```sh
soroban-cost-estimator estimate --wasm contract.wasm --fn transfer --id C... --arg amount=100
```

| Flag | Short | Default | Description | Example |
|---|---|---:|---|---|
| `--wasm <PATH>` | `-w` | required unless `--wasm-dir` | Contract WASM; repeat to estimate multiple files. | `-w token.wasm` |
| `--wasm-dir <DIR>` | — | none | Non-recursively estimate WASM files in a directory. | `--wasm-dir ./target/wasm` |
| `--network <NAME>` | — | `testnet` | Network name (`testnet`, `mainnet`, `futurenet`, `local`, or a configured network). | `--network mainnet` |
| `--rpc-url <URL>` | — | network default | RPC endpoint. | `--rpc-url http://localhost:8000/rpc` |
| `--fn <NAME>` | — | none | Contract function to invoke. | `--fn transfer` |
| `--id <CONTRACT_ID>` | — | none | Deployed contract ID for invocation. | `--id C...` |
| `--arg <KEY=VALUE>` | — | none | Function argument; repeat for multiple arguments. | `--arg amount=100` |
| `--interactive` | `-i` | `false` | Prompt for missing function and arguments using the contract spec. | `-i` |
| `--cache-ttl <DURATION>` | — | cache policy | Use a cached estimate while it is younger than this age. | `--cache-ttl 30m` |
| `--compare` | — | `false` | Show differences from the prior estimate for the same input. | `--compare` |
| `--clear-cache` | — | `false` | Clear this network’s estimates before running. | `--clear-cache` |
| `--no-cache` | — | `false` | Bypass cache reads and writes. | `--no-cache` |
| `--history` | — | `false` | Compare with up to five earlier estimates. | `--history` |
| `--json` | — | `false` | Emit JSON. | `--json` |
| `--auto-snapshot` | — | `false` | Save a config snapshot when network pricing changed. | `--auto-snapshot` |
| `--diff` | — | `false` | Compare baseline WASM with `--wasm-new`. | `--diff --wasm-new new.wasm` |
| `--wasm-new <PATH>` | — | none | New WASM file used with `--diff`. | `--wasm-new new.wasm` |
| `--watch` | — | `false` | Watch the WASM file and estimate again after rebuilds. Ctrl-C exits cleanly. | `--watch` |
| `--dry-run` | — | `false` | Validate and print the planned simulation without an RPC call. | `--dry-run` |
| `--project [COUNTS]` | — | `100,1000,10000` when specified without a value | Project costs for comma-separated invocation counts. | `--project 100,1000` |
| `--repeat <N>` | — | `1` | Repeat the simulation from 1 to 100 times and report latency statistics. | `--repeat 5` |

## `estimate-all`

Estimate each public function exported by a contract. Functions requiring arguments are reported as skipped unless the command can invoke them without arguments.

```sh
soroban-cost-estimator estimate-all --wasm contract.wasm --id C... --fn initialize --json
```

| Flag | Short | Default | Description | Example |
|---|---|---:|---|---|
| `--wasm <PATH>` | `-w` | required unless `--wasm-dir` | Contract WASM; repeat to estimate multiple files. | `-w token.wasm` |
| `--wasm-dir <DIR>` | — | none | Non-recursively estimate contracts in a directory. | `--wasm-dir ./wasm` |
| `--network <NAME>` | — | `testnet` | Network to use. | `--network futurenet` |
| `--rpc-url <URL>` | — | network default | RPC endpoint. | `--rpc-url https://rpc.example` |
| `--id <CONTRACT_ID>` | — | none | Deployed contract ID. | `--id C...` |
| `--no-cache` | — | `false` | Bypass estimate cache. | `--no-cache` |
| `--fn <NAME>` | — | all public functions | Restrict estimates to the named functions; repeatable. | `--fn transfer --fn balance` |
| `--json` | — | `false` | Emit JSON. | `--json` |
| `--auto-snapshot` | — | `false` | Save a snapshot if network pricing changed. | `--auto-snapshot` |

## `config`

### `config snapshot`

Fetch and store the network’s resource configuration. Subcommands `list`, `prune`, `delete`, `diff`, and `validate` operate on existing local snapshots.

```sh
soroban-cost-estimator config snapshot --network testnet --retain 10
soroban-cost-estimator config snapshot list --all --json
```

| Flag / argument | Short | Default | Description | Example |
|---|---|---:|---|---|
| `--network <NAME>` | — | `testnet` | Network to snapshot. | `--network mainnet` |
| `--out <PATH>` | — | generated path | Write to this file. | `--out baseline.json` |
| `--json` | — | `false` | Print the saved snapshot as JSON. | `--json` |
| `--retain <COUNT>` | — | keep all | Keep only the newest snapshots after saving. | `--retain 5` |
| `list --network <NAME>` | — | `testnet` | List stored snapshots for a network. | `list --network mainnet` |
| `list --all` | — | `false` | List snapshots for every network. | `list --all` |
| `list --json` | — | `false` | Print the list as JSON. | `list --json` |
| `prune --network <NAME>` | — | `testnet` | Network whose old snapshots to prune. | `prune --network testnet` |
| `prune --older-than <DAYS>` | — | required | Age threshold; newest snapshot is retained. | `prune --older-than 30` |
| `prune --json` | — | `false` | Print result as JSON. | `prune --json` |
| `delete <FILENAME>` | — | none | Delete a snapshot file. | `delete snapshot.json --yes` |
| `delete --older-than <DAYS>` | — | none | Delete snapshots older than this many days. | `delete --older-than 90 --yes` |
| `delete --network <NAME>` | — | all | Limit age-based deletion to one network. | `delete --older-than 90 --network testnet --yes` |
| `delete --dry-run` | — | `false` | Show what would be deleted. | `delete --older-than 90 --dry-run` |
| `delete --yes` | `-y` | `false` | Skip confirmation. | `delete old.json -y` |
| `diff <SNAPSHOT_A> <SNAPSHOT_B>` | — | required | Compare two local snapshot files without RPC. | `diff old.json new.json` |
| `diff --json` | — | `false` | Emit the comparison as JSON. | `diff old.json new.json --json` |
| `validate <PATH>` | — | required unless `--all` | Validate one snapshot. | `validate baseline.json` |
| `validate --all` | — | `false` | Validate every stored snapshot. | `validate --all` |

### `config diff`

Compare live network settings with the latest snapshot, or compare the two latest local snapshots with `--against-previous`.

```sh
soroban-cost-estimator config diff --network testnet --threshold-percent 5 --summary
```

| Flag | Short | Default | Description | Example |
|---|---|---:|---|---|
| `--network <NAME>` | — | `testnet` | Network to compare. | `--network mainnet` |
| `--against <PATH>` | — | latest saved snapshot | Compare live settings to this snapshot file. | `--against baseline.json` |
| `--against-previous` | — | `false` | Compare the two newest saved snapshots offline; conflicts with `--against`. | `--against-previous` |
| `--pricing-only` | — | `false` | Hide changes unrelated to prices. | `--pricing-only` |
| `--threshold-percent <N>` | — | none | Flag price changes at or above this percentage. | `--threshold-percent 5` |
| `--summary` | — | `false` | Print a one-line change summary. | `--summary` |
| `--json` | — | `false` | Emit JSON. | `--json` |
| `--ignore-pricing-exit` | — | `false` | Return success even when prices changed. | `--ignore-pricing-exit` |
| `--fail-on-any-change` | — | `false` | Return exit code 1 for any changed setting, including non-pricing settings. | `--fail-on-any-change` |

Other `config` subcommands are `list`, `history`, `last-changed`, `validate`, `export`, `import`, and `cache`. Run `soroban-cost-estimator config <subcommand> --help` for their exact arguments.

## `watch`

Poll network resource pricing and report changes. Polling begins immediately. Ctrl-C or SIGTERM stops cleanly.

```sh
soroban-cost-estimator watch --network mainnet --interval 15m --threshold-percent 5
```

| Flag | Short | Default | Description | Example |
|---|---|---:|---|---|
| `--network <NAME>` | — | `testnet` | Network to watch. | `--network mainnet` |
| `--interval <DURATION>` | — | `1h` | Poll interval; accepts seconds or `s`, `m`, `h`, and `d` suffixes. | `--interval 15m` |
| `--threshold-percent <N>` | — | none | Percentage change threshold to flag. | `--threshold-percent 10` |

## `completions`

Generate shell completion definitions for Bash, Zsh, Fish, PowerShell, or Elvish.

```sh
soroban-cost-estimator completions zsh > _soroban-cost-estimator
```

| Argument | Short | Default | Description | Example |
|---|---|---:|---|---|
| `<SHELL>` | — | required | Completion shell: `bash`, `zsh`, `fish`, `powershell`, or `elvish`. | `completions bash` |

## RPC settings

There is no top-level `rpc` subcommand in the current CLI. RPC transport is used by network commands and configured with the global `--header`, `--timeout`, `--connect-timeout`, `--rps`, `--max-retries`, and `--rpc-fallback-url` options. Commands that accept `--rpc-url` can select a primary endpoint explicitly. For example:

```sh
soroban-cost-estimator estimate --wasm contract.wasm --rpc-url http://localhost:8000/soroban/rpc --timeout 10
```

## `wasm`

`wasm info` inspects a compiled artifact offline. The legacy top-level `wasm-info` form is also available.

| Command | Flag / argument | Short | Default | Description | Example |
|---|---|---|---:|---|---|
| `wasm info` | `<WASM>` | — | required | WASM file to inspect. | `wasm info contract.wasm` |
| `wasm info` | `--json` | — | `false` | Emit parsed metadata as JSON. | `wasm info contract.wasm --json` |
| `wasm-info` | `--wasm <PATH>` | `-w` | required | WASM file to inspect. | `wasm-info -w contract.wasm` |
| `wasm-info` | `--json` | — | `false` | Emit parsed metadata as JSON. | `wasm-info -w contract.wasm --json` |

## `cache`

The cache commands read or modify local estimate records. Options shown under `list`, `delete`, and `query` can be combined as filters.

| Command | Flag / argument | Short | Default | Description | Example |
|---|---|---|---:|---|---|
| `cache export` | `--out <PATH>` | `-o` | stdout | Write the versioned JSON export to a file. | `cache export -o backup.json` |
| `cache export` | `--network <NAME>` | — | all networks | Export records for one network. | `cache export --network testnet` |
| `cache list` | `--network <NAME>` | — | all networks | Restrict results to a network. | `cache list --network testnet` |
| `cache list` | `--fn <NAME>` (`--function`) | — | none | Filter by exact function name. | `cache list --fn transfer` |
| `cache list` | `--wasm-hash <PREFIX>` | — | none | Filter by hexadecimal WASM hash prefix. | `cache list --wasm-hash a1b2` |
| `cache list` | `--since <DATE/TIME>` (`--from`) | — | none | Earliest record timestamp. | `cache list --since 2025-01-01` |
| `cache list` | `--to <TIMESTAMP>` | — | none | Latest record timestamp. | `cache list --to 2025-01-31T00:00:00Z` |
| `cache list` | `--older-than <DURATION>` | — | none | Match records older than a duration. | `cache list --older-than 30d` |
| `cache list` | `--limit <N>` | — | `50` | Maximum records returned. | `cache list --limit 20` |
| `cache list` | `--offset <N>` | — | `0` | Number of matching records to skip. | `cache list --offset 20` |
| `cache list` | `--json` | — | `false` | Emit records as JSON. | `cache list --json` |
| `cache delete` | `--network`, `--fn` (`--function`), `--wasm-hash`, `--since` (`--from`), `--to`, `--older-than` | — | no filters | Filter records to delete; at least one filter or `--all` is required. | `cache delete --network testnet --dry-run` |
| `cache delete` | `--all` | — | `false` | Delete all records on all networks; cannot be combined with filters. | `cache delete --all --yes` |
| `cache delete` | `--dry-run` | — | `false` | Report matches without deleting them. | `cache delete --fn transfer --dry-run` |
| `cache delete` | `--yes` | `-y` | `false` | Skip the confirmation prompt. | `cache delete --network testnet -y` |
| `cache delete` | `--json` | — | `false` | Emit the result as JSON. | `cache delete --fn transfer --yes --json` |
| `cache verify` | — | — | — | Check cached records for malformed or corrupt data. | `cache verify` |
| `cache clear` | `--network <NAME>` | — | `testnet` | Delete every cached estimate for the network. | `cache clear --network mainnet` |
| `cache warm` | `--wasm <PATH>` | `-w` | required | Estimate exported functions to pre-populate cache. | `cache warm -w contract.wasm` |
| `cache warm` | `--network <NAME>` | — | `testnet` | Network to estimate against. | `cache warm -w contract.wasm --network futurenet` |
| `cache warm` | `--rpc-url <URL>` | — | network default | RPC endpoint. | `cache warm -w contract.wasm --rpc-url https://rpc.example` |
| `cache warm` | `--id <CONTRACT_ID>` | — | none | Deployed contract ID. | `cache warm -w contract.wasm --id C...` |
| `cache warm` | `--json` | — | `false` | Emit JSON. | `cache warm -w contract.wasm --json` |
| `cache prune` | — | — | — | Evict least-recently-used records to fit global cache quotas. | `cache prune` |
| `cache query` | `--network <NAME>` | — | all networks | Filter by network. | `cache query --network testnet` |
| `cache query` | `--fn <NAME>` (`--function`) | — | none | Filter by function. | `cache query --fn transfer` |
| `cache query` | `--wasm-hash <PREFIX>` | — | none | Filter by WASM hash. | `cache query --wasm-hash a1b2` |
| `cache query` | `--min-fee <STROOPS>` (`--min-stroops`) | — | none | Minimum total fee in stroops. | `cache query --min-fee 1000` |
| `cache query` | `--max-fee <STROOPS>` (`--max-stroops`) | — | none | Maximum total fee in stroops. | `cache query --max-fee 5000` |
| `cache query` | `--since <DATE/TIME>` (`--from`), `--to <TIMESTAMP>` | — | none | Filter by timestamp range. | `cache query --since 2025-01-01` |
| `cache query` | `--json` | — | `false` | Emit results as JSON. | `cache query --json` |
| `cache stats` | `--json` | — | `false` | Emit cache health and disk usage as JSON. | `cache stats --json` |

`config cache <subcommand>` exposes the same cache subcommands and options.

## Other `config` subcommands

| Command | Flag / argument | Short | Default | Description | Example |
|---|---|---|---:|---|---|
| `config list` | `--network <NAME>` | — | `testnet` | List snapshots for a network. | `config list --network mainnet` |
| `config history` | `--network <NAME>` | — | `testnet` | Show saved config history for a network. | `config history --network testnet` |
| `config last-changed` | `--network <NAME>` | — | `testnet` | Show when network config last changed. | `config last-changed --network testnet` |
| `config validate` | `--network <NAME>` | — | `testnet` | Validate network configuration. | `config validate --network testnet` |
| `config export` | `--network <NAME>` | — | all networks | Limit the exported snapshot bundle. | `config export --network testnet --output snapshots.json` |
| `config export` | `--output <PATH>` | — | required | Destination for the bundle. | `config export --output snapshots.json` |
| `config import` | `<BUNDLE>` | — | required | Import a snapshot bundle. | `config import snapshots.json` |

## Environment variables

| Variable | Default | Description | Example |
|---|---|---|---|
| `SOROBAN_NETWORK` | command default (`testnet` for network commands) | Select the network when a command supports environment overrides. | `SOROBAN_NETWORK=mainnet soroban-cost-estimator config snapshot` |
| `SOROBAN_JSON` | `false` | Select JSON as the default output format; explicit `--format` takes precedence. | `SOROBAN_JSON=true soroban-cost-estimator estimate --wasm contract.wasm` |
| `XDG_CONFIG_HOME` | platform default | Base directory used to locate `soroban-cost-estimator/config.toml`. | `XDG_CONFIG_HOME=/tmp/config soroban-cost-estimator --help` |
| `HOME` / `USERPROFILE` | operating-system account home | Fallback base for configuration and application data paths. | `HOME=/tmp soroban-cost-estimator cache stats` |
| `NO_COLOR` | unset | Disable terminal colors when set. | `NO_COLOR=1 soroban-cost-estimator config diff` |
| `COLUMNS` | detected/default width | Terminal column width used for report charts. | `COLUMNS=100 soroban-cost-estimator estimate --wasm contract.wasm` |
| `RUST_LOG` | unset | Set tracing subscriber verbosity/filtering. | `RUST_LOG=debug soroban-cost-estimator estimate --wasm contract.wasm` |

## Exit codes

| Code | Meaning | Example |
|---:|---|---|
| `0` | Command completed successfully. A watch stopped by SIGINT/SIGTERM also exits successfully. | `config snapshot` succeeded. |
| `1` | Runtime or application error; also used by `config diff` when its configured pricing/change gate is met. | RPC failure, invalid WASM, or `config diff --fail-on-any-change` detected a change. |
| `2` | Command-line usage error reported by `clap`, such as an unknown option or missing required argument. | `estimate` without `--wasm` or `--wasm-dir`. |
| `101` | CLI worker thread panicked. | An unexpected panic escaped command handling. |
