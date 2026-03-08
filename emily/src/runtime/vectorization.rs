use super::{ActiveJobControl, EmilyRuntime, VectorizationRuntimeState};
use crate::error::EmilyError;
use crate::model::{
    TextVector, VectorizationJobKind, VectorizationJobSnapshot, VectorizationJobState,
    VectorizationRunRequest, VectorizationStatus,
};
use crate::store::EmilyStore;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{Mutex, RwLock, broadcast};

impl<S: EmilyStore + 'static> EmilyRuntime<S> {
    pub(super) async fn spawn_vectorization_job(
        &self,
        kind: VectorizationJobKind,
        request: VectorizationRunRequest,
    ) -> Result<VectorizationJobSnapshot, EmilyError> {
        if request
            .stream_id
            .as_ref()
            .is_some_and(|stream_id| stream_id.trim().is_empty())
        {
            return Err(EmilyError::InvalidRequest(
                "stream_id cannot be empty when provided".to_string(),
            ));
        }

        let Some(provider) = self.embedding_provider.clone() else {
            return Err(EmilyError::InvalidRequest(
                "embedding provider is not available".to_string(),
            ));
        };

        let config = { self.vectorization.read().await.config.clone() };
        if !config.enabled {
            return Err(EmilyError::InvalidRequest(
                "vectorization is disabled".to_string(),
            ));
        }

        let job_id = format!(
            "vectorization-job-{}",
            self.job_counter.fetch_add(1, Ordering::Relaxed) + 1
        );
        let cancel = Arc::new(AtomicBool::new(false));

        {
            let mut active_control = self.active_job_control.lock().await;
            if active_control.is_some() {
                return Err(EmilyError::InvalidRequest(
                    "a vectorization job is already running".to_string(),
                ));
            }
            *active_control = Some(ActiveJobControl {
                job_id: job_id.clone(),
                cancel: cancel.clone(),
            });
        }

        let job = VectorizationJobSnapshot {
            job_id: job_id.clone(),
            kind,
            state: VectorizationJobState::Running,
            stream_id: request.stream_id.clone(),
            processed: 0,
            vectorized: 0,
            skipped: 0,
            failed: 0,
            last_error: None,
        };
        let stream_id_for_response = job.stream_id.clone();
        self.update_active_job(job.clone()).await;

        let store = Arc::clone(&self.store);
        let vectorization = Arc::clone(&self.vectorization);
        let active_job_control = Arc::clone(&self.active_job_control);
        let vectorization_events = self.vectorization_events.clone();

        tokio::spawn(async move {
            let mut running = job;
            let objects = match store.list_text_objects(request.stream_id.as_deref()).await {
                Ok(objects) => objects,
                Err(error) => {
                    running.last_error = Some(format!("failed listing text objects: {error}"));
                    running.failed = running.failed.saturating_add(1);
                    finalize_job(
                        &vectorization,
                        &active_job_control,
                        &vectorization_events,
                        true,
                        running,
                        VectorizationJobState::Completed,
                    )
                    .await;
                    return;
                }
            };

            for object in objects {
                if cancel.load(Ordering::Relaxed) {
                    finalize_job(
                        &vectorization,
                        &active_job_control,
                        &vectorization_events,
                        true,
                        running,
                        VectorizationJobState::Cancelled,
                    )
                    .await;
                    return;
                }

                running.processed = running.processed.saturating_add(1);
                let should_vectorize = match kind {
                    VectorizationJobKind::Backfill => match store.get_text_vector(&object.id).await
                    {
                        Ok(Some(existing)) => {
                            existing.dimensions != config.expected_dimensions
                                || existing.profile_id != config.profile_id
                        }
                        Ok(None) => true,
                        Err(error) => {
                            running.failed = running.failed.saturating_add(1);
                            running.last_error = Some(format!(
                                "failed loading vector for object {}: {error}",
                                object.id
                            ));
                            publish_running_job(
                                &vectorization,
                                &vectorization_events,
                                true,
                                &running,
                            )
                            .await;
                            continue;
                        }
                    },
                    VectorizationJobKind::Revectorize => true,
                };

                if !should_vectorize {
                    running.skipped = running.skipped.saturating_add(1);
                    publish_running_job(&vectorization, &vectorization_events, true, &running)
                        .await;
                    continue;
                }

                match provider.embed_text(&object.text).await {
                    Ok(vector) => {
                        if vector.is_empty() {
                            running.skipped = running.skipped.saturating_add(1);
                            publish_running_job(
                                &vectorization,
                                &vectorization_events,
                                true,
                                &running,
                            )
                            .await;
                            continue;
                        }
                        match EmilyRuntime::<S>::validate_embedding_vector(
                            &vector,
                            config.expected_dimensions,
                        ) {
                            Ok(()) => {
                                let record = TextVector {
                                    id: format!("vec:{}", object.id),
                                    object_id: object.id.clone(),
                                    stream_id: object.stream_id.clone(),
                                    sequence: object.sequence,
                                    ts_unix_ms: object.ts_unix_ms,
                                    dimensions: vector.len(),
                                    profile_id: config.profile_id.clone(),
                                    vector,
                                };
                                if let Err(error) = store.upsert_text_vector(&record).await {
                                    running.failed = running.failed.saturating_add(1);
                                    running.last_error =
                                        Some(format!("failed upserting vector: {error}"));
                                } else {
                                    running.vectorized = running.vectorized.saturating_add(1);
                                }
                            }
                            Err(error) => {
                                running.failed = running.failed.saturating_add(1);
                                running.last_error = Some(error.to_string());
                            }
                        }
                    }
                    Err(error) => {
                        running.failed = running.failed.saturating_add(1);
                        running.last_error = Some(format!("embedding call failed: {error}"));
                    }
                }
                publish_running_job(&vectorization, &vectorization_events, true, &running).await;
            }

            finalize_job(
                &vectorization,
                &active_job_control,
                &vectorization_events,
                true,
                running,
                VectorizationJobState::Completed,
            )
            .await;
        });

        Ok(VectorizationJobSnapshot {
            job_id,
            kind,
            state: VectorizationJobState::Running,
            stream_id: stream_id_for_response,
            processed: 0,
            vectorized: 0,
            skipped: 0,
            failed: 0,
            last_error: None,
        })
    }
}

async fn publish_running_job(
    vectorization: &Arc<RwLock<VectorizationRuntimeState>>,
    vectorization_events: &broadcast::Sender<VectorizationStatus>,
    provider_available: bool,
    running: &VectorizationJobSnapshot,
) {
    let snapshot = {
        let mut state = vectorization.write().await;
        state.active_job = Some(running.clone());
        VectorizationStatus {
            config: state.config.clone(),
            provider_available,
            provider_status: None,
            active_job: state.active_job.clone(),
            last_job: state.last_job.clone(),
        }
    };
    let _ = vectorization_events.send(snapshot);
}

async fn finalize_job(
    vectorization: &Arc<RwLock<VectorizationRuntimeState>>,
    active_job_control: &Arc<Mutex<Option<ActiveJobControl>>>,
    vectorization_events: &broadcast::Sender<VectorizationStatus>,
    provider_available: bool,
    mut running: VectorizationJobSnapshot,
    state: VectorizationJobState,
) {
    running.state = state;

    {
        let mut control = active_job_control.lock().await;
        if control
            .as_ref()
            .is_some_and(|active| active.job_id == running.job_id)
        {
            *control = None;
        }
    }

    let snapshot = {
        let mut vectorization_state = vectorization.write().await;
        vectorization_state.active_job = None;
        vectorization_state.last_job = Some(running);
        VectorizationStatus {
            config: vectorization_state.config.clone(),
            provider_available,
            provider_status: None,
            active_job: None,
            last_job: vectorization_state.last_job.clone(),
        }
    };
    let _ = vectorization_events.send(snapshot);
}
