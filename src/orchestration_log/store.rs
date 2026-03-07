use crate::orchestration_log::error::OrchestrationLogError;
use crate::orchestration_log::model::{
    CommandKind, CommandPayload, CommandRecord, EventKind, EventPayload, EventRecord,
    NewCommandRecord, NewEventRecord, NewReceiptRecord, ReceiptPayload, ReceiptRecord,
    ReceiptStatus, TimelineEntry,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::path::{Path, PathBuf};

const DB_FILE_NAME: &str = "orchestration.sqlite3";

#[derive(Debug, Clone)]
pub struct OrchestrationLogStore {
    path: PathBuf,
}

impl Default for OrchestrationLogStore {
    fn default() -> Self {
        Self::new(database_path())
    }
}

impl OrchestrationLogStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn record_command(
        &self,
        new_record: NewCommandRecord,
    ) -> Result<CommandRecord, OrchestrationLogError> {
        let mut connection = self.open_connection()?;
        let transaction = connection
            .transaction()
            .map_err(OrchestrationLogError::BeginTransaction)?;

        let existing = transaction
            .query_row(
                "SELECT command_id FROM orchestration_commands WHERE command_id = ?1",
                [new_record.command_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(OrchestrationLogError::Query)?;
        if existing.is_some() {
            return Err(OrchestrationLogError::DuplicateCommandId(
                new_record.command_id,
            ));
        }

        transaction
            .execute(
                "INSERT INTO orchestration_timelines (
                    timeline_id,
                    command_id,
                    last_sequence,
                    created_at_unix_ms,
                    updated_at_unix_ms
                 ) VALUES (?1, ?2, 1, ?3, ?4)",
                params![
                    new_record.timeline_id,
                    new_record.command_id,
                    new_record.requested_at_unix_ms,
                    new_record.recorded_at_unix_ms,
                ],
            )
            .map_err(|source| OrchestrationLogError::UpdateTimeline {
                timeline_id: new_record.timeline_id.clone(),
                source,
            })?;

        let payload_json = serde_json::to_string(&new_record.payload)
            .map_err(OrchestrationLogError::SerializePayload)?;
        let kind = new_record.payload.kind();
        let group_id = new_record.payload.group_id().map(i64::from);
        let group_path = new_record.payload.group_path().to_string();
        transaction
            .execute(
                "INSERT INTO orchestration_commands (
                    command_id,
                    timeline_id,
                    sequence_in_timeline,
                    kind,
                    group_id,
                    group_path,
                    requested_at_unix_ms,
                    recorded_at_unix_ms,
                    payload_json
                 ) VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    new_record.command_id,
                    new_record.timeline_id,
                    kind_name(kind),
                    group_id,
                    group_path,
                    new_record.requested_at_unix_ms,
                    new_record.recorded_at_unix_ms,
                    payload_json,
                ],
            )
            .map_err(|source| OrchestrationLogError::InsertCommand {
                command_id: new_record.command_id.clone(),
                source,
            })?;

        transaction
            .commit()
            .map_err(OrchestrationLogError::CommitTransaction)?;

        Ok(CommandRecord {
            command_id: new_record.command_id,
            timeline_id: new_record.timeline_id,
            sequence_in_timeline: 1,
            kind,
            group_id: new_record.payload.group_id(),
            group_path,
            requested_at_unix_ms: new_record.requested_at_unix_ms,
            recorded_at_unix_ms: new_record.recorded_at_unix_ms,
            payload: new_record.payload,
        })
    }

    pub fn append_event(
        &self,
        command_id: &str,
        new_record: NewEventRecord,
    ) -> Result<EventRecord, OrchestrationLogError> {
        let mut connection = self.open_connection()?;
        let transaction = connection
            .transaction()
            .map_err(OrchestrationLogError::BeginTransaction)?;
        let timeline_id = lookup_timeline_id(&transaction, command_id)?;
        let sequence =
            allocate_sequence(&transaction, &timeline_id, new_record.recorded_at_unix_ms)?;
        let event_id = format!("{timeline_id}:event:{sequence}");
        let kind = new_record.payload.kind();
        let payload_json = serde_json::to_string(&new_record.payload)
            .map_err(OrchestrationLogError::SerializePayload)?;

        transaction
            .execute(
                "INSERT INTO orchestration_events (
                    event_id,
                    command_id,
                    timeline_id,
                    sequence_in_timeline,
                    kind,
                    occurred_at_unix_ms,
                    recorded_at_unix_ms,
                    payload_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    event_id,
                    command_id,
                    timeline_id,
                    sequence,
                    event_name(kind),
                    new_record.occurred_at_unix_ms,
                    new_record.recorded_at_unix_ms,
                    payload_json,
                ],
            )
            .map_err(|source| OrchestrationLogError::InsertEvent {
                command_id: command_id.to_string(),
                source,
            })?;

        transaction
            .commit()
            .map_err(OrchestrationLogError::CommitTransaction)?;

        Ok(EventRecord {
            event_id,
            command_id: command_id.to_string(),
            timeline_id,
            sequence_in_timeline: sequence,
            kind,
            occurred_at_unix_ms: new_record.occurred_at_unix_ms,
            recorded_at_unix_ms: new_record.recorded_at_unix_ms,
            payload: new_record.payload,
        })
    }

    pub fn finalize_receipt(
        &self,
        command_id: &str,
        new_record: NewReceiptRecord,
    ) -> Result<ReceiptRecord, OrchestrationLogError> {
        let mut connection = self.open_connection()?;
        let transaction = connection
            .transaction()
            .map_err(OrchestrationLogError::BeginTransaction)?;
        let timeline_id = lookup_timeline_id(&transaction, command_id)?;

        let existing_receipt = transaction
            .query_row(
                "SELECT command_id FROM orchestration_receipts WHERE command_id = ?1",
                [command_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(OrchestrationLogError::Query)?;
        if existing_receipt.is_some() {
            return Err(OrchestrationLogError::ReceiptAlreadyFinalized(
                command_id.to_string(),
            ));
        }

        let sequence =
            allocate_sequence(&transaction, &timeline_id, new_record.recorded_at_unix_ms)?;
        let payload_json = serde_json::to_string(&new_record.payload)
            .map_err(OrchestrationLogError::SerializePayload)?;
        transaction
            .execute(
                "INSERT INTO orchestration_receipts (
                    command_id,
                    timeline_id,
                    sequence_in_timeline,
                    status,
                    completed_at_unix_ms,
                    recorded_at_unix_ms,
                    payload_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    command_id,
                    timeline_id,
                    sequence,
                    receipt_status_name(new_record.status),
                    new_record.completed_at_unix_ms,
                    new_record.recorded_at_unix_ms,
                    payload_json,
                ],
            )
            .map_err(|source| OrchestrationLogError::InsertReceipt {
                command_id: command_id.to_string(),
                source,
            })?;

        transaction
            .commit()
            .map_err(OrchestrationLogError::CommitTransaction)?;

        Ok(ReceiptRecord {
            command_id: command_id.to_string(),
            timeline_id,
            sequence_in_timeline: sequence,
            status: new_record.status,
            completed_at_unix_ms: new_record.completed_at_unix_ms,
            recorded_at_unix_ms: new_record.recorded_at_unix_ms,
            payload: new_record.payload,
        })
    }

    pub fn load_timeline(
        &self,
        command_id: &str,
    ) -> Result<Vec<TimelineEntry>, OrchestrationLogError> {
        let connection = self.open_connection()?;
        let command = connection
            .query_row(
                "SELECT
                    command_id,
                    timeline_id,
                    sequence_in_timeline,
                    kind,
                    group_id,
                    group_path,
                    requested_at_unix_ms,
                    recorded_at_unix_ms,
                    payload_json
                 FROM orchestration_commands
                 WHERE command_id = ?1",
                [command_id],
                decode_command_row,
            )
            .optional()
            .map_err(OrchestrationLogError::Query)?;
        let Some(command) = command else {
            return Ok(Vec::new());
        };

        let mut entries = vec![TimelineEntry::Command(command)];

        let mut event_statement = connection
            .prepare(
                "SELECT
                    event_id,
                    command_id,
                    timeline_id,
                    sequence_in_timeline,
                    kind,
                    occurred_at_unix_ms,
                    recorded_at_unix_ms,
                    payload_json
                 FROM orchestration_events
                 WHERE command_id = ?1
                 ORDER BY sequence_in_timeline ASC",
            )
            .map_err(OrchestrationLogError::Query)?;
        let event_rows = event_statement
            .query_map([command_id], decode_event_row)
            .map_err(OrchestrationLogError::Query)?;
        for row in event_rows {
            entries.push(TimelineEntry::Event(
                row.map_err(OrchestrationLogError::DecodeRow)?,
            ));
        }

        let receipt = connection
            .query_row(
                "SELECT
                    command_id,
                    timeline_id,
                    sequence_in_timeline,
                    status,
                    completed_at_unix_ms,
                    recorded_at_unix_ms,
                    payload_json
                 FROM orchestration_receipts
                 WHERE command_id = ?1",
                [command_id],
                decode_receipt_row,
            )
            .optional()
            .map_err(OrchestrationLogError::Query)?;
        if let Some(receipt) = receipt {
            entries.push(TimelineEntry::Receipt(receipt));
        }

        entries.sort_by_key(TimelineEntry::sequence_in_timeline);
        Ok(entries)
    }

    pub fn load_recent_commands(
        &self,
        limit: usize,
    ) -> Result<Vec<CommandRecord>, OrchestrationLogError> {
        let connection = self.open_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT
                    command_id,
                    timeline_id,
                    sequence_in_timeline,
                    kind,
                    group_id,
                    group_path,
                    requested_at_unix_ms,
                    recorded_at_unix_ms,
                    payload_json
                 FROM orchestration_commands
                 ORDER BY requested_at_unix_ms DESC, sequence_in_timeline DESC
                 LIMIT ?1",
            )
            .map_err(OrchestrationLogError::Query)?;
        let rows = statement
            .query_map([limit as i64], decode_command_row)
            .map_err(OrchestrationLogError::Query)?;

        let mut commands = Vec::new();
        for row in rows {
            commands.push(row.map_err(OrchestrationLogError::DecodeRow)?);
        }
        Ok(commands)
    }

    fn open_connection(&self) -> Result<Connection, OrchestrationLogError> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| OrchestrationLogError::MissingParent(self.path.display().to_string()))?;
        std::fs::create_dir_all(parent).map_err(|source| {
            OrchestrationLogError::CreateDirectory {
                path: parent.display().to_string(),
                source,
            }
        })?;

        let connection =
            Connection::open(&self.path).map_err(|source| OrchestrationLogError::OpenDb {
                path: self.path.display().to_string(),
                source,
            })?;
        ensure_schema(&connection)?;
        Ok(connection)
    }
}

fn lookup_timeline_id(
    transaction: &Transaction<'_>,
    command_id: &str,
) -> Result<String, OrchestrationLogError> {
    transaction
        .query_row(
            "SELECT timeline_id FROM orchestration_commands WHERE command_id = ?1",
            [command_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(OrchestrationLogError::Query)?
        .ok_or_else(|| OrchestrationLogError::MissingCommand(command_id.to_string()))
}

fn allocate_sequence(
    transaction: &Transaction<'_>,
    timeline_id: &str,
    recorded_at_unix_ms: i64,
) -> Result<i64, OrchestrationLogError> {
    let next_sequence = transaction
        .query_row(
            "SELECT last_sequence FROM orchestration_timelines WHERE timeline_id = ?1",
            [timeline_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(OrchestrationLogError::Query)?
        .map(|value| value.saturating_add(1))
        .ok_or_else(|| OrchestrationLogError::MissingCommand(timeline_id.to_string()))?;

    transaction
        .execute(
            "UPDATE orchestration_timelines
             SET last_sequence = ?2, updated_at_unix_ms = ?3
             WHERE timeline_id = ?1",
            params![timeline_id, next_sequence, recorded_at_unix_ms],
        )
        .map_err(|source| OrchestrationLogError::UpdateTimeline {
            timeline_id: timeline_id.to_string(),
            source,
        })?;
    Ok(next_sequence)
}

fn ensure_schema(connection: &Connection) -> Result<(), OrchestrationLogError> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS orchestration_timelines (
                timeline_id TEXT PRIMARY KEY,
                command_id TEXT NOT NULL UNIQUE,
                last_sequence INTEGER NOT NULL,
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS orchestration_commands (
                command_id TEXT PRIMARY KEY,
                timeline_id TEXT NOT NULL,
                sequence_in_timeline INTEGER NOT NULL,
                kind TEXT NOT NULL,
                group_id INTEGER NULL,
                group_path TEXT NOT NULL,
                requested_at_unix_ms INTEGER NOT NULL,
                recorded_at_unix_ms INTEGER NOT NULL,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS orchestration_events (
                event_id TEXT PRIMARY KEY,
                command_id TEXT NOT NULL,
                timeline_id TEXT NOT NULL,
                sequence_in_timeline INTEGER NOT NULL,
                kind TEXT NOT NULL,
                occurred_at_unix_ms INTEGER NOT NULL,
                recorded_at_unix_ms INTEGER NOT NULL,
                payload_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS orchestration_receipts (
                command_id TEXT PRIMARY KEY,
                timeline_id TEXT NOT NULL,
                sequence_in_timeline INTEGER NOT NULL,
                status TEXT NOT NULL,
                completed_at_unix_ms INTEGER NOT NULL,
                recorded_at_unix_ms INTEGER NOT NULL,
                payload_json TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS orchestration_commands_requested_at
                ON orchestration_commands (requested_at_unix_ms DESC);
            CREATE INDEX IF NOT EXISTS orchestration_commands_group_path
                ON orchestration_commands (group_path);
            CREATE INDEX IF NOT EXISTS orchestration_events_command_timeline
                ON orchestration_events (command_id, timeline_id, sequence_in_timeline);
            CREATE INDEX IF NOT EXISTS orchestration_receipts_timeline
                ON orchestration_receipts (timeline_id, sequence_in_timeline);",
        )
        .map_err(OrchestrationLogError::EnsureSchema)
}

fn decode_command_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CommandRecord> {
    let payload_json: String = row.get(8)?;
    let payload: CommandPayload = serde_json::from_str(&payload_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(CommandRecord {
        command_id: row.get(0)?,
        timeline_id: row.get(1)?,
        sequence_in_timeline: row.get(2)?,
        kind: parse_command_kind(&row.get::<_, String>(3)?).map_err(to_sql_decode_error)?,
        group_id: row
            .get::<_, Option<i64>>(4)?
            .and_then(|value| u32::try_from(value).ok()),
        group_path: row.get(5)?,
        requested_at_unix_ms: row.get(6)?,
        recorded_at_unix_ms: row.get(7)?,
        payload,
    })
}

fn decode_event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EventRecord> {
    let payload_json: String = row.get(7)?;
    let payload: EventPayload = serde_json::from_str(&payload_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(EventRecord {
        event_id: row.get(0)?,
        command_id: row.get(1)?,
        timeline_id: row.get(2)?,
        sequence_in_timeline: row.get(3)?,
        kind: parse_event_kind(&row.get::<_, String>(4)?).map_err(to_sql_decode_error)?,
        occurred_at_unix_ms: row.get(5)?,
        recorded_at_unix_ms: row.get(6)?,
        payload,
    })
}

fn decode_receipt_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReceiptRecord> {
    let payload_json: String = row.get(6)?;
    let payload: ReceiptPayload = serde_json::from_str(&payload_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(ReceiptRecord {
        command_id: row.get(0)?,
        timeline_id: row.get(1)?,
        sequence_in_timeline: row.get(2)?,
        status: parse_receipt_status(&row.get::<_, String>(3)?).map_err(to_sql_decode_error)?,
        completed_at_unix_ms: row.get(4)?,
        recorded_at_unix_ms: row.get(5)?,
        payload,
    })
}

fn to_sql_decode_error(error: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn kind_name(kind: CommandKind) -> &'static str {
    match kind {
        CommandKind::BroadcastSendLine => "broadcast_send_line",
        CommandKind::BroadcastInterrupt => "broadcast_interrupt",
        CommandKind::GitStageFiles => "git_stage_files",
        CommandKind::GitUnstageFiles => "git_unstage_files",
        CommandKind::GitCreateCommit => "git_create_commit",
        CommandKind::GitUpdateCommitMessage => "git_update_commit_message",
        CommandKind::GitCreateTag => "git_create_tag",
        CommandKind::GitCheckoutTarget => "git_checkout_target",
        CommandKind::GitCreateWorktree => "git_create_worktree",
    }
}

fn parse_command_kind(value: &str) -> Result<CommandKind, serde_json::Error> {
    match value {
        "broadcast_send_line" => Ok(CommandKind::BroadcastSendLine),
        "broadcast_interrupt" => Ok(CommandKind::BroadcastInterrupt),
        "git_stage_files" => Ok(CommandKind::GitStageFiles),
        "git_unstage_files" => Ok(CommandKind::GitUnstageFiles),
        "git_create_commit" => Ok(CommandKind::GitCreateCommit),
        "git_update_commit_message" => Ok(CommandKind::GitUpdateCommitMessage),
        "git_create_tag" => Ok(CommandKind::GitCreateTag),
        "git_checkout_target" => Ok(CommandKind::GitCheckoutTarget),
        "git_create_worktree" => Ok(CommandKind::GitCreateWorktree),
        other => Err(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unknown orchestration command kind: {other}"),
        ))),
    }
}

fn event_name(kind: EventKind) -> &'static str {
    match kind {
        EventKind::BroadcastWriteSucceeded => "broadcast_write_succeeded",
        EventKind::BroadcastWriteFailed => "broadcast_write_failed",
        EventKind::GitPathSucceeded => "git_path_succeeded",
        EventKind::GitPathFailed => "git_path_failed",
        EventKind::GitOperationSucceeded => "git_operation_succeeded",
        EventKind::GitOperationFailed => "git_operation_failed",
    }
}

fn parse_event_kind(value: &str) -> Result<EventKind, serde_json::Error> {
    match value {
        "broadcast_write_succeeded" => Ok(EventKind::BroadcastWriteSucceeded),
        "broadcast_write_failed" => Ok(EventKind::BroadcastWriteFailed),
        "git_path_succeeded" => Ok(EventKind::GitPathSucceeded),
        "git_path_failed" => Ok(EventKind::GitPathFailed),
        "git_operation_succeeded" => Ok(EventKind::GitOperationSucceeded),
        "git_operation_failed" => Ok(EventKind::GitOperationFailed),
        other => Err(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unknown orchestration event kind: {other}"),
        ))),
    }
}

fn receipt_status_name(status: ReceiptStatus) -> &'static str {
    match status {
        ReceiptStatus::Succeeded => "succeeded",
        ReceiptStatus::PartiallySucceeded => "partially_succeeded",
        ReceiptStatus::Failed => "failed",
    }
}

fn parse_receipt_status(value: &str) -> Result<ReceiptStatus, serde_json::Error> {
    match value {
        "succeeded" => Ok(ReceiptStatus::Succeeded),
        "partially_succeeded" => Ok(ReceiptStatus::PartiallySucceeded),
        "failed" => Ok(ReceiptStatus::Failed),
        other => Err(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unknown orchestration receipt status: {other}"),
        ))),
    }
}

fn database_path() -> PathBuf {
    if let Ok(value) = std::env::var("GESTALT_ORCHESTRATION_DB_PATH") {
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
