-- 辩论会话表 (代码中使用 debate_sessions，而非 agent_debate_sessions)
CREATE TABLE IF NOT EXISTS debate_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id BIGINT REFERENCES users(id) ON DELETE CASCADE,
    symbol VARCHAR(50) NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'in_progress',
    progress VARCHAR(50) NOT NULL DEFAULT 'fetching_market_data',
    agent_opinions JSONB NOT NULL DEFAULT '[]'::jsonb,
    department_reports JSONB NOT NULL DEFAULT '[]'::jsonb,
    fund_manager_decision JSONB NOT NULL DEFAULT '{}'::jsonb,
    confidence DOUBLE PRECISION,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_debate_sessions_user ON debate_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_debate_sessions_status ON debate_sessions(status);
