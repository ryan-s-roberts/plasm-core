//! Plasm trace sink: HTTP ingest backed by Iceberg (Parquet + SqlCatalog).

pub mod append_port;
pub mod config;
pub mod http;
pub mod iceberg_writer;
mod metrics;
pub mod model;
pub mod persisted;
pub(crate) mod projection;
pub mod projector;
mod spans;
pub mod state;
mod trace_totals;

pub use append_port::{AuditSpanReader, AuditSpanStore, AuditSpanWriter};
pub use config::{
    CatalogConnectionString, Config, IcebergConnectParams, S3WarehouseUri, WarehouseLocation,
};
pub use model::{BillingUsageResponse, TraceGetResponse};
pub use persisted::PersistedTraceSink;
pub use state::AppState;
