use async_trait::async_trait;
use emily::api::EmilyApi;
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use emily::{CreateEpisodeRequest, DatabaseLocator, EpisodeState, RemoteEpisodeState};
use emily_membrane::contracts::{MembraneTaskRequest, RemoteExecutionPersistence};
use emily_membrane::providers::{
    MembraneProvider, MembraneProviderError, ProviderDispatchRequest, ProviderDispatchResult,
    ProviderDispatchStatus, ProviderTarget,
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
        "emily-membrane-remote-acceptance-{}-{}",
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
        episode_id: "ep-membrane-remote".to_string(),
        stream_id: Some("stream-a".to_string()),
        source_kind: "membrane-test".to_string(),
        episode_kind: "single_remote_round".to_string(),
        started_at_unix_ms: 1,
        intent: Some("prove membrane remote write path".to_string()),
        metadata: json!({"origin": "integration-test"}),
    }
}

fn task_request() -> MembraneTaskRequest {
    MembraneTaskRequest {
        task_id: "task-remote-1".to_string(),
        episode_id: "ep-membrane-remote".to_string(),
        task_text: "Summarize the remote membrane path.".to_string(),
        context_fragments: Vec::new(),
        allow_remote: true,
    }
}

fn persistence() -> RemoteExecutionPersistence {
    RemoteExecutionPersistence {
        route_decision_id: "route-remote-1".to_string(),
        route_decided_at_unix_ms: 10,
        provider_request_id: "provider-request-1".to_string(),
        remote_episode_id: "remote-1".to_string(),
        remote_dispatched_at_unix_ms: 11,
        validation_id: "validation-remote-1".to_string(),
        validated_at_unix_ms: 12,
    }
}

fn target() -> ProviderTarget {
    ProviderTarget {
        provider_id: "test-provider".to_string(),
        model_id: Some("deterministic-v1".to_string()),
        profile_id: Some("reasoning".to_string()),
        capability_tags: vec!["analysis".to_string()],
        metadata: json!({"origin": "test"}),
    }
}

struct DeterministicTestProvider;

#[async_trait]
impl MembraneProvider for DeterministicTestProvider {
    fn provider_id(&self) -> &str {
        "test-provider"
    }

    async fn dispatch(
        &self,
        request: ProviderDispatchRequest,
    ) -> Result<ProviderDispatchResult, MembraneProviderError> {
        Ok(ProviderDispatchResult {
            provider_request_id: request.provider_request_id,
            provider_id: self.provider_id().to_string(),
            status: ProviderDispatchStatus::Completed,
            output_text: format!("REMOTE: {}", request.bounded_payload),
            metadata: json!({"mode": "deterministic"}),
        })
    }
}

#[tokio::test]
async fn remote_execution_records_route_remote_episode_validation_and_audits_idempotently() {
    let store = Arc::new(SurrealEmilyStore::new());
    let emily = Arc::new(EmilyRuntime::new(store));
    let runtime =
        MembraneRuntime::with_provider(emily.clone(), Arc::new(DeterministicTestProvider));
    let locator = locator();

    emily.open_db(locator.clone()).await.expect("open db");
    emily
        .create_episode(episode_request())
        .await
        .expect("create episode");

    let first = runtime
        .execute_remote_and_record(task_request(), target(), persistence())
        .await
        .expect("first execution");
    let second = runtime
        .execute_remote_and_record(task_request(), target(), persistence())
        .await
        .expect("replayed execution");

    assert_eq!(first, second);
    assert_eq!(first.route_decision_id, "route-remote-1");
    assert_eq!(first.remote_episode_id, "remote-1");
    assert_eq!(first.validation_id, "validation-remote-1");
    assert!(first.reconstruction.output_text.starts_with("REMOTE: "));

    let routes = emily
        .routing_decisions_for_episode("ep-membrane-remote")
        .await
        .expect("list routes");
    let remote_episodes = emily
        .remote_episodes_for_episode("ep-membrane-remote")
        .await
        .expect("list remote episodes");
    let validations = emily
        .validation_outcomes_for_episode("ep-membrane-remote")
        .await
        .expect("list validations");
    let audits = emily
        .sovereign_audit_records_for_episode("ep-membrane-remote")
        .await
        .expect("list audits");
    let episode = emily
        .episode("ep-membrane-remote")
        .await
        .expect("read episode")
        .expect("episode exists");

    assert_eq!(routes.len(), 1);
    assert_eq!(remote_episodes.len(), 1);
    assert_eq!(remote_episodes[0].state, RemoteEpisodeState::Succeeded);
    assert_eq!(validations.len(), 1);
    assert_eq!(audits.len(), 3);
    assert_eq!(episode.state, EpisodeState::Open);

    emily.close_db().await.expect("close db");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}
