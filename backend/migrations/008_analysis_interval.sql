-- Add analysis_interval (K-line timeframe) column to ai_simulation_configs
ALTER TABLE ai_simulation_configs ADD COLUMN IF NOT EXISTS analysis_interval VARCHAR(10) NOT NULL DEFAULT '1H';
