use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum CmlError {
    #[error("Variable '{name}' not found in environment")]
    VariableNotFound { name: String },

    #[error("CML type error: {message}")]
    TypeError { message: String },

    #[error("CML evaluation error: {message}")]
    EvaluationError { message: String },

    #[error("Invalid path expression: {message}")]
    InvalidPath { message: String },

    #[error("Serialization error: {message}")]
    SerializationError { message: String },

    #[error("Invalid template: {message}")]
    InvalidTemplate { message: String },
}
