use super::SurrealEmilyStore;
use crate::error::EmilyError;
use crate::model::IntegritySnapshot;

impl SurrealEmilyStore {
    fn integrity_snapshot_projection() -> &'static str {
        "type::string(id) AS id, ts_unix_ms, ci_value, tau, integrated_count, quarantined_count, pending_count, deferred_count"
    }

    fn normalize_integrity_snapshot(mut snapshot: IntegritySnapshot) -> IntegritySnapshot {
        let prefix = "integrity_snapshots:`";
        let normalized_id = snapshot
            .id
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix('`'))
            .map_or_else(|| snapshot.id.clone(), ToString::to_string);
        snapshot.id = normalized_id;
        snapshot
    }

    pub(super) async fn upsert_integrity_snapshot_internal(
        &self,
        snapshot: &IntegritySnapshot,
    ) -> Result<(), EmilyError> {
        let client = self.active_client().await?;
        client
            .query("UPSERT type::thing('integrity_snapshots', $id) CONTENT $snapshot")
            .bind(("id", snapshot.id.clone()))
            .bind(("snapshot", snapshot.clone()))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal integrity snapshot upsert failed: {error}"))
            })?;
        Ok(())
    }

    pub(super) async fn latest_integrity_snapshot_internal(
        &self,
    ) -> Result<Option<IntegritySnapshot>, EmilyError> {
        let client = self.active_client().await?;
        let mut response = client
            .query(format!(
                "SELECT {} FROM integrity_snapshots ORDER BY ts_unix_ms DESC LIMIT 1",
                Self::integrity_snapshot_projection()
            ))
            .await
            .map_err(|error| {
                EmilyError::Store(format!("surreal select integrity snapshot failed: {error}"))
            })?;
        let snapshots: Vec<IntegritySnapshot> = response.take(0).map_err(|error| {
            EmilyError::Store(format!(
                "surreal result decode failed (integrity snapshot): {error}"
            ))
        })?;
        Ok(snapshots
            .into_iter()
            .next()
            .map(Self::normalize_integrity_snapshot))
    }
}
