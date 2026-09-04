# Midas

**Midas** is a mobile-first blockchain payment infrastructure foundation for USD-denominated EVM stablecoin balances.

> **v1 safety boundary:** Midas does not generate, import, store, export, or execute with real private keys. It does not scan chains, sweep deposits, broadcast withdrawals, or execute transfers. Those operations require a separately reviewed custody and execution release.

## What this skeleton includes

- Rust/Axum API with SQLite **WAL** persistence.
- USD micro-ledger (`amount_usd_micros` / `balance_delta_usd_micros`) for future EVM USDC and USDT records.
- Schema for users, unique EVM deposit addresses, disabled future key envelopes, supported assets, immutable ledger entries, and root-managed EVM configuration.
- Read-only balance, ledger, and deposit-address APIs.
- One-time `app_meta.root_user_id` bootstrap plus root-only public EVM configuration.
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
  ├─ root_user_id in SQLite app_meta
  └─ SQLite WAL USD ledger
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
| `wallet_key_envelopes` | Future encrypted custody envelope schema; private material is never written in v1 |
| `ledger_entries` | Immutable USD balance deltas and payment history |

## Local development

Midas has no environment-variable configuration. It uses the user data directory, binds its API to `127.0.0.1:8787`, and reads root-managed settings from SQLite.

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

Other applications can use the read-only v1 API after Auth Mini bearer authentication:

```text
GET /api/balances/me
GET /api/ledger/me
GET /api/wallet-addresses/me
```

The API contract is in [`openapi.yaml`](./openapi.yaml). Create-payment, deposit confirmation, sweep, withdrawal, and transfer-execution APIs are intentionally absent until their custody, idempotency, authorization, and on-chain finality designs are implemented.

## Root configuration

The first authenticated user can initialize the one-time root setup with its own Auth Mini subject:

```text
POST /api/setup/initialize
```

The root can then configure public EVM chain metadata, gas-account address, and collection-wallet address through:

```text
GET/PUT /api/admin/evm-config
```

The endpoint never accepts a gas-account private key. `POST /api/admin/custody/private-key` is intentionally `501 Not Implemented`.

## Safe-skeleton deployment

The safe read-only surface is deployed through the tracked Release and Deploy Production workflows to `https://midas.ntnl.io`. The deployed service binds privately to `127.0.0.1:8787`; Caddy terminates TLS and proxies the public hostname. Releases are static Linux artifacts with SHA-256 and Git SHA metadata verification before activation.

This deployment does **not** authorize custody or chain execution. Production continues to reject private-key configuration and does not scan chains, credit deposits, sweep funds, execute withdrawals or transfers, or broadcast transactions. Any future custody or mainnet work still requires reviewed managed secret storage outside SQLite, explicit root bootstrap, rate limits, API client authorization, observability, testnet execution, reconciliation, Safe operations, and legal/compliance approval.
