# Midas

**Midas** is a mobile-first, USD-denominated payment infrastructure for built-in EVM USDC and USDT.

## What Midas includes

- Rust/Axum API with SQLite **WAL** persistence.
- Exact USD micro-ledger (`amount_usd_micros` / `balance_delta_usd_micros`) for USDC and USDT on Ethereum, BNB Smart Chain, Base, Arbitrum One, OP Mainnet, and Polygon. The fixed contract map supports both six- and eighteen-decimal tokens while recording only USD micro-dollars.
- One persisted dedicated EVM-compatible deposit key/address per user, automatically created after initialization; private material is never returned from an API.
- Automatic receipt-driven deposits: Midas scans a bounded RPC `Transfer`-log range for one deposit-address/network pair per second and persists its cursor. It verifies every candidate's ERC-20 receipt through the chain RPC, credits USD, and submits the gas-funding and source-wallet collection sequence. Customers can also submit a network and TxID to verify and claim a missed deposit; the token and amount are always derived from the receipt.
- Atomic internal transfers that automatically provision a new recipient's Midas account and dedicated wallet, withdrawal balance reservations, collection-wallet withdrawal broadcasts, and exact-receipt finalization.
- One-time `app_meta.root_user_id` bootstrap plus a root-only, input-only custody-wallet private key. Its address is derived server-side and the same wallet funds gas, collects deposits, and signs withdrawals.
- Auth Mini backend verification boundary and React `AuthMiniProvider` boundary with automatic redirect to sign-in; Midas has no unauthenticated home page.
- A scannable QR code for the dedicated deposit address.
- Linkit React Components, including `LinkitAppHeaderUser` and `LinkitUserPicker` for username-based transfer recipients.
- A root-only administration area for custody configuration, RPC discovery status, collection operations, all-user balance exposure, and filterable, paginated global ledger review.
- Direct EVM withdrawals: choose a network and USDC/USDT, then submit the destination address. Broadcast destinations appear in a per-token withdrawal address book, where users can save a note for each address × network × token combination and reuse it from the withdrawal drawer.
- Automatic-payment agreements: an owner creates a channel and receives an API key once; a customer explicitly authorizes the channel through a signed-in GUI page; its API key can then make an idempotent USD charge only against that customer's available balance, paired with an immutable credit to the owner.
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
| `app_meta` | `root_user_id` and root-managed configuration secrets that are never returned by APIs |
| `users` | Auth Mini subjects |
| `evm_networks` | Seeded built-in chain metadata with verified RPC URLs |
| `supported_assets` | Seeded USDC / USDT contract and decimal metadata |
| `wallet_addresses` | One user EVM-compatible deposit address (legacy chain field is an internal sentinel) |
| `wallet_private_keys` | Dedicated EVM deposit private keys; readable only by the service account |
| `deposit_discovery_cursors` | Durable bounded-RPC-scan cursor per deposit address and chain |
| `ledger_entries` | Immutable USD balance deltas and payment history |
| `payment_operations` | Per-user idempotency keys for payment writes |
| `deposits`, `deposit_sweeps` | Confirmed deposits and the two-step collection state |
| `address_book_entries` | Legacy per-user, per-chain approved withdrawal destinations |
| `withdrawal_target_notes` | User notes for broadcast withdrawal address × network × token targets |
| `internal_transfers`, `withdrawals` | Atomic internal transfers and direct-destination chain withdrawal state |
| `payment_agreements` | Owner-created payment channels with only a one-way API-key hash and non-secret prefix |
| `payment_agreement_bindings` | Explicit user authorization for automatic-payment channels |
| `payment_agreement_charges` | Agreement-scoped idempotent charges and their paired ledger entries |

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
POST /api/deposits/confirm
POST /api/deposits/claim
POST /api/transfers
GET/PUT /api/withdrawal-targets/me
DELETE /api/withdrawal-targets/me/{id}
POST /api/withdrawals
POST /api/withdrawals/{id}/finalize
GET/POST /api/agreements/owned
GET /api/agreements/bindings/me
GET /api/agreements/{id}
POST/DELETE /api/agreements/{id}/bind
```

An external payment channel charges with `POST /api/agreements/{id}/charges`, an
`X-Api-Key` issued at channel creation, and an `Idempotency-Key`. It supplies
the already-authorized `user_id` and exact `amount_usd_micros`; Midas rejects
unbound users and insufficient available balances without changing either
ledger.

The API contract is in [`openapi.yaml`](./openapi.yaml). Midas verifies every credited deposit and withdrawal receipt independently. RPC logs only discover candidates; Midas never trusts a log for token, recipient, amount, or final success state.

## Root configuration

The first authenticated user can initialize the one-time root setup with its own Auth Mini subject:

```text
POST /api/setup/initialize
```

The root can then configure the single custody wallet through:

```text
GET/PUT /api/admin/evm-config
GET /api/admin/deposit-discovery
```

Midas ships the chain, RPC, and USDC/USDT contract mapping in the binary; callers do not configure these fields. Read APIs never return private key material and return the custody wallet's derived public address only.

## Deployment

Midas is deployed through the tracked Release and Deploy Production workflows to `https://midas.ntnl.io`. The service binds privately to `127.0.0.1:8787`; Caddy terminates TLS and proxies the public hostname. Releases are static Linux artifacts with SHA-256 and Git SHA metadata verification before activation.

Treat the SQLite file as a high-value secret: restrict the `/var/lib/midas` directory to the service account, back it up encrypted, and do not configure a production custody key until the built-in contract map and withdrawal policy have been independently checked.
