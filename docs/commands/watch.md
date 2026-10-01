# `watch`

Watch the network's resource-pricing configuration and print a diff whenever
something changes. `watch` subscribes to **ledger-close notifications over a
WebSocket** so config changes are re-checked the moment the ledger lands, and
falls back to interval polling when WebSockets are unavailable.

## Flags

```
Usage: soroban-cost-estimator watch [OPTIONS]

Options:
      --network <NETWORK>    Network to watch [default: testnet]
      --rpc-url <RPC_URL>    Explicit RPC URL for both the WebSocket subscription
                             and the config fetches, `wss://`/`ws://` or
                             `https://`/`http://` (overrides network-based resolution)
      --interval <INTERVAL>  Polling interval (e.g. "30m", "1h"); used as the
                             fallback when WebSocket ledger-close notifications
                             are unavailable [default: 1h]
  -h, --help                 Print help
```

Global flags (`--rps`, `--timeout`, `--header`, `--rpc-fallback-url`,
`--max-retries`, `--verbose`) also apply.

## Behavior

- Connects to the RPC node's WebSocket endpoint (`wss://…/ws`), which is
  derived from the network's HTTP endpoint — or taken from `--rpc-url`, where
  `https://host` and `wss://host` are both accepted.
- **Re-checks the config on every ledger close** instead of waiting for a fixed
  interval. The first snapshot is taken immediately, so the first diff has a
  baseline to compare against.
- **Reconnects with exponential backoff** (1s, 2s, 4s, … capped at 30s) when
  the connection drops, then resubscribes from the last ledger seen.
- **Falls back to HTTP polling** every `--interval` when the endpoint cannot be
  reached or does not support WebSocket subscriptions, so `watch` keeps working
  against any node. Intervals accept `s`/`m`/`h`/`d` suffixes or bare seconds:
  `3600`, `3600s`, `30m`, `1h`, `1d`. Unparseable input falls back to one hour.
- Each poll fetches the config, diffs it against the previous snapshot, and
  prints the diff (only when something changed), plus the same stale-cache
  cross-reference as `config diff`.
- **SIGINT (Ctrl-C) and SIGTERM shut it down cleanly** (exit code 0): the
  in-flight poll is cancelled rather than writing a partial snapshot.

## Example

```bash
soroban-cost-estimator watch --network testnet --interval 30m
```

Actual output from a live run (stopped with SIGTERM):

```text
Watching testnet for config changes every 1800s... (Ctrl-C to stop)
Subscribed to ledger-close notifications from ledger 3894195 — re-checking config on every new ledger.
Ledger 3894196 closed — re-checking config.
Received stop signal — exiting cleanly.
```

(The interval prints in seconds: `--interval 30m` → `every 1800s`.)

## Example — poll every 10 minutes in CI

```bash
soroban-cost-estimator watch --network mainnet --interval 10m
```

Run under `systemd`, cron, or a CI job, `watch` gives you a prompt the moment
the network's pricing model changes — and the diff output tells you exactly
which pricing fields moved and which cached estimates are affected.

## Example — private or local node

```bash
soroban-cost-estimator watch --rpc-url ws://127.0.0.1:8000/ws --interval 5m
```

The same URL is also used for the config fetches (`ws://` becomes `http://`
for those), so `watch` never talks to a different node than the one it
subscribes to.
