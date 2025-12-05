-- MaiRust Phase 3: Message Threading and Enhanced Tagging
-- This migration adds support for message threading and improved tagging

-- ============================================================================
-- Message Threading Support
-- ============================================================================

-- Add threading columns to messages
ALTER TABLE messages ADD COLUMN IF NOT EXISTS thread_id UUID;
ALTER TABLE messages ADD COLUMN IF NOT EXISTS in_reply_to VARCHAR(255);
ALTER TABLE messages ADD COLUMN IF NOT EXISTS references_headers TEXT;
ALTER TABLE messages ADD COLUMN IF NOT EXISTS thread_position INTEGER DEFAULT 0;
ALTER TABLE messages ADD COLUMN IF NOT EXISTS thread_depth INTEGER DEFAULT 0;

-- Index for thread lookups
CREATE INDEX IF NOT EXISTS idx_messages_thread ON messages(thread_id);
CREATE INDEX IF NOT EXISTS idx_messages_in_reply_to ON messages(in_reply_to);
CREATE INDEX IF NOT EXISTS idx_messages_message_id ON messages(message_id_header);

-- Thread metadata table
CREATE TABLE IF NOT EXISTS threads (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    mailbox_id UUID NOT NULL REFERENCES mailboxes(id) ON DELETE CASCADE,
    subject TEXT,
    participant_addresses JSONB NOT NULL DEFAULT '[]',
    message_count INTEGER NOT NULL DEFAULT 0,
    unread_count INTEGER NOT NULL DEFAULT 0,
    first_message_at TIMESTAMPTZ,
    last_message_at TIMESTAMPTZ,
    last_message_id UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_threads_tenant ON threads(tenant_id);
CREATE INDEX IF NOT EXISTS idx_threads_mailbox ON threads(mailbox_id);
CREATE INDEX IF NOT EXISTS idx_threads_last_message ON threads(last_message_at DESC);

-- ============================================================================
-- Enhanced Tagging System
-- ============================================================================

-- Tags table for structured tag management
CREATE TABLE IF NOT EXISTS tags (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    color VARCHAR(7),  -- Hex color code like #FF5733
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, name)
);

CREATE INDEX IF NOT EXISTS idx_tags_tenant ON tags(tenant_id);
CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(name);

-- Message-tag relationship table
CREATE TABLE IF NOT EXISTS message_tags (
    message_id UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    tag_id UUID NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (message_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_message_tags_message ON message_tags(message_id);
CREATE INDEX IF NOT EXISTS idx_message_tags_tag ON message_tags(tag_id);

-- ============================================================================
-- AI Categorization Support
-- ============================================================================

-- Categories table for AI-assigned categories
CREATE TABLE IF NOT EXISTS categories (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name VARCHAR(100) NOT NULL,
    description TEXT,
    color VARCHAR(7),
    priority INTEGER NOT NULL DEFAULT 0,
    auto_rules JSONB NOT NULL DEFAULT '{}',  -- Automatic categorization rules
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(tenant_id, name)
);

CREATE INDEX IF NOT EXISTS idx_categories_tenant ON categories(tenant_id);

-- Add category column to messages
ALTER TABLE messages ADD COLUMN IF NOT EXISTS category_id UUID REFERENCES categories(id) ON DELETE SET NULL;
ALTER TABLE messages ADD COLUMN IF NOT EXISTS category_confidence REAL;  -- AI confidence score
ALTER TABLE messages ADD COLUMN IF NOT EXISTS ai_summary TEXT;  -- AI-generated summary
ALTER TABLE messages ADD COLUMN IF NOT EXISTS ai_metadata JSONB NOT NULL DEFAULT '{}';

CREATE INDEX IF NOT EXISTS idx_messages_category ON messages(category_id);

-- ============================================================================
-- Plugin System Support
-- ============================================================================

-- Plugin events log
CREATE TABLE IF NOT EXISTS plugin_events (
    id UUID PRIMARY KEY,
    plugin_id VARCHAR(255) NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    tenant_id UUID REFERENCES tenants(id) ON DELETE CASCADE,
    event_type VARCHAR(100) NOT NULL,
    message_id UUID REFERENCES messages(id) ON DELETE CASCADE,
    input_data JSONB,
    output_data JSONB,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',  -- pending, success, failed
    error_message TEXT,
    execution_time_ms INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_plugin_events_plugin ON plugin_events(plugin_id);
CREATE INDEX IF NOT EXISTS idx_plugin_events_tenant ON plugin_events(tenant_id);
CREATE INDEX IF NOT EXISTS idx_plugin_events_message ON plugin_events(message_id);
CREATE INDEX IF NOT EXISTS idx_plugin_events_status ON plugin_events(status);
CREATE INDEX IF NOT EXISTS idx_plugin_events_created ON plugin_events(created_at DESC);

-- Plugin configurations per tenant
CREATE TABLE IF NOT EXISTS plugin_configs (
    plugin_id VARCHAR(255) NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    enabled BOOLEAN NOT NULL DEFAULT false,
    config JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (plugin_id, tenant_id)
);

-- ============================================================================
-- Mailbox Subscriptions (for IMAP)
-- ============================================================================

CREATE TABLE IF NOT EXISTS mailbox_subscriptions (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    mailbox_id UUID NOT NULL REFERENCES mailboxes(id) ON DELETE CASCADE,
    subscribed BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, mailbox_id)
);

-- ============================================================================
-- Default Categories
-- ============================================================================

-- Insert default categories (will be skipped if they already exist)
INSERT INTO categories (id, tenant_id, name, description, color, priority)
SELECT
    gen_random_uuid(),
    t.id,
    c.name,
    c.description,
    c.color,
    c.priority
FROM tenants t
CROSS JOIN (VALUES
    ('Primary', 'Important personal messages', '#4285F4', 100),
    ('Social', 'Social network notifications', '#34A853', 80),
    ('Promotions', 'Marketing and promotional emails', '#FBBC05', 60),
    ('Updates', 'Automated updates and notifications', '#EA4335', 40),
    ('Forums', 'Mailing list and forum messages', '#9E9E9E', 20)
) AS c(name, description, color, priority)
WHERE NOT EXISTS (
    SELECT 1 FROM categories WHERE tenant_id = t.id AND name = c.name
);
