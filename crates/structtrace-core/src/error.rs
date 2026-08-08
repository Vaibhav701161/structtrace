//! Typed failures surfaced by the core library.

use std::path::PathBuf;

/// Result alias used throughout the core library.
pub type Result<T> = std::result::Result<T, CoreError>;

/// User-actionable core failures.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// A required file could not be read.
    #[error("could not read {path}: {source}")]
    Read {
        /// File that failed.
        path: PathBuf,
        /// Operating-system failure.
        source: std::io::Error,
    },
    /// A file could not be written.
    #[error("could not write {path}: {source}")]
    Write {
        /// File that failed.
        path: PathBuf,
        /// Operating-system failure.
        source: std::io::Error,
    },
    /// Configuration is malformed or internally inconsistent.
    #[error("invalid configuration: {0}")]
    Configuration(String),
    /// A dataset row is malformed.
    #[error("invalid dataset at line {line}: {message}")]
    Dataset {
        /// One-based line number.
        line: usize,
        /// Specific validation failure.
        message: String,
    },
    /// A recorded-output row is malformed.
    #[error("invalid recorded output at line {line}: {message}")]
    RecordedOutput {
        /// One-based line number.
        line: usize,
        /// Specific validation failure.
        message: String,
    },
    /// JSON Schema compilation failed before execution.
    #[error("JSON Schema compilation failed: {0}")]
    Schema(String),
    /// An evaluator is invalid or cannot be applied.
    #[error("evaluator `{evaluator_id}` failed: {message}")]
    Evaluator {
        /// Configured evaluator ID.
        evaluator_id: String,
        /// Specific failure.
        message: String,
    },
    /// Statistical settings or inputs are invalid.
    #[error("invalid statistical input: {0}")]
    Statistics(String),
    /// Artifact integrity or compatibility failed.
    #[error("artifact integrity failure: {0}")]
    Artifact(String),
}

/// Construct a read failure while retaining the path.
pub fn read_error(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> CoreError {
    let path = path.into();
    move |source| CoreError::Read { path, source }
}
