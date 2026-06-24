-- =============================================
-- Cross-Exchange Data Storage
-- 跨交易所数据存储
-- =============================================
-- 依据《系统评估与演进规划》第二阶段任务 2：
-- 增加订单簿、成交、清算、基差和跨交易所数据

-- ---------------------------------------------
-- 1. cross_exchange_prices: 跨交易所价格快照
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS cross_exchange_prices (
    id BIGSERIAL PRIMARY KEY,
    symbol VARCHAR(50) NOT NULL,
    exchange VARCHAR(20) NOT NULL,
    last_price DOUBLE PRECISION NOT NULL,
    bid_price DOUBLE PRECISION NOT NULL,
    ask_price DOUBLE PRECISION NOT NULL,
    volume_24h DOUBLE PRECISION DEFAULT 0,
    quote_volume_24h DOUBLE PRECISION DEFAULT 0,
    high_24h DOUBLE PRECISION DEFAULT 0,
    low_24h DOUBLE PRECISION DEFAULT 0,
    timestamp TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_cross_exchange_symbol ON cross_exchange_prices(symbol);
CREATE INDEX IF NOT EXISTS idx_cross_exchange_exchange ON cross_exchange_prices(exchange);
CREATE INDEX IF NOT EXISTS idx_cross_exchange_timestamp ON cross_exchange_prices(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_cross_exchange_symbol_time ON cross_exchange_prices(symbol, timestamp DESC);

-- ---------------------------------------------
-- 2. cross_exchange_spreads: 跨交易所价差记录
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS cross_exchange_spreads (
    id BIGSERIAL PRIMARY KEY,
    symbol VARCHAR(50) NOT NULL,
    exchange_a VARCHAR(20) NOT NULL,
    exchange_b VARCHAR(20) NOT NULL,
    price_a DOUBLE PRECISION NOT NULL,
    price_b DOUBLE PRECISION NOT NULL,
    spread DOUBLE PRECISION NOT NULL,
    spread_pct DOUBLE PRECISION NOT NULL,
    best_bid_exchange VARCHAR(20),
    best_ask_exchange VARCHAR(20),
    timestamp TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_cross_spread_symbol ON cross_exchange_spreads(symbol);
CREATE INDEX IF NOT EXISTS idx_cross_spread_timestamp ON cross_exchange_spreads(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_cross_spread_pair ON cross_exchange_spreads(symbol, timestamp DESC);

-- ---------------------------------------------
-- 3. exchange_klines: 多交易所 K 线存储
-- ---------------------------------------------
-- 用于存储来自不同交易所的 K 线数据，支持跨交易所技术分析
CREATE TABLE IF NOT EXISTS exchange_klines (
    id BIGSERIAL PRIMARY KEY,
    symbol VARCHAR(50) NOT NULL,
    exchange VARCHAR(20) NOT NULL,
    "interval" VARCHAR(10) NOT NULL,
    open_time TIMESTAMP NOT NULL,
    open DOUBLE PRECISION NOT NULL,
    high DOUBLE PRECISION NOT NULL,
    low DOUBLE PRECISION NOT NULL,
    close DOUBLE PRECISION NOT NULL,
    volume DOUBLE PRECISION NOT NULL DEFAULT 0,
    quote_volume DOUBLE PRECISION DEFAULT 0,
    is_closed BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE(symbol, exchange, "interval", open_time)
);

CREATE INDEX IF NOT EXISTS idx_exchange_klines_lookup ON exchange_klines(symbol, exchange, "interval", open_time DESC);
CREATE INDEX IF NOT EXISTS idx_exchange_klines_exchange ON exchange_klines(exchange);
