-- API Keys table for authentication
-- Migration: 20240102000000_api_keys

CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    name VARCHAR(255) NOT NULL,
    key_hash VARCHAR(64) NOT NULL,
    key_prefix VARCHAR(8) NOT NULL,
    scopes JSONB NOT NULL DEFAULT '["*"]',
    expires_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for fast key lookup by prefix
CREATE INDEX IF NOT EXISTS idx_api_keys_prefix ON api_keys(key_prefix);

-- Index for tenant listing
CREATE INDEX IF NOT EXISTS idx_api_keys_tenant ON api_keys(tenant_id);

-- Partial index for non-expired keys (useful for auth queries)
CREATE INDEX IF NOT EXISTS idx_api_keys_active ON api_keys(key_prefix)
    WHERE expires_at IS NULL OR expires_at > NOW();

COMMENT ON TABLE api_keys IS 'API keys for authentication';
COMMENT ON COLUMN api_keys.key_hash IS 'SHA-256 hash of the full API key';
COMMENT ON COLUMN api_keys.key_prefix IS 'First 8 characters of the key for quick lookup';
COMMENT ON COLUMN api_keys.scopes IS 'JSON array of permission scopes (["*"] for full access)';
