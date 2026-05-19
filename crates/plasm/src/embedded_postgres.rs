//! Embedded PostgreSQL via [`pg-embed`] when the crate feature `embedded_postgres` is enabled.
//!
//! **Default (feature enabled):** starts an isolated Postgres **unless** you opt out or already point
//! Plasm at an external database.
//!
//! - **Opt out:** `PLASM_EMBEDDED_POSTGRES=0` (also `false`, `no`, `off`).
//! - **External DB:** any of `DATABASE_URL`, `PLASM_MCP_CONFIG_DATABASE_URL`, or
//!   `PLASM_AUTH_STORAGE_URL` set to a **postgres:** URL whose host is **not** `localhost` /
//!   `127.0.0.1`, or with **no TCP host** (e.g. Unix socket) — embedded autostart is skipped so we do
//!   not overwrite your URLs.
//!
//! **Connection:** prefers `DATABASE_URL` (`postgresql://…` on loopback). If unset, uses
//! `PLASM_EMBEDDED_POSTGRES_USER` (default `postgres`), optional password env/file,
//! `PLASM_EMBEDDED_POSTGRES_PORT` (optional fixed port; otherwise an ephemeral loopback port), and
//! `PLASM_EMBEDDED_POSTGRES_DATABASE` (default [`DEFAULT_EMBEDDED_PG_DATABASE`]).
//!
//! **Timeouts:** `PLASM_EMBEDDED_POSTGRES_TIMEOUT_SECS` caps pg-embed `initdb` / `pg_ctl` waits
//! (default **240** seconds — first-time binary download + init can be slow on cold caches).
//!
//! **Cross-process setup lock (Unix):** while downloading/unpacking PostgreSQL binaries into the
//! shared OS cache directory, Plasm takes an exclusive `flock(2)` on `pg-embed-setup.flock` next to
//! the embedded data cache so concurrent `plasm-server` / PTY test processes cannot corrupt
//! the extracted `bin/` tree (pg-embed’s in-process mutex is not enough across processes).
//!
//! **Superuser password:** pg-embed runs `initdb --pwfile`, which **rejects an empty file**.
//! When no password is set (env unset or empty URL segment), Plasm uses
//! [`DEFAULT_EMBEDDED_SUPERUSER_PASSWORD`] — local appliance only; override via env/file.
//!
//! **Data directory:** `PLASM_EMBEDDED_POSTGRES_DATA_DIR`, else `PGDATA`, else a OS cache path under
//! `plasm/embedded-postgres` (created if missing). For **`plasm-server`**, prefer `--data-dir DIR`
//! (sets `{DIR}/postgres` when this env is unset) so logs and other files can live under `DIR`
//! without colliding with PGDATA; pg-embed **reuses** an existing cluster in that directory.
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

#[cfg(all(unix, feature = "embedded_postgres"))]
/// Serialize pg-embed **binary download / unpack** across processes. The upstream crate only
/// coordinates concurrent acquisition inside a single process; multiple `plasm-server`
/// instances (or PTY tests) sharing the same OS cache dir could corrupt the extracted `bin/`
/// tree and surface `PostgreSQL could not be started` from `pg_ctl`.
struct PgEmbedSetupExclusiveLock {
    file: std::fs::File,
}

#[cfg(all(unix, feature = "embedded_postgres"))]
impl PgEmbedSetupExclusiveLock {
    async fn acquire() -> Result<Self, Box<dyn std::error::Error>> {
        let file = tokio::task::spawn_blocking(Self::acquire_blocking)
            .await
            .map_err(|e| -> Box<dyn std::error::Error> {
                format!("pg_embed flock join: {e}").into()
            })?
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
        Ok(Self { file })
    }

    fn acquire_blocking() -> Result<std::fs::File, String> {
        use std::fs::OpenOptions;
        use std::os::unix::io::AsRawFd;

        let path = pg_embed_setup_flock_path().map_err(|e| e.to_string())?;
        let f = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| format!("open pg_embed flock {}: {e}", path.display()))?;

        let fd = f.as_raw_fd();
        let rc = unsafe { libc::flock(fd, libc::LOCK_EX) };
        if rc != 0 {
            return Err(format!(
                "flock LOCK_EX on {}: {}",
                path.display(),
                std::io::Error::last_os_error()
            ));
        }

        Ok(f)
    }
}

#[cfg(all(unix, feature = "embedded_postgres"))]
impl Drop for PgEmbedSetupExclusiveLock {
    fn drop(&mut self) {
        use std::os::unix::io::AsRawFd;
        let fd = self.file.as_raw_fd();
        unsafe {
            let _ = libc::flock(fd, libc::LOCK_UN);
        }
    }
}

#[cfg(all(unix, feature = "embedded_postgres"))]
fn pg_embed_setup_flock_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    match default_embedded_data_dir() {
        Ok(embedded) => {
            let Some(parent) = embedded.parent() else {
                return Err(
                    "embedded postgres: internal error (embedded data path has no parent)".into(),
                );
            };
            std::fs::create_dir_all(parent)?;
            Ok(parent.join("pg-embed-setup.flock"))
        }
        Err(_) => {
            let p = std::env::temp_dir().join("plasm");
            std::fs::create_dir_all(&p)?;
            Ok(p.join("pg-embed-setup.flock"))
        }
    }
}

/// Default embedded listener port (avoids colliding with a system Postgres on 5432).
#[cfg(feature = "embedded_postgres")]
pub const DEFAULT_EMBEDDED_PG_PORT: u16 = 55_432;

/// Default database created inside the embedded cluster when `DATABASE_URL` is unset.
#[cfg(feature = "embedded_postgres")]
pub const DEFAULT_EMBEDDED_PG_DATABASE: &str = "plasm_appliance";

/// Default superuser password when env/file and URL password are empty (pg-embed `initdb` requires non-empty `--pwfile`).
#[cfg(feature = "embedded_postgres")]
pub const DEFAULT_EMBEDDED_SUPERUSER_PASSWORD: &str = "plasm_embedded_local_dev";

/// Owns an embedded PostgreSQL process started from environment configuration.
pub struct EmbeddedPostgresGuard {
    #[cfg(feature = "embedded_postgres")]
    pg: PgEmbed,
}

#[cfg(feature = "embedded_postgres")]
fn embedded_autostart_gate_open() -> bool {
    !explicit_embedded_opt_out() && !postgres_env_urls_skip_embedded_autostart()
}

#[cfg(feature = "embedded_postgres")]
fn explicit_embedded_opt_out() -> bool {
    match std::env::var("PLASM_EMBEDDED_POSTGRES") {
        Ok(s) => matches!(
            s.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        ),
        Err(_) => false,
    }
}

#[cfg(feature = "embedded_postgres")]
fn postgres_env_urls_skip_embedded_autostart() -> bool {
    for key in [
        "DATABASE_URL",
        "PLASM_MCP_CONFIG_DATABASE_URL",
        "PLASM_AUTH_STORAGE_URL",
    ] {
        let Ok(raw) = std::env::var(key) else {
            continue;
        };
        let s = raw.trim();
        if s.is_empty() {
            continue;
        }
        let Ok(u) = url::Url::parse(s) else {
            continue;
        };
        let scheme = u.scheme();
        if scheme != "postgres" && scheme != "postgresql" {
            continue;
        }
        match u.host_str() {
            Some("localhost" | "127.0.0.1" | "::1") => continue,
            Some(_) => return true,
            None => return true,
        }
    }
    false
}

#[cfg(feature = "embedded_postgres")]
fn persistent_from_env() -> bool {
    match std::env::var("PLASM_EMBEDDED_POSTGRES_PERSISTENT") {
        Ok(s) => !matches!(
            s.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "no" | "off"
        ),
        Err(_) => true,
    }
}

#[cfg(feature = "embedded_postgres")]
fn timeout_from_env() -> Option<Duration> {
    let secs: u64 = std::env::var("PLASM_EMBEDDED_POSTGRES_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(240);
    Some(Duration::from_secs(secs))
}

#[cfg(feature = "embedded_postgres")]
fn embedded_superuser_password_for_pg_embed(password: String) -> String {
    if password.trim().is_empty() {
        DEFAULT_EMBEDDED_SUPERUSER_PASSWORD.to_string()
    } else {
        password
    }
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
fn default_embedded_data_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let p = PathBuf::from(home.trim())
                .join("Library")
                .join("Caches")
                .join("plasm")
                .join("embedded-postgres");
            return Ok(p);
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
            let t = xdg.trim();
            if !t.is_empty() {
                return Ok(PathBuf::from(t).join("plasm").join("embedded-postgres"));
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            return Ok(PathBuf::from(home.trim())
                .join(".cache")
                .join("plasm")
                .join("embedded-postgres"));
        }
    }
    #[cfg(windows)]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let t = local.trim();
            if !t.is_empty() {
                return Ok(PathBuf::from(t).join("plasm").join("embedded-postgres"));
            }
        }
    }
    Err(
        "embedded postgres: set PLASM_EMBEDDED_POSTGRES_DATA_DIR or PGDATA (HOME unset — cannot pick a cache dir)"
            .into(),
    )
}

#[cfg(feature = "embedded_postgres")]
fn pick_free_loopback_tcp_port() -> Result<u16, Box<dyn std::error::Error>> {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

/// Listener port for pg-embed: explicit `PLASM_EMBEDDED_POSTGRES_PORT`, else an ephemeral port.
#[cfg(feature = "embedded_postgres")]
fn embedded_listener_port(explicit: Option<u16>) -> Result<u16, Box<dyn std::error::Error>> {
    match explicit {
        Some(p) => Ok(p),
        None => pick_free_loopback_tcp_port(),
    }
}

/// When reusing a data directory, align `postgresql.conf` `port` with the chosen listener.
#[cfg(feature = "embedded_postgres")]
fn sync_postgresql_conf_port(
    database_dir: &std::path::Path,
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let conf_path = database_dir.join("postgresql.conf");
    if !conf_path.is_file() {
        return Ok(());
    }
    let content = std::fs::read_to_string(&conf_path)?;
    let port_line = format!("port = {port}");
    let mut replaced = false;
    let mut out = String::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('#') && trimmed.starts_with("port") && trimmed.contains('=') {
            out.push_str(&port_line);
            replaced = true;
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    if !replaced {
        out.push_str(&port_line);
        out.push('\n');
    }
    std::fs::write(conf_path, out)?;
    Ok(())
}

#[cfg(feature = "embedded_postgres")]
fn database_dir_from_env() -> Result<PathBuf, Box<dyn std::error::Error>> {
    for key in ["PLASM_EMBEDDED_POSTGRES_DATA_DIR", "PGDATA"] {
        if let Ok(p) = std::env::var(key) {
            let t = p.trim();
            if !t.is_empty() {
                let pb = PathBuf::from(t);
                if let Some(parent) = pb.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::create_dir_all(&pb)?;
                return Ok(pb);
            }
        }
    }
    let dir = default_embedded_data_dir()?;
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
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
fn parse_database_url() -> Result<(String, String, u16, String), Box<dyn std::error::Error>> {
    let raw = std::env::var("DATABASE_URL").map_err(|_| {
        "embedded postgres: DATABASE_URL is unset; use PLASM_EMBEDDED_POSTGRES_* fallbacks"
    })?;
    let u = url::Url::parse(raw.trim()).map_err(|e| e.to_string())?;
    let scheme = u.scheme();
    if scheme != "postgres" && scheme != "postgresql" {
        return Err("embedded postgres: DATABASE_URL must use postgres:// or postgresql://".into());
    }
    let host = u.host_str().ok_or(
        "embedded postgres: DATABASE_URL must use a TCP host (localhost or 127.0.0.1) for pg-embed",
    )?;
    if host != "localhost" && host != "127.0.0.1" && host != "::1" {
        return Err(format!(
            "embedded postgres: host must be localhost, 127.0.0.1, or ::1 (got {host})"
        )
        .into());
    }
    let user = if u.username().is_empty() {
        "postgres".to_string()
    } else {
        u.username().to_string()
    };
    let password = u.password().unwrap_or("").to_string();
    let port = match u.port() {
        Some(p) => p,
        None => pick_free_loopback_tcp_port()?,
    };
    let path = u.path().trim_start_matches('/');
    let database = if path.is_empty() {
        return Err(
            "embedded postgres: DATABASE_URL must include a database name in the path".into(),
        );
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
    let explicit_port = std::env::var("PLASM_EMBEDDED_POSTGRES_PORT")
        .ok()
        .and_then(|s| s.parse().ok());
    let port = embedded_listener_port(explicit_port)?;
    let database = std::env::var("PLASM_EMBEDDED_POSTGRES_DATABASE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_EMBEDDED_PG_DATABASE.to_string());
    Ok((user, password, port, database))
}

impl EmbeddedPostgresGuard {
    /// Whether this binary will try to start embedded Postgres (`embedded_postgres` feature off → always false).
    pub fn will_autostart_embedded_postgres() -> bool {
        #[cfg(not(feature = "embedded_postgres"))]
        {
            false
        }
        #[cfg(feature = "embedded_postgres")]
        {
            embedded_autostart_gate_open()
        }
    }

    /// Human-readable reason embedded autostart was skipped (feature disabled, opt-out, or external URLs).
    pub fn embedded_autostart_skip_reason() -> Option<&'static str> {
        #[cfg(not(feature = "embedded_postgres"))]
        {
            Some("built without embedded_postgres (e.g. `cargo build -p plasm-server --no-default-features`)")
        }
        #[cfg(feature = "embedded_postgres")]
        {
            if explicit_embedded_opt_out() {
                Some("PLASM_EMBEDDED_POSTGRES=0 disables embedded Postgres")
            } else if postgres_env_urls_skip_embedded_autostart() {
                Some("Postgres URL(s) already set for non-loopback or non-TCP host")
            } else {
                None
            }
        }
    }

    /// When the feature is off, always returns `Ok(None)`.
    /// When the feature is on, starts embedded Postgres when [`Self::will_autostart_embedded_postgres`]
    /// is true; otherwise returns `Ok(None)` without error.
    pub async fn try_start_from_env() -> Result<Option<Self>, Box<dyn std::error::Error>> {
        #[cfg(not(feature = "embedded_postgres"))]
        {
            Ok(None)
        }

        #[cfg(feature = "embedded_postgres")]
        {
            if !embedded_autostart_gate_open() {
                return Ok(None);
            }

            #[cfg(unix)]
            let _pg_embed_setup_lock = PgEmbedSetupExclusiveLock::acquire().await?;

            let database_dir = database_dir_from_env()?;
            let (user, password, port, database) = resolve_connection()?;
            std::env::set_var("PLASM_EMBEDDED_POSTGRES_PORT", port.to_string());
            sync_postgresql_conf_port(&database_dir, port)?;
            let password = embedded_superuser_password_for_pg_embed(password);

            info!(port, "embedded postgres: listener port selected");

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
            Ok(())
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
