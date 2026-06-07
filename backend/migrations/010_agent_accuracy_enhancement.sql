-- Enhance decision_memory for multi-dimensional accuracy tracking
ALTER TABLE decision_memory ADD COLUMN IF NOT EXISTS market_trend VARCHAR(20) DEFAULT 'unknown';
ALTER TABLE decision_memory ADD COLUMN IF NOT EXISTS volatility VARCHAR(20) DEFAULT 'medium';
ALTER TABLE decision_memory ADD COLUMN IF NOT EXISTS volume_profile VARCHAR(20) DEFAULT 'stable';
ALTER TABLE decision_memory ADD COLUMN IF NOT EXISTS entry_timing_score FLOAT DEFAULT 0.5;
ALTER TABLE decision_memory ADD COLUMN IF NOT EXISTS risk_reward_ratio FLOAT DEFAULT 0;
ALTER TABLE decision_memory ADD COLUMN IF NOT EXISTS holding_duration_fit BOOLEAN DEFAULT false;
ALTER TABLE decision_memory ADD COLUMN IF NOT EXISTS leverage_fit BOOLEAN DEFAULT false;
ALTER TABLE decision_memory ADD COLUMN IF NOT EXISTS multi_timeframe_alignment FLOAT DEFAULT 0.5;
ALTER TABLE decision_memory ADD COLUMN IF NOT EXISTS position_quality_score FLOAT DEFAULT 0;

-- Create index for faster historical queries
CREATE INDEX IF NOT EXISTS idx_decision_memory_created_at ON decision_memory(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_decision_memory_symbol ON decision_memory(symbol);
CREATE INDEX IF NOT EXISTS idx_decision_memory_success ON decision_memory(success);
