# Midas

**Midas** is a mobile-first, USD-denominated payment infrastructure for built-in EVM USDC and USDT.

## What Midas includes

- Rust/Axum API with SQLite **WAL** persistence.
- Exact USD nanodollar ledger (`amount_usd_nanos` / `balance_delta_usd_nanos`) for USDC and USDT on Ethereum, BNB Smart Chain, Base, Arbitrum One, OP Mainnet, and Polygon. The fixed contract map supports both six- and eighteen-decimal tokens while recording only USD nanodollars.
- One persisted dedicated EVM-compatible deposit key/address per human user, automatically created after initialization; private material is never returned from an API.
- Balance-driven deposit discovery: every 30 seconds Midas reads each human deposit address's USDC/USDT balance on every enabled chain and reconciles it against the credited, not-yet-swept ledger. A balance change or reconciliation gap triggers a bounded recent `Transfer`-log scan (5,000 blocks initially, escalating only while a gap stays unresolved) that resolves the transaction hash; every candidate is verified from its ERC-20 receipt through the chain RPC before crediting, and submitted sweeps are finalized from their receipts so reconciliation stays exact. Customers can also submit a network and TxID to verify and claim a missed deposit; the token and amount are always derived from the receipt.
- Atomic internal transfers that automatically provision a new human recipient's Midas account and dedicated wallet, withdrawal balance reservations, collection-wallet withdrawal broadcasts, and exact-receipt finalization.
- One-time `app_meta.root_user_id` bootstrap plus a root-only, input-only custody-wallet private key. Its address is derived server-side and the same wallet funds gas, collects deposits, and signs withdrawals.
- Auth Mini backend verification boundary and React `AuthMiniProvider` boundary with automatic redirect to sign-in; Midas has no unauthenticated home page.
- A scannable QR code for the dedicated deposit address.
- Linkit React Components, including the zero-prop `LinkitMyInfo` account control and `LinkitUserPicker` for username-based transfer recipients.
- A root-only administration area for custody configuration, RPC discovery status, collection operations, all-user balance exposure, and filterable, paginated global ledger review.
- Root-managed fund users for applications such as 1Exchange and OpenAI LB. A fund user reuses the same Midas `users` ledger model, but has no EVM wallet or blockchain API surface: its transferable Midas user ID, balance, immutable history, and one-time API key are for internal transfers only. Its API key can also read the exact cumulative transfers received from a specified Midas user, so an application can treat its fund user as a public recharge account without asking users to authorize automatic charges.
- Direct EVM withdrawals: choose a network and USDC/USDT, then submit the destination address. Broadcast destinations appear in a per-token withdrawal address book, where users can save a note for each address × network × token combination and reuse it from the withdrawal drawer.
- Automatic-payment agreements: an owner creates a channel, then explicitly rotates and receives its API key once; any Midas user, including that owner, explicitly authorizes the channel through a signed-in GUI page. Its API key can then make idempotent USD charges and ledger-only payouts against authorized users. A payout returns USD from the channel owner to the user and never requests an on-chain withdrawal.
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

Midas records **USD only** in integer nanodollars. Future USDC/USDT chain metadata is retained alongside ledger entries, but no token amount is treated as the system-of-record balance.

Existing SQLite databases migrate on the first startup that carries this version. The USD-unit migration atomically renames every persisted `*_usd_micros` column, multiplies its historical values by 1,000, verifies that no value can overflow `i64`, and records the `nanodollars` unit marker so it cannot run twice. The startup migration also removes any legacy fund-user deposit private keys, discovery cursors, and balance snapshots.

| Table | Purpose |
| --- | --- |
| `app_meta` | `root_user_id` and root-managed configuration secrets that are never returned by APIs |
| `users` | Ledger principals: Auth Mini-backed `human` users and root-managed `fund` users |
| `fund_user_api_keys` | One-way API-key hash and non-secret prefix for each fund user |
| `evm_networks` | Seeded built-in chain metadata with verified RPC URLs |
| `supported_assets` | Seeded USDC / USDT contract and decimal metadata |
| `wallet_addresses` | One human-user EVM-compatible deposit address (legacy chain field is an internal sentinel) |
| `wallet_private_keys` | Dedicated human-user EVM deposit private keys; readable only by the service account |
| `wallet_asset_snapshots` | Latest USDC/USDT balance snapshot, locate state, and scan escalation per human deposit address, chain, and asset |
| `ledger_entries` | Immutable USD balance deltas and payment history |
| `payment_operations` | Per-user idempotency keys for payment writes |
| `deposits`, `deposit_sweeps` | Confirmed deposits and the two-step collection state |
| `address_book_entries` | Legacy per-user, per-chain approved withdrawal destinations |
| `withdrawal_target_notes` | User notes for broadcast withdrawal address × network × token targets |
| `internal_transfers`, `withdrawals` | Atomic internal transfers and direct-destination chain withdrawal state |
| `payment_agreements` | Owner-created payment channels with only a one-way API-key hash and non-secret prefix |
| `payment_agreement_bindings` | Explicit user authorization for automatic-payment channels |
| `payment_agreement_charges`, `payment_agreement_payouts` | Agreement-scoped idempotent charges and ledger-only payouts with their paired ledger entries |

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
GET /api/withdrawal-availability?asset_id=1-USDC
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
POST /api/agreements/{id}/api-key
```

A root operator creates and rotates fund users through `POST /api/admin/fund-users`
and `POST /api/admin/fund-users/{id}/api-key`. The plaintext key is returned once.
With `X-Api-Key`, a fund user may call `GET /api/balances/me`, `GET /api/ledger/me`,
`GET /api/internal-transfers/me/inbound/{sender_user_id}`, the batched
`POST /api/internal-transfers/me/inbound/summary`, and `POST /api/transfers`.
Inbound-transfer aggregates always use the fund user inferred from the key; callers
cannot choose a recipient. They are cumulative exact nanodollars and are not reduced
when the fund user later transfers USD out. Fund API keys cannot access EVM assets,
deposit addresses, deposit verification, withdrawals, or root settings; the root GUI
cannot request an on-chain withdrawal for a fund user.

An external payment channel uses its owner-rotated `X-Api-Key` and an
`Idempotency-Key` for `POST /api/agreements/{id}/charges` and
`POST /api/agreements/{id}/payouts`. Both requests supply the already-authorized
`user_id`, exact `amount_usd_nanos`, and an optional integration `reference`.
Charges debit that user and credit the owner. Payouts debit the owner and credit
that user without broadcasting a blockchain withdrawal. Midas rejects unbound
users and insufficient available balances without changing either ledger.

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

## Withdrawal availability

Signed-in human users see network/token capacity, readiness, gas sufficiency, and
observation time in the withdrawal drawer. The endpoint never returns the custody
address or raw RPC diagnostics. Root custody balances retain their address and
native/USDC/USDT detail. Fund API keys cannot access availability.

The read cache is shared across users, bounded by the 12 built-in assets, expires
after five seconds, caches safe failures, and bounds an RPC observation to eight
seconds. Observations pin token and native balances to one block. Reservations
are read from SQLite on every lookup. Creation bypasses cached observations and
holds the custody execution lock followed by the write lock through validation
and the atomic ledger/withdrawal transaction. This service has one writer process;
multiple processes sharing the database are not a supported deployment.

Pending ledger reservations and awaiting-signature/signed/submitted withdrawals
reduce same-token capacity. Gas reservations span both tokens on the same chain.
Creation also simulates the selected destination and amount before ledger writes.
Each new withdrawal reserves 200,000 gas at twice the observed gas price; signing
must fit that durable budget. Collection gas funding also respects these budgets.
A price spike can leave an accepted withdrawal awaiting a later retry. This is a
conservative availability observation, not a guarantee against later chain changes,
external wallet spending, or additional rollup fees. A submitted transfer already
mined but not yet reconciled can temporarily be counted twice, reducing capacity
safely until settlement. Keep the custody wallet dedicated to this service.

Creation, signing, errors, settlement, custody configuration changes, and collection
spending invalidate cached observations. Existing pre-upgrade withdrawals without
a gas budget temporarily make their chain unavailable until they settle. Insufficient
liquidity/gas or unavailable observations return HTTP 409 before any ledger debit,
withdrawal row, or idempotency operation is created. Idempotent accepted requests
still return their original withdrawal without requiring new liquidity.
