-- =============================================
-- Quantitative Foundations (Phase 2)
-- 量化基础扩展：特征血缘、数据质量、概率决策卡、回测可信等级
--
-- 依据：
--   - docs/系统评估与演进规划.md 第六节"API 与数据设计补充"
--   - docs/deep-research-report.md "差距清单与优先级" P0 项
--
-- 本迁移补齐以下能力：
--   1. 特征血缘（feature_lineage）：每个特征值可追溯到数据源、计算版本、参数 hash
--   2. 数据质量快照（data_quality_snapshots）：freshness、缺口率、异常值率、覆盖率
--   3. 概率预测（signal_predictions）：p_up/p_down/p_flat、分位数、模型版本
--   4. 概率校准报告（signal_calibration_reports）：Brier、LogLoss、校准曲线、样本量
--   5. 决策卡快照（decision_cards）：用户可见的概率决策卡
--   6. 回测可信等级评估（backtest_trust_assessments）：展示/比较/晋级三级门禁
--   7. 风控决策日志（risk_decisions）：风控门禁结果与拒绝原因
--   8. 执行审计日志（execution_audit_logs）：订单执行全链路审计
--   9. 扩展 simulated_fills 增加 slippage_cost 列
--  10. 扩展 performance_reports 增加 alpha/beta/var_95/cvar_95 列
-- =============================================

-- ---------------------------------------------
-- 1. 扩展 simulated_fills：增加 slippage_cost 列
--    每笔成交保留滑点成本金额，进入绩效报告
-- ---------------------------------------------
ALTER TABLE simulated_fills
    ADD COLUMN IF NOT EXISTS slippage_cost DOUBLE PRECISION DEFAULT 0.0;

COMMENT ON COLUMN simulated_fills.slippage_cost IS '滑点成本金额（= notional * slippage_bps / 10000），用于绩效归因';

-- ---------------------------------------------
-- 2. 扩展 performance_reports：增加 alpha/beta/var_95/cvar_95 列
--    使关键风险指标可作为独立列查询，而非仅存在 report_json 中
-- ---------------------------------------------
ALTER TABLE performance_reports
    ADD COLUMN IF NOT EXISTS alpha DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS beta DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS var_95 DOUBLE PRECISION,
    ADD COLUMN IF NOT EXISTS cvar_95 DOUBLE PRECISION;

COMMENT ON COLUMN performance_reports.alpha IS 'Alpha（相对基准的超额收益年化）';
COMMENT ON COLUMN performance_reports.beta IS 'Beta（相对基准的系统性风险暴露）';
COMMENT ON COLUMN performance_reports.var_95 IS '95% 置信度下的在险价值';
COMMENT ON COLUMN performance_reports.cvar_95 IS '95% 置信度下的条件在险价值（尾部期望损失）';

-- ---------------------------------------------
-- 3. feature_lineage：特征血缘
--    每个特征值记录可追溯到数据源、源时间范围、计算版本、参数 hash
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS feature_lineage (
    lineage_id BIGSERIAL PRIMARY KEY,
    feature_id UUID NOT NULL REFERENCES feature_definitions(feature_id) ON DELETE CASCADE,
    symbol VARCHAR(50) NOT NULL,
    timestamp TIMESTAMP WITH TIME ZONE NOT NULL,

    -- 数据源信息
    data_source VARCHAR(100) NOT NULL,          -- 数据源标识（如 binance/okx/coinbase）
    source_time_start TIMESTAMP WITH TIME ZONE, -- 源数据起始时间
    source_time_end TIMESTAMP WITH TIME ZONE,   -- 源数据结束时间

    -- 计算版本信息
    calc_version VARCHAR(50) NOT NULL,         -- 计算代码版本
    parameters_hash VARCHAR(64) NOT NULL,      -- 参数 hash（SHA256，确保可复现）
    parameters JSONB NOT NULL DEFAULT '{}'::jsonb,

    -- 血缘关系
    upstream_feature_ids UUID[] DEFAULT '{}',   -- 上游特征 ID（依赖的其他特征）
    raw_data_refs JSONB,                        -- 原始数据引用（表名+时间范围）

    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE (feature_id, symbol, timestamp)
);

CREATE INDEX IF NOT EXISTS idx_feature_lineage_feature ON feature_lineage(feature_id);
CREATE INDEX IF NOT EXISTS idx_feature_lineage_symbol_time ON feature_lineage(symbol, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_feature_lineage_source ON feature_lineage(data_source);
CREATE INDEX IF NOT EXISTS idx_feature_lineage_hash ON feature_lineage(parameters_hash);

COMMENT ON TABLE feature_lineage IS '特征血缘：记录每个特征值的数据源、计算版本、参数 hash，确保可追溯可复现';

-- ---------------------------------------------
-- 4. data_quality_snapshots：数据质量快照
--    记录数据新鲜度、缺口率、异常值率、覆盖率
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS data_quality_snapshots (
    snapshot_id BIGSERIAL PRIMARY KEY,
    symbol VARCHAR(50) NOT NULL,
    data_source VARCHAR(100) NOT NULL,
    snapshot_time TIMESTAMP WITH TIME ZONE NOT NULL,

    -- 时间范围
    period_start TIMESTAMP WITH TIME ZONE NOT NULL,
    period_end TIMESTAMP WITH TIME ZONE NOT NULL,

    -- 质量指标
    freshness_sec DOUBLE PRECISION,            -- 最新数据距现在的秒数（越小越新鲜）
    gap_count INTEGER NOT NULL DEFAULT 0,       -- 缺口数量
    gap_ratio DOUBLE PRECISION NOT NULL DEFAULT 0.0, -- 缺口率（缺口数/应有点数）
    outlier_count INTEGER NOT NULL DEFAULT 0,  -- 异常值数量
    outlier_ratio DOUBLE PRECISION NOT NULL DEFAULT 0.0, -- 异常值率
    expected_points INTEGER NOT NULL DEFAULT 0, -- 应有的数据点数
    actual_points INTEGER NOT NULL DEFAULT 0,  -- 实际数据点数
    coverage_ratio DOUBLE PRECISION NOT NULL DEFAULT 0.0, -- 覆盖率（actual/expected）

    -- 回填状态
    backfill_status VARCHAR(20) NOT NULL DEFAULT 'pending', -- pending/in_progress/completed/failed
    last_backfill_time TIMESTAMP WITH TIME ZONE,

    -- 质量等级
    quality_grade VARCHAR(10) NOT NULL DEFAULT 'unknown', -- excellent/good/fair/poor/unknown
    metadata JSONB,

    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_data_quality_symbol_time ON data_quality_snapshots(symbol, snapshot_time DESC);
CREATE INDEX IF NOT EXISTS idx_data_quality_source ON data_quality_snapshots(data_source);
CREATE INDEX IF NOT EXISTS idx_data_quality_grade ON data_quality_snapshots(quality_grade);

COMMENT ON TABLE data_quality_snapshots IS '数据质量快照：freshness、缺口率、异常值率、覆盖率，用于数据门禁';

-- ---------------------------------------------
-- 5. signal_predictions：概率预测
--    模型输出的概率分布、分位数、模型版本
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS signal_predictions (
    prediction_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    symbol VARCHAR(50) NOT NULL,
    prediction_time TIMESTAMP WITH TIME ZONE NOT NULL,
    target_horizon_sec INTEGER NOT NULL,       -- 预测目标周期（秒）

    -- 概率分布
    p_up DOUBLE PRECISION NOT NULL,            -- 上涨概率
    p_down DOUBLE PRECISION NOT NULL,          -- 下跌概率
    p_flat DOUBLE PRECISION NOT NULL,          -- 震荡概率

    -- 收益分布分位数
    q10 DOUBLE PRECISION,                       -- 10 分位数（悲观情景）
    q50 DOUBLE PRECISION,                       -- 50 分位数（中位预期）
    q90 DOUBLE PRECISION,                       -- 90 分位数（乐观情景）

    -- 预期波动率与不确定性
    expected_volatility DOUBLE PRECISION,      -- 预期波动率
    mae_estimate DOUBLE PRECISION,             -- 平均绝对误差估计
    uncertainty DOUBLE PRECISION,             -- 不确定性度量

    -- 模型版本与特征
    model_version VARCHAR(50) NOT NULL,
    model_type VARCHAR(50) NOT NULL DEFAULT 'unknown', -- classifier/regressor/ensemble/llm
    feature_version VARCHAR(50),               -- 特征版本
    features_used JSONB,                       -- 使用的特征列表
    market_regime VARCHAR(50),                 -- 预测时的市场状态

    -- 实际结果（回填）
    realized_return DOUBLE PRECISION,         -- 实际收益率
    realized_direction VARCHAR(10),            -- 实际方向（up/down/flat）
    evaluated_at TIMESTAMP WITH TIME ZONE,

    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    CONSTRAINT chk_prob_sum CHECK (p_up >= 0.0 AND p_down >= 0.0 AND p_flat >= 0.0)
);

CREATE INDEX IF NOT EXISTS idx_signal_predictions_symbol_time ON signal_predictions(symbol, prediction_time DESC);
CREATE INDEX IF NOT EXISTS idx_signal_predictions_model ON signal_predictions(model_version);
CREATE INDEX IF NOT EXISTS idx_signal_predictions_regime ON signal_predictions(market_regime);
CREATE INDEX IF NOT EXISTS idx_signal_predictions_evaluated ON signal_predictions(evaluated_at) WHERE evaluated_at IS NOT NULL;

COMMENT ON TABLE signal_predictions IS '概率预测：模型输出的概率分布、分位数、模型版本，用于概率校准';

-- ---------------------------------------------
-- 6. signal_calibration_reports：概率校准报告
--    Brier Score、Log Loss、校准曲线、样本量
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS signal_calibration_reports (
    report_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    model_version VARCHAR(50) NOT NULL,
    symbol VARCHAR(50),                        -- NULL 表示全市场聚合
    market_regime VARCHAR(50),                 -- NULL 表示全状态聚合

    -- 评估时间范围
    eval_start TIMESTAMP WITH TIME ZONE NOT NULL,
    eval_end TIMESTAMP WITH TIME ZONE NOT NULL,

    -- 校准指标
    brier_score DOUBLE PRECISION NOT NULL,     -- Brier 分数（越小越好）
    log_loss DOUBLE PRECISION NOT NULL,        -- 对数损失（越小越好）
    accuracy DOUBLE PRECISION NOT NULL,        -- 方向准确率
    calibration_error DOUBLE PRECISION,       -- 校准误差（预测概率与实际频率的平均偏差）

    -- 校准曲线（分桶）
    calibration_curve JSONB NOT NULL,          -- [{predicted: 0.1, actual: 0.12, count: 50}, ...]

    -- 样本量
    sample_count INTEGER NOT NULL DEFAULT 0,
    up_count INTEGER NOT NULL DEFAULT 0,
    down_count INTEGER NOT NULL DEFAULT 0,
    flat_count INTEGER NOT NULL DEFAULT 0,

    -- 质量评估
    is_well_calibrated BOOLEAN DEFAULT FALSE,  -- 是否校准良好
    degradation_detected BOOLEAN DEFAULT FALSE, -- 是否检测到退化
    metadata JSONB,

    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_calibration_model ON signal_calibration_reports(model_version);
CREATE INDEX IF NOT EXISTS idx_calibration_regime ON signal_calibration_reports(market_regime);
CREATE INDEX IF NOT EXISTS idx_calibration_created ON signal_calibration_reports(created_at DESC);

COMMENT ON TABLE signal_calibration_reports IS '概率校准报告：Brier、LogLoss、校准曲线、样本量，用于策略停机门禁';

-- ---------------------------------------------
-- 7. decision_cards：决策卡快照
--    用户可见的概率决策卡，包含净期望、风险预算、数据血缘
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS decision_cards (
    card_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id BIGINT REFERENCES users(id) ON DELETE SET NULL,
    symbol VARCHAR(50) NOT NULL,
    generated_at TIMESTAMP WITH TIME ZONE NOT NULL,

    -- 建议动作
    suggested_action VARCHAR(20) NOT NULL,      -- open_long/open_short/close/hold/reduce
    target_horizon_sec INTEGER NOT NULL,        -- 预测周期

    -- 概率分布
    p_up DOUBLE PRECISION NOT NULL,
    p_down DOUBLE PRECISION NOT NULL,
    p_flat DOUBLE PRECISION NOT NULL,

    -- 收益区间
    q10 DOUBLE PRECISION,
    q50 DOUBLE PRECISION,
    q90 DOUBLE PRECISION,

    -- 净期望与风险
    expected_value DOUBLE PRECISION NOT NULL,   -- 净期望 EV（扣除费用/滑点/资金费率后）
    worst_case DOUBLE PRECISION,                -- 最坏情形（CVaR 口径）
    position_suggestion DOUBLE PRECISION,      -- 仓位建议（0-1）
    risk_budget_used DOUBLE PRECISION,         -- 已用风险预算

    -- 适用条件
    applicable_regime VARCHAR(50),             -- 适用市场状态
    data_freshness_sec DOUBLE PRECISION,        -- 数据新鲜度

    -- 证据链
    supporting_evidence JSONB,                  -- 支持证据
    opposing_evidence JSONB,                    -- 反对证据
    sample_performance JSONB,                   -- 样本表现
    data_lineage JSONB,                         -- 数据血缘

    -- 失效条件
    invalidation_conditions JSONB,             -- 失效条件列表

    -- 信任闭环字段（v1.8 新增）
    reasons TEXT[],                              -- 决策原因（来自 DecisionEngine）
    blockers TEXT[],                             -- 阻断原因（来自 DecisionEngine）
    trust_level VARCHAR(30),                     -- 回测可信等级（display_only/comparable/promotion_eligible）

    -- 模型版本
    model_version VARCHAR(50) NOT NULL,
    prediction_id UUID REFERENCES signal_predictions(prediction_id) ON DELETE SET NULL,

    -- 用户反馈
    user_action VARCHAR(20),                    -- 用户实际采取的动作
    user_feedback TEXT,
    acted_at TIMESTAMP WITH TIME ZONE,

    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_decision_cards_user ON decision_cards(user_id);
CREATE INDEX IF NOT EXISTS idx_decision_cards_symbol_time ON decision_cards(symbol, generated_at DESC);
CREATE INDEX IF NOT EXISTS idx_decision_cards_action ON decision_cards(suggested_action);

COMMENT ON TABLE decision_cards IS '决策卡快照：概率分布+EV+CVaR+失效条件+数据血缘的可审计对象';

-- ---------------------------------------------
-- 8. backtest_trust_assessments：回测可信等级评估
--    三级门禁：display_only / comparable / promotion_eligible
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS backtest_trust_assessments (
    assessment_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID NOT NULL REFERENCES backtest_jobs(job_id) ON DELETE CASCADE,

    -- 可信等级
    trust_level VARCHAR(30) NOT NULL DEFAULT 'display_only', -- display_only/comparable/promotion_eligible

    -- 评估维度
    test_coverage_passed BOOLEAN NOT NULL DEFAULT FALSE,      -- 撮合/绩效/报告权限测试是否通过
    capital_conservation_passed BOOLEAN NOT NULL DEFAULT FALSE, -- 资金守恒测试是否通过
    slippage_accounted BOOLEAN NOT NULL DEFAULT FALSE,        -- 滑点成本是否已入账
    data_quality_grade VARCHAR(10) NOT NULL DEFAULT 'unknown', -- 数据质量等级
    sample_size_sufficient BOOLEAN NOT NULL DEFAULT FALSE,    -- 样本量是否充足
    walk_forward_validated BOOLEAN NOT NULL DEFAULT FALSE,    -- 是否通过 Walk-forward 验证
    calibration_healthy BOOLEAN NOT NULL DEFAULT FALSE,       -- 概率校准是否健康

    -- 评估详情
    total_trades INTEGER NOT NULL DEFAULT 0,
    test_pass_rate DOUBLE PRECISION NOT NULL DEFAULT 0.0,     -- 测试通过率
    data_coverage_ratio DOUBLE PRECISION NOT NULL DEFAULT 0.0, -- 数据覆盖率
    issues JSONB NOT NULL DEFAULT '[]'::jsonb,                -- 发现的问题列表
    recommendations JSONB NOT NULL DEFAULT '[]'::jsonb,       -- 改进建议

    -- 是否允许晋级实盘
    promotion_eligible BOOLEAN NOT NULL DEFAULT FALSE,
    promotion_blockers JSONB NOT NULL DEFAULT '[]'::jsonb,    -- 晋级阻断项

    assessed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    UNIQUE (job_id)
);

CREATE INDEX IF NOT EXISTS idx_trust_assessments_job ON backtest_trust_assessments(job_id);
CREATE INDEX IF NOT EXISTS idx_trust_assessments_level ON backtest_trust_assessments(trust_level);

COMMENT ON TABLE backtest_trust_assessments IS '回测可信等级评估：展示/比较/晋级三级门禁，未达标禁止上线';

-- ---------------------------------------------
-- 9. risk_decisions：风控决策日志
--    风控门禁结果与拒绝原因
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS risk_decisions (
    decision_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID REFERENCES backtest_jobs(job_id) ON DELETE CASCADE,
    user_id BIGINT REFERENCES users(id) ON DELETE SET NULL,
    decision_time TIMESTAMP WITH TIME ZONE NOT NULL,

    -- 决策上下文
    asset VARCHAR(50) NOT NULL,
    intent_id UUID REFERENCES trade_intents(intent_id) ON DELETE SET NULL,
    order_id UUID REFERENCES simulated_orders(order_id) ON DELETE SET NULL,

    -- 决策结果
    decision VARCHAR(20) NOT NULL,              -- approve/reduce/reject/circuit_break
    reasons JSONB NOT NULL DEFAULT '[]'::jsonb, -- 拒绝原因列表

    -- 风控维度
    checks JSONB NOT NULL DEFAULT '{}'::jsonb,  -- 各项检查结果
    pre_trade_exposure DOUBLE PRECISION,        -- 交易前敞口
    post_trade_exposure DOUBLE PRECISION,       -- 交易后敞口
    risk_budget_used DOUBLE PRECISION,          -- 风险预算使用率
    cvar_95 DOUBLE PRECISION,                   -- CVaR 95%

    -- 触发的门禁
    gates_triggered JSONB NOT NULL DEFAULT '[]'::jsonb, -- 触发的门禁列表

    metadata JSONB,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_risk_decisions_job ON risk_decisions(job_id);
CREATE INDEX IF NOT EXISTS idx_risk_decisions_asset_time ON risk_decisions(asset, decision_time DESC);
CREATE INDEX IF NOT EXISTS idx_risk_decisions_decision ON risk_decisions(decision);

COMMENT ON TABLE risk_decisions IS '风控决策日志：门禁结果与拒绝原因，用于审计与归因';

-- ---------------------------------------------
-- 10. execution_audit_logs：执行审计日志
--     订单执行全链路审计：谁、何时、基于什么证据、通过什么规则
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS execution_audit_logs (
    log_id BIGSERIAL PRIMARY KEY,
    job_id UUID REFERENCES backtest_jobs(job_id) ON DELETE CASCADE,
    user_id BIGINT REFERENCES users(id) ON DELETE SET NULL,
    order_id UUID REFERENCES simulated_orders(order_id) ON DELETE SET NULL,
    fill_id UUID REFERENCES simulated_fills(fill_id) ON DELETE SET NULL,

    -- 审计事件
    event_time TIMESTAMP WITH TIME ZONE NOT NULL,
    event_type VARCHAR(50) NOT NULL,            -- order_created/risk_checked/submitted/filled/cancelled/rejected

    -- 证据链
    signal_id UUID REFERENCES alpha_signals(signal_id) ON DELETE SET NULL,
    decision_card_id UUID REFERENCES decision_cards(card_id) ON DELETE SET NULL,
    risk_decision_id UUID REFERENCES risk_decisions(decision_id) ON DELETE SET NULL,

    -- 版本信息（确保可复现）
    data_version VARCHAR(50),                   -- 数据版本
    feature_version VARCHAR(50),                -- 特征版本
    model_version VARCHAR(50),                  -- 模型版本
    rule_version VARCHAR(50),                   -- 规则版本
    evidence_time TIMESTAMP WITH TIME ZONE,     -- 证据时间

    -- 审计详情
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_execution_audit_job ON execution_audit_logs(job_id);
CREATE INDEX IF NOT EXISTS idx_execution_audit_order ON execution_audit_logs(order_id);
CREATE INDEX IF NOT EXISTS idx_execution_audit_event_time ON execution_audit_logs(event_time DESC);
CREATE INDEX IF NOT EXISTS idx_execution_audit_type ON execution_audit_logs(event_type);

COMMENT ON TABLE execution_audit_logs IS '执行审计日志：订单执行全链路审计，回答谁/何时/基于什么证据/通过什么规则';

-- ============================================================
-- v1.8 补充：decision_cards 表新增信任闭环字段
-- 用于已部署的数据库（CREATE TABLE IF NOT EXISTS 不会添加新列）
-- ============================================================
ALTER TABLE decision_cards ADD COLUMN IF NOT EXISTS reasons TEXT[];
ALTER TABLE decision_cards ADD COLUMN IF NOT EXISTS blockers TEXT[];
ALTER TABLE decision_cards ADD COLUMN IF NOT EXISTS trust_level VARCHAR(30);
