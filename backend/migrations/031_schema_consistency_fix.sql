-- 031 Schema 一致性修复
-- 修复运行期必崩的 schema/代码不一致(评审 C 阶段发现)
-- 设计原则:纯增量 ALTER + 幂等(IF NOT EXISTS / DO $$ EXCEPTION),不改既有列、不改 Rust 代码
--
-- 背景:
--   1) decision_memory:009 建表(含 success),027 用 IF NOT EXISTS 重复定义(含 was_correct/agent_id)
--      被跳过;但 agents/memory.rs、agents/evolution.rs 依赖 027 的 was_correct/agent_id → 运行时报"列不存在"。
--   2) ai_prediction_trades:028 是唯一来源,但缺 holding_period/ai_provider/model_name 列,
--      且从未创建 ai_prediction_status_enum / strategy_direction_enum;
--      routes/ai_predictions.rs:48 依赖这些列与类型 → 运行时报"类型/列不存在"。
--
-- 字段语义说明(decision_memory 两套字段共存,各表其所):
--   - success BOOLEAN        :交易盈亏结果(simulation.rs / ai_analysis.rs 使用),来自 009
--   - was_correct BOOLEAN    :决策方向对错(memory.rs / evolution.rs 反思使用),本迁移新增
--   两者语义不同:一笔方向判断正确(was_correct=true)的交易,可能因止损/资金费率而亏损(success=false)。

-- ============================================================
-- A. decision_memory 补列(让 027 语义的字段共存于 009 的表)
-- ============================================================

ALTER TABLE decision_memory ADD COLUMN IF NOT EXISTS agent_id VARCHAR(100);
ALTER TABLE decision_memory ADD COLUMN IF NOT EXISTS decision_type VARCHAR(50);
ALTER TABLE decision_memory ADD COLUMN IF NOT EXISTS was_correct BOOLEAN;
ALTER TABLE decision_memory ADD COLUMN IF NOT EXISTS outcome DOUBLE PRECISION;
ALTER TABLE decision_memory ADD COLUMN IF NOT EXISTS context JSONB;

-- pnl_percent / market_trend / volatility / volume_profile:evolution.rs:764-765 依赖
-- 010_agent_accuracy_enhancement.sql 可能已补部分,此处幂等补齐(无则加,有则跳过)
ALTER TABLE decision_memory ADD COLUMN IF NOT EXISTS pnl_percent DOUBLE PRECISION;
ALTER TABLE decision_memory ADD COLUMN IF NOT EXISTS market_trend VARCHAR(20);
ALTER TABLE decision_memory ADD COLUMN IF NOT EXISTS volatility DOUBLE PRECISION;

-- 为新增列建索引(反思查询均按 agent_id 分组 + created_at 时间窗)
CREATE INDEX IF NOT EXISTS idx_decision_memory_agent_id ON decision_memory(agent_id);
CREATE INDEX IF NOT EXISTS idx_decision_memory_was_correct ON decision_memory(was_correct);
CREATE INDEX IF NOT EXISTS idx_decision_memory_created_at_desc ON decision_memory(created_at DESC);

-- ============================================================
-- B. ai_prediction_trades 补列
-- ============================================================

ALTER TABLE ai_prediction_trades ADD COLUMN IF NOT EXISTS holding_period VARCHAR(50);
ALTER TABLE ai_prediction_trades ADD COLUMN IF NOT EXISTS ai_provider VARCHAR(100);
ALTER TABLE ai_prediction_trades ADD COLUMN IF NOT EXISTS model_name VARCHAR(100);

-- ============================================================
-- C. 补缺失的枚举类型
--    routes/ai_predictions.rs:48 依赖 strategy_direction_enum / ai_prediction_status_enum
--    全仓库此前均无 CREATE TYPE 定义(仅 028 建了 ai_prediction_result_enum)
-- ============================================================

-- 策略方向枚举(direction 列)
DO $$ BEGIN
    CREATE TYPE strategy_direction_enum AS ENUM ('long', 'short', 'hold');
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

-- 预测状态枚举(status 列)
-- 取值来源(grep 核对):
--   - 创建:routes/ai_predictions.rs:48,135  -> 'pending', 'cancelled'
--   - 验证:routes/validation.rs:61          -> 'TAKE_PROFIT_HIT', 'STOP_LOSS_HIT'
DO $$ BEGIN
    CREATE TYPE ai_prediction_status_enum AS ENUM (
        'pending',
        'active',
        'cancelled',
        'expired',
        'closed',
        'TAKE_PROFIT_HIT',
        'STOP_LOSS_HIT'
    );
EXCEPTION
    WHEN duplicate_object THEN NULL;
END $$;

-- ============================================================
-- D. 将 ai_prediction_trades.status / direction 从 VARCHAR 提升为 enum
--    代码按 enum 类型使用(INSERT 时 'pending'::ai_prediction_status_enum),
--    若保持 VARCHAR 会导致 'pending'::ai_prediction_status_enum 因类型不匹配报错。
--    USING 子句做隐式转换;DO $$ EXCEPTION 兜底历史脏数据/重复执行。
-- ============================================================

DO $$ BEGIN
    ALTER TABLE ai_prediction_trades
        ALTER COLUMN direction DROP DEFAULT,
        ALTER COLUMN direction TYPE strategy_direction_enum
        USING direction::strategy_direction_enum;
EXCEPTION
    WHEN OTHERS THEN
        RAISE NOTICE '跳过 direction 转换: %', SQLERRM;
END $$;

DO $$ BEGIN
    ALTER TABLE ai_prediction_trades
        ALTER COLUMN status DROP DEFAULT,
        ALTER COLUMN status TYPE ai_prediction_status_enum
        USING status::ai_prediction_status_enum;
EXCEPTION
    WHEN OTHERS THEN
        RAISE NOTICE '跳过 status 转换: %', SQLERRM;
END $$;

-- 注意:result 列保持 ai_prediction_result_enum(028 已正确定义,无需改动)
-- 注意:routes/validation.rs:60 的 'WIN'/'LOSS' 大小写与 result_enum('pending','win','loss')不一致,
--       属代码层 bug,不在本 schema 修复范围内(避免 C 阶段范围蔓延),留待后续代码修复。
