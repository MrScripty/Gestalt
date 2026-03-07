use crate::local_restore;
use crate::persistence;
use crate::state::SessionId;
use parking_lot::Mutex;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::thread::{self, JoinHandle};

#[derive(Clone)]
pub(crate) struct AutosaveRequest {
    pub(crate) workspace: persistence::PersistedWorkspaceV1,
    pub(crate) signature: AutosaveSignature,
}

pub(crate) type AutosaveSignature = (u64, Vec<(SessionId, u64)>);

#[derive(Clone)]
pub(crate) struct AutosaveResult {
    pub(crate) signature: AutosaveSignature,
    pub(crate) error: Option<String>,
}

enum AutosaveCommand {
    Save(AutosaveRequest),
    Shutdown,
}

pub(crate) struct AutosaveWorker {
    command_tx: Mutex<Option<SyncSender<AutosaveCommand>>>,
    result_rx: Mutex<Receiver<AutosaveResult>>,
    join_handle: Mutex<Option<JoinHandle<()>>>,
}

impl AutosaveWorker {
    pub(crate) fn spawn(queue_capacity: usize, initial_fingerprint: Option<u64>) -> Self {
        let (command_tx, command_rx) = mpsc::sync_channel::<AutosaveCommand>(queue_capacity);
        let (result_tx, result_rx) = mpsc::channel::<AutosaveResult>();
        let join_handle =
            thread::spawn(move || autosave_worker_loop(command_rx, result_tx, initial_fingerprint));

        Self {
            command_tx: Mutex::new(Some(command_tx)),
            result_rx: Mutex::new(result_rx),
            join_handle: Mutex::new(Some(join_handle)),
        }
    }

    pub(crate) fn try_enqueue(&self, request: AutosaveRequest) -> Result<(), String> {
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

    pub(crate) fn drain_results(&self) -> Vec<AutosaveResult> {
        let mut drained = Vec::new();
        let result_rx = self.result_rx.lock();
        while let Ok(result) = result_rx.try_recv() {
            drained.push(result);
        }

        drained
    }

    pub(crate) fn shutdown(&self) {
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
    result_tx: mpsc::Sender<AutosaveResult>,
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
