# Midas

**Midas** is a mobile-first, USD-denominated payment infrastructure for configured EVM USDC and USDT.

## What Midas includes

- Rust/Axum API with SQLite **WAL** persistence.
- Exact USD micro-ledger (`amount_usd_micros` / `balance_delta_usd_micros`) for six-decimal EVM USDC and USDT.
- One persisted dedicated EVM deposit key/address per user and enabled chain; private material is never returned from an API.
- Receipt-driven deposits: the caller submits one transaction hash, Midas verifies its ERC-20 `Transfer` event, credits USD, then submits the gas-funding and source-wallet collection sequence.
- Atomic internal transfers, withdrawal balance reservations, collection-wallet withdrawal broadcasts, and exact-receipt finalization.
- One-time `app_meta.root_user_id` bootstrap plus root-only EVM network, asset, gas-wallet, and collection-wallet configuration.
- Auth Mini backend verification boundary and React `AuthMiniProvider` boundary.
- React `LinkitProvider` boundary for future profile and recipient experiences.
- Mobile-first web surface that makes disabled execution boundaries explicit.
- OpenAPI contract and CI foundation.

## Architecture

```text
React/Vite GUI
  ├─ Auth Mini browser integration boundary
  └─ Linkit React Components boundary

Axum API
  ├─ Auth Mini JWT verification (`auth-mini-axum`)
  ├─ root_user_id plus custody configuration in SQLite app_meta
  └─ SQLite WAL USD ledger and payment-operation records
```

## Data model

Midas records **USD only** in integer micro-dollars. Future USDC/USDT chain metadata is retained alongside ledger entries, but no token amount is treated as the system-of-record balance.

| Table | Purpose |
| --- | --- |
| `app_meta` | `root_user_id` and public EVM configuration |
| `users` | Auth Mini subjects |
| `evm_networks` | Root-managed chain configuration |
| `supported_assets` | USDC / USDT metadata |
| `wallet_addresses` | One user + one EVM chain deposit address |
| `wallet_private_keys` | Dedicated EVM deposit private keys; readable only by the service account |
| `ledger_entries` | Immutable USD balance deltas and payment history |
| `payment_operations` | Per-user idempotency keys for payment writes |
| `deposits`, `deposit_sweeps` | Confirmed deposits and the two-step collection state |
| `internal_transfers`, `withdrawals` | Atomic internal transfers and chain withdrawal state |

## Local development

Midas has no runtime environment-variable configuration. It binds its API to `127.0.0.1:8787`, uses the platform local-data directory on macOS and `/var/lib/midas` on Linux, and reads root-managed settings from SQLite.

```bash
npm --prefix web ci
npm --prefix web run build
cargo run
```

Open `http://127.0.0.1:8787`. For Vite development:

```bash
npm --prefix web run dev
```

## API integration

Other applications can use the OpenAPI-described payment API with an Auth Mini bearer token. Every payment write requires `Idempotency-Key`:

```text
GET /api/balances/me
GET /api/ledger/me
GET /api/wallet-addresses/me
POST /api/wallet-addresses/me
POST /api/deposits/confirm
POST /api/transfers
POST /api/withdrawals
POST /api/withdrawals/{id}/finalize
```

The API contract is in [`openapi.yaml`](./openapi.yaml). Midas verifies each submitted deposit and withdrawal receipt independently; it does not perform block-range or background event scanning.

## Root configuration

The first authenticated user can initialize the one-time root setup with its own Auth Mini subject:

```text
POST /api/setup/initialize
```

The root can then configure EVM chains/assets, gas account private key, collection-wallet address, and optionally the collection private key through:

```text
GET/PUT /api/admin/evm-config
```

Read APIs only indicate whether a private key is configured; they never return any key material. An address-only collection wallet keeps withdrawals in `awaiting_signer` state, which supports an external Safe/manual signing policy. Configuring its matching private key enables broadcasts.

## Deployment

Midas is deployed through the tracked Release and Deploy Production workflows to `https://midas.ntnl.io`. The service binds privately to `127.0.0.1:8787`; Caddy terminates TLS and proxies the public hostname. Releases are static Linux artifacts with SHA-256 and Git SHA metadata verification before activation.

Custody operations must first be configured by the root on a testnet. Treat the SQLite file as a high-value secret: restrict the `/var/lib/midas` directory to the service account, back it up encrypted, and do not configure production private keys until the configured contracts, RPC endpoints, gas funding amount, and withdrawal policy have been independently checked.
