-- 026_model_cards_and_counterfactuals.sql
-- 模型卡 + 反事实解释
-- 依据《系统评估与演进规划》第四阶段任务3：模型卡、校准曲线、反事实解释

-- ============================================================================
-- 1. 模型卡表（model_cards）
-- ============================================================================
-- 模型发布治理的核心产物，聚合校准报告、信任评估、预测统计
-- 参考 Google ModelCard 规范 + 系统现有 decision_cards.invalidation_conditions 设计

CREATE TABLE IF NOT EXISTS model_cards (
    card_id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    model_version        VARCHAR(50) NOT NULL UNIQUE,
    model_type           VARCHAR(50) NOT NULL,              -- classifier/regressor/ensemble/llm
    model_name           VARCHAR(200) NOT NULL,

    -- 模型描述
    description          TEXT,
    intended_use         TEXT,                              -- 预期用途
    out_of_scope         TEXT,                              -- 不适用场景
    training_data_summary JSONB,                            -- 训练数据摘要（来源、时间范围、样本量）
    feature_version      VARCHAR(50),
    features_used        JSONB,                             -- 特征列表

    -- 质量证据（引用已有表）
    calibration_report_id UUID REFERENCES signal_calibration_reports(report_id) ON DELETE SET NULL,
    trust_assessment_id   UUID,                             -- 软引用 backtest_trust_assessments.assessment_id
    brier_score          DOUBLE PRECISION,
    log_loss             DOUBLE PRECISION,
    accuracy             DOUBLE PRECISION,
    calibration_curve   JSONB,

    -- 失效条件与风险
    invalidation_conditions JSONB NOT NULL DEFAULT '[]'::jsonb,
    known_limitations    JSONB NOT NULL DEFAULT '[]'::jsonb,
    ethical_considerations TEXT,

    -- 版本与发布治理
    status               VARCHAR(20) NOT NULL DEFAULT 'draft',  -- draft/shadow/active/deprecated/rolled_back
    shadow_period_start  TIMESTAMP WITH TIME ZONE,
    shadow_period_end    TIMESTAMP WITH TIME ZONE,
    promotion_eligible   BOOLEAN NOT NULL DEFAULT FALSE,
    previous_version     VARCHAR(50),                       -- 回滚版本指向

    -- 审计
    created_by           BIGINT REFERENCES users(id) ON DELETE SET NULL,
    approved_by          BIGINT REFERENCES users(id) ON DELETE SET NULL,
    approved_at          TIMESTAMP WITH TIME ZONE,

    created_at           TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at           TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_model_cards_status ON model_cards(status);
CREATE INDEX idx_model_cards_model_type ON model_cards(model_type);

-- 自动更新 updated_at
CREATE OR REPLACE FUNCTION update_model_cards_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_model_cards_updated_at
    BEFORE UPDATE ON model_cards
    FOR EACH ROW
    EXECUTE FUNCTION update_model_cards_updated_at();

-- ============================================================================
-- 2. 反事实解释表（counterfactual_explanations）
-- ============================================================================
-- 交易后归因的增强：回答"若不做/早退/反向/减仓会怎样"
-- 关联 trade_attributions，每笔交易可生成多个反事实场景

CREATE TABLE IF NOT EXISTS counterfactual_explanations (
    explanation_id    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    attribution_id    UUID REFERENCES trade_attributions(attribution_id) ON DELETE CASCADE,
    decision_card_id  UUID REFERENCES decision_cards(card_id) ON DELETE SET NULL,
    job_id            UUID REFERENCES backtest_jobs(job_id) ON DELETE CASCADE,
    user_id           BIGINT REFERENCES users(id) ON DELETE SET NULL,
    symbol            VARCHAR(50) NOT NULL,

    -- 反事实场景
    scenario_type     VARCHAR(30) NOT NULL,                 -- no_trade/earlier_exit/later_exit/opposite_direction/reduced_size
    scenario_description TEXT,

    -- 反事实结果
    counterfactual_pnl  DOUBLE PRECISION,                  -- 若执行该场景的盈亏
    actual_pnl          DOUBLE PRECISION,                  -- 实际盈亏
    pnl_delta           DOUBLE PRECISION,                  -- 差值（反事实 - 实际）
    counterfactual_return DOUBLE PRECISION,

    -- 解释内容
    explanation       TEXT NOT NULL,                       -- 自然语言解释
    key_drivers       JSONB NOT NULL DEFAULT '[]'::jsonb,  -- 关键驱动因素
    what_if_inputs    JSONB,                              -- 假设输入参数
    confidence        DOUBLE PRECISION,                   -- 解释置信度

    -- 证据引用
    evidence          JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at        TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_counterfactual_attribution ON counterfactual_explanations(attribution_id);
CREATE INDEX idx_counterfactual_job ON counterfactual_explanations(job_id);
CREATE INDEX idx_counterfactual_scenario ON counterfactual_explanations(scenario_type);

-- ============================================================================
-- 3. 视图：模型卡摘要（方便前端展示）
-- ============================================================================
CREATE OR REPLACE VIEW model_card_summary AS
SELECT
    mc.card_id,
    mc.model_version,
    mc.model_type,
    mc.model_name,
    mc.status,
    mc.promotion_eligible,
    mc.brier_score,
    mc.log_loss,
    mc.accuracy,
    mc.calibration_report_id,
    mc.trust_assessment_id,
    mc.shadow_period_start,
    mc.shadow_period_end,
    mc.previous_version,
    mc.created_at,
    mc.updated_at,
    -- 统计该模型版本的预测数量
    (SELECT COUNT(*) FROM signal_predictions sp WHERE sp.model_version = mc.model_version) AS prediction_count,
    -- 统计该模型版本的决策卡数量
    (SELECT COUNT(*) FROM decision_cards dc WHERE dc.model_version = mc.model_version) AS decision_card_count
FROM model_cards mc;
