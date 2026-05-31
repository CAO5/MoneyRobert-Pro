-- =============================================
-- Agent System Database Migration
-- =============================================

-- AI模拟操盘配置表
CREATE TABLE IF NOT EXISTS ai_simulation_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    symbol VARCHAR(50) NOT NULL DEFAULT 'DOGE-USDT-SWAP',
    mode VARCHAR(20) NOT NULL DEFAULT 'paper',
    level INTEGER NOT NULL DEFAULT 0,
    status VARCHAR(32) NOT NULL DEFAULT 'stopped',
    
    initial_balance DOUBLE PRECISION NOT NULL DEFAULT 100000.0,
    current_balance DOUBLE PRECISION NOT NULL DEFAULT 100000.0,
    max_position_size_percent DOUBLE PRECISION NOT NULL DEFAULT 2.0,
    max_leverage INTEGER NOT NULL DEFAULT 2,
    max_daily_trades INTEGER NOT NULL DEFAULT 3,
    max_daily_loss_percent DOUBLE PRECISION NOT NULL DEFAULT 2.0,
    max_weekly_loss_percent DOUBLE PRECISION NOT NULL DEFAULT 5.0,
    max_single_trade_loss_percent DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    ai_confidence_threshold DOUBLE PRECISION NOT NULL DEFAULT 0.7,
    analysis_interval_minutes INTEGER NOT NULL DEFAULT 30,
    max_holding_period_hours INTEGER NOT NULL DEFAULT 24,
    allowed_symbols TEXT[] NOT NULL DEFAULT ARRAY['DOGE-USDT-SWAP'],
    requires_manual_confirm BOOLEAN NOT NULL DEFAULT true,
    autonomous_mode_enabled BOOLEAN NOT NULL DEFAULT false,
    
    total_trades INTEGER NOT NULL DEFAULT 0,
    winning_trades INTEGER NOT NULL DEFAULT 0,
    losing_trades INTEGER NOT NULL DEFAULT 0,
    win_rate DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    avg_pnl_percent DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    profit_loss_ratio DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    max_drawdown_percent DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    sharpe_ratio DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    
    weekly_pnl DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    weekly_loss_percent DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    daily_pnl DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    daily_loss_percent DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    consecutive_stop_losses INTEGER NOT NULL DEFAULT 0,
    
    running_days INTEGER NOT NULL DEFAULT 0,
    last_trade_at TIMESTAMP WITH TIME ZONE,
    promotion_eligible BOOLEAN NOT NULL DEFAULT false,
    risk_confirmation_signed BOOLEAN NOT NULL DEFAULT false,
    risk_confirmation_signed_at TIMESTAMP WITH TIME ZONE,
    max_acceptable_loss_amount DOUBLE PRECISION,
    
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_simulation_configs_user_id ON ai_simulation_configs(user_id);
CREATE INDEX idx_ai_simulation_configs_mode ON ai_simulation_configs(mode);
CREATE INDEX idx_ai_simulation_configs_level ON ai_simulation_configs(level);
CREATE INDEX idx_ai_simulation_configs_status ON ai_simulation_configs(status);

-- AI模拟交易记录表
CREATE TABLE IF NOT EXISTS ai_simulation_trades (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    config_id UUID NOT NULL REFERENCES ai_simulation_configs(id) ON DELETE CASCADE,
    symbol VARCHAR(50) NOT NULL,
    mode VARCHAR(20) NOT NULL,
    
    direction VARCHAR(10) NOT NULL CHECK (direction IN ('long', 'short', 'hold')),
    entry_price DOUBLE PRECISION NOT NULL,
    exit_price DOUBLE PRECISION,
    quantity DOUBLE PRECISION NOT NULL,
    leverage INTEGER NOT NULL DEFAULT 1,
    stop_loss DOUBLE PRECISION,
    take_profit DOUBLE PRECISION,
    
    ai_confidence DOUBLE PRECISION,
    ai_reasoning JSONB,
    agent_session_id UUID,
    
    pnl DOUBLE PRECISION,
    pnl_percent DOUBLE PRECISION,
    fee_percent DOUBLE PRECISION NOT NULL DEFAULT 0.05,
    net_pnl_percent DOUBLE PRECISION,
    
    status VARCHAR(20) NOT NULL DEFAULT 'open',
    close_reason VARCHAR(50),
    holding_duration_minutes INTEGER,
    
    opened_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    closed_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX idx_ai_simulation_trades_config_id ON ai_simulation_trades(config_id);
CREATE INDEX idx_ai_simulation_trades_status ON ai_simulation_trades(status);
CREATE INDEX idx_ai_simulation_trades_mode ON ai_simulation_trades(mode);
CREATE INDEX idx_ai_simulation_trades_opened_at ON ai_simulation_trades(opened_at DESC);

-- Agent辩论会话表
CREATE TABLE IF NOT EXISTS agent_debate_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    config_id UUID REFERENCES ai_simulation_configs(id) ON DELETE CASCADE,
    user_id BIGINT REFERENCES users(id) ON DELETE CASCADE,
    symbol VARCHAR(50) NOT NULL,
    
    status VARCHAR(32) NOT NULL DEFAULT 'in_progress',
    final_decision JSONB,
    confidence DOUBLE PRECISION,
    
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_agent_debate_sessions_config ON agent_debate_sessions(config_id);
CREATE INDEX idx_agent_debate_sessions_user ON agent_debate_sessions(user_id);
CREATE INDEX idx_agent_debate_sessions_status ON agent_debate_sessions(status);

-- Agent辩论消息表
CREATE TABLE IF NOT EXISTS agent_debate_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL REFERENCES agent_debate_sessions(id) ON DELETE CASCADE,
    agent_name VARCHAR(100) NOT NULL,
    agent_department VARCHAR(50) NOT NULL,
    role VARCHAR(50) NOT NULL,
    
    content TEXT NOT NULL,
    analysis_data JSONB,
    confidence DOUBLE PRECISION,
    sentiment VARCHAR(20),
    
    message_order INTEGER NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_agent_debate_messages_session ON agent_debate_messages(session_id);
CREATE INDEX idx_agent_debate_messages_agent ON agent_debate_messages(agent_name);
CREATE INDEX idx_agent_debate_messages_order ON agent_debate_messages(session_id, message_order);

-- 晋级审核记录表
CREATE TABLE IF NOT EXISTS promotion_audits (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    config_id UUID NOT NULL REFERENCES ai_simulation_configs(id) ON DELETE CASCADE,
    from_level INTEGER NOT NULL,
    to_level INTEGER NOT NULL,
    from_mode VARCHAR(20) NOT NULL,
    to_mode VARCHAR(20) NOT NULL,
    
    stats_snapshot JSONB NOT NULL,
    audit_report JSONB,
    
    status VARCHAR(32) NOT NULL DEFAULT 'pending',
    review_step INTEGER NOT NULL DEFAULT 1,
    reviewed_by VARCHAR(100),
    review_comment TEXT,
    reviewed_at TIMESTAMP WITH TIME ZONE,
    
    observation_period_days INTEGER DEFAULT 14,
    observation_started_at TIMESTAMP WITH TIME ZONE,
    observation_completed_at TIMESTAMP WITH TIME ZONE,
    
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_promotion_audits_config ON promotion_audits(config_id);
CREATE INDEX idx_promotion_audits_status ON promotion_audits(status);

-- 降级记录表
CREATE TABLE IF NOT EXISTS demotion_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    config_id UUID NOT NULL REFERENCES ai_simulation_configs(id) ON DELETE CASCADE,
    from_level INTEGER NOT NULL,
    to_level INTEGER NOT NULL,
    from_mode VARCHAR(20) NOT NULL,
    to_mode VARCHAR(20) NOT NULL,
    
    trigger_reason TEXT NOT NULL,
    stats_snapshot JSONB NOT NULL,
    
    cooldown_until TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_demotion_records_config ON demotion_records(config_id);

-- 每日统计快照表
CREATE TABLE IF NOT EXISTS daily_simulation_stats (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    config_id UUID NOT NULL REFERENCES ai_simulation_configs(id) ON DELETE CASCADE,
    date DATE NOT NULL,
    mode VARCHAR(20) NOT NULL,
    
    trades_count INTEGER NOT NULL DEFAULT 0,
    wins INTEGER NOT NULL DEFAULT 0,
    losses INTEGER NOT NULL DEFAULT 0,
    daily_pnl DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    daily_pnl_percent DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    max_drawdown_percent DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    balance_at_close DOUBLE PRECISION,
    
    rolling_win_rate_50 DOUBLE PRECISION,
    rolling_profit_loss_ratio DOUBLE PRECISION,
    
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    
    UNIQUE(config_id, date, mode)
);

CREATE INDEX idx_daily_simulation_stats_config ON daily_simulation_stats(config_id);
CREATE INDEX idx_daily_simulation_stats_date ON daily_simulation_stats(date DESC);

-- Agent性能表
CREATE TABLE IF NOT EXISTS agent_performance (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_name VARCHAR(100) NOT NULL,
    agent_department VARCHAR(50) NOT NULL,
    
    total_analyses INTEGER NOT NULL DEFAULT 0,
    correct_predictions INTEGER NOT NULL DEFAULT 0,
    accuracy DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    confidence_score_history JSONB,
    recent_performance JSONB,
    credibility_score DOUBLE PRECISION NOT NULL DEFAULT 0.5,
    calibration_factor DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    
    last_analysis_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    
    UNIQUE(agent_name, agent_department)
);

-- 知识节点表 (卢曼卡片笔记法)
CREATE TABLE IF NOT EXISTS knowledge_nodes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    content TEXT NOT NULL,
    tags TEXT[],
    source_type VARCHAR(50),
    source_id UUID,
    verification_count INTEGER NOT NULL DEFAULT 0,
    is_validated BOOLEAN NOT NULL DEFAULT false,
    confidence DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    last_updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_knowledge_nodes_tags ON knowledge_nodes USING GIN(tags);
CREATE INDEX idx_knowledge_nodes_validated ON knowledge_nodes(is_validated);

-- 知识关联表
CREATE TABLE IF NOT EXISTS knowledge_links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    from_node_id UUID NOT NULL REFERENCES knowledge_nodes(id) ON DELETE CASCADE,
    to_node_id UUID NOT NULL REFERENCES knowledge_nodes(id) ON DELETE CASCADE,
    link_type VARCHAR(50) NOT NULL,
    weight DOUBLE PRECISION NOT NULL DEFAULT 0.5,
    
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    
    UNIQUE(from_node_id, to_node_id, link_type)
);

CREATE INDEX idx_knowledge_links_from ON knowledge_links(from_node_id);
CREATE INDEX idx_knowledge_links_to ON knowledge_links(to_node_id);

-- 自主交易决策日志
CREATE TABLE IF NOT EXISTS autonomous_decision_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    config_id UUID NOT NULL REFERENCES ai_simulation_configs(id) ON DELETE CASCADE,
    
    decision_type VARCHAR(50) NOT NULL,
    decision_data JSONB NOT NULL,
    market_data_snapshot JSONB,
    risk_checks_passed JSONB,
    risk_checks_failed JSONB,
    
    executed BOOLEAN NOT NULL DEFAULT false,
    execution_result JSONB,
    
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_autonomous_decision_logs_config ON autonomous_decision_logs(config_id);
CREATE INDEX idx_autonomous_decision_logs_time ON autonomous_decision_logs(created_at DESC);

-- 熔断记录
CREATE TABLE IF NOT EXISTS circuit_break_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    config_id UUID NOT NULL REFERENCES ai_simulation_configs(id) ON DELETE CASCADE,
    
    level INTEGER NOT NULL,
    reason TEXT NOT NULL,
    severity VARCHAR(20) NOT NULL,
    trigger_data JSONB NOT NULL,
    
    resolved_at TIMESTAMP WITH TIME ZONE,
    resolved_by VARCHAR(100),
    resolution_note TEXT,
    
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_circuit_break_records_config ON circuit_break_records(config_id);
CREATE INDEX idx_circuit_break_records_resolved ON circuit_break_records(resolved_at IS NULL);

-- 风险确认书
CREATE TABLE IF NOT EXISTS risk_confirmations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    config_id UUID REFERENCES ai_simulation_configs(id) ON DELETE SET NULL,
    
    version VARCHAR(20) NOT NULL,
    accepted BOOLEAN NOT NULL DEFAULT false,
    accept_reason TEXT,
    max_acceptable_loss DOUBLE PRECISION,
    
    signed_at TIMESTAMP WITH TIME ZONE,
    ip_address VARCHAR(50),
    user_agent TEXT,
    
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_risk_confirmations_user ON risk_confirmations(user_id);
CREATE INDEX idx_risk_confirmations_config ON risk_confirmations(config_id);

-- 紧急停止记录
CREATE TABLE IF NOT EXISTS emergency_stop_records (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    config_id UUID NOT NULL REFERENCES ai_simulation_configs(id) ON DELETE CASCADE,
    
    triggered_by VARCHAR(100) NOT NULL,
    trigger_type VARCHAR(50) NOT NULL,
    trigger_reason TEXT NOT NULL,
    trigger_data JSONB,
    
    stopped_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    resumed_at TIMESTAMP WITH TIME ZONE
);

CREATE INDEX idx_emergency_stop_config ON emergency_stop_records(config_id);
CREATE INDEX idx_emergency_stop_resumed ON emergency_stop_records(resumed_at IS NULL);

-- 回测结果表
CREATE TABLE IF NOT EXISTS backtest_results (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    config_id UUID REFERENCES ai_simulation_configs(id) ON DELETE CASCADE,
    user_id BIGINT REFERENCES users(id) ON DELETE CASCADE,
    
    symbol VARCHAR(50) NOT NULL,
    strategy_name VARCHAR(100),
    parameters JSONB,
    
    start_date TIMESTAMP WITH TIME ZONE NOT NULL,
    end_date TIMESTAMP WITH TIME ZONE NOT NULL,
    
    total_trades INTEGER NOT NULL,
    winning_trades INTEGER NOT NULL,
    win_rate DOUBLE PRECISION NOT NULL,
    total_pnl DOUBLE PRECISION NOT NULL,
    total_pnl_percent DOUBLE PRECISION NOT NULL,
    max_drawdown_percent DOUBLE PRECISION NOT NULL,
    sharpe_ratio DOUBLE PRECISION,
    profit_loss_ratio DOUBLE PRECISION NOT NULL,
    
    trade_logs JSONB,
    equity_curve JSONB,
    
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_backtest_config ON backtest_results(config_id);
CREATE INDEX idx_backtest_user ON backtest_results(user_id);

-- LLM使用日志表
CREATE TABLE IF NOT EXISTS llm_usage_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider VARCHAR(50) NOT NULL,
    model VARCHAR(100) NOT NULL,
    prompt_tokens INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    agent_name VARCHAR(100),
    session_id UUID,
    user_id BIGINT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_llm_usage_logs_time ON llm_usage_logs(created_at DESC);
CREATE INDEX idx_llm_usage_logs_agent ON llm_usage_logs(agent_name);
CREATE INDEX idx_llm_usage_logs_user ON llm_usage_logs(user_id);

-- AI配置表（统一管理AI供应商和API Key）
CREATE TABLE IF NOT EXISTS ai_provider_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id BIGINT REFERENCES users(id) ON DELETE CASCADE,
    provider VARCHAR(50) NOT NULL,
    name VARCHAR(100) NOT NULL,
    api_key_encrypted TEXT NOT NULL,
    base_url VARCHAR(500),
    model VARCHAR(100),
    is_default BOOLEAN NOT NULL DEFAULT false,
    is_active BOOLEAN NOT NULL DEFAULT true,
    max_tokens INTEGER DEFAULT 2048,
    temperature DOUBLE PRECISION DEFAULT 0.7,
    monthly_budget DOUBLE PRECISION,
    current_month_usage DOUBLE PRECISION DEFAULT 0.0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    
    UNIQUE(user_id, provider, name)
);

CREATE INDEX idx_ai_provider_configs_user ON ai_provider_configs(user_id);
CREATE INDEX idx_ai_provider_configs_provider ON ai_provider_configs(provider);
