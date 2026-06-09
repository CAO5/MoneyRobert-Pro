-- System settings table for proxy and other configurations
CREATE TABLE IF NOT EXISTS system_settings (
    key VARCHAR(100) PRIMARY KEY,
    value TEXT NOT NULL,
    value_type VARCHAR(20) NOT NULL DEFAULT 'string', -- string, number, boolean, json
    category VARCHAR(50) NOT NULL DEFAULT 'general', -- general, proxy, exchange, ai
    description TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by VARCHAR(100)
);

-- Insert default proxy settings from environment
INSERT INTO system_settings (key, value, value_type, category, description) VALUES
    ('proxy_enabled', 'false', 'boolean', 'proxy', '是否启用代理'),
    ('proxy_url', '', 'string', 'proxy', '代理地址，如 socks5://127.0.0.1:10809 或 http://127.0.0.1:7890'),
    ('proxy_type', 'socks5', 'string', 'proxy', '代理类型: socks5, http, https'),
    ('proxy_test_url', 'https://www.okx.com', 'string', 'proxy', '代理测试URL')
ON CONFLICT (key) DO NOTHING;

-- Index for category lookups
CREATE INDEX IF NOT EXISTS idx_system_settings_category ON system_settings(category);
