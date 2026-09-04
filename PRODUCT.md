# Midas Product Context

## Product

Midas is No Trade No Life's public blockchain payment infrastructure. It gives an authenticated person or integrating application one USD-denominated balance that is funded by EVM USDC or USDT, can move between Midas users, and can be withdrawn to an EVM address.

## Users and jobs

- A customer opens Midas on a phone, finds their dedicated EVM deposit address, submits a confirmed transaction hash, checks the USD balance, transfers to another Midas user, withdraws, and reviews their immutable history.
- An integrating application such as 1Exchange uses the bearer-authenticated API to show a balance, obtain a deposit address, confirm a deposit, create a transfer or withdrawal, and reconcile operations by idempotency key.
- The root operator initializes the instance once, sets supported EVM networks/assets, and configures the gas and collection wallets without exposing their private keys through read APIs.

## Payment model

- The ledger's sole unit is integer USD micro-dollars. Built-in USDC and USDT maps cover Ethereum, BNB Smart Chain, Base, Arbitrum One, OP Mainnet, and Polygon; token amounts are converted exactly to USD micro-dollars, including the 18-decimal BSC assets.
- Midas never background-scans a chain. The client or integrating application submits one transaction hash; Midas verifies the specified transaction receipt and ERC-20 `Transfer` log against the user's dedicated address.
- A confirmed deposit credits the USD ledger, then queues a two-step collection: the single custody wallet funds native gas and the user's stored deposit key signs the ERC-20 transfer back to that same custody address.
- Transfers are paired, immutable USD ledger entries and use Linkit's username picker. Withdrawals select a chain and USDC/USDT, reserve the available USD balance, accept a direct same-chain EVM destination, and broadcast only when the custody signer exists. The user can then save that destination from its history.

## Trust and operational boundary

- Midas is custodial. Dedicated-address keys and one custody private key are persisted in SQLite but are never returned by an API. The custody address is derived from its private key; the service account is the only account permitted to read the database directory.
- Root configuration is protected by `app_meta.root_user_id`; changing it is not an application feature.
- A transaction is never credited from a client-provided amount. Midas derives asset, recipient, and amount from the final on-chain receipt.

## Platform and UI

- Web application, mobile first, public URL `https://midas.ntnl.io`.
- Auth Mini owns sign-in and session lifecycle; unauthenticated visits redirect directly to Auth Mini rather than rendering a local landing page. Linkit supplies authenticated profile/recipient identity surfaces.
- Chinese and English are first-class, build-time-complete languages.
- The interface uses the standard shadcn Base UI component vocabulary. It is a restrained operational product surface: crisp, familiar controls; a single column and bottom navigation on a phone; denser panels on larger screens.

## Non-goals for this release

- No other fiat valuation or token decimals.
- No anonymous balance or payment access.
- No automatic block-range/event scanning.
- No arbitrary cross-origin API credentials; integrations use an Auth Mini bearer token for the applicable user.
