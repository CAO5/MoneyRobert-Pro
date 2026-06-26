-- 028 补齐缺失的表

-- 1. ai_analysis 表 - AI分析结果
CREATE TABLE IF NOT EXISTS ai_analysis (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    strategy_id UUID NOT NULL,
    content JSONB NOT NULL,
    analysis_type VARCHAR(50) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ai_analysis_strategy_id ON ai_analysis(strategy_id);
CREATE INDEX IF NOT EXISTS idx_ai_analysis_analysis_type ON ai_analysis(analysis_type);
CREATE INDEX IF NOT EXISTS idx_ai_analysis_created_at ON ai_analysis(created_at DESC);

-- 2. ai_prediction_trades 表 - AI预测交易
DO $$ BEGIN
    CREATE TYPE ai_prediction_result_enum AS ENUM ('pending', 'win', 'loss');
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

CREATE TABLE IF NOT EXISTS ai_prediction_trades (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id BIGINT NOT NULL,
    symbol VARCHAR(50) NOT NULL,
    direction VARCHAR(10) NOT NULL,
    confidence_score DOUBLE PRECISION NOT NULL,
    entry_price DOUBLE PRECISION NOT NULL,
    current_price DOUBLE PRECISION,
    stop_loss DOUBLE PRECISION,
    take_profit DOUBLE PRECISION,
    leverage INTEGER NOT NULL DEFAULT 1,
    position_size_percent DOUBLE PRECISION NOT NULL,
    risk_level VARCHAR(20) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    result ai_prediction_result_enum,
    pnl_percent DOUBLE PRECISION,
    reasoning TEXT,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ai_prediction_trades_user_id ON ai_prediction_trades(user_id);
CREATE INDEX IF NOT EXISTS idx_ai_prediction_trades_symbol ON ai_prediction_trades(symbol);
CREATE INDEX IF NOT EXISTS idx_ai_prediction_trades_status ON ai_prediction_trades(status);
CREATE INDEX IF NOT EXISTS idx_ai_prediction_trades_created_at ON ai_prediction_trades(created_at DESC);

-- 3. llm_usage_logs 表 - LLM使用日志
CREATE TABLE IF NOT EXISTS llm_usage_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id BIGINT,
    provider VARCHAR(50) NOT NULL,
    model VARCHAR(100) NOT NULL,
    prompt_tokens INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    cost DOUBLE PRECISION DEFAULT 0.0,
    session_id VARCHAR(100),
    agent_name VARCHAR(100),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_llm_usage_logs_user_id ON llm_usage_logs(user_id);
CREATE INDEX IF NOT EXISTS idx_llm_usage_logs_provider ON llm_usage_logs(provider);
CREATE INDEX IF NOT EXISTS idx_llm_usage_logs_created_at ON llm_usage_logs(created_at DESC);

-- 4. market_data 表 - 市场数据
CREATE TABLE IF NOT EXISTS market_data (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    symbol VARCHAR(50) NOT NULL,
    interval VARCHAR(10) NOT NULL,
    open_time TIMESTAMPTZ NOT NULL,
    open DOUBLE PRECISION NOT NULL,
    high DOUBLE PRECISION NOT NULL,
    low DOUBLE PRECISION NOT NULL,
    close DOUBLE PRECISION NOT NULL,
    volume DOUBLE PRECISION NOT NULL,
    quote_volume DOUBLE PRECISION DEFAULT 0,
    trades_count BIGINT DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(symbol, interval, open_time)
);

CREATE INDEX IF NOT EXISTS idx_market_data_symbol_interval ON market_data(symbol, interval);
CREATE INDEX IF NOT EXISTS idx_market_data_open_time ON market_data(open_time DESC);

-- 5. risk_confirmations 表 - 风险确认
CREATE TABLE IF NOT EXISTS risk_confirmations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id BIGINT NOT NULL,
    config_id UUID,
    version VARCHAR(50) NOT NULL,
    accepted BOOLEAN NOT NULL DEFAULT FALSE,
    accept_reason TEXT,
    max_acceptable_loss DOUBLE PRECISION,
    signed_at TIMESTAMPTZ,
    ip_address VARCHAR(50),
    user_agent TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_risk_confirmations_user_id ON risk_confirmations(user_id);
CREATE INDEX IF NOT EXISTS idx_risk_confirmations_config_id ON risk_confirmations(config_id);
CREATE INDEX IF NOT EXISTS idx_risk_confirmations_created_at ON risk_confirmations(created_at DESC);

-- 6. ai_provider_configs 表 - AI提供商配置
CREATE TABLE IF NOT EXISTS ai_provider_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id BIGINT NOT NULL,
    provider VARCHAR(50) NOT NULL,
    name VARCHAR(100) NOT NULL,
    api_key_encrypted TEXT NOT NULL,
    base_url VARCHAR(500),
    model VARCHAR(100),
    max_tokens INTEGER DEFAULT 2048,
    temperature DOUBLE PRECISION DEFAULT 0.7,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    is_default BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ai_provider_configs_user_id ON ai_provider_configs(user_id);
CREATE INDEX IF NOT EXISTS idx_ai_provider_configs_provider ON ai_provider_configs(provider);
CREATE INDEX IF NOT EXISTS idx_ai_provider_configs_is_active ON ai_provider_configs(is_active);
