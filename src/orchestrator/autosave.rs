use crate::local_restore;
use crate::persistence;
use crate::state::{AppState, SessionId};
use crate::terminal::TerminalManager;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};
use tokio::sync::{Mutex as AsyncMutex, mpsc as tokio_mpsc};
use tokio::time::Instant;

#[derive(Clone)]
pub struct AutosaveRequest {
    pub workspace: persistence::PersistedWorkspaceV1,
    pub signature: AutosaveSignature,
}

pub type AutosaveSignature = (u64, Vec<(SessionId, u64)>);

#[derive(Clone)]
pub struct AutosaveResult {
    pub signature: AutosaveSignature,
    pub error: Option<String>,
}

pub enum AutosaveFeedback {
    Unchanged,
    Clear,
    Set(String),
}

enum AutosaveCommand {
    Save(AutosaveRequest),
    Shutdown,
}

pub struct AutosaveWorker {
    command_tx: Mutex<Option<SyncSender<AutosaveCommand>>>,
    result_rx: AsyncMutex<tokio_mpsc::UnboundedReceiver<AutosaveResult>>,
    join_handle: Mutex<Option<JoinHandle<()>>>,
}

pub struct AutosaveController {
    debounce_ms: u64,
    last_saved_signature: Option<AutosaveSignature>,
    inflight_signature: Option<AutosaveSignature>,
    deferred_request: Option<AutosaveRequest>,
    save_deadline: Option<Instant>,
}

impl AutosaveController {
    pub fn new(debounce_ms: u64) -> Self {
        Self {
            debounce_ms,
            last_saved_signature: None,
            inflight_signature: None,
            deferred_request: None,
            save_deadline: None,
        }
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.save_deadline
    }

    pub fn schedule_save(&mut self) {
        self.save_deadline =
            Some(Instant::now() + std::time::Duration::from_millis(self.debounce_ms));
    }

    pub fn handle_worker_result(
        &mut self,
        worker: &AutosaveWorker,
        result: AutosaveResult,
    ) -> AutosaveFeedback {
        let feedback = if result.error.is_none() {
            self.last_saved_signature = Some(result.signature.clone());
            AutosaveFeedback::Clear
        } else {
            AutosaveFeedback::Set(result.error.unwrap_or_default())
        };

        if self.inflight_signature.as_ref() == Some(&result.signature) {
            self.inflight_signature = None;
        }

        if self.inflight_signature.is_none()
            && let Some(request) = self.deferred_request.take()
        {
            if let Err(error) = self.try_enqueue_request(worker, request.clone()) {
                self.deferred_request = Some(request);
                return AutosaveFeedback::Set(error);
            }
        }

        feedback
    }

    pub async fn flush_if_due(
        &mut self,
        app_state: AppState,
        terminal_manager: Arc<TerminalManager>,
        worker: &AutosaveWorker,
    ) -> AutosaveFeedback {
        self.save_deadline = None;

        let mut terminal_revisions = app_state
            .sessions()
            .iter()
            .map(|session| {
                (
                    session.id,
                    terminal_manager
                        .session_snapshot_revision(session.id)
                        .unwrap_or(0),
                )
            })
            .collect::<Vec<_>>();
        terminal_revisions.sort_unstable_by_key(|(session_id, _)| *session_id);
        let save_signature = (app_state.revision(), terminal_revisions);

        if self.last_saved_signature.as_ref() == Some(&save_signature)
            || self.inflight_signature.as_ref() == Some(&save_signature)
            || self
                .deferred_request
                .as_ref()
                .map(|request| &request.signature)
                == Some(&save_signature)
        {
            return AutosaveFeedback::Unchanged;
        }

        let terminal_manager_for_snapshot = terminal_manager.clone();
        let workspace = match tokio::task::spawn_blocking(move || {
            persistence::build_workspace_snapshot(&app_state, &terminal_manager_for_snapshot)
        })
        .await
        {
            Ok(workspace) => workspace,
            Err(error) => {
                return AutosaveFeedback::Set(format!("Autosave snapshot build failed: {error}"));
            }
        };

        let request = AutosaveRequest {
            workspace,
            signature: save_signature.clone(),
        };

        if self.inflight_signature.is_none() {
            match self.try_enqueue_request(worker, request.clone()) {
                Ok(()) => {
                    self.inflight_signature = Some(save_signature);
                    AutosaveFeedback::Unchanged
                }
                Err(error) => {
                    self.deferred_request = Some(request);
                    AutosaveFeedback::Set(error)
                }
            }
        } else {
            self.deferred_request = Some(request);
            AutosaveFeedback::Unchanged
        }
    }

    fn try_enqueue_request(
        &mut self,
        worker: &AutosaveWorker,
        request: AutosaveRequest,
    ) -> Result<(), String> {
        worker.try_enqueue(request.clone())?;
        self.inflight_signature = Some(request.signature);
        Ok(())
    }
}

impl AutosaveWorker {
    pub fn spawn(queue_capacity: usize, initial_fingerprint: Option<u64>) -> Self {
        let (command_tx, command_rx) = mpsc::sync_channel::<AutosaveCommand>(queue_capacity);
        let (result_tx, result_rx) = tokio_mpsc::unbounded_channel::<AutosaveResult>();
        let join_handle =
            thread::spawn(move || autosave_worker_loop(command_rx, result_tx, initial_fingerprint));

        Self {
            command_tx: Mutex::new(Some(command_tx)),
            result_rx: AsyncMutex::new(result_rx),
            join_handle: Mutex::new(Some(join_handle)),
        }
    }

    pub fn try_enqueue(&self, request: AutosaveRequest) -> Result<(), String> {
        let command_tx = self
            .command_tx
            .lock()
            .as_ref()
            .cloned()
            .ok_or_else(|| "autosave worker offline".to_string())?;

        match command_tx.try_send(AutosaveCommand::Save(request)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err("autosave queue full; retrying".to_string()),
            Err(TrySendError::Disconnected(_)) => Err("autosave worker offline".to_string()),
        }
    }

    pub async fn recv_result(&self) -> Option<AutosaveResult> {
        self.result_rx.lock().await.recv().await
    }

    pub fn shutdown(&self) {
        let command_tx = self.command_tx.lock().take();
        if let Some(command_tx) = command_tx {
            let _ = command_tx.try_send(AutosaveCommand::Shutdown);
            drop(command_tx);
        }

        if let Some(join_handle) = self.join_handle.lock().take() {
            let _ = join_handle.join();
        }
    }
}

fn autosave_worker_loop(
    command_rx: Receiver<AutosaveCommand>,
    result_tx: tokio_mpsc::UnboundedSender<AutosaveResult>,
    mut last_saved_fingerprint: Option<u64>,
) {
    while let Ok(command) = command_rx.recv() {
        match command {
            AutosaveCommand::Save(request) => {
                let result = match request.workspace.stable_fingerprint() {
                    Ok(fingerprint) if last_saved_fingerprint == Some(fingerprint) => {
                        AutosaveResult {
                            signature: request.signature,
                            error: None,
                        }
                    }
                    Ok(fingerprint) => {
                        let projection_error =
                            local_restore::save_projection(&request.workspace.terminals).err();
                        let workspace_error = persistence::save_workspace(&request.workspace).err();

                        match (projection_error, workspace_error) {
                            (None, None) => {
                                last_saved_fingerprint = Some(fingerprint);
                                AutosaveResult {
                                    signature: request.signature,
                                    error: None,
                                }
                            }
                            (Some(projection_error), None) => AutosaveResult {
                                signature: request.signature,
                                error: Some(format!(
                                    "Autosave failed: projection save error: {projection_error}"
                                )),
                            },
                            (None, Some(workspace_error)) => AutosaveResult {
                                signature: request.signature,
                                error: Some(format!("Autosave failed: {workspace_error}")),
                            },
                            (Some(projection_error), Some(workspace_error)) => AutosaveResult {
                                signature: request.signature,
                                error: Some(format!(
                                    "Autosave failed: projection save error: {projection_error}; workspace save error: {workspace_error}"
                                )),
                            },
                        }
                    }
                    Err(error) => AutosaveResult {
                        signature: request.signature,
                        error: Some(format!("Autosave failed: fingerprint error: {error}")),
                    },
                };
                let _ = result_tx.send(result);
            }
            AutosaveCommand::Shutdown => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AutosaveController, AutosaveFeedback};

    #[test]
    fn schedule_save_sets_deadline() {
        let mut controller = AutosaveController::new(1_200);
        assert!(controller.deadline().is_none());

        controller.schedule_save();

        assert!(controller.deadline().is_some());
    }

    #[test]
    fn new_controller_starts_without_feedback_state() {
        let controller = AutosaveController::new(1_200);

        assert!(controller.deadline().is_none());
        assert!(matches!(
            AutosaveFeedback::Unchanged,
            AutosaveFeedback::Unchanged
        ));
    }
}
