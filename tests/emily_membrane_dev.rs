use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use gestalt::emily_membrane_dev::{EmilyMembraneDevRequest, run_membrane_dev_scenario};
use gestalt::emily_seed::SYNTHETIC_AGENT_ROUND_DATASET;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn membrane_dev_scenario_records_local_only_artifacts() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");
    let locator = unique_locator();
    let storage_path = locator.storage_path.clone();

    runtime.block_on(async move {
        let emily_runtime = Arc::new(EmilyRuntime::new(Arc::new(SurrealEmilyStore::new())));
        let snapshot = run_membrane_dev_scenario(
            emily_runtime,
            EmilyMembraneDevRequest {
                dataset: SYNTHETIC_AGENT_ROUND_DATASET.to_string(),
                storage_path: locator.storage_path,
                namespace: locator.namespace,
                database: locator.database,
                task_text:
                    "Summarize the locally available provider-registry context for the debugging task."
                        .to_string(),
                query_text:
                    "Summarize the locally available provider-registry context for the debugging task."
                        .to_string(),
                top_k: 3,
                reset: true,
                reseed: true,
            },
        )
        .await
        .expect("membrane dev scenario should succeed");

        assert_eq!(snapshot.dataset, SYNTHETIC_AGENT_ROUND_DATASET);
        assert_eq!(snapshot.execution.policy.outcome, emily_membrane::contracts::RoutingPolicyOutcome::LocalOnly);
        assert!(snapshot.execution.local_execution.is_some());
        assert_eq!(snapshot.episode_snapshot.routing_decisions.len(), 1);
        assert_eq!(snapshot.episode_snapshot.validation_outcomes.len(), 1);
    });

    let _ = std::fs::remove_dir_all(storage_path);
}

fn unique_locator() -> emily::DatabaseLocator {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let storage_path = std::env::temp_dir().join(format!(
        "gestalt-emily-membrane-dev-{nonce}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&storage_path);
    emily::DatabaseLocator {
        storage_path,
        namespace: "gestalt_test".to_string(),
        database: "default".to_string(),
    }
}
