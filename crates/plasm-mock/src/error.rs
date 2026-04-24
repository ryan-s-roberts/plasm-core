use thiserror::Error;

#[derive(Error, Debug)]
pub enum MockError {
    #[error("Entity '{entity}' not found")]
    EntityNotFound { entity: String },

    #[error("Resource with ID '{id}' not found in entity '{entity}'")]
    ResourceNotFound { entity: String, id: String },

    #[error("Filter error: {message}")]
    FilterError { message: String },

    #[error("Invalid request: {message}")]
    InvalidRequest { message: String },

    #[error("Serialization error: {message}")]
    SerializationError { message: String },

    #[error("Configuration error: {message}")]
    ConfigurationError { message: String },
}

impl From<serde_json::Error> for MockError {
    fn from(err: serde_json::Error) -> Self {
        MockError::SerializationError {
            message: err.to_string(),
        }
    }
}

impl From<plasm_core::TypeError> for MockError {
    fn from(err: plasm_core::TypeError) -> Self {
        MockError::FilterError {
            message: err.to_string(),
        }
    }
}

impl From<plasm_compile::CompileError> for MockError {
    fn from(err: plasm_compile::CompileError) -> Self {
        MockError::FilterError {
            message: err.to_string(),
        }
    }
}
