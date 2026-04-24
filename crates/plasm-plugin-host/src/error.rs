use thiserror::Error;

#[derive(Error, Debug)]
pub enum PluginLoadError {
    #[error("failed to load dynamic library: {0}")]
    LibLoading(#[from] libloading::Error),

    #[error("ABI version mismatch: expected {expected}, got {got}")]
    AbiMismatch { expected: u32, got: u32 },

    #[error("missing plugin export `{0}`")]
    MissingExport(&'static str),

    #[error("plugin catalog metadata: {0}")]
    CatalogMetadata(String),
}
