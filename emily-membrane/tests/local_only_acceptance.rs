use emily::api::EmilyApi;
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use emily::{CreateEpisodeRequest, DatabaseLocator, EpisodeState, ValidationDecision};
use emily_membrane::contracts::{
    LocalExecutionPersistence, MembraneTaskRequest, PolicyExecutionPersistence,
    RoutingPolicyOutcome, RoutingPolicyRequest, RoutingSensitivity,
};
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

fn brief_task_request() -> MembraneTaskRequest {
    MembraneTaskRequest {
        task_id: "task-local-review".to_string(),
        episode_id: "ep-membrane-local".to_string(),
        task_text: "ok".to_string(),
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
    assert!(first.reconstruction.references.iter().any(|reference| {
        reference.source == emily_membrane::contracts::ReconstructionSource::ReconstructionHandle
    }));

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

#[tokio::test]
async fn broader_policy_execution_runs_local_path_and_records_sovereign_state() {
    let store = Arc::new(SurrealEmilyStore::new());
    let emily = Arc::new(EmilyRuntime::new(store));
    let runtime = MembraneRuntime::new(emily.clone());
    let locator = locator();

    emily.open_db(locator.clone()).await.expect("open db");
    emily
        .create_episode(episode_request())
        .await
        .expect("create episode");

    let result = runtime
        .execute_with_policy_and_record(
            task_request(),
            RoutingPolicyRequest {
                task_id: "task-local-1".to_string(),
                episode_id: "ep-membrane-local".to_string(),
                allow_remote: false,
                sensitivity: RoutingSensitivity::Normal,
                preference: emily_membrane::contracts::RemoteRoutingPreference {
                    provider_id: None,
                    profile_id: None,
                    required_capability_tags: Vec::new(),
                    preferred_provider_classes: Vec::new(),
                    max_latency_class: None,
                    max_cost_class: None,
                    minimum_validation_compatibility: None,
                },
            },
            PolicyExecutionPersistence {
                local: Some(persistence()),
                remote: None,
            },
        )
        .await
        .expect("execute broader policy path");

    assert_eq!(result.policy.outcome, RoutingPolicyOutcome::LocalOnly);
    assert!(result.local_execution.is_some());
    assert!(result.remote_execution.is_none());

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

    assert_eq!(routes.len(), 1);
    assert_eq!(validations.len(), 1);
    assert_eq!(audits.len(), 2);

    emily.close_db().await.expect("close db");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}

#[tokio::test]
async fn local_only_execution_records_review_validation_for_brief_output() {
    let store = Arc::new(SurrealEmilyStore::new());
    let emily = Arc::new(EmilyRuntime::new(store));
    let runtime = MembraneRuntime::new(emily.clone());
    let locator = locator();

    emily.open_db(locator.clone()).await.expect("open db");
    emily
        .create_episode(episode_request())
        .await
        .expect("create episode");

    let result = runtime
        .execute_local_only_and_record(
            brief_task_request(),
            LocalExecutionPersistence {
                route_decision_id: "route-local-review".to_string(),
                route_decided_at_unix_ms: 20,
                validation_id: "validation-local-review".to_string(),
                validated_at_unix_ms: 21,
            },
        )
        .await
        .expect("execute review path");

    assert_eq!(
        result.validation.disposition,
        emily_membrane::contracts::MembraneValidationDisposition::NeedsReview
    );
    assert!(result.reconstruction.caution);
    assert!(
        result
            .reconstruction
            .output_text
            .starts_with("Review required before relying on this output.")
    );
    assert!(result.reconstruction.references.iter().any(|reference| {
        reference.source == emily_membrane::contracts::ReconstructionSource::ValidationPolicy
    }));

    let validations = emily
        .validation_outcomes_for_episode("ep-membrane-local")
        .await
        .expect("list validations");

    assert_eq!(validations.len(), 1);
    assert_eq!(validations[0].decision, ValidationDecision::NeedsReview);
    assert_eq!(
        validations[0].metadata["assessments"]
            .as_array()
            .expect("assessment metadata array")
            .len(),
        4
    );

    emily.close_db().await.expect("close db");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}
