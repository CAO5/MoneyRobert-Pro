-- =============================================
-- Microstructure Data (Phase 2 Task 2)
-- 微结构数据：订单簿快照、逐笔成交、清算数据
--
-- 依据：系统评估与演进规划 第二阶段任务2
--   "增加订单簿、成交、清算、基差和跨交易所数据"
--
-- 本迁移新增：
--   1. orderbook_snapshots：订单簿快照（买卖盘深度）
--   2. trade_ticks：逐笔成交（含主动买卖方向）
--   3. liquidation_events：清算/强平事件
--   4. basis_data：基差数据（现货-永续-交割）
-- =============================================

-- ---------------------------------------------
-- 1. orderbook_snapshots：订单簿快照
--    定期采样的买卖盘深度数据
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS orderbook_snapshots (
    snapshot_id BIGSERIAL PRIMARY KEY,
    symbol VARCHAR(50) NOT NULL,
    exchange VARCHAR(50) NOT NULL DEFAULT 'okx',
    timestamp TIMESTAMP WITH TIME ZONE NOT NULL,

    -- 最优买卖价
    best_bid DOUBLE PRECISION NOT NULL,
    best_ask DOUBLE PRECISION NOT NULL,
    spread DOUBLE PRECISION NOT NULL,               -- 价差 = best_ask - best_bid
    spread_bps DOUBLE PRECISION NOT NULL,           -- 价差（基点）

    -- 深度（指定档位的累计量）
    bid_depth_5 DOUBLE PRECISION,                   -- 买盘前 5 档累计量
    ask_depth_5 DOUBLE PRECISION,                   -- 卖盘前 5 档累计量
    bid_depth_10 DOUBLE PRECISION,
    ask_depth_10 DOUBLE PRECISION,
    bid_depth_20 DOUBLE PRECISION,
    ask_depth_20 DOUBLE PRECISION,

    -- 深度不平衡（(bid_depth - ask_depth) / (bid_depth + ask_depth)）
    depth_imbalance_5 DOUBLE PRECISION,
    depth_imbalance_10 DOUBLE PRECISION,
    depth_imbalance_20 DOUBLE PRECISION,

    -- 中价
    mid_price DOUBLE PRECISION NOT NULL,

    -- 完整深度（JSONB）
    bids JSONB,                                     -- [[price, size], ...]
    asks JSONB,

    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_orderbook_symbol_time ON orderbook_snapshots(symbol, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_orderbook_exchange ON orderbook_snapshots(exchange);
CREATE INDEX IF NOT EXISTS idx_orderbook_spread ON orderbook_snapshots(spread_bps);

COMMENT ON TABLE orderbook_snapshots IS '订单簿快照：买卖盘深度、价差、深度不平衡，用于微观结构分析';

-- ---------------------------------------------
-- 2. trade_ticks：逐笔成交
--    含主动买卖方向（taker side）
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS trade_ticks (
    tick_id BIGSERIAL PRIMARY KEY,
    symbol VARCHAR(50) NOT NULL,
    exchange VARCHAR(50) NOT NULL DEFAULT 'okx',
    trade_id VARCHAR(100),                          -- 交易所成交 ID
    timestamp TIMESTAMP WITH TIME ZONE NOT NULL,

    price DOUBLE PRECISION NOT NULL,
    size DOUBLE PRECISION NOT NULL,
    notional DOUBLE PRECISION NOT NULL,             -- 成交额 = price * size

    -- 主动方向
    side VARCHAR(10) NOT NULL,                      -- buy（主动买）/ sell（主动卖）
    is_buyer_maker BOOLEAN NOT NULL DEFAULT FALSE,

    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_trade_ticks_symbol_time ON trade_ticks(symbol, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_trade_ticks_side ON trade_ticks(side);
CREATE INDEX IF NOT EXISTS idx_trade_ticks_exchange ON trade_ticks(exchange);

COMMENT ON TABLE trade_ticks IS '逐笔成交：含主动买卖方向，用于 CVD 和微观结构分析';

-- ---------------------------------------------
-- 3. liquidation_events：清算/强平事件
--    记录强制平仓事件
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS liquidation_events (
    event_id BIGSERIAL PRIMARY KEY,
    symbol VARCHAR(50) NOT NULL,
    exchange VARCHAR(50) NOT NULL DEFAULT 'okx',
    timestamp TIMESTAMP WITH TIME ZONE NOT NULL,

    -- 清算详情
    side VARCHAR(10) NOT NULL,                      -- long（多仓被清算）/ short（空仓被清算）
    price DOUBLE PRECISION NOT NULL,                -- 清算价格
    size DOUBLE PRECISION NOT NULL,                 -- 清算数量
    notional DOUBLE PRECISION NOT NULL,             -- 清算金额

    -- 清算类型
    liquidation_type VARCHAR(20),                   -- forced/adel（减少仓位）

    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_liquidation_symbol_time ON liquidation_events(symbol, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_liquidation_side ON liquidation_events(side);
CREATE INDEX IF NOT EXISTS idx_liquidation_exchange ON liquidation_events(exchange);

COMMENT ON TABLE liquidation_events IS '清算事件：强制平仓记录，用于清算热力图和反身性分析';

-- ---------------------------------------------
-- 4. basis_data：基差数据
--    现货-永续-交割合约基差
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS basis_data (
    basis_id BIGSERIAL PRIMARY KEY,
    symbol VARCHAR(50) NOT NULL,
    exchange VARCHAR(50) NOT NULL DEFAULT 'okx',
    timestamp TIMESTAMP WITH TIME ZONE NOT NULL,

    -- 价格
    spot_price DOUBLE PRECISION,                    -- 现货价格
    perp_price DOUBLE PRECISION,                    -- 永续合约价格
    futures_price DOUBLE PRECISION,                  -- 交割合约价格
    futures_expiry TIMESTAMP WITH TIME ZONE,        -- 交割日期

    -- 基差
    perp_basis DOUBLE PRECISION,                    -- 永续基差 = perp_price - spot_price
    perp_basis_pct DOUBLE PRECISION,                -- 永续基差百分比
    futures_basis DOUBLE PRECISION,                 -- 交割基差 = futures_price - spot_price
    futures_basis_pct DOUBLE PRECISION,             -- 交割基差百分比

    -- 资金费率
    funding_rate DOUBLE PRECISION,                  -- 当前资金费率
    funding_rate_annualized DOUBLE PRECISION,       -- 年化资金费率

    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_basis_symbol_time ON basis_data(symbol, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_basis_exchange ON basis_data(exchange);

COMMENT ON TABLE basis_data IS '基差数据：现货-永续-交割合约价差，用于期限结构分析';
