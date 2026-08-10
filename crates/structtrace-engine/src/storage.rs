//! Versioned SQLite run storage.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use serde::Serialize;
use structtrace_core::{
    artifact::{RunKind, RunStatus},
    dataset::Case,
    evaluation::CaseEvaluation,
    output::VariantOutput,
    statistics::PairedMetrics,
};

/// Current SQLite schema migration version.
pub const DATABASE_VERSION: i64 = 5;

/// Durable local run store.
pub struct RunStore {
    run_dir: PathBuf,
    connection: Connection,
}

/// Marks an allocated run failed when an ordinary error unwinds the operation.
pub(crate) struct FailureStatusGuard<'a> {
    store: &'a RunStore,
    run_id: &'a str,
    armed: bool,
}

impl FailureStatusGuard<'_> {
    /// Preserve the caller's final state after every fallible finalization step succeeded.
    pub(crate) fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for FailureStatusGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.store.record_event(
                "run_failed",
                &serde_json::json!({"reason": "operation returned an error"}),
            );
            let _ = self.store.set_status(self.run_id, RunStatus::Failed);
        }
    }
}

impl RunStore {
    /// Create a new run directory and initialize all versioned tables.
    pub fn create(root: &Path, run_id: &str, run_kind: RunKind) -> anyhow::Result<Self> {
        validate_run_id(run_id)?;
        let run_dir = root.join("runs").join(run_id);
        std::fs::create_dir_all(run_dir.join("logs"))?;
        std::fs::create_dir_all(run_dir.join("report"))?;
        std::fs::create_dir_all(run_dir.join("exports"))?;
        harden_directory_permissions(root)?;
        harden_directory_permissions(&root.join("runs"))?;
        harden_directory_permissions(&run_dir)?;
        harden_directory_permissions(&run_dir.join("logs"))?;
        harden_directory_permissions(&run_dir.join("report"))?;
        harden_directory_permissions(&run_dir.join("exports"))?;
        let connection = Connection::open(run_dir.join("run.sqlite3"))?;
        harden_file_permissions(&run_dir.join("run.sqlite3"))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", true)?;
        migrate(&connection)?;
        connection.execute(
            "INSERT INTO runs (run_id, status, artifact_version, run_kind) VALUES (?1, ?2, 1, ?3)",
            params![
                run_id,
                status_name(RunStatus::Created),
                run_kind_name(run_kind)
            ],
        )?;
        Ok(Self {
            run_dir,
            connection,
        })
    }

    /// Open an existing store after checking migrations.
    pub fn open(run_dir: &Path) -> anyhow::Result<Self> {
        let connection = Connection::open(run_dir.join("run.sqlite3"))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        migrate(&connection)?;
        Ok(Self {
            run_dir: run_dir.to_owned(),
            connection,
        })
    }

    /// Root of portable artifacts for this run.
    pub fn run_dir(&self) -> &Path {
        &self.run_dir
    }

    /// Arm fail-closed lifecycle handling for a fallible run operation.
    pub(crate) fn failure_guard<'a>(&'a self, run_id: &'a str) -> FailureStatusGuard<'a> {
        FailureStatusGuard {
            store: self,
            run_id,
            armed: true,
        }
    }

    /// Update lifecycle state explicitly.
    pub fn set_status(&self, run_id: &str, status: RunStatus) -> anyhow::Result<()> {
        let updated = self.connection.execute(
            "UPDATE runs SET status = ?1 WHERE run_id = ?2",
            params![status_name(status), run_id],
        )?;
        anyhow::ensure!(updated == 1, "run `{run_id}` was not found in its database");
        Ok(())
    }

    /// Current lifecycle state.
    pub fn status(&self, run_id: &str) -> anyhow::Result<RunStatus> {
        let value: String = self.connection.query_row(
            "SELECT status FROM runs WHERE run_id = ?1",
            [run_id],
            |row| row.get(0),
        )?;
        parse_status(&value)
    }

    /// Checkpoint active WAL contents before immutable finalization.
    pub fn checkpoint(&self) -> anyhow::Result<()> {
        self.connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }

    /// Remove derived rows before safely repeating finalization after interruption.
    pub fn reset_for_resume(&self, run_id: &str) -> anyhow::Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        for table in [
            "artifacts",
            "paired_results",
            "outcome_results",
            "evaluator_results",
            "variant_outputs",
            "variants",
            "cases",
        ] {
            transaction.execute(&format!("DELETE FROM {table}"), [])?;
        }
        transaction.execute(
            "UPDATE runs SET status = ?1 WHERE run_id = ?2",
            params![status_name(RunStatus::Validating), run_id],
        )?;
        transaction.execute(
            "INSERT INTO events (event_type, payload_json) VALUES ('resume_finalization', '{}')",
            [],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Persist an immutable case envelope in display order.
    pub fn insert_case(&self, ordinal: usize, case: &Case) -> anyhow::Result<()> {
        self.connection.execute(
            "INSERT INTO cases (ordinal, case_id, input_json, expected_json, model_visible_metadata_json, metadata_json, source_line) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                i64::try_from(ordinal)?,
                case.id,
                serde_json::to_string(&case.input)?,
                optional_json(case.expected.as_ref())?,
                optional_json(case.model_visible_metadata.as_ref())?,
                optional_json(case.metadata.as_ref())?,
                i64::try_from(case.source_line)?,
            ],
        )?;
        Ok(())
    }

    /// Persist a redacted variant definition.
    pub fn insert_variant<T: Serialize>(&self, id: &str, definition: &T) -> anyhow::Result<()> {
        self.connection.execute(
            "INSERT INTO variants (variant_id, definition_json) VALUES (?1, ?2)",
            params![id, serde_json::to_string(definition)?],
        )?;
        Ok(())
    }

    /// Persist the complete adapter envelope for one case.
    pub fn insert_output(&self, variant_id: &str, output: &VariantOutput) -> anyhow::Result<()> {
        self.connection.execute(
            "INSERT INTO variant_outputs (variant_id, case_id, status, envelope_json) VALUES (?1, ?2, ?3, ?4)",
            params![
                variant_id,
                output.case_id,
                output_status_name(output.status),
                serde_json::to_string(output)?,
            ],
        )?;
        Ok(())
    }

    /// Persist evaluator and outcome facts for one scored output.
    pub fn insert_evaluation(
        &self,
        variant_id: &str,
        evaluation: &CaseEvaluation,
    ) -> anyhow::Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        for (evaluator_id, result) in &evaluation.evaluators {
            transaction.execute(
                "INSERT INTO evaluator_results (variant_id, case_id, evaluator_id, result_json) VALUES (?1, ?2, ?3, ?4)",
                params![variant_id, evaluation.case_id, evaluator_id, serde_json::to_string(result)?],
            )?;
        }
        for (outcome_id, result) in &evaluation.outcomes {
            transaction.execute(
                "INSERT INTO outcome_results (variant_id, case_id, outcome_id, status, result_json) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![variant_id, evaluation.case_id, outcome_id, outcome_status_name(result.truth), serde_json::to_string(result)?],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Persist aggregate paired outcome metrics.
    pub fn insert_paired_result(
        &self,
        outcome_id: &str,
        result: &PairedMetrics,
    ) -> anyhow::Result<()> {
        self.connection.execute(
            "INSERT INTO paired_results (outcome_id, result_json) VALUES (?1, ?2)",
            params![outcome_id, serde_json::to_string(result)?],
        )?;
        Ok(())
    }

    /// Append a structured lifecycle event.
    pub fn record_event<T: Serialize>(&self, event_type: &str, payload: &T) -> anyhow::Result<()> {
        self.connection.execute(
            "INSERT INTO events (event_type, payload_json) VALUES (?1, ?2)",
            params![event_type, serde_json::to_string(payload)?],
        )?;
        Ok(())
    }

    /// Bind one finalized portable artifact in SQLite.
    pub fn record_artifact(
        &self,
        relative_path: &str,
        digest: &str,
        byte_length: u64,
    ) -> anyhow::Result<()> {
        self.connection.execute(
            "INSERT OR REPLACE INTO artifacts (relative_path, blake3, byte_length) VALUES (?1, ?2, ?3)",
            params![relative_path, digest, i64::try_from(byte_length)?],
        )?;
        Ok(())
    }
}

#[cfg(unix)]
fn harden_directory_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn harden_directory_permissions(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn harden_file_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn harden_file_permissions(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

fn optional_json(value: Option<&serde_json::Value>) -> anyhow::Result<Option<String>> {
    value
        .map(serde_json::to_string)
        .transpose()
        .map_err(Into::into)
}

fn output_status_name(status: structtrace_core::output::OutputStatus) -> &'static str {
    match status {
        structtrace_core::output::OutputStatus::Ok => "ok",
        structtrace_core::output::OutputStatus::Error => "error",
        structtrace_core::output::OutputStatus::Missing => "missing",
    }
}

fn outcome_status_name(status: structtrace_core::evaluation::OutcomeStatus) -> &'static str {
    match status {
        structtrace_core::evaluation::OutcomeStatus::True => "true",
        structtrace_core::evaluation::OutcomeStatus::False => "false",
        structtrace_core::evaluation::OutcomeStatus::Error => "error",
        structtrace_core::evaluation::OutcomeStatus::NotApplicable => "not_applicable",
    }
}

fn validate_run_id(run_id: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        !run_id.is_empty()
            && run_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
        "invalid run ID"
    );
    Ok(())
}

fn migrate(connection: &Connection) -> anyhow::Result<()> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    anyhow::ensure!(
        version <= DATABASE_VERSION,
        "run database version {version} is newer than supported version {DATABASE_VERSION}"
    );
    if version == 0 {
        connection.execute_batch(
            r#"
            BEGIN;
            CREATE TABLE runs (
                run_id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                artifact_version INTEGER NOT NULL,
                created_at_unix_ms INTEGER,
                completed_at_unix_ms INTEGER,
                run_kind TEXT NOT NULL DEFAULT 'production'
            );
            CREATE TABLE cases (
                ordinal INTEGER NOT NULL,
                case_id TEXT PRIMARY KEY,
                input_json TEXT NOT NULL,
                expected_json TEXT,
                model_visible_metadata_json TEXT,
                metadata_json TEXT,
                source_line INTEGER NOT NULL
            );
            CREATE TABLE variants (
                variant_id TEXT PRIMARY KEY,
                definition_json TEXT NOT NULL
            );
            CREATE TABLE variant_outputs (
                variant_id TEXT NOT NULL,
                case_id TEXT NOT NULL,
                status TEXT NOT NULL,
                envelope_json TEXT NOT NULL,
                PRIMARY KEY (variant_id, case_id),
                FOREIGN KEY (variant_id) REFERENCES variants(variant_id),
                FOREIGN KEY (case_id) REFERENCES cases(case_id)
            );
            CREATE TABLE evaluator_results (
                variant_id TEXT NOT NULL,
                case_id TEXT NOT NULL,
                evaluator_id TEXT NOT NULL,
                result_json TEXT NOT NULL,
                PRIMARY KEY (variant_id, case_id, evaluator_id)
            );
            CREATE TABLE outcome_results (
                variant_id TEXT NOT NULL,
                case_id TEXT NOT NULL,
                outcome_id TEXT NOT NULL,
                status TEXT NOT NULL,
                result_json TEXT NOT NULL,
                PRIMARY KEY (variant_id, case_id, outcome_id)
            );
            CREATE TABLE paired_results (
                outcome_id TEXT PRIMARY KEY,
                result_json TEXT NOT NULL
            );
            CREATE TABLE events (
                sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type TEXT NOT NULL,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE artifacts (
                relative_path TEXT PRIMARY KEY,
                blake3 TEXT NOT NULL,
                byte_length INTEGER NOT NULL
            );
            PRAGMA user_version = 5;
            COMMIT;
            "#,
        )?;
    } else if version == 1 {
        connection.execute_batch(
            r#"
            BEGIN;
            ALTER TABLE cases ADD COLUMN model_visible_metadata_json TEXT;
            ALTER TABLE cases ADD COLUMN source_line INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE runs ADD COLUMN run_kind TEXT NOT NULL DEFAULT 'production';
            ALTER TABLE outcome_results ADD COLUMN result_json TEXT NOT NULL DEFAULT '{}';
            PRAGMA user_version = 5;
            COMMIT;
            "#,
        )?;
    } else if version == 2 {
        connection.execute_batch(
            r#"
            BEGIN;
            ALTER TABLE runs ADD COLUMN run_kind TEXT NOT NULL DEFAULT 'production';
            ALTER TABLE outcome_results ADD COLUMN result_json TEXT NOT NULL DEFAULT '{}';
            PRAGMA user_version = 5;
            COMMIT;
            "#,
        )?;
    } else if version == 3 {
        connection.execute_batch(
            r#"
            BEGIN;
            ALTER TABLE outcome_results ADD COLUMN result_json TEXT NOT NULL DEFAULT '{}';
            PRAGMA user_version = 5;
            COMMIT;
            "#,
        )?;
    } else if version == 4 {
        // Version 5 binds the database to artifact v9 deployment-success semantics. The stored
        // evaluation JSON is self-describing, so no table rewrite is required.
        connection.pragma_update(None, "user_version", DATABASE_VERSION)?;
    }
    Ok(())
}

fn run_kind_name(kind: RunKind) -> &'static str {
    match kind {
        RunKind::Production => "production",
        RunKind::Demo => "demo",
        RunKind::ResearchFixture => "research_fixture",
        RunKind::Test => "test",
    }
}

fn status_name(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Created => "created",
        RunStatus::Validating => "validating",
        RunStatus::Running => "running",
        RunStatus::Interrupted => "interrupted",
        RunStatus::Analyzing => "analyzing",
        RunStatus::Complete => "complete",
        RunStatus::Failed => "failed",
        RunStatus::Corrupt => "corrupt",
    }
}

fn parse_status(value: &str) -> anyhow::Result<RunStatus> {
    match value {
        "created" => Ok(RunStatus::Created),
        "validating" => Ok(RunStatus::Validating),
        "running" => Ok(RunStatus::Running),
        "interrupted" => Ok(RunStatus::Interrupted),
        "analyzing" => Ok(RunStatus::Analyzing),
        "complete" => Ok(RunStatus::Complete),
        "failed" => Ok(RunStatus::Failed),
        "corrupt" => Ok(RunStatus::Corrupt),
        other => anyhow::bail!("unknown run status `{other}`"),
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn creates_schema_and_tracks_lifecycle() {
        let directory = tempdir().unwrap();
        let store = RunStore::create(directory.path(), "01ABC", RunKind::Test).unwrap();
        assert_eq!(store.status("01ABC").unwrap(), RunStatus::Created);
        store.set_status("01ABC", RunStatus::Validating).unwrap();
        assert_eq!(store.status("01ABC").unwrap(), RunStatus::Validating);
        store.checkpoint().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn creates_private_run_storage() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempdir().unwrap();
        let store = RunStore::create(directory.path(), "01PRIVATE", RunKind::Test).unwrap();
        assert_eq!(
            std::fs::metadata(store.run_dir())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(store.run_dir().join("run.sqlite3"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn refuses_path_traversal_run_ids() {
        let directory = tempdir().unwrap();
        assert!(RunStore::create(directory.path(), "../outside", RunKind::Test).is_err());
    }

    #[test]
    fn corrupt_database_fails_closed() {
        let directory = tempdir().unwrap();
        std::fs::write(
            directory.path().join("run.sqlite3"),
            b"not a sqlite database",
        )
        .unwrap();
        assert!(RunStore::open(directory.path()).is_err());
    }
}
