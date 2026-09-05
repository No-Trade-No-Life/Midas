# Midas Product Context

## Product

Midas is No Trade No Life's public blockchain payment infrastructure. It gives an authenticated person or integrating application one USD-denominated balance that is funded by EVM USDC or USDT, can move between Midas users, and can be withdrawn to an EVM address.

## Users and jobs

- A customer opens Midas on a phone, finds their dedicated EVM deposit address, sends a supported stablecoin, checks the USD balance after automatic discovery, can claim a missed deposit by network and TxID, transfers to another Midas user, withdraws, and reviews their immutable history.
- An integrating application such as 1Exchange uses the bearer-authenticated API to show a balance, obtain a deposit address, confirm or claim a deposit, create a transfer or withdrawal, and reconcile operations by idempotency key.
- A channel owner creates an automatic-payment agreement, then explicitly rotates its API key and receives that value once. Any Midas user, including the owner, signs in to a focused authorization page before the channel can charge that user's available balance.
- The root operator initializes the instance once, sets supported EVM networks/assets, and configures the gas and collection wallets without exposing their private keys through read APIs.

## Payment model

- The ledger's sole unit is integer USD micro-dollars. Built-in USDC and USDT maps cover Ethereum, BNB Smart Chain, Base, Arbitrum One, OP Mainnet, and Polygon; token amounts are converted exactly to USD micro-dollars, including the 18-decimal BSC assets.
- The worker polls one address-chain pair per second, using bounded RPC `Transfer`-log queries and a durable cursor to recover from downtime without skipping scanned ranges. It never credits a discovery log directly: every candidate is verified against the configured chain RPC receipt and ERC-20 `Transfer` log. The customer claim endpoint accepts only a chain and TxID; it derives asset and amount from the receipt, and a TxID can be credited only once.
- A confirmed deposit credits the USD ledger, then queues a two-step collection: the single custody wallet funds native gas and the user's stored deposit key signs the ERC-20 transfer back to that same custody address.
- Transfers are paired, immutable USD ledger entries and use Linkit's username picker. Withdrawals select a chain and USDC/USDT, reserve the available USD balance, accept a direct same-chain EVM destination, and broadcast only when the custody signer exists. The user can then save that destination from its history.
- Broadcast withdrawal destinations are grouped by exact address, network, and token. The user may save a note for each target and select it again in the withdrawal drawer.
- Automatic-payment charges use a channel-scoped idempotency key, require a currently bound payer, and create a paired immutable `transfer_out` / `transfer_in` ledger entry. The API key is stored only as a hash; rotation immediately invalidates its predecessor and displays the replacement once.

## Trust and operational boundary

- Midas is custodial. Dedicated-address keys and one custody private key are persisted in SQLite but are never returned by an API. The custody address is derived from its private key; the service account is the only account permitted to read the database directory.
- Root configuration is protected by `app_meta.root_user_id`; changing it is not an application feature.
- A transaction is never credited from a client-provided amount. Midas derives asset, recipient, and amount from the final on-chain receipt.

## Platform and UI

- Web application, mobile first, public URL `https://midas.ntnl.io`.
- Auth Mini owns sign-in and session lifecycle; unauthenticated visits redirect directly to Auth Mini rather than rendering a local landing page. Linkit supplies authenticated profile/recipient identity surfaces.
- Chinese and English are first-class, build-time-complete languages.
- The interface uses the standard shadcn Base UI component vocabulary. It is a restrained operational product surface: crisp, familiar controls; a single column and bottom navigation on a phone; denser panels on larger screens.
- Payment controls include a withdrawal address book, owner-managed automatic receipts, customer-managed automatic payments, and a focused external authorization route.

## Non-goals for this release

- No other fiat valuation or token decimals.
- No anonymous balance or payment access.
- No unbounded block-range/event scanning; each RPC query is capped to a narrow block range and cursor progress is persisted only after processing succeeds.
- No arbitrary cross-origin API credentials; integrations use an Auth Mini bearer token for the applicable user.
