use crate::run_checkpoints::error::RunCheckpointError;
use crate::run_checkpoints::model::{
    NewRunCheckpointRecord, RunCheckpointFile, RunCheckpointRecord,
};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};

const DB_FILE_NAME: &str = "run_checkpoints.sqlite3";
const MAX_GROUP_CHECKPOINTS: i64 = 20;

#[derive(Debug, Clone)]
pub struct RunCheckpointStore {
    path: PathBuf,
}

impl Default for RunCheckpointStore {
    fn default() -> Self {
        Self::new(database_path())
    }
}

impl RunCheckpointStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn record_checkpoint(
        &self,
        new_record: NewRunCheckpointRecord,
    ) -> Result<RunCheckpointRecord, RunCheckpointError> {
        let connection = self.open_connection()?;
        let existing = connection
            .query_row(
                "SELECT run_id FROM run_checkpoints WHERE run_id = ?1",
                [new_record.run_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(RunCheckpointError::Query)?;
        if existing.is_some() {
            return Err(RunCheckpointError::DuplicateRunId(new_record.run_id));
        }

        let baseline_json = serde_json::to_string(&new_record.baseline_files)?;
        connection
            .execute(
                "INSERT INTO run_checkpoints (
                    run_id,
                    group_id,
                    group_path,
                    command_line,
                    repo_root,
                    started_at_unix_ms,
                    head_sha,
                    branch_name,
                    baseline_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    new_record.run_id,
                    i64::from(new_record.group_id),
                    new_record.group_path,
                    new_record.command_line,
                    new_record.repo_root,
                    new_record.started_at_unix_ms,
                    new_record.head_sha,
                    new_record.branch_name,
                    baseline_json,
                ],
            )
            .map_err(RunCheckpointError::Query)?;
        prune_group_history(&connection, &new_record.group_path)?;

        Ok(RunCheckpointRecord {
            run_id: new_record.run_id,
            group_id: new_record.group_id,
            group_path: new_record.group_path,
            command_line: new_record.command_line,
            repo_root: new_record.repo_root,
            started_at_unix_ms: new_record.started_at_unix_ms,
            head_sha: new_record.head_sha,
            branch_name: new_record.branch_name,
            baseline_files: new_record.baseline_files,
        })
    }

    pub fn load_latest_for_group_path(
        &self,
        group_path: &str,
    ) -> Result<Option<RunCheckpointRecord>, RunCheckpointError> {
        let connection = self.open_connection()?;
        connection
            .query_row(
                "SELECT
                    run_id,
                    group_id,
                    group_path,
                    command_line,
                    repo_root,
                    started_at_unix_ms,
                    head_sha,
                    branch_name,
                    baseline_json
                 FROM run_checkpoints
                 WHERE group_path = ?1
                 ORDER BY started_at_unix_ms DESC, rowid DESC
                 LIMIT 1",
                [group_path],
                decode_checkpoint_row,
            )
            .optional()
            .map_err(RunCheckpointError::Query)
    }

    fn open_connection(&self) -> Result<Connection, RunCheckpointError> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| RunCheckpointError::MissingParent(self.path.display().to_string()))?;
        std::fs::create_dir_all(parent).map_err(|source| RunCheckpointError::CreateDirectory {
            path: parent.display().to_string(),
            source,
        })?;

        let connection =
            Connection::open(&self.path).map_err(|source| RunCheckpointError::OpenDb {
                path: self.path.display().to_string(),
                source,
            })?;
        ensure_schema(&connection)?;
        Ok(connection)
    }
}

fn prune_group_history(
    connection: &Connection,
    group_path: &str,
) -> Result<(), RunCheckpointError> {
    connection
        .execute(
            "DELETE FROM run_checkpoints
             WHERE run_id IN (
                SELECT run_id
                FROM run_checkpoints
                WHERE group_path = ?1
                ORDER BY started_at_unix_ms DESC, rowid DESC
                LIMIT -1 OFFSET ?2
             )",
            params![group_path, MAX_GROUP_CHECKPOINTS],
        )
        .map_err(RunCheckpointError::Query)?;
    Ok(())
}

fn ensure_schema(connection: &Connection) -> Result<(), RunCheckpointError> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS run_checkpoints (
                run_id TEXT PRIMARY KEY,
                group_id INTEGER NOT NULL,
                group_path TEXT NOT NULL,
                command_line TEXT NOT NULL,
                repo_root TEXT NOT NULL,
                started_at_unix_ms INTEGER NOT NULL,
                head_sha TEXT NULL,
                branch_name TEXT NULL,
                baseline_json TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS run_checkpoints_group_started
                ON run_checkpoints (group_path, started_at_unix_ms DESC);",
        )
        .map_err(RunCheckpointError::EnsureSchema)
}

fn decode_checkpoint_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunCheckpointRecord> {
    let baseline_json: String = row.get(8)?;
    let baseline_files: Vec<RunCheckpointFile> =
        serde_json::from_str(&baseline_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                8,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;

    Ok(RunCheckpointRecord {
        run_id: row.get(0)?,
        group_id: row.get::<_, i64>(1)?.try_into().map_err(|_| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Integer,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "group_id out of range",
                )),
            )
        })?,
        group_path: row.get(2)?,
        command_line: row.get(3)?,
        repo_root: row.get(4)?,
        started_at_unix_ms: row.get(5)?,
        head_sha: row.get(6)?,
        branch_name: row.get(7)?,
        baseline_files,
    })
}

fn database_path() -> PathBuf {
    if let Ok(value) = std::env::var("GESTALT_RUN_CHECKPOINT_DB_PATH") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    state_home().join("gestalt").join(DB_FILE_NAME)
}

#[cfg(target_os = "linux")]
fn state_home() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(path);
    }

    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local")
        .join("state")
}

#[cfg(target_os = "windows")]
fn state_home() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

#[cfg(target_os = "macos")]
fn state_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Library")
        .join("Application Support")
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn state_home() -> PathBuf {
    std::env::temp_dir()
}
