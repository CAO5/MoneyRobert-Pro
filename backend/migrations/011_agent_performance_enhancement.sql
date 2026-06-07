-- Enhance agent_performance for market context awareness
ALTER TABLE agent_performance ADD COLUMN IF NOT EXISTS trend_accuracy FLOAT DEFAULT 0.5;
ALTER TABLE agent_performance ADD COLUMN IF NOT EXISTS volatility_accuracy FLOAT DEFAULT 0.5;
ALTER TABLE agent_performance ADD COLUMN IF NOT EXISTS volume_accuracy FLOAT DEFAULT 0.5;
ALTER TABLE agent_performance ADD COLUMN IF NOT EXISTS timing_accuracy FLOAT DEFAULT 0.5;
ALTER TABLE agent_performance ADD COLUMN IF NOT EXISTS multi_timeframe_accuracy FLOAT DEFAULT 0.5;
ALTER TABLE agent_performance ADD COLUMN IF NOT EXISTS weighted_accuracy FLOAT DEFAULT 0.5;
ALTER TABLE agent_performance ADD COLUMN IF NOT EXISTS total_predictions INT DEFAULT 0;
ALTER TABLE agent_performance ADD COLUMN IF NOT EXISTS prediction_decay_rate FLOAT DEFAULT 0.95;

-- Create index for credibility ranking
CREATE INDEX IF NOT EXISTS idx_agent_performance_credibility ON agent_performance(credibility_score DESC);
CREATE INDEX IF NOT EXISTS idx_agent_performance_accuracy ON agent_performance(accuracy DESC);
