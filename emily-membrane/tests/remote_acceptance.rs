use async_trait::async_trait;
use emily::api::EmilyApi;
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use emily::{
    AuditRecordKind, CreateEpisodeRequest, DatabaseLocator, EarlDecision, EarlEvaluationRequest,
    EarlSignalVector, EpisodeState, RemoteEpisodeState, ValidationDecision,
};
use emily_membrane::contracts::{
    LocalExecutionPersistence, MembraneTaskRequest, MultiRemoteExecutionPersistence,
    MultiRemoteExecutionPolicy, MultiRemoteReconciliationDecision, MultiRemoteStopCondition,
    PolicyExecutionPersistence, RemoteExecutionPersistence, RemoteRetryAttemptPersistence,
    RemoteRetryExecutionPersistence, RemoteRetryPolicy, RemoteRoutingPreference,
    RetryMutationStrategy, RoutingPolicyOutcome, RoutingPolicyRequest, RoutingSensitivity,
};
use emily_membrane::providers::{
    InMemoryProviderRegistry, MembraneProvider, MembraneProviderError, ProviderCostClass,
    ProviderDispatchRequest, ProviderDispatchResult, ProviderDispatchStatus, ProviderLatencyClass,
    ProviderMetadataClass, ProviderTarget, ProviderValidationCompatibility,
    RegisteredProviderTarget,
};
use emily_membrane::runtime::MembraneRuntime;
use serde_json::json;
use std::sync::{Arc, Mutex};
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

fn protected_task_request() -> MembraneTaskRequest {
    MembraneTaskRequest {
        task_id: "task-remote-protected".to_string(),
        episode_id: "ep-membrane-remote".to_string(),
        task_text: "Summarize the remote membrane path with jeremy@example.com.".to_string(),
        context_fragments: vec![emily_membrane::contracts::ContextFragment {
            fragment_id: "ctx-secret".to_string(),
            text: "Use API_KEY=abcd1234 for /media/jeremy/OrangeCream/Linux Software/Gestalt."
                .to_string(),
        }],
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
        preferred_provider_classes: Vec::new(),
        max_latency_class: None,
        max_cost_class: None,
        minimum_validation_compatibility: None,
    }
}

fn panic_routing_preference() -> RemoteRoutingPreference {
    RemoteRoutingPreference {
        provider_id: Some("panic-provider".to_string()),
        profile_id: Some("reasoning".to_string()),
        required_capability_tags: vec!["analysis".to_string()],
        preferred_provider_classes: Vec::new(),
        max_latency_class: None,
        max_cost_class: None,
        minimum_validation_compatibility: None,
    }
}

fn default_registered_provider_target(target: ProviderTarget) -> RegisteredProviderTarget {
    RegisteredProviderTarget {
        target,
        metadata_class: ProviderMetadataClass::Standard,
        latency_class: ProviderLatencyClass::Medium,
        cost_class: ProviderCostClass::Medium,
        validation_compatibility: ProviderValidationCompatibility::Basic,
        telemetry: None,
    }
}

struct DeterministicTestProvider;

struct ReviewThenSuccessProvider {
    attempt: Mutex<u8>,
}

struct ErrorThenErrorProvider {
    attempt: Mutex<u8>,
}

struct ReviewStatusProvider;

struct AcceptedProvider {
    provider_id: &'static str,
}

struct PanicIfCalledProvider;

#[async_trait]
impl MembraneProvider for DeterministicTestProvider {
    fn provider_id(&self) -> &str {
        "test-provider"
    }

    async fn dispatch(
        &self,
        request: ProviderDispatchRequest,
    ) -> Result<ProviderDispatchResult, MembraneProviderError> {
        let ir = request
            .membrane_ir
            .as_ref()
            .expect("remote dispatch should receive typed membrane ir");
        assert_eq!(ir.task.task_id, request.task_id);
        assert_eq!(ir.task.episode_id, request.episode_id);
        Ok(ProviderDispatchResult {
            provider_request_id: request.provider_request_id,
            provider_id: self.provider_id().to_string(),
            status: ProviderDispatchStatus::Completed,
            output_text: format!("REMOTE: {}", request.bounded_payload),
            metadata: json!({"mode": "deterministic"}),
        })
    }
}

#[async_trait]
impl MembraneProvider for ReviewThenSuccessProvider {
    fn provider_id(&self) -> &str {
        "test-provider"
    }

    async fn dispatch(
        &self,
        request: ProviderDispatchRequest,
    ) -> Result<ProviderDispatchResult, MembraneProviderError> {
        let mut attempt = self.attempt.lock().expect("attempt mutex");
        *attempt += 1;

        Ok(ProviderDispatchResult {
            provider_request_id: request.provider_request_id,
            provider_id: self.provider_id().to_string(),
            status: if *attempt == 1 {
                ProviderDispatchStatus::Failed
            } else {
                assert!(
                    request.bounded_payload.contains("Retry note:"),
                    "second attempt should receive mutated retry hint"
                );
                ProviderDispatchStatus::Completed
            },
            output_text: if *attempt == 1 {
                "transient remote ambiguity".to_string()
            } else {
                format!("REMOTE: {}", request.bounded_payload)
            },
            metadata: json!({"attempt": *attempt}),
        })
    }
}

#[async_trait]
impl MembraneProvider for ErrorThenErrorProvider {
    fn provider_id(&self) -> &str {
        "test-provider"
    }

    async fn dispatch(
        &self,
        request: ProviderDispatchRequest,
    ) -> Result<ProviderDispatchResult, MembraneProviderError> {
        let mut attempt = self.attempt.lock().expect("attempt mutex");
        *attempt += 1;
        if *attempt > 1 {
            assert!(
                request.bounded_payload.contains("Retry note:"),
                "second attempt should receive mutated retry hint"
            );
        }
        Err(MembraneProviderError::Execution(format!(
            "transient transport failure attempt {}",
            *attempt
        )))
    }
}

#[async_trait]
impl MembraneProvider for ReviewStatusProvider {
    fn provider_id(&self) -> &str {
        "review-provider"
    }

    async fn dispatch(
        &self,
        request: ProviderDispatchRequest,
    ) -> Result<ProviderDispatchResult, MembraneProviderError> {
        Ok(ProviderDispatchResult {
            provider_request_id: request.provider_request_id,
            provider_id: self.provider_id().to_string(),
            status: ProviderDispatchStatus::Failed,
            output_text: "review this remote result".to_string(),
            metadata: json!({"mode": "review"}),
        })
    }
}

#[async_trait]
impl MembraneProvider for AcceptedProvider {
    fn provider_id(&self) -> &str {
        self.provider_id
    }

    async fn dispatch(
        &self,
        request: ProviderDispatchRequest,
    ) -> Result<ProviderDispatchResult, MembraneProviderError> {
        Ok(ProviderDispatchResult {
            provider_request_id: request.provider_request_id,
            provider_id: self.provider_id().to_string(),
            status: ProviderDispatchStatus::Completed,
            output_text: format!("REMOTE({}): {}", self.provider_id, request.bounded_payload),
            metadata: json!({"mode": "accepted"}),
        })
    }
}

#[async_trait]
impl MembraneProvider for PanicIfCalledProvider {
    fn provider_id(&self) -> &str {
        "panic-provider"
    }

    async fn dispatch(
        &self,
        _request: ProviderDispatchRequest,
    ) -> Result<ProviderDispatchResult, MembraneProviderError> {
        panic!("panic-provider should have been skipped");
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

fn retry_policy() -> RemoteRetryPolicy {
    RemoteRetryPolicy {
        max_attempts: 2,
        retry_on_provider_error: true,
        retry_on_validation_review: true,
        mutation: RetryMutationStrategy::AppendRetryHintV1,
    }
}

fn retry_persistence() -> RemoteRetryExecutionPersistence {
    RemoteRetryExecutionPersistence {
        route_decision_id: "route-remote-retry".to_string(),
        route_decided_at_unix_ms: 30,
        attempts: vec![
            RemoteRetryAttemptPersistence {
                provider_request_id: "provider-request-retry-1".to_string(),
                remote_episode_id: "remote-retry-1".to_string(),
                remote_dispatched_at_unix_ms: 31,
                validation_id: "validation-retry-1".to_string(),
                validated_at_unix_ms: 32,
                retry_audit_id: None,
                retry_audit_at_unix_ms: None,
                mutation_audit_id: None,
                mutation_audit_at_unix_ms: None,
            },
            RemoteRetryAttemptPersistence {
                provider_request_id: "provider-request-retry-2".to_string(),
                remote_episode_id: "remote-retry-2".to_string(),
                remote_dispatched_at_unix_ms: 41,
                validation_id: "validation-retry-2".to_string(),
                validated_at_unix_ms: 42,
                retry_audit_id: Some("audit-retry-2".to_string()),
                retry_audit_at_unix_ms: Some(40),
                mutation_audit_id: Some("audit-mutation-2".to_string()),
                mutation_audit_at_unix_ms: Some(40),
            },
        ],
    }
}

fn multi_remote_targets() -> Vec<ProviderTarget> {
    vec![
        ProviderTarget {
            provider_id: "review-provider".to_string(),
            model_id: Some("review-v1".to_string()),
            profile_id: Some("reasoning".to_string()),
            capability_tags: vec!["analysis".to_string()],
            metadata: json!({"origin": "test"}),
        },
        ProviderTarget {
            provider_id: "accept-provider".to_string(),
            model_id: Some("accept-v1".to_string()),
            profile_id: Some("reasoning".to_string()),
            capability_tags: vec!["analysis".to_string()],
            metadata: json!({"origin": "test"}),
        },
    ]
}

fn multi_remote_policy_exhaust() -> MultiRemoteExecutionPolicy {
    MultiRemoteExecutionPolicy {
        max_targets: 2,
        stop_condition: MultiRemoteStopCondition::ExhaustTargets,
        reconciliation:
            emily_membrane::contracts::MultiRemoteReconciliationMode::FirstAcceptedElseNeedsReview,
    }
}

fn multi_remote_policy_stop_on_accepted() -> MultiRemoteExecutionPolicy {
    MultiRemoteExecutionPolicy {
        max_targets: 2,
        stop_condition: MultiRemoteStopCondition::StopOnAccepted,
        reconciliation:
            emily_membrane::contracts::MultiRemoteReconciliationMode::FirstAcceptedElseNeedsReview,
    }
}

fn multi_remote_persistence() -> MultiRemoteExecutionPersistence {
    MultiRemoteExecutionPersistence {
        route_decision_id: "route-multi-1".to_string(),
        route_decided_at_unix_ms: 60,
        attempts: vec![
            emily_membrane::contracts::MultiRemoteAttemptPersistence {
                provider_request_id: "provider-request-multi-1".to_string(),
                remote_episode_id: "remote-multi-1".to_string(),
                remote_dispatched_at_unix_ms: 61,
                validation_id: "validation-multi-1".to_string(),
                validated_at_unix_ms: 62,
            },
            emily_membrane::contracts::MultiRemoteAttemptPersistence {
                provider_request_id: "provider-request-multi-2".to_string(),
                remote_episode_id: "remote-multi-2".to_string(),
                remote_dispatched_at_unix_ms: 63,
                validation_id: "validation-multi-2".to_string(),
                validated_at_unix_ms: 64,
            },
        ],
        reconciliation_audit_id: "audit-multi-reconcile-1".to_string(),
        reconciled_at_unix_ms: 65,
    }
}

#[tokio::test]
async fn remote_execution_records_route_remote_episode_validation_and_audits_idempotently() {
    let store = Arc::new(SurrealEmilyStore::new());
    let emily = Arc::new(EmilyRuntime::new(store));
    let registry = Arc::new(InMemoryProviderRegistry::single_target(
        default_registered_provider_target(ProviderTarget {
            provider_id: "test-provider".to_string(),
            model_id: Some("deterministic-v1".to_string()),
            profile_id: Some("reasoning".to_string()),
            capability_tags: vec!["analysis".to_string()],
            metadata: json!({"origin": "test"}),
        }),
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
    assert!(
        first
            .reconstruction
            .output_text
            .starts_with("Membrane rendered remote output from 'remote-1'.")
    );
    assert!(first.reconstruction.references.iter().any(|reference| {
        reference.source == emily_membrane::contracts::ReconstructionSource::RemoteResult
            && reference.reference_id == "remote-1"
    }));

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
async fn policy_selected_remote_execution_uses_earl_caution_and_persists_sovereign_records() {
    let store = Arc::new(SurrealEmilyStore::new());
    let emily = Arc::new(EmilyRuntime::new(store.clone()));
    let registry = Arc::new(InMemoryProviderRegistry::single_target(
        default_registered_provider_target(ProviderTarget {
            provider_id: "test-provider".to_string(),
            model_id: Some("deterministic-v1".to_string()),
            profile_id: Some("reasoning".to_string()),
            capability_tags: vec!["analysis".to_string()],
            metadata: json!({"origin": "test"}),
        }),
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

    let result = runtime
        .execute_remote_with_policy_and_record(
            task_request(),
            RoutingPolicyRequest {
                task_id: "task-remote-1".to_string(),
                episode_id: "ep-membrane-remote".to_string(),
                allow_remote: true,
                sensitivity: RoutingSensitivity::Normal,
                preference: routing_preference(),
            },
            persistence(),
        )
        .await
        .expect("execute policy-selected remote");

    assert_eq!(result.policy.outcome, RoutingPolicyOutcome::SingleRemote);
    assert!(result.policy.caution);
    assert!(
        result
            .policy
            .findings
            .iter()
            .any(|finding| finding.code == "earl-caution-gate")
    );

    let execution = result.remote_execution.expect("remote execution");

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
async fn broader_policy_execution_runs_remote_path_and_persists_sovereign_records() {
    let store = Arc::new(SurrealEmilyStore::new());
    let emily = Arc::new(EmilyRuntime::new(store.clone()));
    let registry = Arc::new(InMemoryProviderRegistry::single_target(
        default_registered_provider_target(ProviderTarget {
            provider_id: "test-provider".to_string(),
            model_id: Some("deterministic-v1".to_string()),
            profile_id: Some("reasoning".to_string()),
            capability_tags: vec!["analysis".to_string()],
            metadata: json!({"origin": "test"}),
        }),
        Arc::new(DeterministicTestProvider) as Arc<dyn MembraneProvider>,
    ));
    let runtime = MembraneRuntime::with_provider_registry(emily.clone(), registry);
    let locator = locator();

    emily.open_db(locator.clone()).await.expect("open db");
    emily
        .create_episode(episode_request())
        .await
        .expect("create episode");
    emily
        .evaluate_episode_risk(caution_earl_request())
        .await
        .expect("evaluate earl");

    let result = runtime
        .execute_with_policy_and_record(
            task_request(),
            RoutingPolicyRequest {
                task_id: "task-remote-1".to_string(),
                episode_id: "ep-membrane-remote".to_string(),
                allow_remote: true,
                sensitivity: RoutingSensitivity::Normal,
                preference: routing_preference(),
            },
            PolicyExecutionPersistence {
                reflex: None,
                local: None,
                remote: Some(persistence()),
            },
        )
        .await
        .expect("execute broader policy path");

    assert_eq!(result.policy.outcome, RoutingPolicyOutcome::SingleRemote);
    assert!(result.local_execution.is_none());
    assert!(result.remote_execution.is_some());

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

    assert_eq!(routes.len(), 1);
    assert_eq!(remote_episodes.len(), 1);
    assert_eq!(validations.len(), 1);

    emily.close_db().await.expect("close db");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}

#[tokio::test]
async fn policy_selected_remote_execution_returns_policy_only_when_earl_reflex_blocks_episode() {
    let store = Arc::new(SurrealEmilyStore::new());
    let emily = Arc::new(EmilyRuntime::new(store.clone()));
    let registry = Arc::new(InMemoryProviderRegistry::single_target(
        default_registered_provider_target(ProviderTarget {
            provider_id: "test-provider".to_string(),
            model_id: Some("deterministic-v1".to_string()),
            profile_id: Some("reasoning".to_string()),
            capability_tags: vec!["analysis".to_string()],
            metadata: json!({"origin": "test"}),
        }),
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

    let result = runtime
        .execute_remote_with_policy_and_record(
            task_request(),
            RoutingPolicyRequest {
                task_id: "task-remote-1".to_string(),
                episode_id: "ep-membrane-remote".to_string(),
                allow_remote: true,
                sensitivity: RoutingSensitivity::Normal,
                preference: routing_preference(),
            },
            persistence(),
        )
        .await
        .expect("execute policy-selected route");

    assert_eq!(result.policy.outcome, RoutingPolicyOutcome::Reflex);
    assert_eq!(
        result.policy.reflex_reason,
        Some(emily_membrane::contracts::RoutingPolicyReflexReason::EarlReflex)
    );
    assert_eq!(
        result.reflex_audit_id.as_deref(),
        Some("task-remote-1:reflex:audit")
    );
    assert!(result.policy.selected_target.is_none());
    assert_eq!(result.policy.findings[0].code, "earl-reflex-gate");
    assert!(result.remote_execution.is_none());

    let routes = emily
        .routing_decisions_for_episode("ep-membrane-remote")
        .await
        .expect("list routes");
    let audits = emily
        .sovereign_audit_records_for_episode("ep-membrane-remote")
        .await
        .expect("list audits");
    assert!(routes.is_empty());
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].id, "task-remote-1:reflex:audit");
    assert_eq!(
        audits[0].metadata["sovereign"]["boundary_profile"],
        "reflex"
    );

    emily.close_db().await.expect("close db");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}

#[tokio::test]
async fn broader_policy_execution_returns_policy_only_for_rejected_route() {
    let store = Arc::new(SurrealEmilyStore::new());
    let emily = Arc::new(EmilyRuntime::new(store.clone()));
    let registry = Arc::new(InMemoryProviderRegistry::single_target(
        default_registered_provider_target(ProviderTarget {
            provider_id: "test-provider".to_string(),
            model_id: Some("deterministic-v1".to_string()),
            profile_id: Some("reasoning".to_string()),
            capability_tags: vec!["analysis".to_string()],
            metadata: json!({"origin": "test"}),
        }),
        Arc::new(DeterministicTestProvider) as Arc<dyn MembraneProvider>,
    ));
    let runtime = MembraneRuntime::with_provider_registry(emily.clone(), registry);
    let locator = locator();

    emily.open_db(locator.clone()).await.expect("open db");
    emily
        .create_episode(episode_request())
        .await
        .expect("create episode");
    emily
        .evaluate_episode_risk(reflex_earl_request())
        .await
        .expect("evaluate earl");

    let result = runtime
        .execute_with_policy_and_record(
            task_request(),
            RoutingPolicyRequest {
                task_id: "task-remote-1".to_string(),
                episode_id: "ep-membrane-remote".to_string(),
                allow_remote: true,
                sensitivity: RoutingSensitivity::Normal,
                preference: routing_preference(),
            },
            PolicyExecutionPersistence {
                reflex: Some(emily_membrane::contracts::PolicyReflexPersistence {
                    audit_id: "audit-reflex-1".to_string(),
                    audited_at_unix_ms: 9,
                }),
                ..PolicyExecutionPersistence::default()
            },
        )
        .await
        .expect("execute broader policy path");

    assert_eq!(result.policy.outcome, RoutingPolicyOutcome::Reflex);
    assert_eq!(
        result.policy.reflex_reason,
        Some(emily_membrane::contracts::RoutingPolicyReflexReason::EarlReflex)
    );
    assert_eq!(result.reflex_audit_id.as_deref(), Some("audit-reflex-1"));
    assert!(result.local_execution.is_none());
    assert!(result.remote_execution.is_none());

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

    assert!(routes.is_empty());
    assert!(remote_episodes.is_empty());
    assert!(validations.is_empty());
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].id, "audit-reflex-1");
    assert_eq!(
        audits[0].metadata["sovereign"]["boundary_profile"],
        "reflex"
    );

    emily.close_db().await.expect("close db");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}

#[tokio::test]
async fn protected_content_reflex_blocks_remote_dispatch_before_provider_call() {
    let store = Arc::new(SurrealEmilyStore::new());
    let emily = Arc::new(EmilyRuntime::new(store.clone()));
    let registry = Arc::new(InMemoryProviderRegistry::single_target(
        default_registered_provider_target(ProviderTarget {
            provider_id: "panic-provider".to_string(),
            model_id: Some("deterministic-v1".to_string()),
            profile_id: Some("reasoning".to_string()),
            capability_tags: vec!["analysis".to_string()],
            metadata: json!({"origin": "test"}),
        }),
        Arc::new(PanicIfCalledProvider) as Arc<dyn MembraneProvider>,
    ));
    let runtime = MembraneRuntime::with_provider_registry(emily.clone(), registry);
    let locator = locator();

    emily.open_db(locator.clone()).await.expect("open db");
    emily
        .create_episode(episode_request())
        .await
        .expect("create episode");

    let result = runtime
        .execute_remote_with_policy_and_record(
            protected_task_request(),
            RoutingPolicyRequest {
                task_id: "task-remote-protected".to_string(),
                episode_id: "ep-membrane-remote".to_string(),
                allow_remote: true,
                sensitivity: RoutingSensitivity::Normal,
                preference: panic_routing_preference(),
            },
            RemoteExecutionPersistence {
                route_decision_id: "route-protected-remote".to_string(),
                route_decided_at_unix_ms: 20,
                provider_request_id: "provider-request-protected".to_string(),
                remote_episode_id: "remote-protected".to_string(),
                remote_dispatched_at_unix_ms: 21,
                validation_id: "validation-protected".to_string(),
                validated_at_unix_ms: 22,
            },
        )
        .await
        .expect("execute protected-content policy path");

    assert_eq!(result.policy.outcome, RoutingPolicyOutcome::Reflex);
    assert_eq!(
        result.policy.reflex_reason,
        Some(emily_membrane::contracts::RoutingPolicyReflexReason::ProtectedContent)
    );
    assert_eq!(
        result.reflex_audit_id.as_deref(),
        Some("task-remote-protected:reflex:audit")
    );
    assert!(result.remote_execution.is_none());
    assert!(
        result
            .policy
            .findings
            .iter()
            .any(|finding| finding.code == "protected-content-blocked")
    );

    let routes = emily
        .routing_decisions_for_episode("ep-membrane-remote")
        .await
        .expect("list routes");
    let remote_episodes = emily
        .remote_episodes_for_episode("ep-membrane-remote")
        .await
        .expect("list remote episodes");
    let audits = emily
        .sovereign_audit_records_for_episode("ep-membrane-remote")
        .await
        .expect("list audits");

    assert!(routes.is_empty());
    assert!(remote_episodes.is_empty());
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].id, "task-remote-protected:reflex:audit");
    assert_eq!(
        audits[0].metadata["sovereign"]["metadata"]["reflex_reason"],
        "protected-content"
    );

    emily.close_db().await.expect("close db");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}

#[tokio::test]
async fn protected_content_reflex_falls_back_to_local_execution() {
    let store = Arc::new(SurrealEmilyStore::new());
    let emily = Arc::new(EmilyRuntime::new(store.clone()));
    let registry = Arc::new(InMemoryProviderRegistry::single_target(
        default_registered_provider_target(ProviderTarget {
            provider_id: "panic-provider".to_string(),
            model_id: Some("deterministic-v1".to_string()),
            profile_id: Some("reasoning".to_string()),
            capability_tags: vec!["analysis".to_string()],
            metadata: json!({"origin": "test"}),
        }),
        Arc::new(PanicIfCalledProvider) as Arc<dyn MembraneProvider>,
    ));
    let runtime = MembraneRuntime::with_provider_registry(emily.clone(), registry);
    let locator = locator();

    emily.open_db(locator.clone()).await.expect("open db");
    emily
        .create_episode(episode_request())
        .await
        .expect("create episode");

    let result = runtime
        .execute_with_policy_and_record(
            protected_task_request(),
            RoutingPolicyRequest {
                task_id: "task-remote-protected".to_string(),
                episode_id: "ep-membrane-remote".to_string(),
                allow_remote: true,
                sensitivity: RoutingSensitivity::Normal,
                preference: panic_routing_preference(),
            },
            PolicyExecutionPersistence {
                reflex: Some(emily_membrane::contracts::PolicyReflexPersistence {
                    audit_id: "audit-reflex-protected".to_string(),
                    audited_at_unix_ms: 20,
                }),
                local: Some(LocalExecutionPersistence {
                    route_decision_id: "route-local-protected".to_string(),
                    route_decided_at_unix_ms: 21,
                    validation_id: "validation-local-protected".to_string(),
                    validated_at_unix_ms: 22,
                }),
                remote: Some(RemoteExecutionPersistence {
                    route_decision_id: "route-remote-protected".to_string(),
                    route_decided_at_unix_ms: 20,
                    provider_request_id: "provider-request-protected".to_string(),
                    remote_episode_id: "remote-protected".to_string(),
                    remote_dispatched_at_unix_ms: 21,
                    validation_id: "validation-protected".to_string(),
                    validated_at_unix_ms: 22,
                }),
            },
        )
        .await
        .expect("execute protected-content fallback path");

    assert_eq!(result.policy.outcome, RoutingPolicyOutcome::Reflex);
    assert_eq!(
        result.policy.reflex_reason,
        Some(emily_membrane::contracts::RoutingPolicyReflexReason::ProtectedContent)
    );
    assert_eq!(
        result.reflex_audit_id.as_deref(),
        Some("audit-reflex-protected")
    );
    assert!(result.remote_execution.is_none());
    assert!(result.local_execution.is_some());
    assert!(
        result
            .local_execution
            .as_ref()
            .expect("local execution")
            .dispatch
            .response_text
            .starts_with("LOCAL:")
    );

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

    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].kind, emily::RoutingDecisionKind::LocalOnly);
    assert!(remote_episodes.is_empty());
    assert_eq!(validations.len(), 1);
    assert!(
        audits
            .iter()
            .any(|audit| audit.id == "audit-reflex-protected")
    );

    emily.close_db().await.expect("close db");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}

#[tokio::test]
async fn remote_retry_execution_retries_review_and_records_boundary_audits() {
    let store = Arc::new(SurrealEmilyStore::new());
    let emily = Arc::new(EmilyRuntime::new(store));
    let registry = Arc::new(InMemoryProviderRegistry::single_target(
        default_registered_provider_target(ProviderTarget {
            provider_id: "test-provider".to_string(),
            model_id: Some("deterministic-v1".to_string()),
            profile_id: Some("reasoning".to_string()),
            capability_tags: vec!["analysis".to_string()],
            metadata: json!({"origin": "test"}),
        }),
        Arc::new(ReviewThenSuccessProvider {
            attempt: Mutex::new(0),
        }) as Arc<dyn MembraneProvider>,
    ));
    let runtime = MembraneRuntime::with_provider_registry(emily.clone(), registry);
    let locator = locator();

    emily.open_db(locator.clone()).await.expect("open db");
    emily
        .create_episode(episode_request())
        .await
        .expect("create episode");

    let result = runtime
        .execute_remote_with_registry_retry_and_record(
            task_request(),
            routing_preference(),
            retry_policy(),
            retry_persistence(),
        )
        .await
        .expect("execute retrying remote path");

    assert!(!result.exhausted);
    assert_eq!(result.attempts.len(), 2);
    assert_eq!(
        result.attempts[0]
            .execution
            .as_ref()
            .expect("first attempt execution")
            .validation
            .disposition,
        emily_membrane::contracts::MembraneValidationDisposition::NeedsReview
    );
    let final_execution = result.final_execution.expect("final execution");
    assert_eq!(final_execution.remote_episode_id, "remote-retry-2");
    assert_eq!(
        final_execution.validation.disposition,
        emily_membrane::contracts::MembraneValidationDisposition::Accepted
    );

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

    assert_eq!(remote_episodes.len(), 2);
    assert_eq!(validations.len(), 2);
    assert!(
        validations
            .iter()
            .any(|validation| validation.decision == ValidationDecision::NeedsReview)
    );
    assert!(
        validations
            .iter()
            .any(|validation| validation.decision == ValidationDecision::Accepted)
    );
    assert_eq!(
        audits
            .iter()
            .filter(|audit| audit.kind == AuditRecordKind::BoundaryEvent)
            .count(),
        2
    );

    emily.close_db().await.expect("close db");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}

#[tokio::test]
async fn remote_retry_execution_exhausts_after_provider_errors() {
    let store = Arc::new(SurrealEmilyStore::new());
    let emily = Arc::new(EmilyRuntime::new(store));
    let registry = Arc::new(InMemoryProviderRegistry::single_target(
        default_registered_provider_target(ProviderTarget {
            provider_id: "test-provider".to_string(),
            model_id: Some("deterministic-v1".to_string()),
            profile_id: Some("reasoning".to_string()),
            capability_tags: vec!["analysis".to_string()],
            metadata: json!({"origin": "test"}),
        }),
        Arc::new(ErrorThenErrorProvider {
            attempt: Mutex::new(0),
        }) as Arc<dyn MembraneProvider>,
    ));
    let runtime = MembraneRuntime::with_provider_registry(emily.clone(), registry);
    let locator = locator();

    emily.open_db(locator.clone()).await.expect("open db");
    emily
        .create_episode(episode_request())
        .await
        .expect("create episode");

    let result = runtime
        .execute_remote_with_registry_retry_and_record(
            task_request(),
            routing_preference(),
            retry_policy(),
            retry_persistence(),
        )
        .await
        .expect("execute retry exhaustion path");

    assert!(result.exhausted);
    assert!(result.final_execution.is_none());
    assert_eq!(result.attempts.len(), 2);
    assert!(
        result
            .attempts
            .iter()
            .all(|attempt| attempt.provider_error.is_some())
    );

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

    assert_eq!(remote_episodes.len(), 2);
    assert!(
        remote_episodes
            .iter()
            .all(|remote_episode| remote_episode.state == RemoteEpisodeState::Failed)
    );
    assert!(validations.is_empty());
    assert_eq!(
        audits
            .iter()
            .filter(|audit| audit.kind == AuditRecordKind::BoundaryEvent)
            .count(),
        4
    );

    emily.close_db().await.expect("close db");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}

#[tokio::test]
async fn multi_remote_execution_reconciles_review_then_accepted_result() {
    let store = Arc::new(SurrealEmilyStore::new());
    let emily = Arc::new(EmilyRuntime::new(store));
    let registry = Arc::new(InMemoryProviderRegistry::with_targets([
        (
            default_registered_provider_target(ProviderTarget {
                provider_id: "review-provider".to_string(),
                model_id: Some("review-v1".to_string()),
                profile_id: Some("reasoning".to_string()),
                capability_tags: vec!["analysis".to_string()],
                metadata: json!({"origin": "test"}),
            }),
            Arc::new(ReviewStatusProvider) as Arc<dyn MembraneProvider>,
        ),
        (
            default_registered_provider_target(ProviderTarget {
                provider_id: "accept-provider".to_string(),
                model_id: Some("accept-v1".to_string()),
                profile_id: Some("reasoning".to_string()),
                capability_tags: vec!["analysis".to_string()],
                metadata: json!({"origin": "test"}),
            }),
            Arc::new(AcceptedProvider {
                provider_id: "accept-provider",
            }) as Arc<dyn MembraneProvider>,
        ),
    ]));
    let runtime = MembraneRuntime::with_provider_registry(emily.clone(), registry);
    let locator = locator();

    emily.open_db(locator.clone()).await.expect("open db");
    emily
        .create_episode(episode_request())
        .await
        .expect("create episode");

    let result = runtime
        .execute_multi_remote_and_record(
            task_request(),
            multi_remote_targets(),
            multi_remote_policy_exhaust(),
            multi_remote_persistence(),
        )
        .await
        .expect("execute multi remote flow");

    assert_eq!(
        result.route.decision,
        emily_membrane::contracts::MembraneRouteKind::MultiRemote
    );
    assert_eq!(result.attempts.len(), 2);
    assert_eq!(
        result.attempts[0].validation_disposition,
        Some(emily_membrane::contracts::MembraneValidationDisposition::NeedsReview)
    );
    assert_eq!(
        result.attempts[1].validation_disposition,
        Some(emily_membrane::contracts::MembraneValidationDisposition::Accepted)
    );
    assert_eq!(
        result.reconciliation.decision,
        MultiRemoteReconciliationDecision::Accepted
    );
    assert_eq!(
        result.reconciliation.selected_target_id.as_deref(),
        Some("accept-provider:accept-v1")
    );
    assert!(
        result
            .reconciliation
            .reconstruction
            .as_ref()
            .expect("reconstruction")
            .output_text
            .contains("accept-provider")
    );

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

    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].kind, emily::RoutingDecisionKind::MultiRemote);
    assert_eq!(routes[0].targets.len(), 2);
    assert_eq!(remote_episodes.len(), 2);
    assert_eq!(validations.len(), 2);
    assert_eq!(
        audits
            .iter()
            .filter(|audit| audit.kind == AuditRecordKind::BoundaryEvent)
            .count(),
        1
    );

    emily.close_db().await.expect("close db");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}

#[tokio::test]
async fn multi_remote_execution_stops_after_first_accepted_result() {
    let store = Arc::new(SurrealEmilyStore::new());
    let emily = Arc::new(EmilyRuntime::new(store));
    let registry = Arc::new(InMemoryProviderRegistry::with_targets([
        (
            default_registered_provider_target(ProviderTarget {
                provider_id: "accept-provider".to_string(),
                model_id: Some("accept-v1".to_string()),
                profile_id: Some("reasoning".to_string()),
                capability_tags: vec!["analysis".to_string()],
                metadata: json!({"origin": "test"}),
            }),
            Arc::new(AcceptedProvider {
                provider_id: "accept-provider",
            }) as Arc<dyn MembraneProvider>,
        ),
        (
            default_registered_provider_target(ProviderTarget {
                provider_id: "panic-provider".to_string(),
                model_id: Some("panic-v1".to_string()),
                profile_id: Some("reasoning".to_string()),
                capability_tags: vec!["analysis".to_string()],
                metadata: json!({"origin": "test"}),
            }),
            Arc::new(PanicIfCalledProvider) as Arc<dyn MembraneProvider>,
        ),
    ]));
    let runtime = MembraneRuntime::with_provider_registry(emily.clone(), registry);
    let locator = locator();

    emily.open_db(locator.clone()).await.expect("open db");
    emily
        .create_episode(episode_request())
        .await
        .expect("create episode");

    let result = runtime
        .execute_multi_remote_and_record(
            task_request(),
            vec![
                ProviderTarget {
                    provider_id: "accept-provider".to_string(),
                    model_id: Some("accept-v1".to_string()),
                    profile_id: Some("reasoning".to_string()),
                    capability_tags: vec!["analysis".to_string()],
                    metadata: json!({"origin": "test"}),
                },
                ProviderTarget {
                    provider_id: "panic-provider".to_string(),
                    model_id: Some("panic-v1".to_string()),
                    profile_id: Some("reasoning".to_string()),
                    capability_tags: vec!["analysis".to_string()],
                    metadata: json!({"origin": "test"}),
                },
            ],
            multi_remote_policy_stop_on_accepted(),
            multi_remote_persistence(),
        )
        .await
        .expect("execute multi remote stop flow");

    assert_eq!(result.attempts.len(), 2);
    assert_eq!(
        result.attempts[0].validation_disposition,
        Some(emily_membrane::contracts::MembraneValidationDisposition::Accepted)
    );
    assert_eq!(
        result.attempts[1].status,
        emily_membrane::contracts::MultiRemoteAttemptStatus::Skipped
    );
    assert_eq!(
        result.attempts[1].skip_reason,
        Some(emily_membrane::contracts::MultiRemoteSkipReason::StopConditionSatisfied)
    );
    assert_eq!(
        result.reconciliation.decision,
        MultiRemoteReconciliationDecision::Accepted
    );

    let remote_episodes = emily
        .remote_episodes_for_episode("ep-membrane-remote")
        .await
        .expect("list remote episodes");
    let validations = emily
        .validation_outcomes_for_episode("ep-membrane-remote")
        .await
        .expect("list validations");

    assert_eq!(remote_episodes.len(), 1);
    assert_eq!(validations.len(), 1);

    emily.close_db().await.expect("close db");
    let _ = std::fs::remove_dir_all(locator.storage_path);
}
