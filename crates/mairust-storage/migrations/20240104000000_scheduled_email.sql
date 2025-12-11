-- MaiRust Scheduled Email Schema
-- This migration adds support for scheduled email sending, campaigns, and recipient lists

-- ============================================================================
-- Recipient Lists
-- ============================================================================

CREATE TABLE IF NOT EXISTS recipient_lists (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    recipient_count INTEGER NOT NULL DEFAULT 0,
    active_count INTEGER NOT NULL DEFAULT 0,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_recipient_lists_tenant_id ON recipient_lists(tenant_id);

-- ============================================================================
-- Recipients
-- ============================================================================

CREATE TABLE IF NOT EXISTS recipients (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    recipient_list_id UUID NOT NULL REFERENCES recipient_lists(id) ON DELETE CASCADE,
    email VARCHAR(254) NOT NULL,
    name VARCHAR(255),
    status VARCHAR(50) NOT NULL DEFAULT 'active',
    attributes JSONB NOT NULL DEFAULT '{}',
    subscribed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    unsubscribed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_recipient_per_list UNIQUE (recipient_list_id, email),
    CONSTRAINT valid_recipient_status CHECK (status IN ('active', 'unsubscribed', 'bounced', 'complained'))
);

CREATE INDEX idx_recipients_list_id ON recipients(recipient_list_id);
CREATE INDEX idx_recipients_email ON recipients(email);
CREATE INDEX idx_recipients_status ON recipients(status);

-- ============================================================================
-- Campaigns
-- ============================================================================

CREATE TABLE IF NOT EXISTS campaigns (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    subject VARCHAR(998) NOT NULL,
    from_address VARCHAR(254) NOT NULL,
    from_name VARCHAR(255),
    reply_to VARCHAR(254),
    html_body TEXT,
    text_body TEXT,
    recipient_list_id UUID REFERENCES recipient_lists(id) ON DELETE SET NULL,
    scheduled_at TIMESTAMPTZ,
    rate_limit_per_hour INTEGER NOT NULL DEFAULT 5000,
    rate_limit_per_minute INTEGER NOT NULL DEFAULT 100,
    status VARCHAR(50) NOT NULL DEFAULT 'draft',
    total_recipients INTEGER NOT NULL DEFAULT 0,
    sent_count INTEGER NOT NULL DEFAULT 0,
    delivered_count INTEGER NOT NULL DEFAULT 0,
    bounced_count INTEGER NOT NULL DEFAULT 0,
    failed_count INTEGER NOT NULL DEFAULT 0,
    opened_count INTEGER NOT NULL DEFAULT 0,
    clicked_count INTEGER NOT NULL DEFAULT 0,
    tags JSONB NOT NULL DEFAULT '[]',
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    CONSTRAINT valid_campaign_status CHECK (status IN ('draft', 'scheduled', 'sending', 'paused', 'completed', 'cancelled', 'failed'))
);

CREATE INDEX idx_campaigns_tenant_id ON campaigns(tenant_id);
CREATE INDEX idx_campaigns_status ON campaigns(status);
CREATE INDEX idx_campaigns_scheduled_at ON campaigns(scheduled_at) WHERE status = 'scheduled';

-- ============================================================================
-- Scheduled Messages
-- ============================================================================

CREATE TABLE IF NOT EXISTS scheduled_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    campaign_id UUID REFERENCES campaigns(id) ON DELETE CASCADE,
    recipient_id UUID REFERENCES recipients(id) ON DELETE SET NULL,
    batch_id UUID,
    from_address VARCHAR(254) NOT NULL,
    to_address VARCHAR(254) NOT NULL,
    subject VARCHAR(998) NOT NULL,
    html_body TEXT,
    text_body TEXT,
    headers JSONB NOT NULL DEFAULT '{}',
    scheduled_at TIMESTAMPTZ NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    last_attempt_at TIMESTAMPTZ,
    last_error TEXT,
    message_id VARCHAR(255),
    sent_at TIMESTAMPTZ,
    delivered_at TIMESTAMPTZ,
    bounced_at TIMESTAMPTZ,
    bounce_type VARCHAR(50),
    bounce_reason TEXT,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT valid_scheduled_status CHECK (status IN ('pending', 'processing', 'sent', 'delivered', 'bounced', 'failed', 'cancelled'))
);

CREATE INDEX idx_scheduled_messages_tenant_id ON scheduled_messages(tenant_id);
CREATE INDEX idx_scheduled_messages_campaign_id ON scheduled_messages(campaign_id);
CREATE INDEX idx_scheduled_messages_batch_id ON scheduled_messages(batch_id);
CREATE INDEX idx_scheduled_messages_status ON scheduled_messages(status);
CREATE INDEX idx_scheduled_messages_pending ON scheduled_messages(scheduled_at) WHERE status = 'pending';
CREATE INDEX idx_scheduled_messages_to_address ON scheduled_messages(to_address);

-- ============================================================================
-- Tenant Rate Limits
-- ============================================================================

CREATE TABLE IF NOT EXISTS tenant_rate_limits (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    per_minute INTEGER NOT NULL DEFAULT 100,
    per_hour INTEGER NOT NULL DEFAULT 5000,
    per_day INTEGER NOT NULL DEFAULT 50000,
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_tenant_rate_limit UNIQUE (tenant_id)
);

CREATE INDEX idx_tenant_rate_limits_tenant_id ON tenant_rate_limits(tenant_id);

-- ============================================================================
-- Rate Limit Counters (sliding window)
-- ============================================================================

CREATE TABLE IF NOT EXISTS rate_limit_counters (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    window_type VARCHAR(20) NOT NULL,
    window_start TIMESTAMPTZ NOT NULL,
    count INTEGER NOT NULL DEFAULT 0,
    limit_value INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_rate_limit_window UNIQUE (tenant_id, window_type, window_start)
);

CREATE INDEX idx_rate_limit_counters_tenant_window ON rate_limit_counters(tenant_id, window_type, window_start);

-- ============================================================================
-- Unsubscribes (global suppression list)
-- ============================================================================

CREATE TABLE IF NOT EXISTS unsubscribes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    email VARCHAR(254) NOT NULL,
    source VARCHAR(50) NOT NULL,
    campaign_id UUID REFERENCES campaigns(id) ON DELETE SET NULL,
    reason TEXT,
    unsubscribed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT unique_unsubscribe UNIQUE (tenant_id, email)
);

CREATE INDEX idx_unsubscribes_tenant_email ON unsubscribes(tenant_id, email);

-- ============================================================================
-- Functions for maintaining counts
-- ============================================================================

-- Function to update recipient counts
CREATE OR REPLACE FUNCTION update_recipient_list_counts()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' OR TG_OP = 'UPDATE' THEN
        UPDATE recipient_lists SET
            recipient_count = (SELECT COUNT(*) FROM recipients WHERE recipient_list_id = NEW.recipient_list_id),
            active_count = (SELECT COUNT(*) FROM recipients WHERE recipient_list_id = NEW.recipient_list_id AND status = 'active'),
            updated_at = NOW()
        WHERE id = NEW.recipient_list_id;
        RETURN NEW;
    ELSIF TG_OP = 'DELETE' THEN
        UPDATE recipient_lists SET
            recipient_count = (SELECT COUNT(*) FROM recipients WHERE recipient_list_id = OLD.recipient_list_id),
            active_count = (SELECT COUNT(*) FROM recipients WHERE recipient_list_id = OLD.recipient_list_id AND status = 'active'),
            updated_at = NOW()
        WHERE id = OLD.recipient_list_id;
        RETURN OLD;
    END IF;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_update_recipient_list_counts
AFTER INSERT OR UPDATE OR DELETE ON recipients
FOR EACH ROW EXECUTE FUNCTION update_recipient_list_counts();

-- Function to update campaign stats
CREATE OR REPLACE FUNCTION update_campaign_stats()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' OR TG_OP = 'UPDATE' THEN
        IF NEW.campaign_id IS NOT NULL THEN
            UPDATE campaigns SET
                sent_count = (SELECT COUNT(*) FROM scheduled_messages WHERE campaign_id = NEW.campaign_id AND status IN ('sent', 'delivered')),
                delivered_count = (SELECT COUNT(*) FROM scheduled_messages WHERE campaign_id = NEW.campaign_id AND status = 'delivered'),
                bounced_count = (SELECT COUNT(*) FROM scheduled_messages WHERE campaign_id = NEW.campaign_id AND status = 'bounced'),
                failed_count = (SELECT COUNT(*) FROM scheduled_messages WHERE campaign_id = NEW.campaign_id AND status = 'failed'),
                updated_at = NOW()
            WHERE id = NEW.campaign_id;
        END IF;
        RETURN NEW;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_update_campaign_stats
AFTER INSERT OR UPDATE ON scheduled_messages
FOR EACH ROW EXECUTE FUNCTION update_campaign_stats();
