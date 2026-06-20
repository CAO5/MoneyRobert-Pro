-- =============================================
-- Trading Simulation & Backtest Module (Phase 1)
-- 交易模拟与历史回测模块 - 核心数据表
-- =============================================

-- ---------------------------------------------
-- 1. backtest_jobs: 回测任务
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS backtest_jobs (
    job_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id BIGINT REFERENCES users(id) ON DELETE SET NULL,
    job_name VARCHAR(200) NOT NULL,
    strategy_id VARCHAR(100),
    strategy_version VARCHAR(50),
    assets VARCHAR(100)[] NOT NULL DEFAULT '{}',
    exchanges VARCHAR(50)[] NOT NULL DEFAULT '{}',
    start_time TIMESTAMP WITH TIME ZONE NOT NULL,
    end_time TIMESTAMP WITH TIME ZONE NOT NULL,
    initial_equity DOUBLE PRECISION NOT NULL DEFAULT 100000.0,
    base_currency VARCHAR(20) NOT NULL DEFAULT 'USDT',
    mode VARCHAR(20) NOT NULL DEFAULT 'backtest',
    status VARCHAR(32) NOT NULL DEFAULT 'created',
    progress DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    current_replay_time TIMESTAMP WITH TIME ZONE,
    data_frequency VARCHAR(10) NOT NULL DEFAULT '1h',

    -- 撮合与风险配置
    fee_model VARCHAR(20) NOT NULL DEFAULT 'fixed',
    fee_taker_bps DOUBLE PRECISION NOT NULL DEFAULT 5.0,
    fee_maker_bps DOUBLE PRECISION NOT NULL DEFAULT 2.0,
    slippage_model VARCHAR(20) NOT NULL DEFAULT 'fixed',
    slippage_bps DOUBLE PRECISION NOT NULL DEFAULT 3.0,
    max_single_position_pct DOUBLE PRECISION NOT NULL DEFAULT 0.1,
    max_total_leverage DOUBLE PRECISION NOT NULL DEFAULT 3.0,
    max_daily_loss_pct DOUBLE PRECISION NOT NULL DEFAULT 0.03,
    min_signal_confidence DOUBLE PRECISION NOT NULL DEFAULT 0.5,
    min_signal_strength DOUBLE PRECISION NOT NULL DEFAULT 0.3,

    -- 结果字段
    total_trades INTEGER DEFAULT 0,
    winning_trades INTEGER DEFAULT 0,
    total_return_pct DOUBLE PRECISION,
    annualized_return_pct DOUBLE PRECISION,
    max_drawdown_pct DOUBLE PRECISION,
    sharpe_ratio DOUBLE PRECISION,
    sortino_ratio DOUBLE PRECISION,
    win_rate DOUBLE PRECISION,
    profit_factor DOUBLE PRECISION,
    fee_total DOUBLE PRECISION DEFAULT 0.0,
    slippage_total DOUBLE PRECISION DEFAULT 0.0,

    error_message TEXT,
    config JSONB,
    started_at TIMESTAMP WITH TIME ZONE,
    completed_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_backtest_jobs_user ON backtest_jobs(user_id);
CREATE INDEX IF NOT EXISTS idx_backtest_jobs_status ON backtest_jobs(status);
CREATE INDEX IF NOT EXISTS idx_backtest_jobs_strategy ON backtest_jobs(strategy_id);
CREATE INDEX IF NOT EXISTS idx_backtest_jobs_created ON backtest_jobs(created_at DESC);

-- ---------------------------------------------
-- 2. alpha_signals: 标准化Alpha信号
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS alpha_signals (
    signal_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID REFERENCES backtest_jobs(job_id) ON DELETE CASCADE,
    strategy_id VARCHAR(100),
    agent_id VARCHAR(100),
    asset VARCHAR(50) NOT NULL,
    exchange VARCHAR(50),
    timeframe VARCHAR(10),
    event_time TIMESTAMP WITH TIME ZONE NOT NULL,
    valid_until TIMESTAMP WITH TIME ZONE,
    direction VARCHAR(10) NOT NULL CHECK (direction IN ('long', 'short', 'hold')),
    signal_strength DOUBLE PRECISION,
    confidence DOUBLE PRECISION,
    expected_return_bps DOUBLE PRECISION,
    expected_holding_period_sec INTEGER,
    market_regime VARCHAR(50),
    features_used JSONB,
    risk_flags JSONB,
    explanation TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),

    -- 回测评估字段
    realized_return_bps DOUBLE PRECISION,
    signal_hit BOOLEAN
);

CREATE INDEX IF NOT EXISTS idx_alpha_signals_job ON alpha_signals(job_id);
CREATE INDEX IF NOT EXISTS idx_alpha_signals_asset_time ON alpha_signals(asset, event_time DESC);
CREATE INDEX IF NOT EXISTS idx_alpha_signals_agent ON alpha_signals(agent_id);
CREATE INDEX IF NOT EXISTS idx_alpha_signals_strategy ON alpha_signals(strategy_id);

-- ---------------------------------------------
-- 3. trade_intents: 交易意图 (信号 -> 目标订单)
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS trade_intents (
    intent_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID REFERENCES backtest_jobs(job_id) ON DELETE CASCADE,
    source_signal_id UUID REFERENCES alpha_signals(signal_id) ON DELETE SET NULL,
    strategy_id VARCHAR(100),
    agent_id VARCHAR(100),
    asset VARCHAR(50) NOT NULL,
    exchange VARCHAR(50),
    side VARCHAR(10) NOT NULL CHECK (side IN ('buy', 'sell')),
    intent_type VARCHAR(32) NOT NULL CHECK (intent_type IN ('open_position', 'increase_position', 'reduce_position', 'close_position', 'stop_loss', 'take_profit')),
    target_position_pct DOUBLE PRECISION,
    target_notional DOUBLE PRECISION,
    target_quantity DOUBLE PRECISION,
    order_type VARCHAR(20) NOT NULL DEFAULT 'market',
    limit_price DOUBLE PRECISION,
    max_slippage_bps DOUBLE PRECISION,
    leverage INTEGER DEFAULT 1,
    stop_loss_price DOUBLE PRECISION,
    take_profit_price DOUBLE PRECISION,

    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    reject_reason VARCHAR(200),
    reason TEXT,
    event_time TIMESTAMP WITH TIME ZONE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_trade_intents_job ON trade_intents(job_id);
CREATE INDEX IF NOT EXISTS idx_trade_intents_signal ON trade_intents(source_signal_id);
CREATE INDEX IF NOT EXISTS idx_trade_intents_time ON trade_intents(event_time DESC);
CREATE INDEX IF NOT EXISTS idx_trade_intents_asset ON trade_intents(asset, event_time DESC);

-- ---------------------------------------------
-- 4. simulated_orders: 模拟订单
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS simulated_orders (
    order_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID REFERENCES backtest_jobs(job_id) ON DELETE CASCADE,
    intent_id UUID REFERENCES trade_intents(intent_id) ON DELETE SET NULL,
    source_signal_id UUID REFERENCES alpha_signals(signal_id) ON DELETE SET NULL,
    strategy_id VARCHAR(100),
    agent_id VARCHAR(100),
    asset VARCHAR(50) NOT NULL,
    exchange VARCHAR(50),
    side VARCHAR(10) NOT NULL CHECK (side IN ('buy', 'sell')),
    order_type VARCHAR(20) NOT NULL DEFAULT 'market',
    price DOUBLE PRECISION,
    quantity DOUBLE PRECISION NOT NULL,
    notional DOUBLE PRECISION,
    filled_quantity DOUBLE PRECISION DEFAULT 0.0,
    filled_price DOUBLE PRECISION,
    fee DOUBLE PRECISION DEFAULT 0.0,
    slippage_bps DOUBLE PRECISION,

    leverage INTEGER DEFAULT 1,
    stop_loss DOUBLE PRECISION,
    take_profit DOUBLE PRECISION,

    status VARCHAR(20) NOT NULL DEFAULT 'submitted',
    reject_reason VARCHAR(200),
    submitted_at TIMESTAMP WITH TIME ZONE NOT NULL,
    filled_at TIMESTAMP WITH TIME ZONE,
    cancelled_at TIMESTAMP WITH TIME ZONE,

    risk_checks_passed JSONB,
    risk_checks_failed JSONB,
    metadata JSONB
);

CREATE INDEX IF NOT EXISTS idx_simulated_orders_job ON simulated_orders(job_id);
CREATE INDEX IF NOT EXISTS idx_simulated_orders_status ON simulated_orders(status);
CREATE INDEX IF NOT EXISTS idx_simulated_orders_time ON simulated_orders(submitted_at DESC);
CREATE INDEX IF NOT EXISTS idx_simulated_orders_asset ON simulated_orders(asset);

-- ---------------------------------------------
-- 5. simulated_fills: 模拟成交
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS simulated_fills (
    fill_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id UUID NOT NULL REFERENCES simulated_orders(order_id) ON DELETE CASCADE,
    job_id UUID REFERENCES backtest_jobs(job_id) ON DELETE CASCADE,
    asset VARCHAR(50) NOT NULL,
    exchange VARCHAR(50),
    side VARCHAR(10) NOT NULL CHECK (side IN ('buy', 'sell')),
    filled_quantity DOUBLE PRECISION NOT NULL,
    filled_price DOUBLE PRECISION NOT NULL,
    notional DOUBLE PRECISION,
    fee DOUBLE PRECISION DEFAULT 0.0,
    fee_asset VARCHAR(20) DEFAULT 'USDT',
    slippage_bps DOUBLE PRECISION,
    maker_taker VARCHAR(10) DEFAULT 'taker',

    -- 归因字段
    signal_id UUID REFERENCES alpha_signals(signal_id) ON DELETE SET NULL,
    strategy_id VARCHAR(100),
    agent_id VARCHAR(100),
    intent_type VARCHAR(32),

    fill_time TIMESTAMP WITH TIME ZONE NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_simulated_fills_job ON simulated_fills(job_id);
CREATE INDEX IF NOT EXISTS idx_simulated_fills_order ON simulated_fills(order_id);
CREATE INDEX IF NOT EXISTS idx_simulated_fills_time ON simulated_fills(fill_time DESC);
CREATE INDEX IF NOT EXISTS idx_simulated_fills_agent ON simulated_fills(agent_id);
CREATE INDEX IF NOT EXISTS idx_simulated_fills_strategy ON simulated_fills(strategy_id);

-- ---------------------------------------------
-- 6. simulated_positions: 模拟持仓 (当前持仓快照)
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS simulated_positions (
    position_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID REFERENCES backtest_jobs(job_id) ON DELETE CASCADE,
    asset VARCHAR(50) NOT NULL,
    exchange VARCHAR(50),
    side VARCHAR(10) NOT NULL CHECK (side IN ('long', 'short')),
    quantity DOUBLE PRECISION NOT NULL,
    avg_entry_price DOUBLE PRECISION NOT NULL,
    mark_price DOUBLE PRECISION,
    notional DOUBLE PRECISION,
    unrealized_pnl DOUBLE PRECISION DEFAULT 0.0,
    realized_pnl DOUBLE PRECISION DEFAULT 0.0,
    margin DOUBLE PRECISION,
    leverage INTEGER DEFAULT 1,
    liquidation_price DOUBLE PRECISION,
    stop_loss_price DOUBLE PRECISION,
    take_profit_price DOUBLE PRECISION,

    -- 来源归因
    open_signal_id UUID REFERENCES alpha_signals(signal_id) ON DELETE SET NULL,
    strategy_id VARCHAR(100),
    agent_id VARCHAR(100),

    opened_at TIMESTAMP WITH TIME ZONE NOT NULL,
    closed_at TIMESTAMP WITH TIME ZONE,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_simulated_positions_job ON simulated_positions(job_id);
CREATE INDEX IF NOT EXISTS idx_simulated_positions_asset ON simulated_positions(asset);
CREATE INDEX IF NOT EXISTS idx_simulated_positions_agent ON simulated_positions(agent_id);

-- ---------------------------------------------
-- 7. account_snapshots: 账户权益曲线快照
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS account_snapshots (
    snapshot_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID REFERENCES backtest_jobs(job_id) ON DELETE CASCADE,
    timestamp TIMESTAMP WITH TIME ZONE NOT NULL,
    cash DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    margin_used DOUBLE PRECISION DEFAULT 0.0,
    unrealized_pnl DOUBLE PRECISION DEFAULT 0.0,
    realized_pnl DOUBLE PRECISION DEFAULT 0.0,
    total_equity DOUBLE PRECISION NOT NULL,
    total_notional DOUBLE PRECISION,
    leverage DOUBLE PRECISION,
    drawdown_pct DOUBLE PRECISION,
    position_count INTEGER DEFAULT 0,
    open_order_count INTEGER DEFAULT 0,
    metadata JSONB
);

CREATE INDEX IF NOT EXISTS idx_account_snapshots_job ON account_snapshots(job_id);
CREATE INDEX IF NOT EXISTS idx_account_snapshots_time ON account_snapshots(job_id, timestamp);

-- ---------------------------------------------
-- 8. trade_attributions: 完整交易归因 (开仓+平仓一个完整周期)
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS trade_attributions (
    attribution_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID REFERENCES backtest_jobs(job_id) ON DELETE CASCADE,
    asset VARCHAR(50) NOT NULL,
    strategy_id VARCHAR(100),
    agent_id VARCHAR(100),

    direction VARCHAR(10) NOT NULL,
    entry_time TIMESTAMP WITH TIME ZONE NOT NULL,
    exit_time TIMESTAMP WITH TIME ZONE,
    entry_price DOUBLE PRECISION NOT NULL,
    exit_price DOUBLE PRECISION,
    quantity DOUBLE PRECISION NOT NULL,

    pnl DOUBLE PRECISION,
    pnl_bps DOUBLE PRECISION,
    fee_total DOUBLE PRECISION DEFAULT 0.0,
    slippage_total_bps DOUBLE PRECISION,
    holding_period_sec INTEGER,
    market_regime_at_entry VARCHAR(50),

    signal_confidence DOUBLE PRECISION,
    signal_strength DOUBLE PRECISION,
    entry_signal_id UUID REFERENCES alpha_signals(signal_id) ON DELETE SET NULL,
    exit_reason VARCHAR(50),

    result VARCHAR(10) CHECK (result IN ('win', 'loss', 'break_even')),
    attribution JSONB,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_trade_attributions_job ON trade_attributions(job_id);
CREATE INDEX IF NOT EXISTS idx_trade_attributions_agent ON trade_attributions(agent_id);
CREATE INDEX IF NOT EXISTS idx_trade_attributions_strategy ON trade_attributions(strategy_id);
CREATE INDEX IF NOT EXISTS idx_trade_attributions_result ON trade_attributions(result);
CREATE INDEX IF NOT EXISTS idx_trade_attributions_time ON trade_attributions(job_id, entry_time DESC);

-- ---------------------------------------------
-- 9. performance_reports: 完整回测报告
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS performance_reports (
    report_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID NOT NULL REFERENCES backtest_jobs(job_id) ON DELETE CASCADE,

    -- 总体绩效
    total_return DOUBLE PRECISION,
    annualized_return DOUBLE PRECISION,
    max_drawdown DOUBLE PRECISION,
    max_drawdown_duration_sec INTEGER,
    sharpe_ratio DOUBLE PRECISION,
    sortino_ratio DOUBLE PRECISION,
    calmar_ratio DOUBLE PRECISION,
    win_rate DOUBLE PRECISION,
    profit_factor DOUBLE PRECISION,
    average_win DOUBLE PRECISION,
    average_loss DOUBLE PRECISION,
    payoff_ratio DOUBLE PRECISION,
    turnover DOUBLE PRECISION,
    fee_ratio DOUBLE PRECISION,
    total_fee DOUBLE PRECISION,
    total_slippage_cost DOUBLE PRECISION,

    -- 交易统计
    total_trades INTEGER,
    winning_trades INTEGER,
    losing_trades INTEGER,
    max_consecutive_wins INTEGER,
    max_consecutive_losses INTEGER,
    avg_holding_period_sec DOUBLE PRECISION,

    -- 维度归因
    by_agent JSONB,
    by_strategy JSONB,
    by_asset JSONB,
    by_regime JSONB,
    by_weekday JSONB,
    by_hour JSONB,

    -- 权益曲线 (精简)
    equity_curve_start DOUBLE PRECISION,
    equity_curve_end DOUBLE PRECISION,
    equity_peak DOUBLE PRECISION,
    equity_valley DOUBLE PRECISION,

    -- 风控拦截统计
    rejected_intents_count INTEGER DEFAULT 0,
    risk_rejection_reasons JSONB,

    -- 准入判定
    eligibility_status VARCHAR(32) DEFAULT 'pending',
    eligibility_reason TEXT,
    recommendation JSONB,

    report_json JSONB,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_performance_reports_job ON performance_reports(job_id);
CREATE INDEX IF NOT EXISTS idx_performance_reports_status ON performance_reports(eligibility_status);

-- ---------------------------------------------
-- 10. risk_events: 风控事件
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS risk_events (
    event_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID REFERENCES backtest_jobs(job_id) ON DELETE CASCADE,
    event_time TIMESTAMP WITH TIME ZONE NOT NULL,
    event_type VARCHAR(50) NOT NULL,
    severity VARCHAR(20) NOT NULL,
    asset VARCHAR(50),
    reason TEXT,
    details JSONB,
    intent_id UUID REFERENCES trade_intents(intent_id) ON DELETE SET NULL,
    order_id UUID REFERENCES simulated_orders(order_id) ON DELETE SET NULL,
    action_taken VARCHAR(50) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_risk_events_job ON risk_events(job_id);
CREATE INDEX IF NOT EXISTS idx_risk_events_type ON risk_events(event_type);
CREATE INDEX IF NOT EXISTS idx_risk_events_time ON risk_events(event_time DESC);
