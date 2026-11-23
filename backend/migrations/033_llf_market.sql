-- 同一研究采集器覆盖六个现货/永续产品。保留原生单位，永续张数由当时的合约定义换算。
CREATE TABLE llf_market_data (
    id BIGSERIAL PRIMARY KEY,
    symbol TEXT NOT NULL,
    channel TEXT NOT NULL,
    exchange_ts TIMESTAMPTZ NOT NULL,
    received_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    session_id UUID,
    event_key TEXT NOT NULL,
    quality TEXT NOT NULL,
    UNIQUE(symbol, channel, event_key)
);
CREATE INDEX llf_market_history ON llf_market_data(symbol, channel, exchange_ts DESC);

-- 研究任务读取已归档的最新事件，raw_id 可还原当时的交易所数据。
CREATE TABLE llf_market_latest (
    symbol TEXT NOT NULL,
    channel TEXT NOT NULL,
    raw_id BIGINT NOT NULL REFERENCES llf_market_data(id),
    exchange_ts TIMESTAMPTZ NOT NULL,
    received_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    session_id UUID,
    quality TEXT NOT NULL,
    PRIMARY KEY(symbol, channel)
);
CREATE TABLE llf_market_session (
    id SMALLINT PRIMARY KEY CHECK(id=1),
    session_id UUID NOT NULL,
    connected BOOLEAN NOT NULL,
    connected_at TIMESTAMPTZ NOT NULL,
    last_error TEXT
);
-- WS 重连使全市场观察窗口重新开始。单标的成交流跳号单独落库，方便按窗口核验数据质量。
CREATE TABLE llf_market_gaps (
    id BIGSERIAL PRIMARY KEY,
    symbol TEXT NOT NULL,
    channel TEXT NOT NULL,
    session_id UUID NOT NULL,
    detected_at TIMESTAMPTZ NOT NULL,
    reason TEXT NOT NULL
);
CREATE INDEX llf_market_gap_time ON llf_market_gaps(symbol, channel, detected_at DESC);
