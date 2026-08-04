# FAQ

## Why do fee numbers drift between runs of the same invocation?

Because `simulateTransaction` applies a margin of roughly 20% as a buffer
convention, and the rent component varies slightly with ledger state. The
recorded cross-check demonstrates this: the same `increment(step=5)` call
reported 18,999 stroops in the original run, 19,001 an earlier run (~40–50
ledgers prior), 17,606 on a re-check ~46,000 ledgers later, and 17,122 on the
most recent re-check. CPU instructions, entry counts, and byte sizes
reproduce exactly — the variance is a 1–2 stroop run-to-run rent difference
plus slow rate/rent drift on the network, not a math error. If you need
number stability, treat the fee as "approximately right within ~20%" and the
resource footprint as exact.

## Why does `--fn` require `--id`?

`simulateTransaction` loads the contract instance from the ledger by its
contract ID. A zeroed ID is not a valid instance, so the tool refuses to
guess: when `--fn` is given, `--id <64-hex>` must name a deployed contract.
Without `--fn`, no instance is needed — the tool simulates uploading the
WASM itself.

## What happens when a `ConfigSetting*` source can't be fetched?

The affected rate is set to 0 and a warning naming the source is printed to
stderr — for example:

```
Warning: fee rate source(s) ContractComputeV0 unavailable — affected rate(s) set to 0 (non-refundable fee understated)
```

The non-refundable fee is then *visibly understated* rather than silently
wrong: the CPU fee would compute as 0 stroops while the authoritative total
still comes from `simulateTransaction`. Treat a warning as a hard "do not
trust this number" signal and re-run once the RPC is healthy.

## Why is `Memory Bytes` 0 on modern RPC endpoints?

CPU instructions and memory were historically reported in a `cost` object in
the `simulateTransaction` response. Modern RPC versions dropped that object
and carry resource usage inside `transactionData` XDR — which includes CPU
instructions and footprint entry/byte counts but not memory bytes. The tool
reports what the response actually contains: memory is 0 because modern RPC
responses no longer expose it.

## Can the refundable portion really be refunded?

The refundable portion covers events and ledger **rent** — the deposit for
keeping your contract's data alive. Whether and how much comes back depends
on how long the data lives versus the TTL you pay for. The tool reports it
separately so you can reason about it; it does not predict the refund.

## Which networks can I use?

`testnet`, `mainnet`, and `futurenet` are resolved to the well-known
endpoints (see [Installation](installation.md)). Any other network name is
rejected unless you pass an explicit `--rpc-url`, which overrides resolution
for every command.

## Does the tool submit transactions?

No. Every command simulates only — nothing is submitted to the network. The
native-CLI cross-check in [Verification](verification.md) runs with
`--send=no` for the same reason.
