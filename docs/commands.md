# Command Reference

Every `soroban-cost-estimator` command is documented below with its full flag
table, behavioral notes, and runnable examples. Commands are grouped by
functionality: **estimation**, **network config management**, and **cache
management**.

## Table of Contents

- [Estimation](#estimation)
  - [`estimate`](#estimate) — simulate a single invocation
  - [`estimate-all`](#estimate-all) — enumerate and estimate every function
- [Network Config](#network-config)
  - [`config snapshot`](#config-snapshot) — fetch and save config settings
  - [`config diff`](#config-diff) — compare config against a snapshot
  - [`config history`](#config-history) — chronological change log
  - [`config last-changed`](#config-last-changed) — last-change timestamps
- [Cache Management](#cache-management)
  - [`cache verify`](#cache-verify) — check cache integrity
  - [`cache warm`](#cache-warm) — pre-populate cache
- [Monitoring](#monitoring)
  - [`watch`](#watch) — poll and diff on interval

---

## Estimation

### `estimate`

Simulate a single contract invocation and print the cost report.

**Usage**

```
soroban-cost-estimator estimate [OPTIONS] --wasm <WASM>
```

**Flags**

| Flag | Short | Required | Default | Description |
|------|-------|----------|---------|-------------|
| `--wasm <WASM>` | `-w` | ✅ | — | Path to the compiled Soroban contract `.wasm` file |
| `--network <NETWORK>` | | | `testnet` | Network to simulate against (`testnet`, `mainnet`, `futurenet`) |
| `--rpc-url <RPC_URL>` | | | — | Explicit RPC URL (overrides network-based resolution) |
| `--fn <FN>` | | | — | Contract function name to invoke |
| `--id <ID>` | | | — | Deployed contract ID (64 hex chars). Required when `--fn` is used |
| `--arg <KEY=VAL>` | | | — | Function arguments as `key=value` pairs (value is type-inferred; repeatable) |
| `--cache-ttl <DURATION>` | | | — | Skip re-simulation when a cached estimate is still fresh (e.g. `30m`, `1h`, `7d`) |
| `--json` | | | `false` | Output as JSON instead of a human-readable table |
| `--help` | `-h` | | | Print help |

**Behavior**

- **Without `--fn`** — simulates *uploading* the contract WASM to the network.
  The report's function is `(wasm upload)`.
- **With `--fn`** — simulates invoking a specific contract function against a
  **deployed** contract. `--id <64-hex>` is required because
  `simulateTransaction` loads the contract instance from the ledger; it cannot
  simulate against a zeroed ID.
- **`--arg` values are type-inferred**: `true`/`false` → bool, integers →
  `i64`/`u64`, everything else → string. This is enough for cost estimation.
- The read/write entry counts and byte sizes are decoded from the simulation
  response's resource **footprint** — real values from the ledger footprint, not
  zero-filled placeholders.
- If a fee-rate source (`ConfigSetting*`) can't be fetched, a warning naming
  the source is printed and only the affected rate is zeroed, so the
  non-refundable fee is visibly understated rather than silently wrong.
- A simulation that returns no cost data and no latest ledger fails loudly
  with an error naming `--id`, `--fn`, and the RPC endpoint.
- **Caching**: The result is written to the estimate cache, keyed by
  `wasm_hash + function_name + args_hash`. Use `--cache-ttl` to skip
  re-simulation when the cached entry is still fresh.
- **Exit codes**: 0 on success, 1 on any error (simulation failure, missing
  WASM file, network error, etc.).

**Examples**

Upload simulation (no function invocation):

```bash
soroban-cost-estimator estimate \
  --wasm tests/fixtures/contract.wasm \
  --network testnet
```

Invoke a function against a deployed contract:

```bash
soroban-cost-estimator estimate \
  --wasm tests/fixtures/contract.wasm \
  --id CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T \
  --network testnet \
  --fn increment \
  --arg step=5
```

Machine-readable JSON output (useful for CI pipelines):

```bash
soroban-cost-estimator estimate \
  --wasm tests/fixtures/contract.wasm \
  --id CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T \
  --network testnet \
  --fn increment \
  --arg step=5 \
  --json
```

Skip re-simulation if a cached estimate is fresh enough:

```bash
soroban-cost-estimator estimate \
  --wasm tests/fixtures/contract.wasm \
  --id CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T \
  --network testnet \
  --fn increment \
  --arg step=5 \
  --cache-ttl 1h
```

**Sample table output**

```text
Function: increment
Network: testnet (ledger 3961551)
WASM hash: ea14bca998e98f0ddb338e8e5cef6e19f07378a3b71e8b4f8868cedc857e4ecd

+------------------+----------+---------------+
| Resource         | Consumed | Fee (stroops) |
+=============================================+
| CPU Instructions | 524389   |               |
|------------------+----------+---------------+
| Memory Bytes     | 0        |               |
|------------------+----------+---------------+
| Read Entries     | 1        |               |
|------------------+----------+---------------+
| Write Entries    | 1        |               |
|------------------+----------+---------------+
| Read Bytes       | 0        |               |
|------------------+----------+---------------+
| Write Bytes      | 136      |               |
|------------------+----------+---------------+
| Transaction Size | 156      |               |
+------------------+----------+---------------+

Fee Breakdown:
  Non-refundable: 4491 stroops
  Refundable:     12631 stroops
  Total:          17122 stroops (0.0017122)
```

**Sample JSON output**

```json
{
  "function": "increment",
  "wasm_hash": "ea14bca998e98f0ddb338e8e5cef6e19f07378a3b71e8b4f8868cedc857e4ecd",
  "cpu_instructions": 524389,
  "memory_bytes": 0,
  "tx_size": 156,
  "read_entries": 1,
  "write_entries": 1,
  "read_bytes": 0,
  "write_bytes": 136,
  "fee": {
    "non_refundable_stroops": 4491,
    "refundable_stroops": 12631,
    "total_stroops": 17122,
    "total_xlm": "0.0017122"
  },
  "ledger": 3961544,
  "network": "testnet"
}
```

See [Resource Fees](concepts/resource-fees.md) for the fee-breakdown math.

---

### `estimate-all`

Enumerate every public contract function and estimate each zero-argument one.

**Usage**

```
soroban-cost-estimator estimate-all [OPTIONS] --wasm <WASM>
```

**Flags**

| Flag | Short | Required | Default | Description |
|------|-------|----------|---------|-------------|
| `--wasm <WASM>` | `-w` | ✅ | — | Path to the compiled Soroban contract `.wasm` file |
| `--network <NETWORK>` | | | `testnet` | Network to simulate against |
| `--id <ID>` | | | — | Deployed contract ID (64 hex chars) to invoke each function against |
| `--json` | | | `false` | Output as JSON instead of a human-readable list |
| `--help` | `-h` | | | Print help |

**Behavior**

- Enumerates the contract's public functions from the WASM, decoding **typed
  parameter lists** from the `contractspecv0` section when present.
- Estimates each **zero-argument** function and prints a `[i/N]` progress
  line before every simulation, so you can watch progress on contracts with
  many functions.
- Functions requiring arguments are reported as
  `Skipped — needs --fn/--arg (N param(s))`, prompting you to specify them
  manually — they are **not** silently skipped.
- Without `--id`, simulations run against a zeroed contract ID and will
  almost certainly fail; the tool prints a note telling you to pass `--id`
  for real numbers.
- All zero-argument function results are cached, just like `estimate`.
- **Exit codes**: 0 on success, 1 on error.

**Examples**

```bash
soroban-cost-estimator estimate-all \
  --wasm tests/fixtures/contract.wasm \
  --id CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T \
  --network testnet
```

**Sample output**

```text
Enumerated 1 function(s) in WASM:
  1. increment(step: i64)

Contract spec: present (typed params decoded from contractspecv0)
[1/1] increment
── Estimating 'increment' ── Skipped: needs --fn/--arg (1 param(s))
```

JSON mode:

```bash
soroban-cost-estimator estimate-all \
  --wasm tests/fixtures/contract.wasm \
  --id CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T \
  --network testnet \
  --json
```

```json
[
  {
    "function": "increment",
    "status": "skipped",
    "reason": "needs --fn/--arg (1 param(s))"
  }
]
```

---

## Network Config

### `config snapshot`

Fetch all six `ConfigSetting` ledger entries, decode them via XDR, timestamp
them, and save to disk.

**Usage**

```
soroban-cost-estimator config snapshot [OPTIONS]
```

**Flags**

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--network <NETWORK>` | | `testnet` | Network to fetch config from (`testnet`, `mainnet`, `futurenet`) |
| `--out <OUT>` | | `~/.soroban-cost-estimator/snapshots/` | Explicit output path |
| `--json` | | `false` | Print the snapshot as JSON (still saves it) |
| `--help` | `-h` | | Print help |

**Behavior**

- Fetches all six `ConfigSetting*` entries in **one batched**
  `getLedgerEntries` RPC call.
- Decodes each entry's XDR (`stellar-xdr` 27.x, big-endian) into a typed
  snapshot model.
- Saves the snapshot as
  `~/.soroban-cost-estimator/snapshots/{network}-{timestamp}.json` — the
  timestamp makes every snapshot a versioned artifact.
- `--json` also prints the full snapshot as JSON to stdout.
- `--out` writes to an explicit path instead of the default directory.

**What you get**

The snapshot JSON contains decoded values for all six settings:

| Setting | Key fields |
|---------|------------|
| `contract_compute` | `fee_rate_per_instructions_increment`, memory limits |
| `contract_ledger_cost` | Read/write entry fees, per-KB disk fees, rent rates |
| `contract_historical_data` | `fee_historical1_kb` |
| `contract_events` | `fee_contract_events1_kb` |
| `contract_bandwidth` | `fee_tx_size1_kb` |
| `state_archival` | TTLs, rent-rate denominators, eviction policy |

Take a fresh snapshot after every protocol vote and keep them around:
[`config diff`](#config-diff) compares the current configuration against your
most recent snapshot.

**Examples**

```bash
soroban-cost-estimator config snapshot --network testnet
```

**Sample output**

```text
Config snapshot saved to: /home/you/.soroban-cost-estimator/snapshots/testnet-2026-08-04T07-15-38.487702259+00-00.json
Network: testnet
Ledger:  3470630
Time:    2026-08-04T07:15:38.487702259+00:00
```

The printed `Ledger` is the last ledger at which the config entries were
modified on-chain — it is *not* the network's current ledger, and that is
intentional: it is the ledger against which stale-cache checks are made.

---

### `config diff`

Compare the network's current resource-pricing configuration against the most
recent snapshot.

**Usage**

```
soroban-cost-estimator config diff [OPTIONS]
```

**Flags**

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--network <NETWORK>` | | `testnet` | Network to compare against |
| `--against <AGAINST>` | | latest snapshot | Explicit snapshot path to compare against |
| `--help` `-h` | | | Print help |

**Behavior**

- Fetches the current config, then compares it **field by field** against the
  latest snapshot for the network (or the snapshot at `--against`).
- `💰` marks a **pricing** change — a rate that feeds the fee math; `📋`
  marks a non-pricing change (a cap, limit, or window size).
- Always cross-references the estimate cache and reports cached estimates
  recorded at an earlier ledger as potentially stale.
- **Exit code 0** when nothing changed; **exit code 1** when a pricing change
  was detected — scripts and CI can branch on it.
- When a pricing change (protocol/config upgrade) is detected, the new config
  is **automatically saved** as a snapshot, so it becomes the baseline for the
  next diff. A failed save is reported as a warning and does not change the
  exit code.

**Examples**

Basic diff against the latest snapshot:

```bash
soroban-cost-estimator config diff --network testnet
```

Diff against an explicit snapshot:

```bash
soroban-cost-estimator config diff --network testnet \
  --against ~/.soroban-cost-estimator/snapshots/testnet-2026-08-04T07-15-38.487702259+00-00.json
```

**Sample output (no changes)**

```text
Config diff: 2026-08-04T07:15:38.487702259+00:00 (ledger 3470630) → 2026-08-04T07:15:39.237129507+00:00 (ledger 3470630)
Network: testnet

✅ No changes detected.

  1 cached estimate(s) from earlier ledger(s) — may be stale:
    - (wasm upload) @ ledger 0 (current: 3470630)
```

**Sample output (pricing change)**

```text
Config diff: 2026-07-31T13-14-29.307394561+00:00 (ledger 3470630) → 2026-08-04T07:15:39.237129507+00:00 (ledger 3470630)
Network: testnet

Found 1 field change(s):

  💰 contract_compute.fee_rate_per_instructions_increment
      Old: 5
      New: 7

⚠️  Pricing changes detected! Your cached estimates may be stale.
  Protocol upgrade detected — new config auto-saved to ~/.soroban-cost-estimator/snapshots/testnet-<timestamp>.json
```

See [Config Drift](concepts/config-drift.md) for the workflow this enables.

---

### `config history`

Show the full chronological change log across all stored snapshots for a
network.

**Usage**

```
soroban-cost-estimator config history [OPTIONS]
```

**Flags**

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--network <NETWORK>` | | `testnet` | Network whose snapshot history to inspect |
| `--help` | `-h` | | Print help |

**Behavior**

- Loads all stored snapshots for the given network from
  `~/.soroban-cost-estimator/snapshots/`.
- Orders them chronologically and prints a field-by-field change log showing
  what changed between consecutive snapshots.
- Useful for understanding the full history of network pricing changes over
  time — not just the most recent diff.

**Examples**

```bash
soroban-cost-estimator config history --network testnet
```

**Sample output**

```text
Config change history for testnet (3 snapshots):

Snapshot 1 → 2 (2026-08-01 → 2026-08-03, ledger 3400000 → 3450000):
  💰 contract_compute.fee_rate_per_instructions_increment: 5 → 7

Snapshot 2 → 3 (2026-08-03 → 2026-08-04, ledger 3450000 → 3470630):
  ✅ No changes detected.
```

---

### `config last-changed`

Show when each config setting last changed, across all stored snapshots.

**Usage**

```
soroban-cost-estimator config last-changed [OPTIONS]
```

**Flags**

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--network <NETWORK>` | | `testnet` | Network whose snapshot history to inspect |
| `--help` | `-h` | | Print help |

**Behavior**

- Loads all stored snapshots for the given network and determines the last
  point at which each individual setting changed.
- Prints a per-setting timestamp and snapshot reference.
- Helpful for identifying which settings have been stable for a long time
  versus which changed recently.

**Examples**

```bash
soroban-cost-estimator config last-changed --network testnet
```

**Sample output**

```text
Last-changed timestamps for testnet:

  contract_compute:                  2026-08-01 (snapshot testnet-2026-08-01T...)
  contract_ledger_cost:              2026-08-01 (snapshot testnet-2026-08-01T...)
  contract_historical_data:          never changed
  contract_events:                   never changed
  contract_bandwidth:                never changed
  state_archival:                    never changed
```

---

## Cache Management

### `cache verify`

Check that every cached estimate is valid JSON and not corrupted.

**Usage**

```
soroban-cost-estimator cache verify [OPTIONS]
```

**Flags**

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--help` | `-h` | | Print help |

**Behavior**

- Reads every file in `~/.soroban-cost-estimator/cache/` and attempts to
  parse it as a valid `CachedEstimate` JSON entry.
- Prints a summary line per corrupted entry (filename).
- **Exit code 0** if the cache is empty or every entry is valid.
- **Exit code 1** and lists corrupted filenames if any entry fails.
- Scripts and CI can treat a corrupt cache as an error condition.
- No network calls are made — this is pure file I/O.

**Examples**

```bash
soroban-cost-estimator cache verify
```

**Sample output (healthy cache)**

```text
Checked 5 cache entries.
All cache entries are valid.
```

**Sample output (corrupt cache)**

```text
Checked 5 cache entries.
2 of 5 cache entries failed verification:
  - abc123_increment_step5.json
  - def456_upload_.json
```

---

### `cache warm`

Pre-populate the cache by estimating every exported function (equivalent to
running [`estimate-all`](#estimate-all)).

**Usage**

```
soroban-cost-estimator cache warm [OPTIONS] --wasm <WASM>
```

**Flags**

| Flag | Short | Required | Default | Description |
|------|-------|----------|---------|-------------|
| `--wasm <WASM>` | `-w` | ✅ | — | Path to the compiled Soroban contract `.wasm` file |
| `--network <NETWORK>` | | | `testnet` | Network to simulate against |
| `--id <ID>` | | | — | Deployed contract ID (64 hex chars) to invoke each function against |
| `--json` | | | `false` | Output as JSON instead of a human-readable list |
| `--help` | `-h` | | | Print help |

**Behavior**

- Internally delegates to `estimate-all` — enumerates all exported functions
  and simulates each zero-argument one against the network.
- Each successful simulation is saved to the cache, keyed by
  `wasm_hash + function_name + args_hash`.
- Useful for warming the cache before running `estimate` with `--cache-ttl`,
  so the first call in a CI pipeline hits the cache instead of triggering a
  fresh simulation.

**Examples**

```bash
soroban-cost-estimator cache warm \
  --wasm tests/fixtures/contract.wasm \
  --id CC4WIEYYSCFGDJXMLZ73FKUUJNDEOJRNOOBZHI55QR27NW4RCNTHAQ5T \
  --network testnet
```

---

## Monitoring

### `watch`

Poll the network's resource-pricing configuration on an interval and print a
diff whenever something changes.

**Usage**

```
soroban-cost-estimator watch [OPTIONS]
```

**Flags**

| Flag | Required | Default | Description |
|------|----------|---------|-------------|
| `--network <NETWORK>` | | `testnet` | Network to watch |
| `--interval <INTERVAL>` | | `1h` | Polling interval (accepts `s`/`m`/`h`/`d` suffixes or bare seconds) |
| `--help` | `-h` | | Print help |

**Behavior**

- Polls **immediately**, then every `--interval`.
- Intervals accept `s`/`m`/`h`/`d` suffixes or bare seconds:
  `3600`, `3600s`, `30m`, `1h`, `1d`. Unparseable input falls back to
  one hour.
- Each poll fetches the config, diffs it against the previous snapshot, and
  prints the diff **only when something changed**, plus the same stale-cache
  cross-reference as `config diff`.
- **SIGINT (Ctrl-C) and SIGTERM shut it down cleanly** (exit code 0): the
  in-flight poll is cancelled rather than writing a partial snapshot.
- Each poll also saves a snapshot, so you always have a recent baseline for
  manual `config diff` runs.

**Examples**

Watch testnet every 30 minutes:

```bash
soroban-cost-estimator watch --network testnet --interval 30m
```

Watch mainnet every 10 minutes (useful in CI or cron):

```bash
soroban-cost-estimator watch --network mainnet --interval 10m
```

**Sample output**

```text
Watching testnet for config changes every 3600s... (Ctrl-C to stop)
Received stop signal — exiting cleanly.
```

(The interval prints in seconds: `--interval 1h` → `every 3600s`.)

---

## Global Options

All commands accept:

| Flag | Description |
|------|-------------|
| `--help` / `-h` | Print command-specific help |

## Network Resolution

By default, commands use these RPC endpoints:

| Network | Endpoint |
|---------|----------|
| `testnet` | `https://soroban-testnet.stellar.org` |
| `mainnet` | `https://soroban.stellar.org` |
| `futurenet` | `https://rpc-futurenet.stellar.org` |

Override with `--rpc-url` on the `estimate` command for custom endpoints.

## Storage Locations

All data is stored locally under `~/.soroban-cost-estimator/`:

| Directory | Contents |
|-----------|----------|
| `snapshots/` | Timestamped config snapshots (JSON) |
| `cache/` | Past estimate results, keyed by wasm hash + function + args hash |

## Related Pages

- [Resource Fees](concepts/resource-fees.md) — how fees are calculated
- [Config Drift](concepts/config-drift.md) — the drift-tracking workflow
- [Caching](concepts/caching.md) — how the estimate cache works
- [Migration](migration.md) — migrating from the Stellar CLI
- [FAQ](faq.md) — frequently asked questions
