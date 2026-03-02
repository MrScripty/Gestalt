use crate::state::SessionId;
use crate::terminal::PersistedTerminalState;
use rusqlite::{Connection, params};
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LocalRestoreError {
    #[error("failed opening restore db {path}: {source}")]
    OpenDb {
        path: String,
        #[source]
        source: rusqlite::Error,
    },
    #[error("failed preparing restore projection read query: {0}")]
    PrepareRead(rusqlite::Error),
    #[error("failed querying restore projection rows: {0}")]
    QueryRows(rusqlite::Error),
    #[error("failed decoding restore row: {0}")]
    DecodeRow(rusqlite::Error),
    #[error("failed to derive restore database parent directory for {0}")]
    MissingParent(String),
    #[error("failed creating restore database directory {path}: {source}")]
    CreateDirectory {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed opening restore transaction: {0}")]
    BeginTransaction(rusqlite::Error),
    #[error("failed upserting restore projection for session {session_id}: {source}")]
    UpsertProjection {
        session_id: SessionId,
        #[source]
        source: rusqlite::Error,
    },
    #[error("failed committing restore projection transaction: {0}")]
    CommitTransaction(rusqlite::Error),
    #[error("failed ensuring restore schema: {0}")]
    EnsureSchema(rusqlite::Error),
}

#[derive(Debug, Clone)]
pub struct SessionProjection {
    pub session_id: SessionId,
    pub cwd: String,
    pub rows: u16,
    pub cols: u16,
    pub cursor_row: u16,
    pub cursor_col: u16,
    pub hide_cursor: bool,
    pub bracketed_paste: bool,
    pub history_before_sequence: Option<u64>,
}

pub fn load_projection_map() -> Result<HashMap<SessionId, SessionProjection>, LocalRestoreError> {
    let path = database_path();
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let connection = Connection::open(&path).map_err(|source| LocalRestoreError::OpenDb {
        path: path.display().to_string(),
        source,
    })?;
    ensure_schema(&connection)?;

    let mut statement = connection
        .prepare(
            "SELECT session_id, cwd, rows, cols, cursor_row, cursor_col, hide_cursor, bracketed_paste, history_before_sequence
             FROM terminal_restore_projection",
        )
        .map_err(LocalRestoreError::PrepareRead)?;

    let rows = statement
        .query_map([], |row| {
            let session_id: u32 = row.get(0)?;
            let history_before: Option<i64> = row.get(8)?;
            Ok(SessionProjection {
                session_id,
                cwd: row.get(1)?,
                rows: row.get(2)?,
                cols: row.get(3)?,
                cursor_row: row.get(4)?,
                cursor_col: row.get(5)?,
                hide_cursor: row.get::<_, i64>(6)? != 0,
                bracketed_paste: row.get::<_, i64>(7)? != 0,
                history_before_sequence: history_before.and_then(|value| u64::try_from(value).ok()),
            })
        })
        .map_err(LocalRestoreError::QueryRows)?;

    let mut map = HashMap::new();
    for row in rows {
        let projection = row.map_err(LocalRestoreError::DecodeRow)?;
        map.insert(projection.session_id, projection);
    }

    Ok(map)
}

pub fn save_projection(terminals: &[PersistedTerminalState]) -> Result<(), LocalRestoreError> {
    let path = database_path();
    let parent = path
        .parent()
        .ok_or_else(|| LocalRestoreError::MissingParent(path.display().to_string()))?;
    std::fs::create_dir_all(parent).map_err(|source| LocalRestoreError::CreateDirectory {
        path: parent.display().to_string(),
        source,
    })?;

    let mut connection = Connection::open(&path).map_err(|source| LocalRestoreError::OpenDb {
        path: path.display().to_string(),
        source,
    })?;
    ensure_schema(&connection)?;

    let transaction = connection
        .transaction()
        .map_err(LocalRestoreError::BeginTransaction)?;

    for terminal in terminals {
        transaction
            .execute(
                "INSERT INTO terminal_restore_projection (
                    session_id,
                    cwd,
                    rows,
                    cols,
                    cursor_row,
                    cursor_col,
                    hide_cursor,
                    bracketed_paste,
                    history_before_sequence,
                    updated_at_unix_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9)
                 ON CONFLICT(session_id) DO UPDATE SET
                    cwd=excluded.cwd,
                    rows=excluded.rows,
                    cols=excluded.cols,
                    cursor_row=excluded.cursor_row,
                    cursor_col=excluded.cursor_col,
                    hide_cursor=excluded.hide_cursor,
                    bracketed_paste=excluded.bracketed_paste,
                    updated_at_unix_ms=excluded.updated_at_unix_ms",
                params![
                    terminal.session_id,
                    terminal.cwd,
                    terminal.rows,
                    terminal.cols,
                    terminal.cursor_row,
                    terminal.cursor_col,
                    i64::from(terminal.hide_cursor),
                    i64::from(terminal.bracketed_paste),
                    current_unix_ms(),
                ],
            )
            .map_err(|source| LocalRestoreError::UpsertProjection {
                session_id: terminal.session_id,
                source,
            })?;
    }

    transaction
        .commit()
        .map_err(LocalRestoreError::CommitTransaction)?;

    Ok(())
}

fn ensure_schema(connection: &Connection) -> Result<(), LocalRestoreError> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS terminal_restore_projection (
                session_id INTEGER PRIMARY KEY,
                cwd TEXT NOT NULL,
                rows INTEGER NOT NULL,
                cols INTEGER NOT NULL,
                cursor_row INTEGER NOT NULL,
                cursor_col INTEGER NOT NULL,
                hide_cursor INTEGER NOT NULL,
                bracketed_paste INTEGER NOT NULL,
                history_before_sequence INTEGER NULL,
                updated_at_unix_ms INTEGER NOT NULL
            );",
        )
        .map_err(LocalRestoreError::EnsureSchema)
}

fn database_path() -> PathBuf {
    if let Ok(value) = std::env::var("GESTALT_RESTORE_DB_PATH") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .map(|home| home.join(".local/share/gestalt/restore.sqlite3"))
        .unwrap_or_else(|| std::env::temp_dir().join("gestalt-restore.sqlite3"))
}

fn current_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}
