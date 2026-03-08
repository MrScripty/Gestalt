use super::EmilyRuntime;
use crate::error::EmilyError;
use crate::model::{
    AppendAuditRecordRequest, AuditRecord, CreateEpisodeRequest, EpisodeRecord, EpisodeState,
    EpisodeTraceKind, EpisodeTraceLink, OutcomeRecord, OutcomeStatus, RecordOutcomeRequest,
    TraceLinkRequest,
};
use crate::store::EmilyStore;

impl<S: EmilyStore + 'static> EmilyRuntime<S> {
    pub(super) fn validate_required_text(field_name: &str, value: &str) -> Result<(), EmilyError> {
        if value.trim().is_empty() {
            return Err(EmilyError::InvalidRequest(format!(
                "{field_name} cannot be empty"
            )));
        }
        Ok(())
    }

    pub(super) fn validate_optional_text(
        field_name: &str,
        value: Option<&str>,
    ) -> Result<(), EmilyError> {
        if let Some(value) = value
            && value.trim().is_empty()
        {
            return Err(EmilyError::InvalidRequest(format!(
                "{field_name} cannot be empty when provided"
            )));
        }
        Ok(())
    }

    fn build_episode_record(request: CreateEpisodeRequest) -> EpisodeRecord {
        EpisodeRecord {
            id: request.episode_id,
            stream_id: request.stream_id,
            source_kind: request.source_kind,
            episode_kind: request.episode_kind,
            state: EpisodeState::Open,
            started_at_unix_ms: request.started_at_unix_ms,
            closed_at_unix_ms: None,
            intent: request.intent,
            metadata: request.metadata,
            last_outcome_id: None,
            created_at_unix_ms: request.started_at_unix_ms,
            updated_at_unix_ms: request.started_at_unix_ms,
        }
    }

    fn episode_matches_create_request(
        episode: &EpisodeRecord,
        request: &CreateEpisodeRequest,
    ) -> bool {
        episode.id == request.episode_id
            && episode.stream_id == request.stream_id
            && episode.source_kind == request.source_kind
            && episode.episode_kind == request.episode_kind
            && episode.started_at_unix_ms == request.started_at_unix_ms
            && episode.intent == request.intent
            && episode.metadata == request.metadata
    }

    fn build_trace_link(request: TraceLinkRequest) -> EpisodeTraceLink {
        let trace_kind = request.trace_kind;
        EpisodeTraceLink {
            id: format!(
                "episode:{}:{}:{}",
                request.episode_id,
                Self::trace_kind_slug(trace_kind),
                request.object_id
            ),
            episode_id: request.episode_id,
            object_id: request.object_id,
            trace_kind,
            linked_at_unix_ms: request.linked_at_unix_ms,
            metadata: request.metadata,
        }
    }

    fn build_outcome_record(request: RecordOutcomeRequest) -> OutcomeRecord {
        OutcomeRecord {
            id: request.outcome_id,
            episode_id: request.episode_id,
            status: request.status,
            recorded_at_unix_ms: request.recorded_at_unix_ms,
            summary: request.summary,
            metadata: request.metadata,
        }
    }

    fn build_audit_record(request: AppendAuditRecordRequest) -> AuditRecord {
        AuditRecord {
            id: request.audit_id,
            episode_id: request.episode_id,
            kind: request.kind,
            ts_unix_ms: request.ts_unix_ms,
            summary: request.summary,
            metadata: request.metadata,
        }
    }

    fn trace_kind_slug(kind: EpisodeTraceKind) -> &'static str {
        match kind {
            EpisodeTraceKind::Input => "input",
            EpisodeTraceKind::Output => "output",
            EpisodeTraceKind::Context => "context",
            EpisodeTraceKind::Summary => "summary",
            EpisodeTraceKind::Note => "note",
            EpisodeTraceKind::Other => "other",
        }
    }

    fn conflict_error(record_type: &str, id: &str) -> EmilyError {
        EmilyError::InvalidRequest(format!(
            "{record_type} '{id}' already exists with different content"
        ))
    }

    fn apply_outcome_to_episode(episode: &EpisodeRecord, outcome: &OutcomeRecord) -> EpisodeRecord {
        let mut updated = episode.clone();
        updated.closed_at_unix_ms = Some(
            updated
                .closed_at_unix_ms
                .map_or(outcome.recorded_at_unix_ms, |ts| {
                    ts.max(outcome.recorded_at_unix_ms)
                }),
        );
        updated.updated_at_unix_ms = updated.updated_at_unix_ms.max(outcome.recorded_at_unix_ms);

        let should_advance = updated.last_outcome_id.is_none()
            || outcome.recorded_at_unix_ms >= updated.updated_at_unix_ms;
        if should_advance {
            updated.last_outcome_id = Some(outcome.id.clone());
            updated.state = match outcome.status {
                OutcomeStatus::Cancelled => EpisodeState::Cancelled,
                OutcomeStatus::Succeeded
                | OutcomeStatus::Failed
                | OutcomeStatus::Partial
                | OutcomeStatus::Unknown => EpisodeState::Completed,
            };
        }

        updated
    }

    pub(super) async fn create_episode_internal(
        &self,
        request: CreateEpisodeRequest,
    ) -> Result<EpisodeRecord, EmilyError> {
        Self::validate_required_text("episode_id", &request.episode_id)?;
        Self::validate_required_text("source_kind", &request.source_kind)?;
        Self::validate_required_text("episode_kind", &request.episode_kind)?;
        Self::validate_optional_text("stream_id", request.stream_id.as_deref())?;
        Self::validate_optional_text("intent", request.intent.as_deref())?;

        if let Some(existing) = self.store.get_episode(&request.episode_id).await? {
            if Self::episode_matches_create_request(&existing, &request) {
                return Ok(existing);
            }
            return Err(Self::conflict_error("episode", &request.episode_id));
        }

        let record = Self::build_episode_record(request);
        self.store.upsert_episode(&record).await?;
        Ok(record)
    }

    pub(super) async fn link_text_to_episode_internal(
        &self,
        request: TraceLinkRequest,
    ) -> Result<EpisodeTraceLink, EmilyError> {
        Self::validate_required_text("episode_id", &request.episode_id)?;
        Self::validate_required_text("object_id", &request.object_id)?;

        if self.store.get_episode(&request.episode_id).await?.is_none() {
            return Err(EmilyError::InvalidRequest(format!(
                "episode '{}' does not exist",
                request.episode_id
            )));
        }
        if self
            .store
            .get_text_object(&request.object_id)
            .await?
            .is_none()
        {
            return Err(EmilyError::InvalidRequest(format!(
                "text object '{}' does not exist",
                request.object_id
            )));
        }

        let link = Self::build_trace_link(request);
        if let Some(existing) = self.store.get_episode_trace_link(&link.id).await? {
            if existing == link {
                return Ok(existing);
            }
            return Err(Self::conflict_error("episode trace link", &link.id));
        }

        self.store.upsert_episode_trace_link(&link).await?;
        Ok(link)
    }

    pub(super) async fn record_outcome_internal(
        &self,
        request: RecordOutcomeRequest,
    ) -> Result<OutcomeRecord, EmilyError> {
        Self::validate_required_text("outcome_id", &request.outcome_id)?;
        Self::validate_required_text("episode_id", &request.episode_id)?;
        Self::validate_optional_text("summary", request.summary.as_deref())?;

        let Some(existing_episode) = self.store.get_episode(&request.episode_id).await? else {
            return Err(EmilyError::InvalidRequest(format!(
                "episode '{}' does not exist",
                request.episode_id
            )));
        };
        if matches!(existing_episode.state, EpisodeState::Blocked) {
            return Err(EmilyError::EpisodeGated(format!(
                "episode '{}' is blocked by EARL",
                request.episode_id
            )));
        }

        let outcome = Self::build_outcome_record(request);
        match self.store.get_outcome(&outcome.id).await? {
            Some(existing) if existing != outcome => {
                return Err(Self::conflict_error("outcome", &outcome.id));
            }
            Some(_) => {}
            None => {
                self.store.upsert_outcome(&outcome).await?;
            }
        }

        let updated_episode = Self::apply_outcome_to_episode(&existing_episode, &outcome);
        if updated_episode != existing_episode {
            self.store.upsert_episode(&updated_episode).await?;
        }
        self.apply_ecgl_after_outcome(&updated_episode, &outcome)
            .await?;

        Ok(outcome)
    }

    pub(super) async fn append_audit_record_internal(
        &self,
        request: AppendAuditRecordRequest,
    ) -> Result<AuditRecord, EmilyError> {
        Self::validate_required_text("audit_id", &request.audit_id)?;
        Self::validate_required_text("episode_id", &request.episode_id)?;
        Self::validate_required_text("summary", &request.summary)?;

        if self.store.get_episode(&request.episode_id).await?.is_none() {
            return Err(EmilyError::InvalidRequest(format!(
                "episode '{}' does not exist",
                request.episode_id
            )));
        }

        let audit = Self::build_audit_record(request);
        if let Some(existing) = self.store.get_audit_record(&audit.id).await? {
            if existing == audit {
                return Ok(existing);
            }
            return Err(Self::conflict_error("audit record", &audit.id));
        }

        self.store.upsert_audit_record(&audit).await?;
        Ok(audit)
    }
}
