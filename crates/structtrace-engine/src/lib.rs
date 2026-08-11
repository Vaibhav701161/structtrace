//! Durable orchestration for matched StructTrace runs.

#![forbid(unsafe_code)]

/// Adapter-aware run orchestration.
pub mod configured;
mod process_logs;
/// Complete recorded-output comparison workflow.
pub mod recorded;
/// Artifact hash verification and complete score recomputation.
pub mod replay;
/// SQLite storage and portable artifact finalization.
pub mod storage;

pub use configured::{resume_configured, run_configured};
pub use recorded::{
    CompletedRun, RunProgress, run_recorded, run_recorded_observed, run_recorded_with_config,
    run_recorded_with_config_kind,
};
pub use replay::{ReplayReport, replay_run};
