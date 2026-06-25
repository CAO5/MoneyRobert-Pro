-- =============================================
-- Post-Trade Attribution & Strategy Failure (Phase 4)
-- 交易后自动归因与策略失效检测
--
-- 依据：系统评估与演进规划 第四阶段任务4
--   "交易后自动归因和策略失效提醒"
--
-- 本迁移新增：
--   1. trade_attributions：交易后归因分析结果
--   2. strategy_failure_alerts：策略失效告警
-- =============================================

-- ---------------------------------------------
-- 1. trade_attributions：交易后归因分析
--    对每笔已平仓交易做归因：盈亏来源分解
-- ---------------------------------------------
DROP TABLE IF EXISTS trade_attributions CASCADE;
CREATE TABLE trade_attributions (
    attribution_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID REFERENCES backtest_jobs(job_id) ON DELETE CASCADE,
    user_id BIGINT REFERENCES users(id) ON DELETE SET NULL,

    -- 交易标识
    symbol VARCHAR(50) NOT NULL,
    order_id UUID REFERENCES simulated_orders(order_id) ON DELETE SET NULL,
    fill_id UUID REFERENCES simulated_fills(fill_id) ON DELETE SET NULL,
    decision_card_id UUID REFERENCES decision_cards(card_id) ON DELETE SET NULL,

    -- 归因时间
    entry_time TIMESTAMP WITH TIME ZONE NOT NULL,
    exit_time TIMESTAMP WITH TIME ZONE,
    holding_period_sec INTEGER,

    -- 盈亏分解
    gross_pnl DOUBLE PRECISION NOT NULL DEFAULT 0.0,       -- 毛盈亏
    fee_cost DOUBLE PRECISION NOT NULL DEFAULT 0.0,        -- 手续费
    slippage_cost DOUBLE PRECISION NOT NULL DEFAULT 0.0,    -- 滑点成本
    funding_cost DOUBLE PRECISION NOT NULL DEFAULT 0.0,     -- 资金费率成本
    impact_cost DOUBLE PRECISION NOT NULL DEFAULT 0.0,     -- 市场冲击成本
    net_pnl DOUBLE PRECISION NOT NULL DEFAULT 0.0,         -- 净盈亏

    -- 归因维度
    direction VARCHAR(10) NOT NULL,                          -- long/short
    market_regime VARCHAR(50),                               -- 入场时市场状态
    exit_regime VARCHAR(50),                                 -- 出场时市场状态
    signal_source VARCHAR(100),                              -- 信号来源（agent/model）
    signal_confidence DOUBLE PRECISION,                     -- 入场时信号置信度
    calibrated_probability DOUBLE PRECISION,                -- 校准后概率

    -- 归因标签
    win_loss VARCHAR(10),                                   -- win/loss/breakeven
    exit_reason VARCHAR(50),                                -- 止盈/止损/信号反转/时间止损/手动
    attribution_tags JSONB NOT NULL DEFAULT '[]'::jsonb,    -- 归因标签数组

    -- 对比基准
    benchmark_return DOUBLE PRECISION,                      -- 同期买入持有收益
    alpha DOUBLE PRECISION,                                 -- 超额收益

    -- 证据链
    evidence JSONB NOT NULL DEFAULT '{}'::jsonb,            -- 归因证据

    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_trade_attr_job ON trade_attributions(job_id);
CREATE INDEX IF NOT EXISTS idx_trade_attr_symbol ON trade_attributions(symbol);
CREATE INDEX IF NOT EXISTS idx_trade_attr_user ON trade_attributions(user_id);
CREATE INDEX IF NOT EXISTS idx_trade_attr_win_loss ON trade_attributions(win_loss);
CREATE INDEX IF NOT EXISTS idx_trade_attr_regime ON trade_attributions(market_regime);
CREATE INDEX IF NOT EXISTS idx_trade_attr_exit_reason ON trade_attributions(exit_reason);
CREATE INDEX IF NOT EXISTS idx_trade_attr_time ON trade_attributions(entry_time DESC);

COMMENT ON TABLE trade_attributions IS '交易后归因：盈亏来源分解、信号质量评估、基准对比';

-- ---------------------------------------------
-- 2. strategy_failure_alerts：策略失效告警
--    当策略表现退化时自动告警
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS strategy_failure_alerts (
    alert_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    strategy_id UUID,                                       -- 策略标识
    strategy_name VARCHAR(200) NOT NULL,
    symbol VARCHAR(50),                                     -- NULL 表示全市场
    user_id BIGINT REFERENCES users(id) ON DELETE SET NULL,

    -- 告警类型
    alert_type VARCHAR(50) NOT NULL,                         -- drawdown_breach/calibration_drift/win_rate_drop/profit_factor_drop/correlation_breakdown/regime_shift

    -- 告警级别
    severity VARCHAR(20) NOT NULL DEFAULT 'warning',        -- info/warning/critical

    -- 告警内容
    title VARCHAR(500) NOT NULL,
    description TEXT NOT NULL,

    -- 触发指标
    trigger_metric VARCHAR(100) NOT NULL,                   -- 触发指标名
    trigger_value DOUBLE PRECISION NOT NULL,                 -- 触发值
    threshold_value DOUBLE PRECISION,                       -- 阈值
    baseline_value DOUBLE PRECISION,                        -- 基线值

    -- 评估窗口
    eval_window_start TIMESTAMP WITH TIME ZONE NOT NULL,
    eval_window_end TIMESTAMP WITH TIME ZONE NOT NULL,
    sample_count INTEGER NOT NULL DEFAULT 0,

    -- 建议动作
    recommended_action VARCHAR(200),                         -- 建议动作
    auto_action_taken VARCHAR(100),                          -- 已执行的自动动作

    -- 状态
    status VARCHAR(20) NOT NULL DEFAULT 'active',            -- active/acknowledged/resolved/dismissed
    acknowledged_at TIMESTAMP WITH TIME ZONE,
    acknowledged_by BIGINT REFERENCES users(id),
    resolved_at TIMESTAMP WITH TIME ZONE,

    -- 详情
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,

    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_strategy_alerts_strategy ON strategy_failure_alerts(strategy_id);
CREATE INDEX IF NOT EXISTS idx_strategy_alerts_symbol ON strategy_failure_alerts(symbol);
CREATE INDEX IF NOT EXISTS idx_strategy_alerts_user ON strategy_failure_alerts(user_id);
CREATE INDEX IF NOT EXISTS idx_strategy_alerts_status ON strategy_failure_alerts(status);
CREATE INDEX IF NOT EXISTS idx_strategy_alerts_severity ON strategy_failure_alerts(severity);
CREATE INDEX IF NOT EXISTS idx_strategy_alerts_type ON strategy_failure_alerts(alert_type);
CREATE INDEX IF NOT EXISTS idx_strategy_alerts_created ON strategy_failure_alerts(created_at DESC);

COMMENT ON TABLE strategy_failure_alerts IS '策略失效告警：回撤突破/校准漂移/胜率下降/盈亏比下降/相关性断裂/状态切换';
