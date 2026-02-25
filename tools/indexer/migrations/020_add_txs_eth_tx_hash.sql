ALTER TABLE txs
  ADD COLUMN IF NOT EXISTS eth_tx_hash bytea;

CREATE UNIQUE INDEX IF NOT EXISTS idx_txs_eth_tx_hash_unique
  ON txs (eth_tx_hash)
  WHERE eth_tx_hash IS NOT NULL;
