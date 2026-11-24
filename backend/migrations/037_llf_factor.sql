INSERT INTO feature_definitions (name, description, category, version, parameters, unit)
VALUES ('llf_v0_2', 'LLF v0.2 研究因子，公式待冻结', 'microstructure', '0.2',
        '{"formula_status":"unconfirmed","observation_interval_seconds":1,"proposed_baseline_days":30}'::jsonb,
        'score')
ON CONFLICT (name) DO NOTHING;

-- 输入和质量记录需要保留空分数，只有已冻结且可计算的因子才写入 feature_values。
CREATE TABLE llf_snapshots (
    id BIGSERIAL PRIMARY KEY,
    asset TEXT NOT NULL CHECK (asset IN ('BTC', 'ETH', 'SOL')),
    sample_second BIGINT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    factor_name TEXT NOT NULL DEFAULT 'llf_v0_2' REFERENCES feature_definitions(name),
    factor_version TEXT NOT NULL DEFAULT '0.2',
    formula_status TEXT NOT NULL DEFAULT 'unconfirmed',
    components JSONB NOT NULL DEFAULT '{"oi_load":null,"carry":null,"flow":null,"liquidity":null}',
    score DOUBLE PRECISION,
    percentile DOUBLE PRECISION CHECK (percentile BETWEEN 0 AND 100),
    quality TEXT NOT NULL CHECK (quality IN ('waiting', 'incomplete', 'stale', 'ready')),
    reason TEXT NOT NULL,
    inputs JSONB NOT NULL,
    UNIQUE (factor_name, asset, sample_second),
    CHECK (formula_status <> 'unconfirmed' OR (
        score IS NULL AND percentile IS NULL
        AND components = '{"oi_load":null,"carry":null,"flow":null,"liquidity":null}'::jsonb
    ))
);
CREATE INDEX llf_snapshots_latest_idx ON llf_snapshots (factor_name, asset, observed_at DESC);

-- 记录真实入库的 OI 覆盖日。覆盖日仅用于采集进度，不代表该日满足研究基准的样本质量。
CREATE TABLE llf_oi_coverage (
    asset TEXT NOT NULL CHECK (asset IN ('BTC', 'ETH', 'SOL')),
    sample_day DATE NOT NULL,
    first_at TIMESTAMPTZ NOT NULL,
    last_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (asset, sample_day)
);
