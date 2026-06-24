-- =============================================
-- 统一执行引擎：扩展 positions/trades 表
-- 依据《系统评估与演进规划》第三阶段任务 3：
-- 统一模拟、回测和实盘执行模型
--
-- 目标：让模拟盘与回测/实盘使用统一的费用、滑点、保证金、盈亏模型
-- =============================================

-- ---------------------------------------------
-- 1. 扩展 positions 表：增加费用/滑点/保证金/已实现盈亏
-- ---------------------------------------------
-- 确保 quantity 和 opened_at 列存在（兼容旧 schema 中可能使用 size/created_at 的情况）
ALTER TABLE positions ADD COLUMN IF NOT EXISTS quantity DOUBLE PRECISION;
ALTER TABLE positions ADD COLUMN IF NOT EXISTS opened_at TIMESTAMP WITH TIME ZONE;
-- 如果旧 schema 使用 size，将数据迁移到 quantity
UPDATE positions SET quantity = size WHERE quantity IS NULL AND size IS NOT NULL;

ALTER TABLE positions ADD COLUMN IF NOT EXISTS filled_price DOUBLE PRECISION;
ALTER TABLE positions ADD COLUMN IF NOT EXISTS fee DOUBLE PRECISION NOT NULL DEFAULT 0.0;
ALTER TABLE positions ADD COLUMN IF NOT EXISTS slippage_bps DOUBLE PRECISION NOT NULL DEFAULT 0.0;
ALTER TABLE positions ADD COLUMN IF NOT EXISTS slippage_cost DOUBLE PRECISION NOT NULL DEFAULT 0.0;
ALTER TABLE positions ADD COLUMN IF NOT EXISTS margin DOUBLE PRECISION NOT NULL DEFAULT 0.0;
ALTER TABLE positions ADD COLUMN IF NOT EXISTS realized_pnl DOUBLE PRECISION NOT NULL DEFAULT 0.0;
ALTER TABLE positions ADD COLUMN IF NOT EXISTS notional DOUBLE PRECISION NOT NULL DEFAULT 0.0;
ALTER TABLE positions ADD COLUMN IF NOT EXISTS closed_at TIMESTAMP WITH TIME ZONE;
ALTER TABLE positions ADD COLUMN IF NOT EXISTS close_price DOUBLE PRECISION;
ALTER TABLE positions ADD COLUMN IF NOT EXISTS close_fee DOUBLE PRECISION NOT NULL DEFAULT 0.0;

-- ---------------------------------------------
-- 2. 扩展 trades 表：增加费用/滑点明细
-- ---------------------------------------------
ALTER TABLE trades ADD COLUMN IF NOT EXISTS entry_fee DOUBLE PRECISION NOT NULL DEFAULT 0.0;
ALTER TABLE trades ADD COLUMN IF NOT EXISTS exit_fee DOUBLE PRECISION NOT NULL DEFAULT 0.0;
ALTER TABLE trades ADD COLUMN IF NOT EXISTS slippage_bps DOUBLE PRECISION NOT NULL DEFAULT 0.0;
ALTER TABLE trades ADD COLUMN IF NOT EXISTS slippage_cost DOUBLE PRECISION NOT NULL DEFAULT 0.0;
ALTER TABLE trades ADD COLUMN IF NOT EXISTS gross_pnl DOUBLE PRECISION;
ALTER TABLE trades ADD COLUMN IF NOT EXISTS net_pnl DOUBLE PRECISION;
ALTER TABLE trades ADD COLUMN IF NOT EXISTS margin DOUBLE PRECISION;

-- ---------------------------------------------
-- 3. 扩展 paper_trading_accounts 表：增加费用统计
-- ---------------------------------------------
ALTER TABLE paper_trading_accounts ADD COLUMN IF NOT EXISTS total_fees DOUBLE PRECISION NOT NULL DEFAULT 0.0;
ALTER TABLE paper_trading_accounts ADD COLUMN IF NOT EXISTS total_slippage_cost DOUBLE PRECISION NOT NULL DEFAULT 0.0;
ALTER TABLE paper_trading_accounts ADD COLUMN IF NOT EXISTS margin_used DOUBLE PRECISION NOT NULL DEFAULT 0.0;
ALTER TABLE paper_trading_accounts ADD COLUMN IF NOT EXISTS total_equity DOUBLE PRECISION NOT NULL DEFAULT 100000.0;
ALTER TABLE paper_trading_accounts ADD COLUMN IF NOT EXISTS peak_equity DOUBLE PRECISION NOT NULL DEFAULT 100000.0;
ALTER TABLE paper_trading_accounts ADD COLUMN IF NOT EXISTS drawdown_pct DOUBLE PRECISION NOT NULL DEFAULT 0.0;

-- ---------------------------------------------
-- 4. 创建 paper_trading_fills 表：统一成交记录
--    与回测的 simulated_fills 表结构对齐
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS paper_trading_fills (
    fill_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    order_id UUID,
    symbol VARCHAR(50) NOT NULL,
    side VARCHAR(10) NOT NULL CHECK (side IN ('buy', 'sell')),
    quantity DOUBLE PRECISION NOT NULL,
    price DOUBLE PRECISION NOT NULL,
    notional DOUBLE PRECISION NOT NULL,
    fee DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    slippage_bps DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    slippage_cost DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    fee_rate_bps DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    is_maker BOOLEAN NOT NULL DEFAULT FALSE,
    fill_time TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    position_id UUID,
    close_reason VARCHAR(50),
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_paper_fills_user ON paper_trading_fills(user_id);
CREATE INDEX IF NOT EXISTS idx_paper_fills_symbol ON paper_trading_fills(symbol);
CREATE INDEX IF NOT EXISTS idx_paper_fills_time ON paper_trading_fills(fill_time DESC);
CREATE INDEX IF NOT EXISTS idx_paper_fills_position ON paper_trading_fills(position_id);
