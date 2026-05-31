ALTER TABLE api_keys ADD COLUMN IF NOT EXISTS key_type VARCHAR(20) NOT NULL DEFAULT 'exchange';
ALTER TABLE api_keys ADD COLUMN IF NOT EXISTS metadata JSONB DEFAULT '{}';

CREATE INDEX IF NOT EXISTS idx_api_keys_key_type ON api_keys(key_type);
