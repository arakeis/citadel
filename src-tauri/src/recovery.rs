//! When `citadel.db` can't be opened at startup, the app comes up without a
//! local account and the window shows a recovery screen instead (ADR 0009).
//! The backend decides which steps can help; the frontend owns the words.
use std::path::PathBuf;

use citadel_core::store::{CitadelStore, OpenFailure, StoreError};
use serde::{Deserialize, Serialize};
use tauri::Manager;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DatabaseProblemKind {
    Damaged,
    NewerVersion,
    MigrationFailed,
    MigrationInterrupted,
    Unreadable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryStep {
    RestoreBackup,
    StartFresh,
    TryAgain,
    Quit,
}

#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseBackup {
    pub path: String,
    /// RFC 3339, when the file system knows it.
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseProblem {
    pub kind: DatabaseProblemKind,
    pub database_path: String,
    pub details: String,
    pub backup: Option<DatabaseBackup>,
    /// The steps that can help, best first; `quit` is always last.
    pub steps: Vec<RecoveryStep>,
}

/// Managed in place of the local account when startup couldn't open it.
pub struct PendingRecovery {
    database: PathBuf,
    backup: Option<PathBuf>,
    problem: DatabaseProblem,
}

impl PendingRecovery {
    pub fn new(database: PathBuf, error: &StoreError) -> Self {
        let newest = CitadelStore::backups(&database)
            .unwrap_or_default()
            .into_iter()
            .next();
        let kind = kind(error);
        let problem = DatabaseProblem {
            kind,
            database_path: database.display().to_string(),
            details: error.to_string(),
            backup: newest.as_ref().map(|backup| DatabaseBackup {
                path: backup.path.display().to_string(),
                created_at: backup
                    .modified
                    .map(|modified| chrono::DateTime::<chrono::Utc>::from(modified).to_rfc3339()),
            }),
            steps: steps(kind, newest.is_some()),
        };
        Self {
            database,
            backup: newest.map(|backup| backup.path),
            problem,
        }
    }
}

fn kind(error: &StoreError) -> DatabaseProblemKind {
    match error {
        StoreError::Unusable { failure, .. } => match failure {
            OpenFailure::Corrupt => DatabaseProblemKind::Damaged,
            OpenFailure::Incompatible => DatabaseProblemKind::NewerVersion,
            OpenFailure::MigrationFailed => DatabaseProblemKind::MigrationFailed,
            OpenFailure::MigrationInterrupted => DatabaseProblemKind::MigrationInterrupted,
        },
        _ => DatabaseProblemKind::Unreadable,
    }
}

fn steps(kind: DatabaseProblemKind, has_backup: bool) -> Vec<RecoveryStep> {
    use RecoveryStep::*;
    let restore = has_backup.then_some(RestoreBackup);
    let mut steps: Vec<RecoveryStep> = match kind {
        DatabaseProblemKind::Damaged => restore.into_iter().chain([StartFresh]).collect(),
        DatabaseProblemKind::NewerVersion => restore.into_iter().collect(),
        // The database is unchanged, so its backup would fail the same way.
        DatabaseProblemKind::MigrationFailed => vec![StartFresh],
        DatabaseProblemKind::MigrationInterrupted => vec![TryAgain, StartFresh],
        DatabaseProblemKind::Unreadable => Vec::new(),
    };
    steps.push(Quit);
    steps
}

#[tauri::command]
#[specta::specta]
pub fn clb_query_database_problem(handle: tauri::AppHandle) -> Option<DatabaseProblem> {
    handle
        .try_state::<PendingRecovery>()
        .map(|pending| pending.problem.clone())
}

/// Runs a recovery step, then restarts so startup opens the result.
#[tauri::command]
#[specta::specta]
pub fn clb_cmd_recover_database(
    handle: tauri::AppHandle,
    step: RecoveryStep,
) -> Result<(), String> {
    let pending = handle
        .try_state::<PendingRecovery>()
        .ok_or("Citadel's database doesn't need recovering.")?;
    if !pending.problem.steps.contains(&step) {
        return Err(format!("{step:?} can't help with this database."));
    }
    let database = &pending.database;
    match step {
        RecoveryStep::Quit => handle.exit(0),
        RecoveryStep::RestoreBackup => {
            let backup = pending
                .backup
                .as_ref()
                .ok_or("There's no backup to restore.")?;
            CitadelStore::restore_backup(database, backup).map_err(|error| error.to_string())?;
            handle.restart();
        }
        RecoveryStep::StartFresh => {
            CitadelStore::set_aside(database).map_err(|error| error.to_string())?;
            handle.restart();
        }
        RecoveryStep::TryAgain => {
            CitadelStore::retry_migration(database).map_err(|error| error.to_string())?;
            handle.restart();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use RecoveryStep::*;

    fn unusable(failure: OpenFailure) -> StoreError {
        StoreError::Unusable {
            failure,
            message: "details".into(),
        }
    }

    fn offered(error: StoreError, has_backup: bool) -> Vec<RecoveryStep> {
        steps(kind(&error), has_backup)
    }

    #[test]
    fn each_problem_offers_only_the_steps_that_can_help() {
        assert_eq!(
            offered(unusable(OpenFailure::Corrupt), true),
            [RestoreBackup, StartFresh, Quit]
        );
        assert_eq!(
            offered(unusable(OpenFailure::Corrupt), false),
            [StartFresh, Quit]
        );
        assert_eq!(
            offered(unusable(OpenFailure::Incompatible), true),
            [RestoreBackup, Quit]
        );
        assert_eq!(offered(unusable(OpenFailure::Incompatible), false), [Quit]);
        assert_eq!(
            offered(unusable(OpenFailure::MigrationFailed), true),
            [StartFresh, Quit]
        );
        assert_eq!(
            offered(unusable(OpenFailure::MigrationInterrupted), true),
            [TryAgain, StartFresh, Quit]
        );
        assert_eq!(offered(StoreError::Storage("disk".into()), true), [Quit]);
    }

    #[test]
    fn the_newest_backup_this_build_can_open_is_offered() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("citadel.db");
        std::fs::write(
            &database,
            b"not sqlite, just bytes long enough for a header",
        )
        .unwrap();
        let error = CitadelStore::open(&database).err().unwrap();
        let pending = PendingRecovery::new(database, &error);
        assert_eq!(pending.problem.kind, DatabaseProblemKind::Damaged);
        assert!(pending.problem.backup.is_none());
        assert_eq!(pending.problem.steps, [StartFresh, Quit]);
    }
}
