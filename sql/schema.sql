PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS app_meta (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS users (
  id TEXT PRIMARY KEY,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS evm_networks (
  chain_id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  rpc_url TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS supported_assets (
  id TEXT PRIMARY KEY,
  chain_id INTEGER NOT NULL REFERENCES evm_networks(chain_id),
  symbol TEXT NOT NULL CHECK (symbol IN ('USDC', 'USDT')),
  contract_address TEXT NOT NULL,
  token_decimals INTEGER NOT NULL CHECK (token_decimals BETWEEN 6 AND 18),
  enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
  UNIQUE(chain_id, contract_address),
  UNIQUE(chain_id, symbol)
);

CREATE TABLE IF NOT EXISTS wallet_addresses (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users(id),
  chain_id INTEGER NOT NULL REFERENCES evm_networks(chain_id),
  address TEXT NOT NULL,
  custody_status TEXT NOT NULL DEFAULT 'configured' CHECK (custody_status = 'configured'),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(chain_id, address),
  UNIQUE(user_id, chain_id)
);

-- Midas is a custodial service. This table deliberately keeps the user deposit
-- key separate from the public wallet-address record. It is never selected by
-- a response handler and the database directory is readable only by the
-- service account.
CREATE TABLE IF NOT EXISTS wallet_private_keys (
  wallet_address_id TEXT PRIMARY KEY REFERENCES wallet_addresses(id) ON DELETE CASCADE,
  private_key TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS ledger_entries (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users(id),
  kind TEXT NOT NULL CHECK (kind IN ('deposit', 'withdrawal', 'transfer_in', 'transfer_out', 'adjustment')),
  status TEXT NOT NULL CHECK (status IN ('pending', 'posted', 'rejected', 'disabled')),
  asset_id TEXT REFERENCES supported_assets(id),
  amount_usd_micros INTEGER NOT NULL,
  balance_delta_usd_micros INTEGER NOT NULL,
  counterparty_user_id TEXT REFERENCES users(id),
  external_reference TEXT UNIQUE,
  note TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  posted_at TEXT,
  CHECK (amount_usd_micros >= 0)
);

CREATE INDEX IF NOT EXISTS ledger_entries_user_created_idx
  ON ledger_entries(user_id, created_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS payment_operations (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users(id),
  kind TEXT NOT NULL CHECK (kind IN ('deposit', 'transfer', 'withdrawal')),
  idempotency_key TEXT NOT NULL,
  resource_id TEXT,
  status TEXT NOT NULL CHECK (status IN ('accepted', 'submitted', 'completed', 'failed')),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(user_id, kind, idempotency_key)
);

CREATE TABLE IF NOT EXISTS deposits (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users(id),
  wallet_address_id TEXT NOT NULL REFERENCES wallet_addresses(id),
  asset_id TEXT NOT NULL REFERENCES supported_assets(id),
  ledger_entry_id TEXT NOT NULL UNIQUE REFERENCES ledger_entries(id),
  transaction_hash TEXT NOT NULL,
  log_index INTEGER NOT NULL,
  raw_amount TEXT NOT NULL,
  sweep_status TEXT NOT NULL CHECK (sweep_status IN ('queued', 'submitted', 'swept', 'awaiting_configuration', 'failed')),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(asset_id, transaction_hash, log_index)
);

CREATE TABLE IF NOT EXISTS deposit_sweeps (
  id TEXT PRIMARY KEY,
  deposit_id TEXT NOT NULL UNIQUE REFERENCES deposits(id) ON DELETE CASCADE,
  gas_transaction_hash TEXT,
  token_transaction_hash TEXT,
  status TEXT NOT NULL CHECK (status IN ('queued', 'submitted', 'swept', 'failed')),
  error_message TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS internal_transfers (
  id TEXT PRIMARY KEY,
  sender_user_id TEXT NOT NULL REFERENCES users(id),
  recipient_user_id TEXT NOT NULL REFERENCES users(id),
  amount_usd_micros INTEGER NOT NULL CHECK (amount_usd_micros > 0),
  sender_ledger_entry_id TEXT NOT NULL UNIQUE REFERENCES ledger_entries(id),
  recipient_ledger_entry_id TEXT NOT NULL UNIQUE REFERENCES ledger_entries(id),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS address_book_entries (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users(id),
  chain_id INTEGER NOT NULL REFERENCES evm_networks(chain_id),
  label TEXT NOT NULL,
  address TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(user_id, chain_id, address)
);

CREATE TABLE IF NOT EXISTS withdrawals (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users(id),
  asset_id TEXT NOT NULL REFERENCES supported_assets(id),
  ledger_entry_id TEXT NOT NULL UNIQUE REFERENCES ledger_entries(id),
  address_book_entry_id TEXT REFERENCES address_book_entries(id),
  destination_address TEXT NOT NULL,
  amount_usd_micros INTEGER NOT NULL CHECK (amount_usd_micros > 0),
  transaction_hash TEXT,
  status TEXT NOT NULL CHECK (status IN ('awaiting_signer', 'submitted', 'completed', 'failed')),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS deposits_user_created_idx
  ON deposits(user_id, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS withdrawals_user_created_idx
  ON withdrawals(user_id, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS address_book_entries_user_chain_idx
  ON address_book_entries(user_id, chain_id, created_at DESC, id DESC);
