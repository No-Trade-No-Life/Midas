# Midas Design Context

## Register

Product. A person checking or moving money needs clarity and calm, not a brand spectacle.

## Confirmed direction

- **Strategy:** Restrained. Standard shadcn Base UI tokens and variants carry almost all visual treatment; Midas uses a deep crimson primary only for the current action and current navigation state.
- **Scene:** A customer checks a stablecoin payment balance on a phone at a desk under ordinary indoor light; the surface should feel dependable, compact, and immediately legible.
- **Anchors:** Stripe Dashboard for transactional clarity, Linear for density discipline, and shadcn Base UI for component behavior.
- **Typography:** One familiar sans-serif system stack with a fixed product scale. No display type, gradient text, decorative illustrations, or synthetic metric cards.
- **Layout:** The 1Exchange application shell: a collapsible, persisted navigation rail on larger screens; a sticky header with the single-line page title and the appearance control; on phones a sticky bottom tab bar plus a left navigation drawer.
- **Theme:** Light, dark, and system appearance stored as one `midas.theme` choice; the first paint already carries it, and the mark's favicon and browser theme color follow the effective scheme.

## Information architecture

- **Home:** USD balance, its dedicated deposit address, and the next safe action.
- **Activity:** searchable/paginated immutable ledger records with a plain-language status and a small, progressive "deposit not received?" action that opens a focused claim drawer.
- **Move money:** focused Deposit, Transfer, and Withdraw flows, each with visible network/asset/amount and non-optimistic completion feedback.
- **Settings:** language, account, and root-only EVM setup. Secrets are input-only and visibly never re-displayed.

## Responsive behavior

- Phones use a single column, 44px minimum control targets, a sticky bottom navigation, and drawers for focused money-moving flows.
- Larger screens retain the same information architecture, showing an inline navigation rail and more activity context rather than a different product.
- Every dynamic area has loading, empty, error, and long-value states. Payment writes remain pending until the server confirms their result.
