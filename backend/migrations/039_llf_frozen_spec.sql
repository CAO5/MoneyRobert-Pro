-- The earlier observation rows remain archived and are excluded from the minute baseline.
ALTER TABLE llf_snapshots ADD COLUMN raw_values DOUBLE PRECISION[];
ALTER TABLE llf_snapshots ALTER COLUMN formula_status SET DEFAULT 'confirmed';
ALTER TABLE llf_snapshots DROP CONSTRAINT llf_snapshots_factor_name_asset_sample_second_key;
ALTER TABLE llf_snapshots ADD UNIQUE (factor_name, asset, sample_second, formula_status);
UPDATE feature_definitions SET description = 'LLF v0.2 · LLF-OKX-3ASSET-1M-001',
    parameters = '{"formula_status":"confirmed","spec_id":"LLF-OKX-3ASSET-1M-001","observation_interval_seconds":60,"baseline_days":30,"minimum_samples":38880}'::jsonb
WHERE name = 'llf_v0_2';
CREATE INDEX llf_minute_baseline ON llf_snapshots(asset, observed_at)
INCLUDE (raw_values, score) WHERE formula_status = 'confirmed' AND raw_values IS NOT NULL;
