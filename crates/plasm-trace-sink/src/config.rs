//! Environment-driven configuration (dev-friendly defaults).

use std::path::{Path, PathBuf};

use anyhow::Context;

/// Validated non-empty SqlCatalog JDBC URL (**Postgres only**; Iceberg metadata does not use SQLite).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogConnectionString(String);

impl CatalogConnectionString {
    fn new(raw: String) -> anyhow::Result<Self> {
        if raw.trim().is_empty() {
            anyhow::bail!("catalog connection URL must be non-empty");
        }
        let t = raw.trim();
        let lower = t.to_ascii_lowercase();
        if lower.starts_with("sqlite:") {
            anyhow::bail!(
                "PLASM_TRACE_SINK_CATALOG_URL cannot use sqlite: — Iceberg SqlCatalog metadata requires Postgres (postgresql:// or postgres://)"
            );
        }
        if !(lower.starts_with("postgres://") || lower.starts_with("postgresql://")) {
            anyhow::bail!(
                "PLASM_TRACE_SINK_CATALOG_URL must be postgres:// or postgresql:// (got non-Postgres URL)"
            );
        }
        Ok(Self(raw))
    }

    /// Resolve from `PLASM_TRACE_SINK_CATALOG_URL` (`explicit`); **no default** sqlite file.
    pub fn resolve(_data_dir: &Path, explicit: Option<&str>) -> anyhow::Result<Self> {
        let Some(u) = explicit.map(str::trim).filter(|s| !s.is_empty()) else {
            anyhow::bail!(
                "PLASM_TRACE_SINK_CATALOG_URL is required: Iceberg SqlCatalog metadata uses Postgres only (set a postgresql:// or postgres:// JDBC URL)"
            );
        };
        Self::new(u.to_string())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Userinfo redaction for Postgres URLs (safe for structured logs).
    #[must_use]
    pub fn redacted_for_logs(&self) -> String {
        redact_jdbc_userinfo(&self.0)
    }
}

fn redact_jdbc_userinfo(url: &str) -> String {
    if let Some(at) = url.find('@') {
        if url.starts_with("postgresql://") || url.starts_with("postgres://") {
            return format!("postgresql://***:***{}", &url[at..]);
        }
    }
    url.to_string()
}

/// Validated Iceberg warehouse base for S3-compatible object storage (`s3://bucket` or `s3://bucket/prefix`).
///
/// Construct via [`S3WarehouseUri::parse`]. Used when resolving [`WarehouseLocation::S3`].
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct S3WarehouseUri(String);

impl S3WarehouseUri {
    /// Normalize a raw URI to stable `s3://…` (accepts `s3a://` input).
    ///
    /// Errors are **not** tied to a specific environment variable; callers attach context
    /// (e.g. `PLASM_TRACE_SINK_WAREHOUSE_URL`) when appropriate.
    pub fn parse(raw: &str) -> anyhow::Result<Self> {
        let s = raw.trim();
        let host_and_path = s
            .strip_prefix("s3://")
            .or_else(|| s.strip_prefix("s3a://"))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "S3 warehouse URI must start with s3:// or s3a:// (got {:?})",
                    s.chars().take(32).collect::<String>()
                )
            })?;
        let host_and_path = host_and_path.trim_end_matches('/');
        if host_and_path.is_empty() {
            anyhow::bail!("S3 warehouse URI must include a bucket name");
        }
        Ok(Self(format!("s3://{host_and_path}")))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Iceberg warehouse: local Parquet files or S3-compatible object storage (`s3://` / `s3a://`).
///
/// S3 mode uses [`iceberg_rust::object_store::ObjectStoreBuilder::s3`] (`AmazonS3Builder::from_env`).
/// Set `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, and for Vultr (and similar) `AWS_ENDPOINT_URL`
/// / `AWS_REGION` per [`object_store`](https://docs.rs/object_store) S3 configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WarehouseLocation {
    Filesystem(PathBuf),
    S3 { base_url: String },
}

impl WarehouseLocation {
    #[must_use]
    pub fn summary_for_logs(&self) -> String {
        match self {
            Self::Filesystem(p) => p.display().to_string(),
            Self::S3 { base_url } => base_url.clone(),
        }
    }
}

/// Lawful bundle for [`crate::iceberg_writer::IcebergSink::connect`]: catalog + warehouse.
///
/// Build via [`Config::iceberg_connect_params`] or construct manually in tests, then pass
/// `&params` to [`IcebergSink::connect`](crate::iceberg_writer::IcebergSink::connect).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IcebergConnectParams {
    pub catalog: CatalogConnectionString,
    pub warehouse: WarehouseLocation,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub listen: String,
    pub data_dir: PathBuf,
    /// Postgres SqlCatalog JDBC URL (`PLASM_TRACE_SINK_CATALOG_URL`); required for startup.
    pub catalog_url: Option<String>,
    /// Local Parquet root when **no** S3 warehouse URL is set ([`Self::warehouse_s3_url`] empty).
    ///
    /// **Precedence:** If `warehouse_s3_url` is non-empty after trim, [`IcebergConnectParams`]
    /// uses [`WarehouseLocation::S3`] and **`warehouse_fs_path` is ignored** for Iceberg data files
    /// (no warning). Otherwise [`WarehouseLocation::Filesystem`] uses this path.
    pub warehouse_fs_path: PathBuf,
    /// When non-empty, Iceberg Parquet uses S3-compatible storage ([`WarehouseLocation::S3`]).
    ///
    /// Takes precedence over [`Self::warehouse_fs_path`]. Set `PLASM_TRACE_SINK_WAREHOUSE_URL`
    /// at runtime; see [`S3WarehouseUri::parse`] for accepted forms.
    pub warehouse_s3_url: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        let data_dir = std::env::var("PLASM_TRACE_SINK_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("var/plasm-trace-sink"));
        let warehouse_fs_path = std::env::var("PLASM_TRACE_SINK_WAREHOUSE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| data_dir.join("iceberg_warehouse"));
        let warehouse_s3_url = std::env::var("PLASM_TRACE_SINK_WAREHOUSE_URL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let catalog_url = std::env::var("PLASM_TRACE_SINK_CATALOG_URL")
            .ok()
            .filter(|s| !s.trim().is_empty());
        Self {
            listen: std::env::var("PLASM_TRACE_SINK_LISTEN")
                .unwrap_or_else(|_| "127.0.0.1:7070".to_string()),
            data_dir,
            catalog_url,
            warehouse_fs_path,
            warehouse_s3_url,
        }
    }

    /// Resolved catalog URL + warehouse for Iceberg startup.
    pub fn iceberg_connect_params(&self) -> anyhow::Result<IcebergConnectParams> {
        let warehouse = if let Some(raw) = &self.warehouse_s3_url {
            let uri = S3WarehouseUri::parse(raw).with_context(|| {
                format!(
                    "invalid PLASM_TRACE_SINK_WAREHOUSE_URL (filesystem path PLASM_TRACE_SINK_WAREHOUSE_PATH={} ignored for Iceberg data files while this URL is set)",
                    self.warehouse_fs_path.display()
                )
            })?;
            WarehouseLocation::S3 {
                base_url: uri.as_str().to_string(),
            }
        } else {
            WarehouseLocation::Filesystem(self.warehouse_fs_path.clone())
        };
        Ok(IcebergConnectParams {
            catalog: CatalogConnectionString::resolve(&self.data_dir, self.catalog_url.as_deref())?,
            warehouse,
        })
    }

    /// Resolved SqlCatalog JDBC URL string (same as [`IcebergConnectParams::catalog`]).
    ///
    /// Returns an error if [`Self::iceberg_connect_params`] fails (e.g. missing catalog URL, invalid warehouse URL).
    pub fn resolved_catalog_url(&self) -> anyhow::Result<String> {
        Ok(self.iceberg_connect_params()?.catalog.as_str().to_string())
    }

    /// Rejects deprecated in-memory-only mode (`PLASM_TRACE_SINK_ICEBERG=0`).
    pub fn ensure_iceberg_not_disabled() -> anyhow::Result<()> {
        match std::env::var("PLASM_TRACE_SINK_ICEBERG") {
            Ok(v)
                if v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("no") =>
            {
                anyhow::bail!(
                    "PLASM_TRACE_SINK_ICEBERG={v} is no longer supported; plasm-trace-sink requires Iceberg (no in-memory mode)."
                );
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{CatalogConnectionString, S3WarehouseUri};

    #[test]
    fn catalog_requires_explicit_postgres_url() {
        assert!(CatalogConnectionString::resolve(Path::new("."), None).is_err());
    }

    #[test]
    fn catalog_rejects_sqlite_url() {
        assert!(CatalogConnectionString::resolve(Path::new("."), Some("sqlite://x/y.db")).is_err());
    }

    #[test]
    fn s3_warehouse_uri_accepts_bucket_only() {
        assert_eq!(
            S3WarehouseUri::parse("s3://my-bucket").unwrap().as_str(),
            "s3://my-bucket"
        );
    }

    #[test]
    fn s3_warehouse_uri_normalizes_prefix_and_scheme() {
        assert_eq!(
            S3WarehouseUri::parse("s3a://b/prefix//").unwrap().as_str(),
            "s3://b/prefix"
        );
    }

    #[test]
    fn s3_warehouse_uri_rejects_non_s3() {
        assert!(S3WarehouseUri::parse("https://x").is_err());
    }
}
