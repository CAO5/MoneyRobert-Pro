CREATE TABLE IF NOT EXISTS vet_shadow_runs (
    id UUID PRIMARY KEY,
    factor_version TEXT NOT NULL,
    started_at BIGINT NOT NULL,
    finished_at BIGINT,
    contracts JSONB NOT NULL
);

CREATE TABLE IF NOT EXISTS vet_shadow_samples (
    run_id UUID NOT NULL REFERENCES vet_shadow_runs(id),
    asset TEXT NOT NULL CHECK (asset IN ('BTC','ETH','SOL')),
    factor_version TEXT NOT NULL,
    exchange_ts BIGINT NOT NULL,
    available_at BIGINT NOT NULL CHECK (available_at >= exchange_ts),
    a DOUBLE PRECISION,
    l_depth DOUBLE PRECISION,
    vet DOUBLE PRECISION,
    bid_depth DOUBLE PRECISION,
    ask_depth DOUBLE PRECISION,
    total_depth DOUBLE PRECISION,
    mid_price DOUBLE PRECISION,
    past_1m_rv DOUBLE PRECISION,
    past_5m_rv DOUBLE PRECISION,
    past_30m_rv DOUBLE PRECISION,
    data_source TEXT NOT NULL,
    quality_status TEXT NOT NULL,
    diagnostics JSONB NOT NULL,
    origin_ts BIGINT CHECK (origin_ts > available_at),
    origin_price DOUBLE PRECISION,
    PRIMARY KEY (run_id, asset, exchange_ts)
);

CREATE TABLE IF NOT EXISTS vet_shadow_labels (
    run_id UUID NOT NULL,
    asset TEXT NOT NULL,
    exchange_ts BIGINT NOT NULL,
    minutes SMALLINT NOT NULL CHECK (minutes IN (1,5,15)),
    target_ts BIGINT NOT NULL,
    completed_at BIGINT NOT NULL CHECK (completed_at >= target_ts),
    realized_variance DOUBLE PRECISION,
    realized_volatility DOUBLE PRECISION,
    status TEXT NOT NULL,
    PRIMARY KEY (run_id, asset, exchange_ts, minutes),
    FOREIGN KEY (run_id, asset, exchange_ts) REFERENCES vet_shadow_samples(run_id, asset, exchange_ts)
);

CREATE INDEX IF NOT EXISTS vet_shadow_labels_target ON vet_shadow_labels(run_id,asset,target_ts);

CREATE TABLE IF NOT EXISTS vet_shadow_checkpoints (
    run_id UUID NOT NULL REFERENCES vet_shadow_runs(id),
    hours INTEGER NOT NULL,
    created_at BIGINT NOT NULL,
    report JSONB NOT NULL,
    PRIMARY KEY (run_id,hours)
);

CREATE OR REPLACE VIEW vet_shadow_observations AS
SELECT s.*, f1.realized_volatility AS future_1m_rv,
       f5.realized_volatility AS future_5m_rv, f15.realized_volatility AS future_15m_rv,
       f5.realized_variance AS future_5m_variance,
       f5.target_ts AS future_5m_target_ts, f5.completed_at AS future_5m_completed_at
FROM vet_shadow_samples s
LEFT JOIN vet_shadow_labels f1 ON f1.run_id=s.run_id AND f1.asset=s.asset AND f1.exchange_ts=s.exchange_ts AND f1.minutes=1
LEFT JOIN vet_shadow_labels f5 ON f5.run_id=s.run_id AND f5.asset=s.asset AND f5.exchange_ts=s.exchange_ts AND f5.minutes=5
LEFT JOIN vet_shadow_labels f15 ON f15.run_id=s.run_id AND f15.asset=s.asset AND f15.exchange_ts=s.exchange_ts AND f15.minutes=15;
