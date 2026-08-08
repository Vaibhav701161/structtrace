//! Interoperability adapters for StructTrace variants and evaluators.

#![forbid(unsafe_code)]

pub mod command;
pub mod evaluator;
pub mod openai;
pub mod protocol;
pub mod python;

/// Stable variant protocol identifier.
pub const VARIANT_PROTOCOL: &str = "structtrace.variant";
