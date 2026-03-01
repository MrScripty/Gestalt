use crate::state::SessionId;
use crate::terminal::PersistedTerminalState;
use rusqlite::{Connection, params};
use std::collections::HashMap;
use std::path::PathBuf;

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

pub fn load_projection_map() -> Result<HashMap<SessionId, SessionProjection>, String> {
    let path = database_path();
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let connection = Connection::open(&path)
        .map_err(|error| format!("failed opening restore db {}: {error}", path.display()))?;
    ensure_schema(&connection)?;

    let mut statement = connection
        .prepare(
            "SELECT session_id, cwd, rows, cols, cursor_row, cursor_col, hide_cursor, bracketed_paste, history_before_sequence
             FROM terminal_restore_projection",
        )
        .map_err(|error| format!("failed preparing restore projection read query: {error}"))?;

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
        .map_err(|error| format!("failed querying restore projection rows: {error}"))?;

    let mut map = HashMap::new();
    for row in rows {
        let projection = row.map_err(|error| format!("failed decoding restore row: {error}"))?;
        map.insert(projection.session_id, projection);
    }

    Ok(map)
}

pub fn save_projection(terminals: &[PersistedTerminalState]) -> Result<(), String> {
    let path = database_path();
    let parent = path.parent().ok_or_else(|| {
        format!(
            "failed to derive restore database parent directory for {}",
            path.display()
        )
    })?;
    std::fs::create_dir_all(parent).map_err(|error| {
        format!(
            "failed creating restore database directory {}: {error}",
            parent.display()
        )
    })?;

    let mut connection = Connection::open(&path)
        .map_err(|error| format!("failed opening restore db {}: {error}", path.display()))?;
    ensure_schema(&connection)?;

    let transaction = connection
        .transaction()
        .map_err(|error| format!("failed opening restore transaction: {error}"))?;

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
            .map_err(|error| {
                format!(
                    "failed upserting restore projection for session {}: {error}",
                    terminal.session_id
                )
            })?;
    }

    transaction
        .commit()
        .map_err(|error| format!("failed committing restore projection transaction: {error}"))?;

    Ok(())
}

fn ensure_schema(connection: &Connection) -> Result<(), String> {
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
        .map_err(|error| format!("failed ensuring restore schema: {error}"))
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
