-- plasm-agent-core canonical schema (appliance + hosted plasm-mcp).
-- Idempotent DDL safe on shared SaaS Postgres (Phoenix may already own oauth_provider_apps).

-- Tenant MCP configuration (`project_mcp_*`). No FK to project_outbound_auth_configs
-- so this migration is self-contained; auth_config_id integrity is application-level.

CREATE TABLE IF NOT EXISTS project_mcp_configs (
    id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    workspace_slug TEXT NOT NULL,
    project_slug TEXT NOT NULL,
    space_type TEXT NOT NULL DEFAULT 'organization',
    owner_subject TEXT,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    config_version BIGINT NOT NULL DEFAULT 1,
    endpoint_secret_hash BYTEA NOT NULL,
    mcp_api_key_fingerprint TEXT,
    auth_optional_entry_ids TEXT[] NOT NULL DEFAULT '{}',
    inserted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS project_mcp_configs_tenant_workspace_project
    ON project_mcp_configs (tenant_id, workspace_slug, project_slug);

CREATE INDEX IF NOT EXISTS project_mcp_configs_endpoint_secret_hash
    ON project_mcp_configs (endpoint_secret_hash);

CREATE INDEX IF NOT EXISTS project_mcp_configs_tenant_space
    ON project_mcp_configs (tenant_id, space_type);

CREATE TABLE IF NOT EXISTS project_mcp_allowed_graphs (
    id UUID PRIMARY KEY,
    config_id UUID NOT NULL REFERENCES project_mcp_configs (id) ON DELETE CASCADE,
    entry_id TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true
);

CREATE UNIQUE INDEX IF NOT EXISTS project_mcp_allowed_graphs_config_entry
    ON project_mcp_allowed_graphs (config_id, entry_id);

CREATE TABLE IF NOT EXISTS project_mcp_allowed_capabilities (
    id UUID PRIMARY KEY,
    config_id UUID NOT NULL REFERENCES project_mcp_configs (id) ON DELETE CASCADE,
    entry_id TEXT NOT NULL,
    capability_name TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true
);

CREATE UNIQUE INDEX IF NOT EXISTS project_mcp_allowed_capabilities_unique
    ON project_mcp_allowed_capabilities (config_id, entry_id, capability_name);

CREATE TABLE IF NOT EXISTS project_mcp_credentials (
    id UUID PRIMARY KEY,
    config_id UUID NOT NULL REFERENCES project_mcp_configs (id) ON DELETE CASCADE,
    kind TEXT NOT NULL DEFAULT 'bearer',
    label TEXT,
    secret_hash BYTEA NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    revoked_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    inserted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS project_mcp_credentials_config_id
    ON project_mcp_credentials (config_id);

CREATE INDEX IF NOT EXISTS project_mcp_credentials_config_status
    ON project_mcp_credentials (config_id, status);

CREATE TABLE IF NOT EXISTS project_mcp_auth_bindings (
    id UUID PRIMARY KEY,
    config_id UUID NOT NULL REFERENCES project_mcp_configs (id) ON DELETE CASCADE,
    entry_id TEXT NOT NULL,
    auth_config_id UUID NOT NULL,
    inserted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS project_mcp_auth_bindings_config_entry
    ON project_mcp_auth_bindings (config_id, entry_id);

CREATE INDEX IF NOT EXISTS project_mcp_auth_bindings_auth_config
    ON project_mcp_auth_bindings (auth_config_id);

-- Discovery embeddings (BYTEA f32 LE). No pgvector extension required.

CREATE TABLE IF NOT EXISTS plasm_catalog_discovery_embeddings (
    catalog_cgs_hash TEXT NOT NULL,
    embedding_model_id TEXT NOT NULL,
    line_text TEXT NOT NULL,
    embedding_dim SMALLINT NOT NULL,
    embedding BYTEA NOT NULL,
    inserted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (catalog_cgs_hash, embedding_model_id, line_text)
);

CREATE INDEX IF NOT EXISTS plasm_catalog_discovery_embeddings_model_hash
    ON plasm_catalog_discovery_embeddings (embedding_model_id, catalog_cgs_hash);

-- Outbound OAuth provider registry. Phoenix-compatible baseline columns.

CREATE TABLE IF NOT EXISTS oauth_provider_apps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    entry_id TEXT NOT NULL,
    provider TEXT,
    authorization_endpoint TEXT,
    token_endpoint TEXT,
    client_id TEXT NOT NULL,
    client_secret_key TEXT NOT NULL,
    redirect_uri_note TEXT,
    docs_url TEXT,
    enabled BOOLEAN NOT NULL DEFAULT true,
    last_synced_at TIMESTAMPTZ,
    last_sync_error TEXT,
    updated_by_subject TEXT,
    inserted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS oauth_provider_apps_entry_id_key ON oauth_provider_apps (entry_id);

ALTER TABLE oauth_provider_apps
    ADD COLUMN IF NOT EXISTS device_authorization_endpoint TEXT;
