//! Embedded PostgreSQL via [`pg-embed`] when the crate feature `embedded_postgres` is enabled.
//!
//! Runtime opt-in: `PLASM_EMBEDDED_POSTGRES=1` (also accepts `true`, `yes`).
//!
//! Connection targets:
//! - Prefer `DATABASE_URL` (`postgresql://…`), host must be `localhost` or `127.0.0.1`.
//! - If unset: `PLASM_EMBEDDED_POSTGRES_USER` (default `postgres`), password via
//!   `PLASM_EMBEDDED_POSTGRES_PASSWORD` or `PLASM_EMBEDDED_POSTGRES_PASSWORD_FILE`,
//!   `PLASM_EMBEDDED_POSTGRES_PORT` (default `5432`), `PLASM_EMBEDDED_POSTGRES_DATABASE`.
//!
//! Data directory: `PLASM_EMBEDDED_POSTGRES_DATA_DIR`, else `PGDATA`, else error when enabled.
//!
//! Optional pgvector binaries directory before server start: `PLASM_PGVECTOR_EXTENSION_DIR`.
//!
//! [`pg-embed`]: https://crates.io/crates/pg-embed

#[cfg(feature = "embedded_postgres")]
use std::path::PathBuf;
#[cfg(feature = "embedded_postgres")]
use std::time::Duration;

#[cfg(feature = "embedded_postgres")]
use pg_embed::pg_enums::PgAuthMethod;
#[cfg(feature = "embedded_postgres")]
use pg_embed::pg_fetch::{PgFetchSettings, PG_V15};
#[cfg(feature = "embedded_postgres")]
use pg_embed::postgres::{PgEmbed, PgSettings};
#[cfg(feature = "embedded_postgres")]
use tracing::info;

/// Owns an embedded PostgreSQL process started from environment configuration.
pub struct EmbeddedPostgresGuard {
    #[cfg(feature = "embedded_postgres")]
    pg: PgEmbed,
}

#[cfg(feature = "embedded_postgres")]
fn embedded_enabled_from_env() -> bool {
    match std::env::var("PLASM_EMBEDDED_POSTGRES") {
        Ok(s) => {
            matches!(
                s.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        }
        Err(_) => false,
    }
}

#[cfg(feature = "embedded_postgres")]
fn persistent_from_env() -> bool {
    match std::env::var("PLASM_EMBEDDED_POSTGRES_PERSISTENT") {
        Ok(s) => {
            !matches!(
                s.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        }
        Err(_) => true,
    }
}

#[cfg(feature = "embedded_postgres")]
fn timeout_from_env() -> Option<Duration> {
    let secs: u64 = std::env::var("PLASM_EMBEDDED_POSTGRES_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(120);
    Some(Duration::from_secs(secs))
}

#[cfg(feature = "embedded_postgres")]
fn read_password_from_env() -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(p) = std::env::var("PLASM_EMBEDDED_POSTGRES_PASSWORD") {
        let t = p.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    if let Ok(path) = std::env::var("PLASM_EMBEDDED_POSTGRES_PASSWORD_FILE") {
        let raw = std::fs::read_to_string(path.trim())?;
        let t = raw.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    Ok(String::new())
}

#[cfg(feature = "embedded_postgres")]
fn database_dir_from_env() -> Result<PathBuf, Box<dyn std::error::Error>> {
    for key in ["PLASM_EMBEDDED_POSTGRES_DATA_DIR", "PGDATA"] {
        if let Ok(p) = std::env::var(key) {
            let t = p.trim();
            if !t.is_empty() {
                return Ok(PathBuf::from(t));
            }
        }
    }
    Err(
        "embedded postgres: set PLASM_EMBEDDED_POSTGRES_DATA_DIR or PGDATA to a writable data directory"
            .into(),
    )
}

#[cfg(feature = "embedded_postgres")]
fn build_postgresql_url(
    user: &str,
    password: &str,
    port: u16,
    database: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut u = url::Url::parse("postgresql://127.0.0.1").map_err(|e| e.to_string())?;
    u.set_port(Some(port)).map_err(|_| "invalid port")?;
    u.set_username(user).map_err(|_| "invalid username")?;
    if password.is_empty() {
        u.set_password(None).map_err(|_| "invalid password")?;
    } else {
        u.set_password(Some(password))
            .map_err(|_| "invalid password")?;
    }
    u.set_path(&format!("/{database}"));
    Ok(u.to_string())
}

#[cfg(feature = "embedded_postgres")]
fn parse_database_url() -> Result<(String, String, u16, String), Box<dyn std::error::Error>>
{
    let raw = std::env::var("DATABASE_URL").map_err(|_| {
        "embedded postgres: DATABASE_URL is unset; set it or use PLASM_EMBEDDED_POSTGRES_* fallbacks"
    })?;
    let u = url::Url::parse(raw.trim()).map_err(|e| e.to_string())?;
    let scheme = u.scheme();
    if scheme != "postgres" && scheme != "postgresql" {
        return Err("embedded postgres: DATABASE_URL must use postgres:// or postgresql://".into());
    }
    let host = u
        .host_str()
        .ok_or("embedded postgres: DATABASE_URL must include a host")?;
    if host != "localhost" && host != "127.0.0.1" {
        return Err(format!(
            "embedded postgres: host must be localhost or 127.0.0.1 (got {host})"
        )
        .into());
    }
    let user = if u.username().is_empty() {
        "postgres".to_string()
    } else {
        u.username().to_string()
    };
    let password = u.password().unwrap_or("").to_string();
    let port = u.port().unwrap_or(5432);
    let path = u.path().trim_start_matches('/');
    let database = if path.is_empty() {
        return Err("embedded postgres: DATABASE_URL must include a database name in the path".into());
    } else {
        path.to_string()
    };
    Ok((user, password, port, database))
}

#[cfg(feature = "embedded_postgres")]
fn resolve_connection() -> Result<(String, String, u16, String), Box<dyn std::error::Error>> {
    if std::env::var("DATABASE_URL")
        .ok()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
    {
        return parse_database_url();
    }
    let user = std::env::var("PLASM_EMBEDDED_POSTGRES_USER")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "postgres".to_string());
    let password = read_password_from_env()?;
    let port: u16 = std::env::var("PLASM_EMBEDDED_POSTGRES_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5432);
    let database = std::env::var("PLASM_EMBEDDED_POSTGRES_DATABASE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .ok_or(
            "embedded postgres: DATABASE_URL unset — set PLASM_EMBEDDED_POSTGRES_DATABASE (and password file/env)",
        )?;
    Ok((user, password, port, database))
}

impl EmbeddedPostgresGuard {
    /// When the feature is off, always returns `Ok(None)`.
    /// When the feature is on, starts embedded Postgres only if `PLASM_EMBEDDED_POSTGRES` is truthy.
    pub async fn try_start_from_env(
    ) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        #[cfg(not(feature = "embedded_postgres"))]
        {
            let _ = std::env::var("PLASM_EMBEDDED_POSTGRES");
            return Ok(None);
        }

        #[cfg(feature = "embedded_postgres")]
        {
            if !embedded_enabled_from_env() {
                return Ok(None);
            }

            let database_dir = database_dir_from_env()?;
            let (user, password, port, database) = resolve_connection()?;

            let pg_settings = PgSettings {
                database_dir,
                port,
                user: user.clone(),
                password: password.clone(),
                auth_method: PgAuthMethod::Plain,
                persistent: persistent_from_env(),
                timeout: timeout_from_env(),
                migration_dir: None,
            };

            let fetch_settings = PgFetchSettings {
                version: PG_V15,
                ..Default::default()
            };

            let mut pg = PgEmbed::new(pg_settings, fetch_settings)
                .await
                .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;

            info!(
                port = port,
                database = %database,
                "embedded postgres: downloading or reusing PostgreSQL 15 binaries (pg-embed)"
            );

            pg.setup()
                .await
                .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;

            if let Ok(dir) = std::env::var("PLASM_PGVECTOR_EXTENSION_DIR") {
                let p = dir.trim();
                if !p.is_empty() {
                    let ext_path = PathBuf::from(p);
                    info!(path = %ext_path.display(), "embedded postgres: installing pgvector extension files");
                    pg.install_extension(&ext_path)
                        .await
                        .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;
                }
            }

            pg.start_db()
                .await
                .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;

            if !pg
                .database_exists(&database)
                .await
                .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?
            {
                pg.create_database(&database)
                    .await
                    .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;
            }

            let url = build_postgresql_url(&user, &password, port, &database)?;
            if std::env::var("DATABASE_URL")
                .ok()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
            {
                std::env::set_var("DATABASE_URL", &url);
            }
            if std::env::var("PLASM_AUTH_STORAGE_URL")
                .ok()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
            {
                std::env::set_var("PLASM_AUTH_STORAGE_URL", &url);
            }

            info!("embedded postgres: server ready");
            Ok(Some(Self { pg }))
        }
    }

    pub async fn shutdown(self) -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(not(feature = "embedded_postgres"))]
        {
            return Ok(());
        }
        #[cfg(feature = "embedded_postgres")]
        {
            let mut pg = self.pg;
            pg.stop_db()
                .await
                .map_err(|e| -> Box<dyn std::error::Error> { e.to_string().into() })?;
            Ok(())
        }
    }
}
