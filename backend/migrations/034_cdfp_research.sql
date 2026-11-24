CREATE TABLE IF NOT EXISTS cdfp_runs (
    id UUID PRIMARY KEY,
    started_at BIGINT NOT NULL,
    finished_at BIGINT,
    factor_version TEXT NOT NULL,
    contracts JSONB NOT NULL
);

CREATE TABLE IF NOT EXISTS cdfp_samples (
    run_id UUID NOT NULL REFERENCES cdfp_runs(id),
    asset TEXT NOT NULL CHECK (asset IN ('BTC', 'ETH', 'SOL')),
    factor_version TEXT NOT NULL,
    factor_time BIGINT NOT NULL,
    available_time BIGINT NOT NULL,
    cdfp_value DOUBLE PRECISION CHECK (cdfp_value BETWEEN -1 AND 1),
    issued_cdfp_value DOUBLE PRECISION CHECK (issued_cdfp_value BETWEEN -1 AND 1),
    spot_imbalance DOUBLE PRECISION,
    perp_imbalance DOUBLE PRECISION,
    same_direction_bins SMALLINT,
    volume_quality TEXT NOT NULL,
    market_regime TEXT NOT NULL,
    quality_status TEXT NOT NULL,
    momentum DOUBLE PRECISION,
    abs_return_1m DOUBLE PRECISION,
    realized_vol_15m DOUBLE PRECISION,
    thresholds JSONB NOT NULL,
    bins JSONB NOT NULL,
    gap_detected_time BIGINT,
    price_time BIGINT,
    price_event_time BIGINT,
    start_price DOUBLE PRECISION,
    labels JSONB NOT NULL,
    PRIMARY KEY (run_id, asset, factor_time)
);

CREATE INDEX IF NOT EXISTS cdfp_runs_started_at ON cdfp_runs (started_at DESC);
CREATE INDEX IF NOT EXISTS cdfp_latest_completed ON cdfp_samples (run_id, asset, factor_time DESC)
    WHERE labels->'5'->>'status' = 'COMPLETE';
