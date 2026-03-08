use async_trait::async_trait;
use emily::api::EmilyApi;
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use emily::{
    CreateEpisodeRequest, DatabaseLocator, EarlDecision, EarlEvaluationRequest, EarlSignalVector,
    EpisodeState, RemoteEpisodeState,
};
use emily_membrane::contracts::{
    MembraneTaskRequest, RemoteExecutionPersistence, RemoteRoutingPreference, RoutingPolicyOutcome,
    RoutingPolicyRequest, RoutingSensitivity,
};
use emily_membrane::providers::{
    InMemoryProviderRegistry, MembraneProvider, MembraneProviderError, ProviderDispatchRequest,
    ProviderDispatchResult, ProviderDispatchStatus, ProviderTarget, RegisteredProviderTarget,
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

fn routing_preference() -> RemoteRoutingPreference {
    RemoteRoutingPreference {
        provider_id: Some("test-provider".to_string()),
        profile_id: Some("reasoning".to_string()),
        required_capability_tags: vec!["analysis".to_string()],
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

fn caution_earl_request() -> EarlEvaluationRequest {
    EarlEvaluationRequest {
        evaluation_id: "earl-caution-1".to_string(),
        episode_id: "ep-membrane-remote".to_string(),
        evaluated_at_unix_ms: 5,
        signals: EarlSignalVector {
            uncertainty: 0.7,
            conflict: 0.65,
            continuity_drift: 0.5,
            constraint_pressure: 0.2,
            tool_instability: 0.1,
            novelty_spike: 0.2,
        },
        metadata: json!({"origin": "integration-test"}),
    }
}

fn reflex_earl_request() -> EarlEvaluationRequest {
    EarlEvaluationRequest {
        evaluation_id: "earl-reflex-1".to_string(),
        episode_id: "ep-membrane-remote".to_string(),
        evaluated_at_unix_ms: 5,
        signals: EarlSignalVector {
            uncertainty: 0.8,
            conflict: 0.3,
            continuity_drift: 0.95,
            constraint_pressure: 0.2,
            tool_instability: 0.1,
            novelty_spike: 0.2,
        },
        metadata: json!({"origin": "integration-test"}),
    }
}

#[tokio::test]
async fn remote_execution_records_route_remote_episode_validation_and_audits_idempotently() {
    let store = Arc::new(SurrealEmilyStore::new());
    let emily = Arc::new(EmilyRuntime::new(store));
    let registry = Arc::new(InMemoryProviderRegistry::single_target(
        RegisteredProviderTarget {
            target: ProviderTarget {
                provider_id: "test-provider".to_string(),
                model_id: Some("deterministic-v1".to_string()),
                profile_id: Some("reasoning".to_string()),
                capability_tags: vec!["analysis".to_string()],
                metadata: json!({"origin": "test"}),
            },
        },
        Arc::new(DeterministicTestProvider) as Arc<dyn MembraneProvider>,
    ));
    let runtime = MembraneRuntime::with_provider_registry(emily.clone(), registry);
    let locator = locator();

    emily.open_db(locator.clone()).await.expect("open db");
    emily
        .create_episode(episode_request())
        .await
        .expect("create episode");

    let first = runtime
        .execute_remote_with_registry_and_record(
            task_request(),
            routing_preference(),
            persistence(),
        )
        .await
        .expect("first execution");
    let second = runtime
        .execute_remote_with_registry_and_record(
            task_request(),
            routing_preference(),
            persistence(),
        )
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

#[tokio::test]
async fn routing_policy_uses_earl_caution_before_remote_execution_and_persists_sovereign_records() {
    let store = Arc::new(SurrealEmilyStore::new());
    let emily = Arc::new(EmilyRuntime::new(store.clone()));
    let registry = Arc::new(InMemoryProviderRegistry::single_target(
        RegisteredProviderTarget {
            target: ProviderTarget {
                provider_id: "test-provider".to_string(),
                model_id: Some("deterministic-v1".to_string()),
                profile_id: Some("reasoning".to_string()),
                capability_tags: vec!["analysis".to_string()],
                metadata: json!({"origin": "test"}),
            },
        },
        Arc::new(DeterministicTestProvider) as Arc<dyn MembraneProvider>,
    ));
    let runtime = MembraneRuntime::with_provider_registry(emily.clone(), registry);
    let locator = locator();

    emily.open_db(locator.clone()).await.expect("open db");
    emily
        .create_episode(episode_request())
        .await
        .expect("create episode");
    let earl = emily
        .evaluate_episode_risk(caution_earl_request())
        .await
        .expect("evaluate earl");

    assert_eq!(earl.decision, EarlDecision::Caution);

    let policy = runtime
        .evaluate_routing_policy(RoutingPolicyRequest {
            task_id: "task-remote-1".to_string(),
            episode_id: "ep-membrane-remote".to_string(),
            allow_remote: true,
            sensitivity: RoutingSensitivity::Normal,
            preference: routing_preference(),
        })
        .await
        .expect("evaluate routing policy");

    assert_eq!(policy.outcome, RoutingPolicyOutcome::SingleRemote);
    assert!(policy.caution);
    assert!(
        policy
            .findings
            .iter()
            .any(|finding| finding.code == "earl-caution-gate")
    );

    let target = policy.selected_target.expect("selected target");
    let execution = runtime
        .execute_remote_and_record(task_request(), target, persistence())
        .await
        .expect("execute remote");

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

    assert_eq!(execution.route_decision_id, "route-remote-1");
    assert_eq!(routes.len(), 1);
    assert_eq!(remote_episodes.len(), 1);
    assert_eq!(validations.len(), 1);

    emily.close_db().await.expect("close db");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}

#[tokio::test]
async fn routing_policy_rejects_remote_dispatch_when_earl_reflex_blocks_episode() {
    let store = Arc::new(SurrealEmilyStore::new());
    let emily = Arc::new(EmilyRuntime::new(store.clone()));
    let registry = Arc::new(InMemoryProviderRegistry::single_target(
        RegisteredProviderTarget {
            target: ProviderTarget {
                provider_id: "test-provider".to_string(),
                model_id: Some("deterministic-v1".to_string()),
                profile_id: Some("reasoning".to_string()),
                capability_tags: vec!["analysis".to_string()],
                metadata: json!({"origin": "test"}),
            },
        },
        Arc::new(DeterministicTestProvider) as Arc<dyn MembraneProvider>,
    ));
    let runtime = MembraneRuntime::with_provider_registry(emily.clone(), registry);
    let locator = locator();

    emily.open_db(locator.clone()).await.expect("open db");
    emily
        .create_episode(episode_request())
        .await
        .expect("create episode");
    let earl = emily
        .evaluate_episode_risk(reflex_earl_request())
        .await
        .expect("evaluate earl");

    assert_eq!(earl.decision, EarlDecision::Reflex);

    let policy = runtime
        .evaluate_routing_policy(RoutingPolicyRequest {
            task_id: "task-remote-1".to_string(),
            episode_id: "ep-membrane-remote".to_string(),
            allow_remote: true,
            sensitivity: RoutingSensitivity::Normal,
            preference: routing_preference(),
        })
        .await
        .expect("evaluate routing policy");

    assert_eq!(policy.outcome, RoutingPolicyOutcome::Rejected);
    assert!(policy.selected_target.is_none());
    assert_eq!(policy.findings[0].code, "earl-reflex-gate");

    let routes = emily
        .routing_decisions_for_episode("ep-membrane-remote")
        .await
        .expect("list routes");
    assert!(routes.is_empty());

    emily.close_db().await.expect("close db");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}
