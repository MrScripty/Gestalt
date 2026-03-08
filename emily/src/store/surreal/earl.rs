use super::SurrealEmilyStore;
use crate::error::EmilyError;
use crate::model::EarlEvaluationRecord;

impl SurrealEmilyStore {
    fn normalize_earl_record_id(value: &str, table: &str) -> String {
        let prefix = format!("{table}:`");
        value
            .strip_prefix(&prefix)
            .and_then(|rest| rest.strip_suffix('`'))
            .map_or_else(|| value.to_string(), ToString::to_string)
    }

    fn normalize_earl_evaluation(mut evaluation: EarlEvaluationRecord) -> EarlEvaluationRecord {
        evaluation.id = Self::normalize_earl_record_id(&evaluation.id, "earl_evaluations");
        evaluation.episode_id = Self::normalize_earl_record_id(&evaluation.episode_id, "episodes");
        evaluation
    }

    fn earl_evaluation_projection() -> &'static str {
        "type::string(id) AS id, episode_id, evaluated_at_unix_ms, signals, risk_score, decision, host_action, retryable, rationale, metadata"
    }

    pub(super) async fn upsert_earl_evaluation_internal(
        &self,
        evaluation: &EarlEvaluationRecord,
    ) -> Result<(), EmilyError> {
        let client = self.active_client().await?;
        client
            .query("UPSERT type::thing('earl_evaluations', $id) CONTENT $evaluation")
            .bind(("id", evaluation.id.clone()))
            .bind(("evaluation", evaluation.clone()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal EARL evaluation upsert failed: {error}"))
            })?;
        Ok(())
    }

    pub(super) async fn get_earl_evaluation_internal(
        &self,
        evaluation_id: &str,
    ) -> Result<Option<EarlEvaluationRecord>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query(format!(
                "SELECT {} FROM type::thing('earl_evaluations', $id)",
                Self::earl_evaluation_projection()
            ))
            .bind(("id", evaluation_id.to_string()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal select EARL evaluation failed: {error}"))
            })?;
        let evaluations: Vec<EarlEvaluationRecord> = response.take(0).map_err(|error| {
            EmilyError::Store(format!(
                "surreal result decode failed (EARL evaluation): {error}"
            ))
        })?;
        Ok(evaluations
            .into_iter()
            .next()
            .map(Self::normalize_earl_evaluation))
    }

    pub(super) async fn list_earl_evaluations_internal(
        &self,
        episode_id: &str,
    ) -> Result<Vec<EarlEvaluationRecord>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query(format!(
                "SELECT {} FROM earl_evaluations WHERE episode_id = $episode_id",
                Self::earl_evaluation_projection()
            ))
            .bind(("episode_id", episode_id.to_string()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal select EARL evaluations failed: {error}"))
            })?;
        let mut evaluations: Vec<EarlEvaluationRecord> = response.take(0).map_err(|error| {
            EmilyError::Store(format!(
                "surreal result decode failed (EARL evaluations): {error}"
            ))
        })?;
        evaluations = evaluations
            .into_iter()
            .map(Self::normalize_earl_evaluation)
            .collect();
        evaluations
            .sort_by(|left, right| left.evaluated_at_unix_ms.cmp(&right.evaluated_at_unix_ms));
        Ok(evaluations)
    }
}
