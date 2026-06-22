-- ============================================================================
-- 017: 特征存储 + 市场状态识别系统
-- 依据《系统评估与演进规划》第二阶段"量化基础"
-- ============================================================================

-- ---------------------------------------------
-- 1. feature_definitions: 特征定义（版本化）
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS feature_definitions (
    feature_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL UNIQUE,
    description TEXT,
    category VARCHAR(50) NOT NULL,  -- momentum / volume / volatility / microstructure / funding / regime
    version VARCHAR(20) NOT NULL DEFAULT '1.0',
    parameters JSONB NOT NULL DEFAULT '{}'::jsonb,  -- 计算参数（period, std_dev 等）
    unit VARCHAR(20),  -- bps / ratio / count / percent
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_feature_definitions_category ON feature_definitions(category);
CREATE INDEX IF NOT EXISTS idx_feature_definitions_name ON feature_definitions(name);

-- ---------------------------------------------
-- 2. feature_values: 特征值时间序列
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS feature_values (
    id BIGSERIAL PRIMARY KEY,
    feature_id UUID NOT NULL REFERENCES feature_definitions(feature_id) ON DELETE CASCADE,
    symbol VARCHAR(50) NOT NULL,
    timestamp TIMESTAMP WITH TIME ZONE NOT NULL,
    value DOUBLE PRECISION NOT NULL,
    metadata JSONB,

    UNIQUE (feature_id, symbol, timestamp)
);

CREATE INDEX IF NOT EXISTS idx_feature_values_lookup ON feature_values(feature_id, symbol, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_feature_values_symbol_time ON feature_values(symbol, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_feature_values_time ON feature_values(timestamp DESC);

-- ---------------------------------------------
-- 3. market_regimes: 市场状态历史
-- 5 类状态：trending_bull / trending_bear / ranging / high_volatility / crisis
-- ---------------------------------------------
CREATE TABLE IF NOT EXISTS market_regimes (
    id BIGSERIAL PRIMARY KEY,
    symbol VARCHAR(50) NOT NULL,
    timestamp TIMESTAMP WITH TIME ZONE NOT NULL,
    regime VARCHAR(30) NOT NULL,  -- trending_bull / trending_bear / ranging / high_volatility / crisis
    confidence DOUBLE PRECISION NOT NULL DEFAULT 0.5,
    -- 状态判定依据
    adx DOUBLE PRECISION,              -- 趋势强度
    volatility_percentile DOUBLE PRECISION,  -- 波动率分位数
    return_percentile DOUBLE PRECISION,     -- 收益率分位数
    metadata JSONB,

    UNIQUE (symbol, timestamp)
);

CREATE INDEX IF NOT EXISTS idx_market_regimes_symbol_time ON market_regimes(symbol, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_market_regimes_regime ON market_regimes(regime);
CREATE INDEX IF NOT EXISTS idx_market_regimes_time ON market_regimes(timestamp DESC);

-- ---------------------------------------------
-- 4. 扩展数据保留：klines 已永久保留，此处扩展 ticker 和 funding
-- 依据规划文档：Ticker 仅保留 24h、资金费率仅 7 天，不足以训练或验证中长期模型
-- ---------------------------------------------
-- 注：此处不修改 collector.rs 的清理逻辑，而是新增 OHLCV 聚合表用于长期存储
CREATE TABLE IF NOT EXISTS ohlcv_daily (
    symbol VARCHAR(50) NOT NULL,
    date DATE NOT NULL,
    open DOUBLE PRECISION NOT NULL,
    high DOUBLE PRECISION NOT NULL,
    low DOUBLE PRECISION NOT NULL,
    close DOUBLE PRECISION NOT NULL,
    volume DOUBLE PRECISION NOT NULL DEFAULT 0,
    quote_volume DOUBLE PRECISION DEFAULT 0,
    trade_count INTEGER DEFAULT 0,

    PRIMARY KEY (symbol, date)
);

CREATE INDEX IF NOT EXISTS idx_ohlcv_daily_symbol_date ON ohlcv_daily(symbol, date DESC);

-- ---------------------------------------------
-- 5. 扩展 alpha_signals 表：填充 market_regime 字段（已存在，此处添加索引）
-- ---------------------------------------------
CREATE INDEX IF NOT EXISTS idx_alpha_signals_regime ON alpha_signals(market_regime) WHERE market_regime IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_alpha_signals_asset_time ON alpha_signals(asset, event_time DESC);

-- ---------------------------------------------
-- 6. 扩展 performance_reports 表：by_regime 已存在，此处确保索引
-- ---------------------------------------------
-- by_regime JSONB 字段已在 013 中创建，无需修改

-- ---------------------------------------------
-- 7. 插入默认特征定义
-- ---------------------------------------------
INSERT INTO feature_definitions (name, description, category, version, parameters, unit) VALUES
    ('rsi_14', '14 周期相对强弱指标', 'momentum', '1.0', '{"period": 14}'::jsonb, 'ratio'),
    ('macd_signal', 'MACD 信号线', 'momentum', '1.0', '{"fast": 12, "slow": 26, "signal": 9}'::jsonb, 'ratio'),
    ('sma_20', '20 周期简单移动平均', 'momentum', '1.0', '{"period": 20}'::jsonb, 'price'),
    ('ema_12', '12 周期指数移动平均', 'momentum', '1.0', '{"period": 12}'::jsonb, 'price'),
    ('bollinger_width', '布林带宽度', 'volatility', '1.0', '{"period": 20, "std_dev": 2.0}'::jsonb, 'price'),
    ('atr_14', '14 周期真实波幅', 'volatility', '1.0', '{"period": 14}'::jsonb, 'price'),
    ('realized_volatility_20', '20 周期实现波动率', 'volatility', '1.0', '{"period": 20}'::jsonb, 'percent'),
    ('return_1d', '1 日收益率', 'momentum', '1.0', '{"period": 1}'::jsonb, 'percent'),
    ('return_7d', '7 日累计收益率', 'momentum', '1.0', '{"period": 7}'::jsonb, 'percent'),
    ('adx_14', '14 周期平均定向指数', 'momentum', '1.0', '{"period": 14}'::jsonb, 'ratio'),
    ('volume_sma_20', '20 周期成交量均值', 'volume', '1.0', '{"period": 20}'::jsonb, 'count'),
    ('volume_ratio', '当前成交量 / 20 周期均值', 'volume', '1.0', '{"period": 20}'::jsonb, 'ratio'),
    ('funding_rate', '当前资金费率', 'funding', '1.0', '{}'::jsonb, 'bps'),
    ('high_low_range', '日内高低差 / 收盘', 'volatility', '1.0', '{}'::jsonb, 'percent'),
    ('market_regime', '市场状态分类', 'regime', '1.0', '{}'::jsonb, 'category')
ON CONFLICT (name) DO NOTHING;
