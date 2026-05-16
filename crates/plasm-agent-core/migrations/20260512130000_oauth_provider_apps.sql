-- Outbound OAuth provider registry (`oauth_provider_apps`), Phoenix-compatible baseline columns
-- plus optional RFC 8628 `device_authorization_endpoint`.
-- Client secrets are not stored here; `client_secret_key` references auth-framework KV.

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
