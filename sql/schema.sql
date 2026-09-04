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
  usd_scale INTEGER NOT NULL DEFAULT 6 CHECK (usd_scale = 6),
  enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
  UNIQUE(chain_id, contract_address),
  UNIQUE(chain_id, symbol)
);

CREATE TABLE IF NOT EXISTS wallet_addresses (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users(id),
  chain_id INTEGER NOT NULL REFERENCES evm_networks(chain_id),
  address TEXT NOT NULL,
  custody_status TEXT NOT NULL DEFAULT 'disabled' CHECK (custody_status IN ('disabled', 'configured')),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(chain_id, address),
  UNIQUE(user_id, chain_id)
);

-- v1 intentionally never writes private material. This future custody envelope is nullable
-- so schema preparation never requires or synthesizes a real private key.
CREATE TABLE IF NOT EXISTS wallet_key_envelopes (
  wallet_address_id TEXT PRIMARY KEY REFERENCES wallet_addresses(id) ON DELETE CASCADE,
  encrypted_private_key BLOB,
  key_version TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CHECK (encrypted_private_key IS NULL OR key_version IS NOT NULL)
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
