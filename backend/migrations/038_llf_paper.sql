-- A research account has no login and must never be reachable by a user's account reset.
CREATE TABLE llf_paper_accounts (
    account_id TEXT PRIMARY KEY CHECK (account_id = 'LLF001-PAPER'),
    state JSONB NOT NULL,
    max_drawdown_pct DOUBLE PRECISION NOT NULL DEFAULT 0
);
-- Persist the initial virtual cash even during observation, before the first signal.
WITH capital AS (SELECT 100000.0 AS usdt)
INSERT INTO llf_paper_accounts(account_id,state)
SELECT 'LLF001-PAPER', jsonb_build_object(
    'job_id',gen_random_uuid(),'timestamp',now(),'initial_equity',usdt,'cash',usdt,
    'margin_used',0,'unrealized_pnl',0,'realized_pnl',0,'total_equity',usdt,
    'total_notional',0,'leverage',0,'drawdown_pct',0,'peak_equity',usdt
) FROM capital;

CREATE TABLE llf_paper_signals (
    signal_id UUID PRIMARY KEY, snapshot_id BIGINT NOT NULL REFERENCES llf_snapshots(id),
    asset TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('entry','exit')),
    observed_at TIMESTAMPTZ NOT NULL, decided_at TIMESTAMPTZ NOT NULL,
    percentile DOUBLE PRECISION, entry_mark DOUBLE PRECISION,
    status TEXT NOT NULL, reason TEXT NOT NULL, risk_check JSONB NOT NULL,
    order_usdt DOUBLE PRECISION, fee_bps DOUBLE PRECISION,
    UNIQUE (snapshot_id,kind)
);
CREATE INDEX llf_signals_time ON llf_paper_signals(observed_at DESC);

CREATE TABLE llf_paper_assets (
    asset TEXT PRIMARY KEY CHECK (asset IN ('BTC','ETH','SOL')),
    armed BOOLEAN NOT NULL DEFAULT TRUE, last_tick TIMESTAMPTZ,
    pending_signal UUID REFERENCES llf_paper_signals(signal_id), last_book_id BIGINT
);
INSERT INTO llf_paper_assets(asset) VALUES ('BTC'),('ETH'),('SOL');

CREATE TABLE llf_paper_trades (
    position_id UUID PRIMARY KEY, asset TEXT NOT NULL,
    entry_signal UUID NOT NULL REFERENCES llf_paper_signals(signal_id),
    opened_at TIMESTAMPTZ NOT NULL, closed_at TIMESTAMPTZ,
    entry_quantity DOUBLE PRECISION NOT NULL, fee_bps DOUBLE PRECISION NOT NULL,
    position JSONB NOT NULL, net_fills DOUBLE PRECISION NOT NULL DEFAULT 0,
    fees DOUBLE PRECISION NOT NULL DEFAULT 0, slippage_cost DOUBLE PRECISION NOT NULL DEFAULT 0,
    funding DOUBLE PRECISION NOT NULL DEFAULT 0, mae_usdt DOUBLE PRECISION NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX llf_one_open_position ON llf_paper_trades(asset) WHERE closed_at IS NULL;
CREATE INDEX llf_trades_open_time ON llf_paper_trades(asset,opened_at DESC);

CREATE TABLE llf_paper_orders (
    order_id UUID PRIMARY KEY, signal_id UUID NOT NULL REFERENCES llf_paper_signals(signal_id),
    book_id BIGINT NOT NULL REFERENCES llf_market_data(id), created_at TIMESTAMPTZ NOT NULL,
    side TEXT NOT NULL, requested_qty DOUBLE PRECISION NOT NULL, filled_qty DOUBLE PRECISION NOT NULL,
    status TEXT NOT NULL, risk_check JSONB NOT NULL,
    UNIQUE(signal_id,book_id)
);
CREATE TABLE llf_paper_fills (
    fill_id UUID PRIMARY KEY, order_id UUID NOT NULL REFERENCES llf_paper_orders(order_id),
    position_id UUID NOT NULL REFERENCES llf_paper_trades(position_id),
    side TEXT NOT NULL, quantity DOUBLE PRECISION NOT NULL, price DOUBLE PRECISION NOT NULL,
    fee DOUBLE PRECISION NOT NULL, slippage_bps DOUBLE PRECISION NOT NULL,
    slippage_cost DOUBLE PRECISION NOT NULL, levels JSONB NOT NULL, fill_time TIMESTAMPTZ NOT NULL
);
CREATE INDEX llf_fills_by_position ON llf_paper_fills(position_id,fill_time);

CREATE TABLE llf_paper_ledger (
    entry_id UUID PRIMARY KEY, account_id TEXT NOT NULL REFERENCES llf_paper_accounts(account_id),
    position_id UUID NOT NULL REFERENCES llf_paper_trades(position_id),
    fill_id UUID UNIQUE REFERENCES llf_paper_fills(fill_id), kind TEXT NOT NULL,
    effective_at TIMESTAMPTZ NOT NULL, cash_delta DOUBLE PRECISION NOT NULL,
    margin_delta DOUBLE PRECISION NOT NULL DEFAULT 0,
    funding_raw_id BIGINT REFERENCES llf_market_data(id), mark_raw_id BIGINT REFERENCES llf_market_data(id),
    UNIQUE(position_id,kind,effective_at),
    CHECK ((kind='funding' AND funding_raw_id IS NOT NULL AND mark_raw_id IS NOT NULL) OR fill_id IS NOT NULL)
);
