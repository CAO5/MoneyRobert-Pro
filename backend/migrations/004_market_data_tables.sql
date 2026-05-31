CREATE TABLE IF NOT EXISTS klines (
    id BIGSERIAL PRIMARY KEY,
    symbol VARCHAR(50) NOT NULL,
    "interval" VARCHAR(10) NOT NULL,
    open_time TIMESTAMP NOT NULL,
    open DOUBLE PRECISION NOT NULL,
    high DOUBLE PRECISION NOT NULL,
    low DOUBLE PRECISION NOT NULL,
    close DOUBLE PRECISION NOT NULL,
    volume DOUBLE PRECISION NOT NULL DEFAULT 0,
    quote_volume DOUBLE PRECISION DEFAULT 0,
    trades_count BIGINT DEFAULT 0,
    is_closed BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE(symbol, "interval", open_time)
);

CREATE INDEX IF NOT EXISTS idx_klines_symbol_interval ON klines(symbol, "interval");
CREATE INDEX IF NOT EXISTS idx_klines_open_time ON klines(open_time DESC);

CREATE TABLE IF NOT EXISTS ticker_history (
    id BIGSERIAL PRIMARY KEY,
    symbol VARCHAR(50) NOT NULL,
    last DOUBLE PRECISION NOT NULL DEFAULT 0,
    open_24h DOUBLE PRECISION NOT NULL DEFAULT 0,
    high_24h DOUBLE PRECISION NOT NULL DEFAULT 0,
    low_24h DOUBLE PRECISION NOT NULL DEFAULT 0,
    volume_24h DOUBLE PRECISION NOT NULL DEFAULT 0,
    best_bid DOUBLE PRECISION NOT NULL DEFAULT 0,
    best_ask DOUBLE PRECISION NOT NULL DEFAULT 0,
    timestamp TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ticker_history_symbol ON ticker_history(symbol);
CREATE INDEX IF NOT EXISTS idx_ticker_history_timestamp ON ticker_history(timestamp DESC);

CREATE TABLE IF NOT EXISTS funding_rate_history (
    id BIGSERIAL PRIMARY KEY,
    symbol VARCHAR(50) NOT NULL,
    funding_rate DOUBLE PRECISION NOT NULL,
    funding_time TIMESTAMP NOT NULL,
    realized_rate DOUBLE PRECISION,
    avg_premium_index DOUBLE PRECISION,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_funding_rate_history_symbol ON funding_rate_history(symbol);
CREATE INDEX IF NOT EXISTS idx_funding_rate_history_created_at ON funding_rate_history(created_at DESC);

CREATE TABLE IF NOT EXISTS long_short_ratio_history (
    id BIGSERIAL PRIMARY KEY,
    symbol VARCHAR(50) NOT NULL,
    long_ratio DOUBLE PRECISION NOT NULL DEFAULT 0,
    short_ratio DOUBLE PRECISION NOT NULL DEFAULT 0,
    long_short_ratio DOUBLE PRECISION,
    timestamp TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_long_short_ratio_history_symbol ON long_short_ratio_history(symbol);
CREATE INDEX IF NOT EXISTS idx_long_short_ratio_history_timestamp ON long_short_ratio_history(timestamp DESC);

CREATE TABLE IF NOT EXISTS open_interests (
    id BIGSERIAL PRIMARY KEY,
    symbol VARCHAR(50) NOT NULL,
    open_interest DOUBLE PRECISION NOT NULL DEFAULT 0,
    open_interest_value DOUBLE PRECISION,
    timestamp TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_open_interests_symbol ON open_interests(symbol);
CREATE INDEX IF NOT EXISTS idx_open_interests_timestamp ON open_interests(timestamp DESC);

CREATE TABLE IF NOT EXISTS equity_snapshots (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    total_equity DOUBLE PRECISION NOT NULL DEFAULT 0,
    available_balance DOUBLE PRECISION NOT NULL DEFAULT 0,
    unrealized_pnl DOUBLE PRECISION NOT NULL DEFAULT 0,
    realized_pnl DOUBLE PRECISION NOT NULL DEFAULT 0,
    position_margin DOUBLE PRECISION NOT NULL DEFAULT 0,
    snapshot_type VARCHAR(20) NOT NULL DEFAULT 'hourly',
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_equity_snapshots_user_id ON equity_snapshots(user_id);
CREATE INDEX IF NOT EXISTS idx_equity_snapshots_created_at ON equity_snapshots(created_at DESC);

INSERT INTO klines (symbol, "interval", open_time, open, high, low, close, volume)
VALUES
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '24 hours', 67500.0, 68200.0, 67100.0, 67800.0, 1250.5),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '23 hours', 67800.0, 68500.0, 67500.0, 68100.0, 1180.3),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '22 hours', 68100.0, 68900.0, 67900.0, 68500.0, 1320.7),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '21 hours', 68500.0, 69100.0, 68200.0, 68700.0, 1090.2),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '20 hours', 68700.0, 69300.0, 68500.0, 69000.0, 1450.8),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '19 hours', 69000.0, 69500.0, 68600.0, 68800.0, 1210.4),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '18 hours', 68800.0, 69200.0, 68400.0, 68600.0, 980.6),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '17 hours', 68600.0, 69000.0, 68100.0, 68300.0, 1150.3),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '16 hours', 68300.0, 68800.0, 67900.0, 68100.0, 1070.1),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '15 hours', 68100.0, 68700.0, 67800.0, 68500.0, 1290.5),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '14 hours', 68500.0, 69200.0, 68300.0, 68900.0, 1380.9),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '13 hours', 68900.0, 69600.0, 68700.0, 69200.0, 1520.2),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '12 hours', 69200.0, 69800.0, 69000.0, 69400.0, 1410.7),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '11 hours', 69400.0, 70100.0, 69200.0, 69700.0, 1630.4),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '10 hours', 69700.0, 70300.0, 69400.0, 69500.0, 1340.8),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '9 hours', 69500.0, 70000.0, 69100.0, 69300.0, 1180.3),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '8 hours', 69300.0, 69800.0, 69000.0, 69600.0, 1250.6),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '7 hours', 69600.0, 70200.0, 69400.0, 69900.0, 1390.1),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '6 hours', 69900.0, 70500.0, 69700.0, 70100.0, 1470.5),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '5 hours', 70100.0, 70800.0, 69900.0, 70400.0, 1580.9),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '4 hours', 70400.0, 71000.0, 70200.0, 70600.0, 1320.4),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '3 hours', 70600.0, 71200.0, 70400.0, 70800.0, 1410.7),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '2 hours', 70800.0, 71500.0, 70600.0, 71100.0, 1550.2),
    ('BTC-USDT-SWAP', '1H', NOW() - INTERVAL '1 hours', 71100.0, 71800.0, 70900.0, 71300.0, 1680.6),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '24 hours', 3450.0, 3520.0, 3420.0, 3480.0, 8500.3),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '23 hours', 3480.0, 3550.0, 3460.0, 3510.0, 9200.1),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '22 hours', 3510.0, 3580.0, 3490.0, 3540.0, 8800.7),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '21 hours', 3540.0, 3600.0, 3520.0, 3560.0, 7900.4),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '20 hours', 3560.0, 3620.0, 3540.0, 3590.0, 9500.2),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '19 hours', 3590.0, 3650.0, 3570.0, 3610.0, 8700.8),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '18 hours', 3610.0, 3660.0, 3580.0, 3590.0, 8100.5),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '17 hours', 3590.0, 3640.0, 3560.0, 3570.0, 7600.3),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '16 hours', 3570.0, 3620.0, 3540.0, 3600.0, 8300.1),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '15 hours', 3600.0, 3670.0, 3580.0, 3640.0, 9100.6),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '14 hours', 3640.0, 3700.0, 3620.0, 3670.0, 9800.4),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '13 hours', 3670.0, 3730.0, 3650.0, 3700.0, 10200.7),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '12 hours', 3700.0, 3760.0, 3680.0, 3720.0, 9500.2),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '11 hours', 3720.0, 3780.0, 3700.0, 3750.0, 10800.5),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '10 hours', 3750.0, 3800.0, 3730.0, 3770.0, 9200.8),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '9 hours', 3770.0, 3820.0, 3750.0, 3790.0, 8900.3),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '8 hours', 3790.0, 3850.0, 3770.0, 3820.0, 9600.1),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '7 hours', 3820.0, 3880.0, 3800.0, 3850.0, 10100.4),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '6 hours', 3850.0, 3910.0, 3830.0, 3880.0, 10700.7),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '5 hours', 3880.0, 3940.0, 3860.0, 3910.0, 11200.2),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '4 hours', 3910.0, 3960.0, 3890.0, 3930.0, 9800.6),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '3 hours', 3930.0, 3990.0, 3910.0, 3960.0, 10500.3),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '2 hours', 3960.0, 4020.0, 3940.0, 3990.0, 11000.8),
    ('ETH-USDT-SWAP', '1H', NOW() - INTERVAL '1 hours', 3990.0, 4050.0, 3970.0, 4020.0, 11500.1)
ON CONFLICT (symbol, "interval", open_time) DO NOTHING;

INSERT INTO ticker_history (symbol, last, open_24h, high_24h, low_24h, volume_24h, best_bid, best_ask, timestamp)
VALUES
    ('BTC-USDT-SWAP', 71300.0, 67500.0, 71800.0, 67100.0, 28500.5, 71295.0, 71305.0, NOW()),
    ('ETH-USDT-SWAP', 4020.0, 3450.0, 4050.0, 3420.0, 215000.3, 4019.5, 4020.5, NOW()),
    ('SOL-USDT-SWAP', 178.5, 172.0, 180.2, 170.5, 850000.0, 178.4, 178.6, NOW()),
    ('DOGE-USDT-SWAP', 0.412, 0.398, 0.418, 0.395, 1250000000.0, 0.4119, 0.4121, NOW()),
    ('XRP-USDT-SWAP', 2.35, 2.28, 2.38, 2.25, 95000000.0, 2.349, 2.351, NOW());

INSERT INTO funding_rate_history (symbol, funding_rate, funding_time, realized_rate, avg_premium_index, created_at)
VALUES
    ('BTC-USDT-SWAP', 0.0001, NOW() - INTERVAL '8 hours', 0.00012, 0.00011, NOW() - INTERVAL '8 hours'),
    ('BTC-USDT-SWAP', 0.00008, NOW() - INTERVAL '16 hours', 0.00009, 0.000085, NOW() - INTERVAL '16 hours'),
    ('BTC-USDT-SWAP', 0.00015, NOW() - INTERVAL '24 hours', 0.00014, 0.00013, NOW() - INTERVAL '24 hours'),
    ('ETH-USDT-SWAP', 0.00012, NOW() - INTERVAL '8 hours', 0.00013, 0.000125, NOW() - INTERVAL '8 hours'),
    ('ETH-USDT-SWAP', 0.00005, NOW() - INTERVAL '16 hours', 0.00006, 0.000055, NOW() - INTERVAL '16 hours'),
    ('ETH-USDT-SWAP', 0.00018, NOW() - INTERVAL '24 hours', 0.00017, 0.00016, NOW() - INTERVAL '24 hours'),
    ('SOL-USDT-SWAP', 0.0002, NOW() - INTERVAL '8 hours', 0.00022, 0.00021, NOW() - INTERVAL '8 hours'),
    ('SOL-USDT-SWAP', -0.00005, NOW() - INTERVAL '16 hours', -0.00004, -0.00003, NOW() - INTERVAL '16 hours'),
    ('DOGE-USDT-SWAP', 0.00025, NOW() - INTERVAL '8 hours', 0.00027, 0.00026, NOW() - INTERVAL '8 hours'),
    ('XRP-USDT-SWAP', 0.00008, NOW() - INTERVAL '8 hours', 0.00009, 0.000085, NOW() - INTERVAL '8 hours');

INSERT INTO long_short_ratio_history (symbol, long_ratio, short_ratio, long_short_ratio, timestamp)
VALUES
    ('BTC-USDT-SWAP', 0.55, 0.45, 1.22, NOW()),
    ('BTC-USDT-SWAP', 0.52, 0.48, 1.08, NOW() - INTERVAL '4 hours'),
    ('BTC-USDT-SWAP', 0.58, 0.42, 1.38, NOW() - INTERVAL '8 hours'),
    ('ETH-USDT-SWAP', 0.53, 0.47, 1.13, NOW()),
    ('ETH-USDT-SWAP', 0.50, 0.50, 1.00, NOW() - INTERVAL '4 hours'),
    ('ETH-USDT-SWAP', 0.56, 0.44, 1.27, NOW() - INTERVAL '8 hours'),
    ('SOL-USDT-SWAP', 0.60, 0.40, 1.50, NOW()),
    ('DOGE-USDT-SWAP', 0.48, 0.52, 0.92, NOW()),
    ('XRP-USDT-SWAP', 0.51, 0.49, 1.04, NOW());

INSERT INTO open_interests (symbol, open_interest, open_interest_value, timestamp)
VALUES
    ('BTC-USDT-SWAP', 125000.5, 8912500000.0, NOW()),
    ('BTC-USDT-SWAP', 124500.3, 8875000000.0, NOW() - INTERVAL '4 hours'),
    ('BTC-USDT-SWAP', 123800.7, 8820000000.0, NOW() - INTERVAL '8 hours'),
    ('ETH-USDT-SWAP', 2500000.0, 10050000000.0, NOW()),
    ('ETH-USDT-SWAP', 2480000.0, 9920000000.0, NOW() - INTERVAL '4 hours'),
    ('SOL-USDT-SWAP', 15000000.0, 2677500000.0, NOW()),
    ('DOGE-USDT-SWAP', 5000000000.0, 2060000000.0, NOW()),
    ('XRP-USDT-SWAP', 800000000.0, 1880000000.0, NOW());

INSERT INTO equity_snapshots (user_id, total_equity, available_balance, unrealized_pnl, realized_pnl, position_margin, snapshot_type, created_at)
VALUES
    (1, 100000.0, 85000.0, 1500.0, 3500.0, 15000.0, 'hourly', NOW()),
    (1, 99500.0, 84500.0, 1200.0, 3300.0, 15000.0, 'hourly', NOW() - INTERVAL '1 hour'),
    (1, 99000.0, 84000.0, 800.0, 3100.0, 15000.0, 'hourly', NOW() - INTERVAL '2 hours');
