# Stellar network and RPC reference

Use this page to choose a Stellar network and Stellar RPC endpoint for `soroban-cost-estimator`. The built-in endpoints match `src/rpc/client.rs`. Protocol versions, provider offerings, and service availability change; consult the official sources linked below before relying on them.

## Built-in networks

| Network | CLI default RPC URL | Network passphrase | Network ID (SHA-256) | Protocol version | Friendbot |
|---|---|---|---|---|---|
| Testnet (`testnet`, default) | `https://soroban-testnet.stellar.org` | `Test SDF Network ; September 2015` | `cee0302d59844d32bdca915c8203dd44b33fbb7edc19051ea37abedf28ecd472` | 29 (as of 2026-10-21) | `https://friendbot.stellar.org` |
| Mainnet (`mainnet`) | `https://soroban.stellar.org` | `Public Global Stellar Network ; September 2015` | `7ac33997544e3175d266bd022439b22cdb16508c01163f26e5cb2a3e1045a979` | 29 (as of 2026-10-21) | None |
| Futurenet (`futurenet`) | `https://rpc-futurenet.stellar.org` | `Test SDF Future Network ; October 2022` | `a3a1c6a78286713e29be0e9785670fa838d13917cd8eaeb4a3579ff1debc7fd5` | Pre-release network; changes with test releases | `https://friendbot-futurenet.stellar.org` |
| Local Quickstart (`local`) | No built-in URL; use `http://localhost:8000/rpc` | `Standalone Network ; February 2017` | `baefd734b8d3e48472cff83912375fedbc7573701912fe308af730180f97d74a` | Quickstart latest by default; configurable | `http://localhost:8000/friendbot` |

A Stellar Network ID (also called a chain ID in other ecosystems) is the SHA-256 hash of the exact network passphrase. It is used in transaction signing and contract address generation. The local ID above hashes Quickstart's default standalone passphrase; a custom passphrase has a different ID. See [Stellar networks](https://developers.stellar.org/docs/networks).

The protocol version is a moving network property. Mainnet and Testnet use Protocol 29 as of the date above. Futurenet tests unreleased changes; local Quickstart can select a version with `--protocol-version`. Check [Stellar software versions](https://developers.stellar.org/docs/networks/software-versions) and the [Quickstart options](https://github.com/stellar/quickstart) for current releases.

The CLI accepts `local` as a network name but has no built-in local URL. Provide `--rpc-url` or configure `local` under `[rpc_urls]`.

## Public and private RPC providers

The [official Stellar RPC provider directory](https://developers.stellar.org/docs/data/apis/rpc/providers) lists current network coverage, dedicated-node availability, and archive access. Provider offerings and access requirements change; use this directory as the source of truth, not as an endorsement or uptime guarantee.

| Provider | Testnet | Mainnet | Futurenet | Provider notes |
|---|---:|---:|---:|---|
| [SDF](https://developers.stellar.org/docs/data/apis/rpc/providers) | Yes | No public SDF endpoint listed | Yes | Public development endpoints. |
| [Blockdaemon](https://www.blockdaemon.com/) | Yes | Yes | No | Shared and dedicated offerings. |
| [Validation Cloud](https://validationcloud.io/) | Yes | Yes | No | Dedicated nodes and archive offering listed. |
| [QuickNode](https://www.quicknode.com/) | Yes | Yes | No | Dedicated nodes listed. |
| [NowNodes](https://nownodes.io/) | Yes | Yes | Yes | Dedicated nodes listed. |
| [Gateway](https://gateway.fm/) | Yes | Yes | No | Dedicated nodes and archive listed. |
| [Ankr](https://www.ankr.com/) | Yes | Yes | No | Archive access listed. |
| [Infstones](https://infstones.com/) | No | Yes | No | Dedicated nodes listed. |
| [Obsrvr](https://www.withobsrvr.com/) | Yes | Yes | No | Archive access listed. |
| [Nodies](https://nodies.org/) | Yes | Yes | No | Public endpoints listed. |
| [OnFinality](https://onfinality.io/) | No | Yes | No | Dedicated nodes and archive listed. |
| [Lightsail / Quasar](https://quasar.lightsail.network/) | No | Yes | No | Public RPC and archive endpoints listed. |
| [Uniblock](https://www.uniblock.dev/) | Yes | Yes | No | Provider plans vary. |
| [Exaion](https://exaion.edf.fr/) | No | Yes | No | Dedicated nodes and archive listed. |
| [Alchemy](https://www.alchemy.com/) | Yes | Yes | No | Dedicated nodes listed. |
| [GetBlock](https://getblock.io/) | No | Yes | No | Dedicated nodes and archive listed. |
| [node101](https://node101.io/) | Yes | Yes | No | Dedicated nodes and archive listed. |

Public RPC URLs currently documented by Stellar include:

| Provider | Network | URL |
|---|---|---|
| SDF | Testnet | `https://soroban-testnet.stellar.org` |
| SDF | Futurenet | `https://rpc-futurenet.stellar.org` |
| Gateway | Testnet | `https://soroban-rpc.testnet.stellar.gateway.fm` |
| Gateway | Mainnet | `https://soroban-rpc.mainnet.stellar.gateway.fm` |
| sorobanrpc.com | Mainnet | `https://mainnet.sorobanrpc.com` |
| Nodies | Testnet | `https://stellar-soroban-testnet-public.nodies.app` |
| Nodies | Mainnet | `https://stellar-soroban-public.nodies.app` |
| OnFinality | Mainnet | `https://stellar.api.onfinality.io/public` |

Shared public endpoints are convenient for development and may have usage limits. Private plans may require API keys, and dedicated-node or archive access varies by provider. Confirm current terms directly with the provider. See the [official provider list](https://developers.stellar.org/docs/data/apis/rpc/providers) for current URLs and other options. SDF does not list a public Mainnet RPC endpoint; use a third-party service or run your own RPC node for Mainnet.

## Configure a custom RPC endpoint

Pass a custom endpoint directly with `--rpc-url`:

```sh
soroban-cost-estimator estimate \
  --wasm contract.wasm \
  --network mainnet \
  --rpc-url https://rpc.example.net
```

The application can keep endpoint defaults in `config.toml`:

```toml
default_network = "testnet"
timeout_secs = 30

[rpc_urls]
testnet = "https://soroban-testnet.stellar.org"
mainnet = "https://rpc.example.net"
local = "http://localhost:8000/rpc"
```

An explicit `--rpc-url` takes precedence over `[rpc_urls]`; configured URLs take precedence over built-in endpoints. The file can be selected with `--config PATH`. See the [configuration file guide](../README.md#configuration-file).

## API keys and request headers

For providers that require a request header, use global, repeatable `--header` (`-H`). Both `KEY=VALUE` and `KEY: VALUE` forms are accepted:

```sh
soroban-cost-estimator estimate \
  --wasm contract.wasm \
  --network mainnet \
  --rpc-url https://rpc.example.net \
  --header "Authorization=Bearer YOUR_API_KEY"
```

Sensitive header values are redacted in verbose request logs. Avoid putting keys in shell history or committing them to source control.

The current TOML format supports network URLs, default network, output format, and timeout; it does **not** support request headers or environment-variable interpolation. If a provider issues a key-bearing URL, you can configure that URL under `[rpc_urls]`, but protect the file as a secret. Otherwise pass the key with `--header`.

## Local standalone network with `stellar/quickstart`

Run the official Quickstart Docker image for local development:

```sh
docker run --rm -it -p 8000:8000 stellar/quickstart:latest --local
```

The RPC endpoint is `http://localhost:8000/rpc`, Friendbot is `http://localhost:8000/friendbot`, and the default passphrase is `Standalone Network ; February 2017`. Quickstart is for development and testing, not production.

Use the local endpoint explicitly:

```sh
soroban-cost-estimator estimate --wasm contract.wasm --network local --rpc-url http://localhost:8000/rpc
```
For local protocol tests, Quickstart supports `--protocol-version` and `--limits`; see its [current options and security notes](https://github.com/stellar/quickstart). The default standalone passphrase is fixed and is not a security boundary for shared networks.
