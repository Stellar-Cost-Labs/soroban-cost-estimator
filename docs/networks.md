# Network Configuration & RPC Provider Reference

This guide lists all Stellar Soroban networks supported by `soroban-cost-estimator`, their default JSON-RPC and WebSocket endpoints, network passphrases, known public and commercial RPC providers, and local development container setup.

---

## 1. Built-In Network Presets

Pass `--network <name>` to any command (`estimate`, `estimate-all`, `config snapshot`, `config diff`, `watch`) to select a built-in network preset:

| Network (`--network`) | Default HTTP RPC Endpoint | Default WebSocket Endpoint | Network Passphrase | Chain ID (SHA-256 Prefix) | Protocol Version |
|---|---|---|---|---|---|
| `testnet` *(default)* | `https://soroban-testnet.stellar.org` | `wss://soroban-testnet.stellar.org/ws` | `Test SDF Network ; September 2015` | `cee0302d59844d32` | Protocol 21+ |
| `mainnet` | `https://soroban.stellar.org` | `wss://soroban.stellar.org/ws` | `Public Global Stellar Network ; September 2015` | `7ac33997544e3175` | Protocol 21+ |
| `futurenet` | `https://rpc-futurenet.stellar.org` | `wss://rpc-futurenet.stellar.org/ws` | `Test SDF Future Network ; October 2022` | `d2c2882848802685` | Preview / Next |
| `local` *(via `--rpc-url`)* | `http://localhost:8000/soroban/rpc` | `ws://localhost:8000/soroban/rpc/ws` | `Standalone Network ; February 2017` | `a9425a27d70b5b53` | Configurable |

---

## 2. Public vs. Private RPC Providers

| Provider | Networks | Rate Limits / Notes | Authentication |
|---|---|---|---|
| **Stellar Development Foundation (SDF)** | Testnet, Futurenet | Free public endpoints for testing and development | None |
| **Validation Cloud** | Mainnet, Testnet | High-throughput enterprise RPC & archival nodes | API Key in URL path or `X-API-Key` header |
| **Blockdaemon** | Mainnet, Testnet | Dedicated and shared Soroban JSON-RPC endpoints | `Authorization: Bearer <TOKEN>` or `X-API-Key` |
| **QuickNode** | Mainnet, Testnet | Low-latency global endpoints with WebSocket streaming | Token in endpoint URL or `Authorization` header |
| **NowNodes** | Mainnet, Testnet | Pay-per-request and tiered RPC access | `api-key: <KEY>` header |
| **Ankr / Gateway.fm** | Mainnet, Testnet | Public and premium tiered RPC endpoints | Optional Bearer token |

> **Production Recommendation:** Public Mainnet endpoints enforce strict rate limits (`429 Too Many Requests`). For CI/CD pipelines, batch `estimate-all` runs, or continuous `watch` monitoring on Mainnet, configure a dedicated private RPC provider with `--rpc-url` and `--header`.

---

## 3. Configuring Custom RPC Endpoints & API Keys

### 3.1 Via Command-Line Flags (`--rpc-url` and `--header` / `-H`)

Override the default RPC endpoint with `--rpc-url` and attach authentication headers with `-H` / `--header`:

```bash
soroban-cost-estimator estimate \
    --wasm contract.wasm \
    --network mainnet \
    --rpc-url https://soroban-mainnet.provider.example.com \
    -H "Authorization=Bearer $SOROBAN_RPC_TOKEN" \
    -H "X-API-Key=$PROVIDER_API_KEY"
```

- Both `KEY=VALUE` and `KEY: VALUE` syntaxes are supported.
- Sensitive headers (`Authorization`, `Proxy-Authorization`, `X-API-Key`, `API-Key`, `Cookie`) are automatically redacted as `<redacted>` in `--verbose` and `RUST_LOG=debug` output.
- Configure fallback endpoints for automatic failover with `--fallback-rpc-url <URL>`.

### 3.2 Via Environment Variables

```bash
export SOROBAN_RPC_URL="https://soroban-mainnet.provider.example.com"
soroban-cost-estimator config snapshot --network mainnet --rpc-url "$SOROBAN_RPC_URL"
```

### 3.3 Via Project Configuration (`config.toml`)

You can persist your default network, custom RPC endpoints, and timeout settings in `.soroban-cost-estimator.toml` (or `~/.soroban-cost-estimator/config.toml`):

```toml
default_network = "mainnet"
timeout_secs = 30
connect_timeout_secs = 5
max_retries = 3

[networks.mainnet]
rpc_url = "https://soroban-mainnet.provider.example.com"
fallback_rpc_urls = [
    "https://backup-rpc.provider.example.com"
]

[networks.mainnet.headers]
Authorization = "Bearer ${SOROBAN_RPC_TOKEN}"
X-API-Key = "${PROVIDER_API_KEY}"
```

---

## 4. Local Standalone Setup with `stellar/quickstart`

To run deterministic offline or local integration simulations against a private standalone node, launch the official `stellar/quickstart` Docker container:

```bash
docker run --rm -it \
    -p 8000:8000 \
    --name stellar-local \
    stellar/quickstart:latest \
    --local \
    --enable-soroban-rpc
```

Once the container is healthy (`curl http://localhost:8000/soroban/rpc`), fund a test account via the local Friendbot (`http://localhost:8000/friendbot?addr=<G...>`) and point `soroban-cost-estimator` to the local RPC URL:

```bash
# Take a snapshot of the local standalone network config
soroban-cost-estimator config snapshot \
    --network testnet \
    --rpc-url http://localhost:8000/soroban/rpc

# Estimate contract upload cost against the local container
soroban-cost-estimator estimate \
    --wasm target/wasm32-unknown-unknown/release/my_contract.wasm \
    --network testnet \
    --rpc-url http://localhost:8000/soroban/rpc
```
