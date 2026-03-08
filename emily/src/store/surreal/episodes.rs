use super::SurrealEmilyStore;
use crate::error::EmilyError;
use crate::model::{AuditRecord, EpisodeRecord, EpisodeTraceLink, OutcomeRecord};

impl SurrealEmilyStore {
    fn normalize_record_id(value: &str, table: &str) -> String {
        let prefix = format!("{table}:`");
        value
            .strip_prefix(&prefix)
            .and_then(|rest| rest.strip_suffix('`'))
            .map_or_else(|| value.to_string(), ToString::to_string)
    }

    fn normalize_episode(mut episode: EpisodeRecord) -> EpisodeRecord {
        episode.id = Self::normalize_record_id(&episode.id, "episodes");
        if let Some(last_outcome_id) = episode.last_outcome_id.as_ref() {
            episode.last_outcome_id = Some(Self::normalize_record_id(last_outcome_id, "outcomes"));
        }
        episode
    }

    fn normalize_trace_link(mut link: EpisodeTraceLink) -> EpisodeTraceLink {
        link.id = Self::normalize_record_id(&link.id, "episode_trace_links");
        link.episode_id = Self::normalize_record_id(&link.episode_id, "episodes");
        link.object_id = Self::normalize_record_id(&link.object_id, "text_objects");
        link
    }

    fn normalize_outcome(mut outcome: OutcomeRecord) -> OutcomeRecord {
        outcome.id = Self::normalize_record_id(&outcome.id, "outcomes");
        outcome.episode_id = Self::normalize_record_id(&outcome.episode_id, "episodes");
        outcome
    }

    fn normalize_audit(mut audit: AuditRecord) -> AuditRecord {
        audit.id = Self::normalize_record_id(&audit.id, "audit_records");
        audit.episode_id = Self::normalize_record_id(&audit.episode_id, "episodes");
        audit
    }

    fn episode_projection() -> &'static str {
        "type::string(id) AS id, stream_id, source_kind, episode_kind, state, started_at_unix_ms, closed_at_unix_ms, intent, metadata, last_outcome_id, created_at_unix_ms, updated_at_unix_ms"
    }

    fn trace_link_projection() -> &'static str {
        "type::string(id) AS id, episode_id, object_id, trace_kind, linked_at_unix_ms, metadata"
    }

    fn outcome_projection() -> &'static str {
        "type::string(id) AS id, episode_id, status, recorded_at_unix_ms, summary, metadata"
    }

    fn audit_projection() -> &'static str {
        "type::string(id) AS id, episode_id, kind, ts_unix_ms, summary, metadata"
    }

    pub(super) async fn upsert_episode_internal(
        &self,
        episode: &EpisodeRecord,
    ) -> Result<(), EmilyError> {
        let client = self.active_client().await?;
        client
            .query("UPSERT type::thing('episodes', $id) CONTENT $episode")
            .bind(("id", episode.id.clone()))
            .bind(("episode", episode.clone()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal episode upsert failed: {error}"))
            })?;
        Ok(())
    }

    pub(super) async fn get_episode_internal(
        &self,
        episode_id: &str,
    ) -> Result<Option<EpisodeRecord>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query(format!(
                "SELECT {} FROM type::thing('episodes', $id)",
                Self::episode_projection()
            ))
            .bind(("id", episode_id.to_string()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal select episode failed: {error}"))
            })?;
        let episodes: Vec<EpisodeRecord> = response.take(0).map_err(|error| {
            EmilyError::Store(format!("surreal result decode failed (episode): {error}"))
        })?;
        Ok(episodes.into_iter().next().map(Self::normalize_episode))
    }

    pub(super) async fn upsert_episode_trace_link_internal(
        &self,
        link: &EpisodeTraceLink,
    ) -> Result<(), EmilyError> {
        let client = self.active_client().await?;
        client
            .query("UPSERT type::thing('episode_trace_links', $id) CONTENT $link")
            .bind(("id", link.id.clone()))
            .bind(("link", link.clone()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal trace link upsert failed: {error}"))
            })?;
        Ok(())
    }

    pub(super) async fn get_episode_trace_link_internal(
        &self,
        link_id: &str,
    ) -> Result<Option<EpisodeTraceLink>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query(format!(
                "SELECT {} FROM type::thing('episode_trace_links', $id)",
                Self::trace_link_projection()
            ))
            .bind(("id", link_id.to_string()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal select trace link failed: {error}"))
            })?;
        let links: Vec<EpisodeTraceLink> = response.take(0).map_err(|error| {
            EmilyError::Store(format!(
                "surreal result decode failed (trace link): {error}"
            ))
        })?;
        Ok(links.into_iter().next().map(Self::normalize_trace_link))
    }

    pub(super) async fn list_episode_trace_links_internal(
        &self,
        episode_id: &str,
    ) -> Result<Vec<EpisodeTraceLink>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query(format!(
                "SELECT {} FROM episode_trace_links WHERE episode_id = $episode_id",
                Self::trace_link_projection()
            ))
            .bind(("episode_id", episode_id.to_string()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal select trace links failed: {error}"))
            })?;
        let mut links: Vec<EpisodeTraceLink> = response.take(0).map_err(|error| {
            EmilyError::Store(format!(
                "surreal result decode failed (trace links): {error}"
            ))
        })?;
        links = links.into_iter().map(Self::normalize_trace_link).collect();
        links.sort_by(|left, right| left.linked_at_unix_ms.cmp(&right.linked_at_unix_ms));
        Ok(links)
    }

    pub(super) async fn upsert_outcome_internal(
        &self,
        outcome: &OutcomeRecord,
    ) -> Result<(), EmilyError> {
        let client = self.active_client().await?;
        client
            .query("UPSERT type::thing('outcomes', $id) CONTENT $outcome")
            .bind(("id", outcome.id.clone()))
            .bind(("outcome", outcome.clone()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal outcome upsert failed: {error}"))
            })?;
        Ok(())
    }

    pub(super) async fn get_outcome_internal(
        &self,
        outcome_id: &str,
    ) -> Result<Option<OutcomeRecord>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query(format!(
                "SELECT {} FROM type::thing('outcomes', $id)",
                Self::outcome_projection()
            ))
            .bind(("id", outcome_id.to_string()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal select outcome failed: {error}"))
            })?;
        let outcomes: Vec<OutcomeRecord> = response.take(0).map_err(|error| {
            EmilyError::Store(format!("surreal result decode failed (outcome): {error}"))
        })?;
        Ok(outcomes.into_iter().next().map(Self::normalize_outcome))
    }

    pub(super) async fn list_outcomes_internal(
        &self,
        episode_id: &str,
    ) -> Result<Vec<OutcomeRecord>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query(format!(
                "SELECT {} FROM outcomes WHERE episode_id = $episode_id",
                Self::outcome_projection()
            ))
            .bind(("episode_id", episode_id.to_string()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal select outcomes failed: {error}"))
            })?;
        let mut outcomes: Vec<OutcomeRecord> = response.take(0).map_err(|error| {
            EmilyError::Store(format!("surreal result decode failed (outcomes): {error}"))
        })?;
        outcomes = outcomes.into_iter().map(Self::normalize_outcome).collect();
        outcomes.sort_by(|left, right| left.recorded_at_unix_ms.cmp(&right.recorded_at_unix_ms));
        Ok(outcomes)
    }

    pub(super) async fn upsert_audit_record_internal(
        &self,
        audit: &AuditRecord,
    ) -> Result<(), EmilyError> {
        let client = self.active_client().await?;
        client
            .query("UPSERT type::thing('audit_records', $id) CONTENT $audit")
            .bind(("id", audit.id.clone()))
            .bind(("audit", audit.clone()))
            .await
            .map_err(|error| EmilyError::Store(format!("surreal audit upsert failed: {error}")))?;
        Ok(())
    }

    pub(super) async fn get_audit_record_internal(
        &self,
        audit_id: &str,
    ) -> Result<Option<AuditRecord>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query(format!(
                "SELECT {} FROM type::thing('audit_records', $id)",
                Self::audit_projection()
            ))
            .bind(("id", audit_id.to_string()))
            .await
            .map_err(|error| EmilyError::Store(format!("surreal select audit failed: {error}")))?;
        let audits: Vec<AuditRecord> = response.take(0).map_err(|error| {
            EmilyError::Store(format!("surreal result decode failed (audit): {error}"))
        })?;
        Ok(audits.into_iter().next().map(Self::normalize_audit))
    }

    pub(super) async fn list_audit_records_internal(
        &self,
        episode_id: &str,
    ) -> Result<Vec<AuditRecord>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query(format!(
                "SELECT {} FROM audit_records WHERE episode_id = $episode_id",
                Self::audit_projection()
            ))
            .bind(("episode_id", episode_id.to_string()))
            .await
            .map_err(|error| EmilyError::Store(format!("surreal select audits failed: {error}")))?;
        let mut audits: Vec<AuditRecord> = response.take(0).map_err(|error| {
            EmilyError::Store(format!("surreal result decode failed (audits): {error}"))
        })?;
        audits = audits.into_iter().map(Self::normalize_audit).collect();
        audits.sort_by(|left, right| left.ts_unix_ms.cmp(&right.ts_unix_ms));
        Ok(audits)
    }
}
