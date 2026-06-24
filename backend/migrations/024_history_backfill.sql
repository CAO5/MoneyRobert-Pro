-- =============================================
-- Historical Data Backfill & Table Unification
-- 历史数据回填 + 表名统一
-- =============================================
-- 依据《系统评估与演进规划》第二阶段任务 1：
-- 建立长期历史数据和特征仓库
--
-- 问题：
-- 1. collector 写入 klines 表，但回测引擎/AI分析/Dashboard 读取 market_data 表
-- 2. 两表结构几乎相同但数据不互通，导致回测引擎可能加载空数据
-- 3. 无历史数据回填机制
--
-- 解决方案：
-- 1. 将 market_data 表中的数据迁移到 klines 表
-- 2. 删除 market_data 表
-- 3. 创建 market_data 视图，从 klines 表查询（统一读写入口）
-- 4. 创建 backfill_jobs 表跟踪回填任务

-- ---------------------------------------------
-- 1. 迁移 market_data 数据到 klines（如果存在）
-- ---------------------------------------------
INSERT INTO klines (symbol, "interval", open_time, open, high, low, close, volume, quote_volume, trades_count, is_closed, created_at)
SELECT
    symbol,
    "interval",
    open_time,
    open,
    high,
    low,
    close,
    volume,
    COALESCE(quote_volume, 0),
    COALESCE(trades_count, 0),
    true,
    created_at
FROM market_data
ON CONFLICT (symbol, "interval", open_time) DO NOTHING;

-- ---------------------------------------------
-- 2. 删除旧的 market_data 表
-- ---------------------------------------------
DROP TABLE IF EXISTS market_data;

-- ---------------------------------------------
-- 3. 创建 market_data 视图（从 klines 查询）
-- ---------------------------------------------
-- 回测引擎、AI 分析、Dashboard 等模块查询 market_data 时，
-- 实际读取的是 klines 表的数据，确保数据一致性。
CREATE OR REPLACE VIEW market_data AS
SELECT
    id,
    symbol,
    "interval" AS interval,
    open_time,
    open,
    high,
    low,
    close,
    volume,
    quote_volume,
    trades_count,
    is_closed,
    created_at,
    updated_at
FROM klines;

-- 为视图创建注释
COMMENT ON VIEW market_data IS 'Unified market data view backed by klines table. Written by collector + backfiller.';

-- ---------------------------------------------
-- 4. 创建回填任务跟踪表
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS backfill_jobs (
    job_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    symbol VARCHAR(50) NOT NULL,
    bar VARCHAR(10) NOT NULL,
    from_time TIMESTAMP WITH TIME ZONE NOT NULL,
    to_time TIMESTAMP WITH TIME ZONE NOT NULL,

    -- 进度跟踪
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    -- pending / running / completed / failed / partial
    fetched_count INTEGER NOT NULL DEFAULT 0,
    inserted_count INTEGER NOT NULL DEFAULT 0,
    updated_count INTEGER NOT NULL DEFAULT 0,
    skipped_count INTEGER NOT NULL DEFAULT 0,
    gaps_detected INTEGER NOT NULL DEFAULT 0,
    gaps_filled INTEGER NOT NULL DEFAULT 0,

    -- 错误信息
    error_message TEXT,
    errors JSONB DEFAULT '[]'::jsonb,

    -- 时间统计
    elapsed_secs DOUBLE PRECISION,
    started_at TIMESTAMP WITH TIME ZONE,
    completed_at TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_backfill_jobs_symbol ON backfill_jobs(symbol);
CREATE INDEX IF NOT EXISTS idx_backfill_jobs_status ON backfill_jobs(status);
CREATE INDEX IF NOT EXISTS idx_backfill_jobs_bar ON backfill_jobs(bar);
CREATE INDEX IF NOT EXISTS idx_backfill_jobs_created ON backfill_jobs(created_at DESC);

-- ---------------------------------------------
-- 5. 更新 data_quality_snapshots 表的 backfill_status
-- ---------------------------------------------
-- 添加触发器：当 backfill 完成后，自动更新对应的数据质量快照
CREATE OR REPLACE FUNCTION update_backfill_status()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.status = 'completed' OR NEW.status = 'partial' THEN
        UPDATE data_quality_snapshots
        SET backfill_status = 'completed',
            last_backfill_time = NOW()
        WHERE symbol = NEW.symbol
          AND data_source LIKE '%' || NEW.bar || '%';
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trigger_backfill_status ON backfill_jobs;
CREATE TRIGGER trigger_backfill_status
    AFTER UPDATE OF status ON backfill_jobs
    FOR EACH ROW
    EXECUTE FUNCTION update_backfill_status();
