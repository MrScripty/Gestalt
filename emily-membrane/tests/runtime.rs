use emily::EmilyError;
use emily::api::EmilyApi;
use emily::{
    AppendAuditRecordRequest, AppendSovereignAuditRecordRequest, AuditRecord, ContextPacket,
    ContextQuery, CreateEpisodeRequest, DatabaseLocator, EarlEvaluationRecord,
    EarlEvaluationRequest, EarlHostAction, EarlSignalVector, EpisodeRecord, EpisodeState,
    EpisodeTraceLink, HealthSnapshot, HistoryPage, HistoryPageRequest, IngestTextRequest,
    IntegritySnapshot, MemoryPolicy, OutcomeRecord, RecordOutcomeRequest, RemoteEpisodeRecord,
    RemoteEpisodeRequest, RoutingDecision, TextObject, TraceLinkRequest,
    UpdateRemoteEpisodeStateRequest, ValidationOutcome, VectorizationConfig,
    VectorizationConfigPatch, VectorizationJobSnapshot, VectorizationRunRequest,
    VectorizationStatus,
};
use emily_membrane::contracts::{
    ContextFragment, MembraneRouteKind, MembraneTaskRequest, MembraneValidationDisposition,
    RemoteExecutionPersistence, RemoteRoutingPreference, RoutingPlan, RoutingPolicyFindingSeverity,
    RoutingPolicyOutcome, RoutingPolicyRequest, RoutingSensitivity, ValidationEnvelope,
};
use emily_membrane::providers::{
    InMemoryProviderRegistry, MembraneProvider, MembraneProviderError, ProviderDispatchRequest,
    ProviderDispatchResult, ProviderDispatchStatus, ProviderTarget, RegisteredProviderTarget,
};
use emily_membrane::runtime::{MembraneRuntime, MembraneRuntimeError};
use std::sync::Arc;

#[derive(Clone)]
struct StubEmilyApi {
    episode: Option<EpisodeRecord>,
    latest_earl: Option<EarlEvaluationRecord>,
}

struct StubProvider {
    provider_id: &'static str,
}

impl Default for StubEmilyApi {
    fn default() -> Self {
        Self {
            episode: Some(open_episode("episode-1")),
            latest_earl: None,
        }
    }
}

impl StubEmilyApi {
    fn with_episode_state(state: EpisodeState) -> Self {
        Self {
            episode: Some(open_episode("episode-1").with_state(state)),
            latest_earl: None,
        }
    }

    fn with_latest_earl(decision: emily::EarlDecision) -> Self {
        Self {
            episode: Some(open_episode("episode-1")),
            latest_earl: Some(EarlEvaluationRecord {
                id: format!("earl-{decision:?}").to_lowercase(),
                episode_id: "episode-1".to_string(),
                evaluated_at_unix_ms: 5,
                signals: EarlSignalVector {
                    uncertainty: 0.1,
                    conflict: 0.1,
                    continuity_drift: 0.1,
                    constraint_pressure: 0.1,
                    tool_instability: 0.1,
                    novelty_spike: 0.1,
                },
                risk_score: 0.5,
                decision,
                host_action: match decision {
                    emily::EarlDecision::Ok => EarlHostAction::Proceed,
                    emily::EarlDecision::Caution => EarlHostAction::Clarify,
                    emily::EarlDecision::Reflex => EarlHostAction::Abort,
                },
                retryable: decision == emily::EarlDecision::Caution,
                rationale: "test earl state".to_string(),
                metadata: serde_json::json!({}),
            }),
        }
    }
}

trait StubEpisodeExt {
    fn with_state(self, state: EpisodeState) -> Self;
}

impl StubEpisodeExt for EpisodeRecord {
    fn with_state(mut self, state: EpisodeState) -> Self {
        self.state = state;
        self
    }
}

fn open_episode(episode_id: &str) -> EpisodeRecord {
    EpisodeRecord {
        id: episode_id.to_string(),
        stream_id: Some("stream-a".to_string()),
        source_kind: "membrane-test".to_string(),
        episode_kind: "routing".to_string(),
        state: EpisodeState::Open,
        started_at_unix_ms: 1,
        closed_at_unix_ms: None,
        intent: Some("test routing policy".to_string()),
        metadata: serde_json::json!({}),
        last_outcome_id: None,
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
    }
}

fn unused<T>() -> Result<T, EmilyError> {
    Err(EmilyError::Runtime("unused test stub".to_string()))
}

#[async_trait::async_trait]
impl EmilyApi for StubEmilyApi {
    async fn open_db(&self, _locator: DatabaseLocator) -> Result<(), EmilyError> {
        unused()
    }

    async fn switch_db(&self, _locator: DatabaseLocator) -> Result<(), EmilyError> {
        unused()
    }

    async fn close_db(&self) -> Result<(), EmilyError> {
        unused()
    }

    async fn ingest_text(&self, _request: IngestTextRequest) -> Result<TextObject, EmilyError> {
        unused()
    }

    async fn create_episode(
        &self,
        _request: CreateEpisodeRequest,
    ) -> Result<EpisodeRecord, EmilyError> {
        unused()
    }

    async fn episode(&self, episode_id: &str) -> Result<Option<EpisodeRecord>, EmilyError> {
        Ok(self
            .episode
            .as_ref()
            .filter(|episode| episode.id == episode_id)
            .cloned())
    }

    async fn link_text_to_episode(
        &self,
        _request: TraceLinkRequest,
    ) -> Result<EpisodeTraceLink, EmilyError> {
        unused()
    }

    async fn record_outcome(
        &self,
        _request: RecordOutcomeRequest,
    ) -> Result<OutcomeRecord, EmilyError> {
        unused()
    }

    async fn append_audit_record(
        &self,
        _request: AppendAuditRecordRequest,
    ) -> Result<AuditRecord, EmilyError> {
        unused()
    }

    async fn record_routing_decision(
        &self,
        _decision: RoutingDecision,
    ) -> Result<RoutingDecision, EmilyError> {
        unused()
    }

    async fn create_remote_episode(
        &self,
        _request: RemoteEpisodeRequest,
    ) -> Result<RemoteEpisodeRecord, EmilyError> {
        unused()
    }

    async fn update_remote_episode_state(
        &self,
        _request: UpdateRemoteEpisodeStateRequest,
    ) -> Result<RemoteEpisodeRecord, EmilyError> {
        unused()
    }

    async fn record_validation_outcome(
        &self,
        _outcome: ValidationOutcome,
    ) -> Result<ValidationOutcome, EmilyError> {
        unused()
    }

    async fn append_sovereign_audit_record(
        &self,
        _request: AppendSovereignAuditRecordRequest,
    ) -> Result<AuditRecord, EmilyError> {
        unused()
    }

    async fn routing_decision(
        &self,
        _decision_id: &str,
    ) -> Result<Option<RoutingDecision>, EmilyError> {
        unused()
    }

    async fn routing_decisions_for_episode(
        &self,
        _episode_id: &str,
    ) -> Result<Vec<RoutingDecision>, EmilyError> {
        unused()
    }

    async fn remote_episode(
        &self,
        _remote_episode_id: &str,
    ) -> Result<Option<RemoteEpisodeRecord>, EmilyError> {
        unused()
    }

    async fn remote_episodes_for_episode(
        &self,
        _episode_id: &str,
    ) -> Result<Vec<RemoteEpisodeRecord>, EmilyError> {
        unused()
    }

    async fn validation_outcome(
        &self,
        _validation_id: &str,
    ) -> Result<Option<ValidationOutcome>, EmilyError> {
        unused()
    }

    async fn validation_outcomes_for_episode(
        &self,
        _episode_id: &str,
    ) -> Result<Vec<ValidationOutcome>, EmilyError> {
        unused()
    }

    async fn sovereign_audit_records_for_episode(
        &self,
        _episode_id: &str,
    ) -> Result<Vec<AuditRecord>, EmilyError> {
        unused()
    }

    async fn evaluate_episode_risk(
        &self,
        _request: EarlEvaluationRequest,
    ) -> Result<EarlEvaluationRecord, EmilyError> {
        unused()
    }

    async fn latest_earl_evaluation_for_episode(
        &self,
        episode_id: &str,
    ) -> Result<Option<EarlEvaluationRecord>, EmilyError> {
        Ok(self
            .latest_earl
            .as_ref()
            .filter(|evaluation| evaluation.episode_id == episode_id)
            .cloned())
    }

    async fn latest_integrity_snapshot(&self) -> Result<Option<IntegritySnapshot>, EmilyError> {
        unused()
    }

    async fn query_context(&self, _query: ContextQuery) -> Result<ContextPacket, EmilyError> {
        unused()
    }

    async fn page_history_before(
        &self,
        _request: HistoryPageRequest,
    ) -> Result<HistoryPage, EmilyError> {
        unused()
    }

    async fn memory_policy(&self) -> Result<MemoryPolicy, EmilyError> {
        unused()
    }

    async fn set_memory_policy(&self, _policy: MemoryPolicy) -> Result<(), EmilyError> {
        unused()
    }

    async fn health(&self) -> Result<HealthSnapshot, EmilyError> {
        unused()
    }

    async fn vectorization_status(&self) -> Result<VectorizationStatus, EmilyError> {
        unused()
    }

    async fn update_vectorization_config(
        &self,
        _patch: VectorizationConfigPatch,
    ) -> Result<VectorizationConfig, EmilyError> {
        unused()
    }

    async fn start_backfill(
        &self,
        _request: VectorizationRunRequest,
    ) -> Result<VectorizationJobSnapshot, EmilyError> {
        unused()
    }

    async fn start_revectorize(
        &self,
        _request: VectorizationRunRequest,
    ) -> Result<VectorizationJobSnapshot, EmilyError> {
        unused()
    }

    async fn cancel_vectorization_job(&self, _job_id: &str) -> Result<(), EmilyError> {
        unused()
    }
}

#[async_trait::async_trait]
impl MembraneProvider for StubProvider {
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
            output_text: "ok".to_string(),
            metadata: serde_json::json!({}),
        })
    }
}

#[tokio::test]
async fn runtime_executes_deterministic_local_flow() {
    let runtime = MembraneRuntime::new(Arc::new(StubEmilyApi::default()));
    let request = MembraneTaskRequest {
        task_id: "task-1".into(),
        episode_id: "episode-1".into(),
        task_text: "Summarize the local evidence.".into(),
        context_fragments: vec![
            ContextFragment {
                fragment_id: "ctx-1".into(),
                text: "first fragment".into(),
            },
            ContextFragment {
                fragment_id: "ctx-2".into(),
                text: "second fragment".into(),
            },
        ],
        allow_remote: true,
    };

    let compiled = runtime.compile(request).await.expect("compile");
    assert!(compiled.compiled_task.bounded_prompt.contains("Context:"));
    assert_eq!(compiled.compiled_task.context_fragment_ids.len(), 2);

    let route = runtime.route(&compiled).await.expect("route");
    assert_eq!(route.decision, MembraneRouteKind::LocalOnly);
    assert!(route.targets.is_empty());

    let dispatch = runtime
        .dispatch_local(&compiled, &route)
        .await
        .expect("dispatch");
    assert_eq!(
        dispatch.status,
        emily_membrane::contracts::DispatchStatus::LocalCompleted
    );
    assert!(dispatch.response_text.starts_with("LOCAL: "));

    let validation = runtime.validate(&dispatch).await.expect("validate");
    assert_eq!(
        validation.disposition,
        MembraneValidationDisposition::Accepted
    );

    let reconstruction = runtime.reconstruct(&validation).await.expect("reconstruct");
    assert_eq!(reconstruction.task_id, "task-1");
    assert_eq!(reconstruction.output_text, dispatch.response_text);
    assert!(!reconstruction.caution);
}

#[tokio::test]
async fn dispatch_local_rejects_remote_route_shapes() {
    let runtime = MembraneRuntime::new(Arc::new(StubEmilyApi::default()));
    let compiled = runtime
        .compile(MembraneTaskRequest {
            task_id: "task-1".into(),
            episode_id: "episode-1".into(),
            task_text: "Route test".into(),
            context_fragments: Vec::new(),
            allow_remote: true,
        })
        .await
        .expect("compile");

    let remote_plan = RoutingPlan {
        task_id: "task-1".into(),
        decision: MembraneRouteKind::SingleRemote,
        targets: Vec::new(),
        rationale: Some("pretend remote route".into()),
    };

    let error = runtime
        .dispatch_local(&compiled, &remote_plan)
        .await
        .expect_err("remote route should fail local dispatch");

    assert!(matches!(error, MembraneRuntimeError::InvalidRequest(_)));
}

#[tokio::test]
async fn reconstruct_rejects_rejected_validation() {
    let runtime = MembraneRuntime::new(Arc::new(StubEmilyApi::default()));
    let error = runtime
        .reconstruct(&ValidationEnvelope {
            task_id: "task-1".into(),
            disposition: MembraneValidationDisposition::Rejected,
            findings: Vec::new(),
            validated_text: None,
        })
        .await
        .expect_err("rejected validation should not reconstruct");

    assert!(matches!(error, MembraneRuntimeError::InvalidState(_)));
}

#[tokio::test]
async fn execute_remote_and_record_requires_provider() {
    let runtime = MembraneRuntime::new(Arc::new(StubEmilyApi::default()));
    let error = runtime
        .execute_remote_and_record(
            MembraneTaskRequest {
                task_id: "task-1".into(),
                episode_id: "episode-1".into(),
                task_text: "remote task".into(),
                context_fragments: Vec::new(),
                allow_remote: true,
            },
            ProviderTarget {
                provider_id: "provider-a".into(),
                model_id: Some("model-x".into()),
                profile_id: Some("reasoning".into()),
                capability_tags: vec!["analysis".into()],
                metadata: serde_json::json!({}),
            },
            RemoteExecutionPersistence {
                route_decision_id: "route-1".into(),
                route_decided_at_unix_ms: 10,
                provider_request_id: "provider-request-1".into(),
                remote_episode_id: "remote-1".into(),
                remote_dispatched_at_unix_ms: 11,
                validation_id: "validation-1".into(),
                validated_at_unix_ms: 12,
            },
        )
        .await
        .expect_err("remote execution without provider registry should fail");

    assert!(matches!(error, MembraneRuntimeError::InvalidState(_)));
}

#[tokio::test]
async fn execute_remote_and_record_rejects_missing_registered_provider() {
    let registry = Arc::new(InMemoryProviderRegistry::new([]));
    let runtime =
        MembraneRuntime::with_provider_registry(Arc::new(StubEmilyApi::default()), registry);
    let error = runtime
        .execute_remote_and_record(
            MembraneTaskRequest {
                task_id: "task-1".into(),
                episode_id: "episode-1".into(),
                task_text: "remote task".into(),
                context_fragments: Vec::new(),
                allow_remote: true,
            },
            ProviderTarget {
                provider_id: "provider-a".into(),
                model_id: Some("model-x".into()),
                profile_id: Some("reasoning".into()),
                capability_tags: vec!["analysis".into()],
                metadata: serde_json::json!({}),
            },
            RemoteExecutionPersistence {
                route_decision_id: "route-1".into(),
                route_decided_at_unix_ms: 10,
                provider_request_id: "provider-request-1".into(),
                remote_episode_id: "remote-1".into(),
                remote_dispatched_at_unix_ms: 11,
                validation_id: "validation-1".into(),
                validated_at_unix_ms: 12,
            },
        )
        .await
        .expect_err("remote execution with a missing provider should fail");

    assert!(matches!(error, MembraneRuntimeError::InvalidRequest(_)));
}

#[tokio::test]
async fn select_remote_target_matches_registry_metadata() {
    let registry = Arc::new(InMemoryProviderRegistry::with_targets([
        (
            RegisteredProviderTarget {
                target: ProviderTarget {
                    provider_id: "provider-a".into(),
                    model_id: Some("model-a".into()),
                    profile_id: Some("reasoning".into()),
                    capability_tags: vec!["analysis".into()],
                    metadata: serde_json::json!({"rank": 2}),
                },
            },
            Arc::new(StubProvider {
                provider_id: "provider-a",
            }) as Arc<dyn MembraneProvider>,
        ),
        (
            RegisteredProviderTarget {
                target: ProviderTarget {
                    provider_id: "provider-b".into(),
                    model_id: Some("model-b".into()),
                    profile_id: Some("retrieval".into()),
                    capability_tags: vec!["search".into()],
                    metadata: serde_json::json!({"rank": 1}),
                },
            },
            Arc::new(StubProvider {
                provider_id: "provider-b",
            }) as Arc<dyn MembraneProvider>,
        ),
    ]));
    let runtime =
        MembraneRuntime::with_provider_registry(Arc::new(StubEmilyApi::default()), registry);

    let target = runtime
        .select_remote_target(&RemoteRoutingPreference {
            provider_id: Some("provider-a".into()),
            profile_id: Some("reasoning".into()),
            required_capability_tags: vec!["analysis".into()],
        })
        .await
        .expect("select target");

    assert_eq!(target.provider_id, "provider-a");
    assert_eq!(target.profile_id.as_deref(), Some("reasoning"));
    assert_eq!(target.model_id.as_deref(), Some("model-a"));
}

#[tokio::test]
async fn evaluate_routing_policy_returns_local_only_when_remote_disabled() {
    let registry = Arc::new(InMemoryProviderRegistry::with_targets([(
        RegisteredProviderTarget {
            target: ProviderTarget {
                provider_id: "provider-a".into(),
                model_id: Some("model-a".into()),
                profile_id: Some("reasoning".into()),
                capability_tags: vec!["analysis".into()],
                metadata: serde_json::json!({}),
            },
        },
        Arc::new(StubProvider {
            provider_id: "provider-a",
        }) as Arc<dyn MembraneProvider>,
    )]));
    let runtime =
        MembraneRuntime::with_provider_registry(Arc::new(StubEmilyApi::default()), registry);

    let result = runtime
        .evaluate_routing_policy(RoutingPolicyRequest {
            task_id: "task-1".into(),
            episode_id: "episode-1".into(),
            allow_remote: false,
            sensitivity: RoutingSensitivity::Normal,
            preference: RemoteRoutingPreference {
                provider_id: None,
                profile_id: None,
                required_capability_tags: Vec::new(),
            },
        })
        .await
        .expect("evaluate policy");

    assert_eq!(result.outcome, RoutingPolicyOutcome::LocalOnly);
    assert!(result.selected_target.is_none());
    assert_eq!(result.findings[0].code, "remote-disabled");
}

#[tokio::test]
async fn evaluate_routing_policy_rejects_critical_sensitivity() {
    let registry = Arc::new(InMemoryProviderRegistry::with_targets([(
        RegisteredProviderTarget {
            target: ProviderTarget {
                provider_id: "provider-a".into(),
                model_id: Some("model-a".into()),
                profile_id: Some("reasoning".into()),
                capability_tags: vec!["analysis".into()],
                metadata: serde_json::json!({}),
            },
        },
        Arc::new(StubProvider {
            provider_id: "provider-a",
        }) as Arc<dyn MembraneProvider>,
    )]));
    let runtime =
        MembraneRuntime::with_provider_registry(Arc::new(StubEmilyApi::default()), registry);

    let result = runtime
        .evaluate_routing_policy(RoutingPolicyRequest {
            task_id: "task-1".into(),
            episode_id: "episode-1".into(),
            allow_remote: true,
            sensitivity: RoutingSensitivity::Critical,
            preference: RemoteRoutingPreference {
                provider_id: None,
                profile_id: None,
                required_capability_tags: Vec::new(),
            },
        })
        .await
        .expect("evaluate policy");

    assert_eq!(result.outcome, RoutingPolicyOutcome::Rejected);
    assert!(result.selected_target.is_none());
    assert_eq!(
        result.findings[0].severity,
        RoutingPolicyFindingSeverity::Block
    );
}

#[tokio::test]
async fn evaluate_routing_policy_prefers_best_matching_target() {
    let registry = Arc::new(InMemoryProviderRegistry::with_targets([
        (
            RegisteredProviderTarget {
                target: ProviderTarget {
                    provider_id: "provider-a".into(),
                    model_id: Some("model-a".into()),
                    profile_id: Some("reasoning".into()),
                    capability_tags: vec!["analysis".into(), "synthesis".into()],
                    metadata: serde_json::json!({}),
                },
            },
            Arc::new(StubProvider {
                provider_id: "provider-a",
            }) as Arc<dyn MembraneProvider>,
        ),
        (
            RegisteredProviderTarget {
                target: ProviderTarget {
                    provider_id: "provider-b".into(),
                    model_id: Some("model-b".into()),
                    profile_id: Some("reasoning".into()),
                    capability_tags: vec!["analysis".into()],
                    metadata: serde_json::json!({}),
                },
            },
            Arc::new(StubProvider {
                provider_id: "provider-b",
            }) as Arc<dyn MembraneProvider>,
        ),
    ]));
    let runtime =
        MembraneRuntime::with_provider_registry(Arc::new(StubEmilyApi::default()), registry);

    let result = runtime
        .evaluate_routing_policy(RoutingPolicyRequest {
            task_id: "task-1".into(),
            episode_id: "episode-1".into(),
            allow_remote: true,
            sensitivity: RoutingSensitivity::High,
            preference: RemoteRoutingPreference {
                provider_id: None,
                profile_id: Some("reasoning".into()),
                required_capability_tags: vec!["analysis".into()],
            },
        })
        .await
        .expect("evaluate policy");

    assert_eq!(result.outcome, RoutingPolicyOutcome::SingleRemote);
    assert!(result.caution);
    assert_eq!(
        result
            .selected_target
            .as_ref()
            .map(|target| target.provider_id.as_str()),
        Some("provider-a")
    );
}

#[tokio::test]
async fn evaluate_routing_policy_uses_deterministic_tie_breaking() {
    let registry = Arc::new(InMemoryProviderRegistry::with_targets([
        (
            RegisteredProviderTarget {
                target: ProviderTarget {
                    provider_id: "provider-b".into(),
                    model_id: Some("model-b".into()),
                    profile_id: Some("reasoning".into()),
                    capability_tags: vec!["analysis".into()],
                    metadata: serde_json::json!({}),
                },
            },
            Arc::new(StubProvider {
                provider_id: "provider-b",
            }) as Arc<dyn MembraneProvider>,
        ),
        (
            RegisteredProviderTarget {
                target: ProviderTarget {
                    provider_id: "provider-a".into(),
                    model_id: Some("model-a".into()),
                    profile_id: Some("reasoning".into()),
                    capability_tags: vec!["analysis".into()],
                    metadata: serde_json::json!({}),
                },
            },
            Arc::new(StubProvider {
                provider_id: "provider-a",
            }) as Arc<dyn MembraneProvider>,
        ),
    ]));
    let runtime =
        MembraneRuntime::with_provider_registry(Arc::new(StubEmilyApi::default()), registry);

    let result = runtime
        .evaluate_routing_policy(RoutingPolicyRequest {
            task_id: "task-1".into(),
            episode_id: "episode-1".into(),
            allow_remote: true,
            sensitivity: RoutingSensitivity::Normal,
            preference: RemoteRoutingPreference {
                provider_id: None,
                profile_id: Some("reasoning".into()),
                required_capability_tags: vec!["analysis".into()],
            },
        })
        .await
        .expect("evaluate policy");

    assert_eq!(result.outcome, RoutingPolicyOutcome::SingleRemote);
    assert_eq!(
        result
            .selected_target
            .as_ref()
            .map(|target| target.provider_id.as_str()),
        Some("provider-a")
    );
}

#[tokio::test]
async fn evaluate_routing_policy_rejects_missing_episode_anchor() {
    let registry = Arc::new(InMemoryProviderRegistry::with_targets([(
        RegisteredProviderTarget {
            target: ProviderTarget {
                provider_id: "provider-a".into(),
                model_id: Some("model-a".into()),
                profile_id: Some("reasoning".into()),
                capability_tags: vec!["analysis".into()],
                metadata: serde_json::json!({}),
            },
        },
        Arc::new(StubProvider {
            provider_id: "provider-a",
        }) as Arc<dyn MembraneProvider>,
    )]));
    let runtime = MembraneRuntime::with_provider_registry(
        Arc::new(StubEmilyApi {
            episode: None,
            latest_earl: None,
        }),
        registry,
    );

    let result = runtime
        .evaluate_routing_policy(RoutingPolicyRequest {
            task_id: "task-1".into(),
            episode_id: "episode-1".into(),
            allow_remote: true,
            sensitivity: RoutingSensitivity::Normal,
            preference: RemoteRoutingPreference {
                provider_id: None,
                profile_id: Some("reasoning".into()),
                required_capability_tags: vec!["analysis".into()],
            },
        })
        .await
        .expect("evaluate policy");

    assert_eq!(result.outcome, RoutingPolicyOutcome::Rejected);
    assert_eq!(result.findings[0].code, "episode-missing");
}

#[tokio::test]
async fn evaluate_routing_policy_cautions_when_latest_earl_requires_clarification() {
    let registry = Arc::new(InMemoryProviderRegistry::with_targets([(
        RegisteredProviderTarget {
            target: ProviderTarget {
                provider_id: "provider-a".into(),
                model_id: Some("model-a".into()),
                profile_id: Some("reasoning".into()),
                capability_tags: vec!["analysis".into()],
                metadata: serde_json::json!({}),
            },
        },
        Arc::new(StubProvider {
            provider_id: "provider-a",
        }) as Arc<dyn MembraneProvider>,
    )]));
    let runtime = MembraneRuntime::with_provider_registry(
        Arc::new(StubEmilyApi::with_latest_earl(emily::EarlDecision::Caution)),
        registry,
    );

    let result = runtime
        .evaluate_routing_policy(RoutingPolicyRequest {
            task_id: "task-1".into(),
            episode_id: "episode-1".into(),
            allow_remote: true,
            sensitivity: RoutingSensitivity::Normal,
            preference: RemoteRoutingPreference {
                provider_id: None,
                profile_id: Some("reasoning".into()),
                required_capability_tags: vec!["analysis".into()],
            },
        })
        .await
        .expect("evaluate policy");

    assert_eq!(result.outcome, RoutingPolicyOutcome::SingleRemote);
    assert!(result.caution);
    assert!(
        result
            .findings
            .iter()
            .any(|finding| finding.code == "earl-caution-gate")
    );
}

#[tokio::test]
async fn evaluate_routing_policy_rejects_latest_earl_reflex_gate() {
    let registry = Arc::new(InMemoryProviderRegistry::with_targets([(
        RegisteredProviderTarget {
            target: ProviderTarget {
                provider_id: "provider-a".into(),
                model_id: Some("model-a".into()),
                profile_id: Some("reasoning".into()),
                capability_tags: vec!["analysis".into()],
                metadata: serde_json::json!({}),
            },
        },
        Arc::new(StubProvider {
            provider_id: "provider-a",
        }) as Arc<dyn MembraneProvider>,
    )]));
    let runtime = MembraneRuntime::with_provider_registry(
        Arc::new(StubEmilyApi::with_latest_earl(emily::EarlDecision::Reflex)),
        registry,
    );

    let result = runtime
        .evaluate_routing_policy(RoutingPolicyRequest {
            task_id: "task-1".into(),
            episode_id: "episode-1".into(),
            allow_remote: true,
            sensitivity: RoutingSensitivity::Normal,
            preference: RemoteRoutingPreference {
                provider_id: None,
                profile_id: Some("reasoning".into()),
                required_capability_tags: vec!["analysis".into()],
            },
        })
        .await
        .expect("evaluate policy");

    assert_eq!(result.outcome, RoutingPolicyOutcome::Rejected);
    assert_eq!(result.findings[0].code, "earl-reflex-gate");
}

#[tokio::test]
async fn evaluate_routing_policy_cautions_existing_cautioned_episode() {
    let registry = Arc::new(InMemoryProviderRegistry::with_targets([(
        RegisteredProviderTarget {
            target: ProviderTarget {
                provider_id: "provider-a".into(),
                model_id: Some("model-a".into()),
                profile_id: Some("reasoning".into()),
                capability_tags: vec!["analysis".into()],
                metadata: serde_json::json!({}),
            },
        },
        Arc::new(StubProvider {
            provider_id: "provider-a",
        }) as Arc<dyn MembraneProvider>,
    )]));
    let runtime = MembraneRuntime::with_provider_registry(
        Arc::new(StubEmilyApi::with_episode_state(EpisodeState::Cautioned)),
        registry,
    );

    let result = runtime
        .evaluate_routing_policy(RoutingPolicyRequest {
            task_id: "task-1".into(),
            episode_id: "episode-1".into(),
            allow_remote: true,
            sensitivity: RoutingSensitivity::Normal,
            preference: RemoteRoutingPreference {
                provider_id: None,
                profile_id: Some("reasoning".into()),
                required_capability_tags: vec!["analysis".into()],
            },
        })
        .await
        .expect("evaluate policy");

    assert_eq!(result.outcome, RoutingPolicyOutcome::SingleRemote);
    assert!(result.caution);
    assert!(
        result
            .findings
            .iter()
            .any(|finding| finding.code == "episode-cautioned")
    );
}
