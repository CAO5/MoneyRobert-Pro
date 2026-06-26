-- 027 决策记忆表
CREATE TABLE IF NOT EXISTS decision_memory (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id VARCHAR(100) NOT NULL,
    session_id VARCHAR(100),
    symbol VARCHAR(50) NOT NULL,
    decision_type VARCHAR(50) NOT NULL,
    confidence DOUBLE PRECISION NOT NULL,
    was_correct BOOLEAN,
    outcome DOUBLE PRECISION,
    context JSONB,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_decision_memory_agent_id ON decision_memory(agent_id);
CREATE INDEX IF NOT EXISTS idx_decision_memory_symbol ON decision_memory(symbol);
CREATE INDEX IF NOT EXISTS idx_decision_memory_created_at ON decision_memory(created_at DESC);

-- 027 自动交易配置表
CREATE TABLE IF NOT EXISTS auto_trading_configs (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL,
    name VARCHAR(255) NOT NULL,
    mode VARCHAR(50) NOT NULL DEFAULT 'paper',
    status VARCHAR(50) NOT NULL DEFAULT 'inactive',
    symbols TEXT[] NOT NULL DEFAULT '{}',
    max_position_size DOUBLE PRECISION NOT NULL DEFAULT 10000.0,
    max_leverage INTEGER NOT NULL DEFAULT 1,
    risk_per_trade DOUBLE PRECISION NOT NULL DEFAULT 2.0,
    max_daily_trades INTEGER NOT NULL DEFAULT 10,
    max_daily_loss DOUBLE PRECISION NOT NULL DEFAULT 5.0,
    stop_loss_percent DOUBLE PRECISION NOT NULL DEFAULT 2.0,
    take_profit_percent DOUBLE PRECISION NOT NULL DEFAULT 5.0,
    ai_confidence_threshold DOUBLE PRECISION NOT NULL DEFAULT 70.0,
    auto_entry BOOLEAN NOT NULL DEFAULT false,
    auto_exit BOOLEAN NOT NULL DEFAULT false,
    enable_stop_loss BOOLEAN NOT NULL DEFAULT true,
    enable_take_profit BOOLEAN NOT NULL DEFAULT true,
    ai_analysis_version VARCHAR(50) NOT NULL DEFAULT 'v1',
    trading_hours JSONB,
    notification_settings JSONB,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_auto_trading_configs_user_id ON auto_trading_configs(user_id);
CREATE INDEX IF NOT EXISTS idx_auto_trading_configs_status ON auto_trading_configs(status);

-- 027 AI 模拟交易配置表
CREATE TABLE IF NOT EXISTS ai_simulation_trade_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id BIGINT NOT NULL,
    name VARCHAR(255) NOT NULL,
    initial_balance DOUBLE PRECISION NOT NULL DEFAULT 100000.0,
    max_position_size DOUBLE PRECISION NOT NULL DEFAULT 10000.0,
    max_leverage INTEGER NOT NULL DEFAULT 1,
    risk_per_trade DOUBLE PRECISION NOT NULL DEFAULT 2.0,
    max_daily_trades INTEGER NOT NULL DEFAULT 10,
    max_daily_loss DOUBLE PRECISION NOT NULL DEFAULT 5.0,
    stop_loss_percent DOUBLE PRECISION NOT NULL DEFAULT 2.0,
    take_profit_percent DOUBLE PRECISION NOT NULL DEFAULT 5.0,
    ai_confidence_threshold DOUBLE PRECISION NOT NULL DEFAULT 70.0,
    auto_entry BOOLEAN NOT NULL DEFAULT false,
    auto_exit BOOLEAN NOT NULL DEFAULT false,
    enable_stop_loss BOOLEAN NOT NULL DEFAULT true,
    enable_take_profit BOOLEAN NOT NULL DEFAULT true,
    ai_analysis_version VARCHAR(50) NOT NULL DEFAULT 'v1',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ai_simulation_trade_configs_user_id ON ai_simulation_trade_configs(user_id);
