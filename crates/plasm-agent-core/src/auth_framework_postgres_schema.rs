//! PostgreSQL DDL aligned with [`auth_framework::storage::postgres::PostgresStorage`] queries.
//!
//! **Do not** call upstream [`PostgresStorage::migrate`](auth_framework::storage::postgres::PostgresStorage::migrate):
//! in **0.5.0-rc1** it sends one batch with MySQL-style `INDEX …` inside `CREATE TABLE`, which
//! PostgreSQL rejects. This module applies equivalent tables and indexes before [`PostgresStorage::new`].
//!
//! [`auth_framework::tokens::AuthToken`] (with `postgres-storage`) derives `sqlx::FromRow` and is
//! loaded via `SELECT * FROM auth_tokens`, so the table must expose `user_profile`, `permissions`,
//! and `roles` (upstream `INSERT`/`UPDATE` paths omit them, but reads still expect the columns).
//! `ALTER … ADD COLUMN IF NOT EXISTS` upgrades older Plasm-created databases.

use sqlx::PgPool;

pub async fn ensure_auth_storage_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    // Unqualified names resolve via the connection `search_path` (for example `plasm_agent_auth`
    // when using `scripts/export-plasm-local-auth-storage.sh`).
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS auth_tokens (
            token_id VARCHAR(255) PRIMARY KEY,
            user_id VARCHAR(255) NOT NULL,
            access_token TEXT NOT NULL UNIQUE,
            refresh_token TEXT,
            token_type VARCHAR(50),
            expires_at TIMESTAMPTZ NOT NULL,
            scopes TEXT[],
            issued_at TIMESTAMPTZ NOT NULL,
            auth_method VARCHAR(100) NOT NULL,
            subject VARCHAR(255),
            issuer VARCHAR(255),
            client_id VARCHAR(255),
            user_profile JSONB,
            permissions TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
            roles TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
            metadata JSONB,
            created_at TIMESTAMPTZ DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;

    // Existing databases created before 0.5-aligned columns: add missing fields.
    sqlx::query("ALTER TABLE auth_tokens ADD COLUMN IF NOT EXISTS user_profile JSONB")
        .execute(pool)
        .await?;
    sqlx::query(
        "ALTER TABLE auth_tokens ADD COLUMN IF NOT EXISTS permissions TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[]",
    )
    .execute(pool)
    .await?;
    sqlx::query(
        "ALTER TABLE auth_tokens ADD COLUMN IF NOT EXISTS roles TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[]",
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_auth_tokens_user_id ON auth_tokens (user_id)")
        .execute(pool)
        .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_auth_tokens_expires_at ON auth_tokens (expires_at)",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sessions (
            session_id VARCHAR(255) PRIMARY KEY,
            user_id VARCHAR(255) NOT NULL,
            data JSONB NOT NULL,
            expires_at TIMESTAMPTZ,
            created_at TIMESTAMPTZ DEFAULT NOW(),
            last_activity TIMESTAMPTZ,
            ip_address TEXT,
            user_agent TEXT
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions (user_id)")
        .execute(pool)
        .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions (expires_at)")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS kv_store (
            key VARCHAR(255) PRIMARY KEY,
            value BYTEA NOT NULL,
            expires_at TIMESTAMPTZ,
            created_at TIMESTAMPTZ DEFAULT NOW()
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_kv_store_expires_at ON kv_store (expires_at)")
        .execute(pool)
        .await?;

    Ok(())
}
