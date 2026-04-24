use thiserror::Error;

use crate::bootstrap_secrets::BootstrapSecretsError;

#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Schema error: {0}")]
    Schema(String),

    #[error("No entity '{0}' in schema")]
    EntityNotFound(String),

    #[error("No capability '{kind}' for entity '{entity}'")]
    CapabilityNotFound { entity: String, kind: String },

    #[error("Argument error: {0}")]
    Argument(String),

    #[error("Execution error: {0}")]
    Execution(#[from] plasm_runtime::RuntimeError),

    #[error("Compilation error: {0}")]
    Compilation(#[from] plasm_compile::CompileError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Bootstrap(#[from] BootstrapSecretsError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
