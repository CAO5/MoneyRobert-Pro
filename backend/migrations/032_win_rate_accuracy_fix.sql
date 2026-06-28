-- Fix win rate calculation accuracy: add slippage and funding rate fields
-- Update default fee_percent to realistic non-VIP rate (0.08%)

ALTER TABLE ai_simulation_trades
    ADD COLUMN IF NOT EXISTS slippage_bps DOUBLE PRECISION NOT NULL DEFAULT 3.0,
    ADD COLUMN IF NOT EXISTS funding_rate_8h DOUBLE PRECISION NOT NULL DEFAULT 0.01;

ALTER TABLE ai_simulation_trades
    ALTER COLUMN fee_percent SET DEFAULT 0.08;

UPDATE ai_simulation_configs
SET fee_percent = 0.08
WHERE fee_percent < 0.08;