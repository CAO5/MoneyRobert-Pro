-- =============================================
-- Calibration Models (Phase 2+)
-- 概率校准模型持久化：Platt/Isotonic/Linear 参数存储
--
-- 依据：系统评估与演进规划 第二阶段任务3
--   "建立市场状态模型及概率校准"
--
-- 本迁移新增：
--   1. calibration_models：校准模型参数表，存储拟合后的校准模型
--   2. 扩展 signal_calibration_reports 增加 model_id 关联
-- =============================================

-- ---------------------------------------------
-- 1. calibration_models：校准模型参数
--    存储 Platt/Isotonic/Linear 校准模型参数
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS calibration_models (
    model_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    model_name VARCHAR(100) NOT NULL,
    model_type VARCHAR(20) NOT NULL,          -- platt / isotonic / linear

    -- 关联维度
    symbol VARCHAR(50),                        -- NULL 表示全市场
    market_regime VARCHAR(50),                 -- NULL 表示全状态
    target_horizon_sec INTEGER,                -- 预测周期（秒）
    source_model_version VARCHAR(50),          -- 来源模型版本

    -- Platt 模型参数
    platt_a DOUBLE PRECISION,
    platt_b DOUBLE PRECISION,

    -- Linear 模型参数
    linear_factor DOUBLE PRECISION,
    linear_bias DOUBLE PRECISION,

    -- Isotonic 模型参数（映射点数组）
    isotonic_points JSONB,                     -- [{p: 0.1, calibrated: 0.08}, ...]

    -- 训练信息
    training_start TIMESTAMP WITH TIME ZONE,
    training_end TIMESTAMP WITH TIME ZONE,
    sample_count INTEGER NOT NULL DEFAULT 0,
    train_brier_score DOUBLE PRECISION,
    train_calibration_error DOUBLE PRECISION,

    -- 状态
    status VARCHAR(20) NOT NULL DEFAULT 'active', -- active / deprecated / invalid
    is_default BOOLEAN NOT NULL DEFAULT FALSE,    -- 是否为该维度的默认模型

    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_calibration_models_type ON calibration_models(model_type);
CREATE INDEX IF NOT EXISTS idx_calibration_models_symbol ON calibration_models(symbol);
CREATE INDEX IF NOT EXISTS idx_calibration_models_regime ON calibration_models(market_regime);
CREATE INDEX IF NOT EXISTS idx_calibration_models_status ON calibration_models(status);
CREATE INDEX IF NOT EXISTS idx_calibration_models_default ON calibration_models(is_default) WHERE is_default = TRUE;

COMMENT ON TABLE calibration_models IS '概率校准模型：存储 Platt/Isotonic/Linear 校准模型参数，用于推理时校准置信度';

-- ---------------------------------------------
-- 2. 扩展 signal_calibration_reports
--    增加 model_id 关联到具体校准模型
-- ---------------------------------------------
ALTER TABLE signal_calibration_reports
    ADD COLUMN IF NOT EXISTS model_id UUID REFERENCES calibration_models(model_id) ON DELETE SET NULL;

COMMENT ON COLUMN signal_calibration_reports.model_id IS '关联的校准模型 ID';
