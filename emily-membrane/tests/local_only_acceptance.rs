use emily::api::EmilyApi;
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use emily::{CreateEpisodeRequest, DatabaseLocator, EpisodeState};
use emily_membrane::contracts::{LocalExecutionPersistence, MembraneTaskRequest};
use emily_membrane::runtime::MembraneRuntime;
use serde_json::json;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

fn locator() -> DatabaseLocator {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch");
    let storage_path = std::env::temp_dir().join(format!(
        "emily-membrane-local-acceptance-{}-{}",
        std::process::id(),
        now.as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&storage_path);
    DatabaseLocator {
        storage_path,
        namespace: "ns".to_string(),
        database: "db".to_string(),
    }
}

fn episode_request() -> CreateEpisodeRequest {
    CreateEpisodeRequest {
        episode_id: "ep-membrane-local".to_string(),
        stream_id: Some("stream-a".to_string()),
        source_kind: "membrane-test".to_string(),
        episode_kind: "local_only_round".to_string(),
        started_at_unix_ms: 1,
        intent: Some("prove membrane write path".to_string()),
        metadata: json!({"origin": "integration-test"}),
    }
}

fn task_request() -> MembraneTaskRequest {
    MembraneTaskRequest {
        task_id: "task-local-1".to_string(),
        episode_id: "ep-membrane-local".to_string(),
        task_text: "Summarize the local-only membrane path.".to_string(),
        context_fragments: Vec::new(),
        allow_remote: false,
    }
}

fn persistence() -> LocalExecutionPersistence {
    LocalExecutionPersistence {
        route_decision_id: "route-local-1".to_string(),
        route_decided_at_unix_ms: 10,
        validation_id: "validation-local-1".to_string(),
        validated_at_unix_ms: 11,
    }
}

#[tokio::test]
async fn local_only_execution_records_route_validation_and_audits_idempotently() {
    let store = Arc::new(SurrealEmilyStore::new());
    let emily = Arc::new(EmilyRuntime::new(store));
    let runtime = MembraneRuntime::new(emily.clone());
    let locator = locator();

    emily.open_db(locator.clone()).await.expect("open db");
    emily
        .create_episode(episode_request())
        .await
        .expect("create episode");

    let first = runtime
        .execute_local_only_and_record(task_request(), persistence())
        .await
        .expect("first execution");
    let second = runtime
        .execute_local_only_and_record(task_request(), persistence())
        .await
        .expect("replayed execution");

    assert_eq!(first, second);
    assert_eq!(first.route_decision_id, "route-local-1");
    assert_eq!(first.validation_id, "validation-local-1");
    assert!(first.reconstruction.output_text.starts_with("LOCAL: "));

    let routes = emily
        .routing_decisions_for_episode("ep-membrane-local")
        .await
        .expect("list routes");
    let validations = emily
        .validation_outcomes_for_episode("ep-membrane-local")
        .await
        .expect("list validations");
    let audits = emily
        .sovereign_audit_records_for_episode("ep-membrane-local")
        .await
        .expect("list audits");
    let episode = emily
        .episode("ep-membrane-local")
        .await
        .expect("read episode")
        .expect("episode exists");

    assert_eq!(routes.len(), 1);
    assert_eq!(validations.len(), 1);
    assert_eq!(audits.len(), 2);
    assert_eq!(episode.state, EpisodeState::Open);

    emily.close_db().await.expect("close db");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}
