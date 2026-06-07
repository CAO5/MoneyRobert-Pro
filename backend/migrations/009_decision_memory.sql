-- Decision Memory: 记录每次辩论决策及其实际结果，用于历史反思
CREATE TABLE IF NOT EXISTS decision_memory (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id BIGINT NOT NULL,
    config_id UUID REFERENCES ai_simulation_configs(id) ON DELETE SET NULL,
    trade_id UUID UNIQUE REFERENCES ai_simulation_trades(id) ON DELETE SET NULL,
    debate_session_id UUID,

    symbol VARCHAR(50) NOT NULL,
    action VARCHAR(16) NOT NULL,
    confidence DOUBLE PRECISION NOT NULL,
    leverage INTEGER NOT NULL DEFAULT 1,
    stop_loss DOUBLE PRECISION,
    take_profit DOUBLE PRECISION,

    agent_opinions JSONB NOT NULL DEFAULT '[]',
    department_reports JSONB NOT NULL DEFAULT '[]',
    reasoning TEXT,

    -- Actual outcome (filled when trade closes)
    actual_outcome VARCHAR(20),
    actual_pnl DOUBLE PRECISION,
    actual_pnl_percent DOUBLE PRECISION,
    success BOOLEAN,
    holding_duration_minutes INTEGER,
    close_reason VARCHAR(50),

    -- Reflection (auto-generated after trade closes)
    reflection TEXT,

    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_decision_memory_user ON decision_memory(user_id);
CREATE INDEX idx_decision_memory_config ON decision_memory(config_id);
CREATE INDEX idx_decision_memory_symbol ON decision_memory(symbol);
CREATE INDEX idx_decision_memory_success ON decision_memory(success) WHERE success IS NOT NULL;
CREATE INDEX idx_decision_memory_created ON decision_memory(created_at DESC);
