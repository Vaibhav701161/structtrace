//! Stable domain model and deterministic evaluation semantics for StructTrace.

#![forbid(unsafe_code)]

pub mod artifact;
pub mod config;
pub mod dataset;
pub mod error;
pub mod evaluation;
pub mod gate;
pub mod hashing;
pub mod inspection;
pub mod output;
/// Privacy and deterministic report redaction.
pub mod privacy;
pub mod statistics;

pub use error::{CoreError, Result};

/// Current on-disk artifact format.
pub const ARTIFACT_FORMAT_VERSION: u32 = 3;

/// Current command and evaluator JSONL protocol.
pub const PROTOCOL_VERSION: u32 = 1;
