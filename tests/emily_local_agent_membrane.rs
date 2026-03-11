use emily::api::EmilyApi;
use emily::model::{DatabaseLocator, RemoteEpisodeState, ValidationDecision};
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use emily_membrane::providers::{
    InMemoryProviderRegistry, MembraneProvider, MembraneProviderError, ProviderDispatchRequest,
    ProviderDispatchResult, ProviderDispatchStatus, ProviderMetadataClass, ProviderTarget,
    ProviderValidationCompatibility, RegisteredProviderTarget,
};
use gestalt::emily_bridge::EmilyBridge;
use gestalt::emily_seed::{SYNTHETIC_TERMINAL_DATASET, seed_builtin_corpus};
use gestalt::local_agent_context::prepare_local_agent_command;
use gestalt::local_agent_episode::{
    episode_request_from_prepared_command, record_local_agent_episode,
};
use gestalt::local_agent_membrane::run_local_agent_membrane_pass_with_registry;
use gestalt::orchestrator::{GroupOrchestratorSnapshot, GroupTerminalState, TerminalRound};
use gestalt::state::{SessionRole, SessionStatus};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Clone)]
struct StubProvider {
    provider_id: &'static str,
    result: Result<ProviderDispatchResult, MembraneProviderError>,
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
        match &self.result {
            Ok(result) => Ok(ProviderDispatchResult {
                provider_request_id: request.provider_request_id,
                provider_id: self.provider_id.to_string(),
                status: result.status,
                output_text: result.output_text.clone(),
                metadata: result.metadata.clone(),
            }),
            Err(error) => Err(error.clone()),
        }
    }
}

#[test]
fn local_agent_membrane_pass_records_local_only_sovereign_artifacts() {
    let locator = unique_locator("emily-local-agent-membrane");
    let storage_path = locator.storage_path.clone();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    runtime.block_on(async {
        let bridge = seeded_bridge(locator.clone()).await;
        let prepared = prepare_local_agent_command(
            bridge.clone(),
            seeded_group_orchestrator(),
            "Summarize recent terminal context for the local-only membrane path".to_string(),
        )
        .await;
        let episode = record_episode(bridge.clone(), &prepared, "run-membrane").await;
        let membrane = run_local_agent_membrane_pass_with_registry(
            bridge.clone(),
            &episode.episode_id,
            &prepared,
            None,
            false,
        )
        .await
        .expect("local-only membrane pass should succeed");

        assert_eq!(
            membrane.policy_outcome,
            emily_membrane::contracts::RoutingPolicyOutcome::LocalOnly
        );
        assert_eq!(
            membrane.validation_disposition,
            Some(emily_membrane::contracts::MembraneValidationDisposition::Accepted)
        );
        assert!(!membrane.executed_remote);
        assert!(membrane.reference_count >= 2);
        assert_eq!(
            membrane.route_decision_id.as_deref(),
            Some("local-agent:run-membrane:local-agent-membrane:local:route")
        );
        assert_eq!(
            membrane.validation_id.as_deref(),
            Some("local-agent:run-membrane:local-agent-membrane:local:validation")
        );

        let routes = bridge
            .routing_decisions_for_episode_async(episode.episode_id.clone())
            .await
            .expect("routing decisions should load");
        let validations = bridge
            .validation_outcomes_for_episode_async(episode.episode_id.clone())
            .await
            .expect("validation outcomes should load");
        let audits = bridge
            .sovereign_audit_records_for_episode_async(episode.episode_id.clone())
            .await
            .expect("sovereign audits should load");

        assert_eq!(routes.len(), 1);
        assert_eq!(validations.len(), 1);
        assert_eq!(validations[0].decision, ValidationDecision::Accepted);
        assert_eq!(audits.len(), 2);
    });

    thread::sleep(Duration::from_millis(50));
    let _ = std::fs::remove_dir_all(storage_path);
}

#[test]
fn local_agent_membrane_pass_records_remote_artifacts_with_registry() {
    let locator = unique_locator("emily-local-agent-membrane-remote");
    let storage_path = locator.storage_path.clone();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    runtime.block_on(async {
        let bridge = seeded_bridge(locator.clone()).await;
        let prepared = prepare_local_agent_command(
            bridge.clone(),
            seeded_group_orchestrator(),
            "Summarize recent terminal context for the remote membrane path".to_string(),
        )
        .await;
        let episode = record_episode(bridge.clone(), &prepared, "run-membrane-remote").await;
        let registry = registry_with_provider(Arc::new(StubProvider {
            provider_id: "stub-remote",
            result: Ok(ProviderDispatchResult {
                provider_request_id: "unused".to_string(),
                provider_id: "stub-remote".to_string(),
                status: ProviderDispatchStatus::Completed,
                output_text: "Remote summary completed successfully.".to_string(),
                metadata: serde_json::json!({}),
            }),
        }));

        let membrane = run_local_agent_membrane_pass_with_registry(
            bridge.clone(),
            &episode.episode_id,
            &prepared,
            Some(registry),
            true,
        )
        .await
        .expect("remote membrane pass should succeed");

        assert_eq!(
            membrane.policy_outcome,
            emily_membrane::contracts::RoutingPolicyOutcome::SingleRemote
        );
        assert!(membrane.executed_remote);
        assert_eq!(
            membrane.validation_disposition,
            Some(emily_membrane::contracts::MembraneValidationDisposition::Accepted)
        );
        assert_eq!(
            membrane.remote_episode_id.as_deref(),
            Some("local-agent:run-membrane-remote:local-agent-membrane:remote:episode")
        );
        assert!(membrane.fallback_reason.is_none());

        let routes = bridge
            .routing_decisions_for_episode_async(episode.episode_id.clone())
            .await
            .expect("routing decisions should load");
        let remote_episodes = bridge
            .remote_episodes_for_episode_async(episode.episode_id.clone())
            .await
            .expect("remote episodes should load");
        let validations = bridge
            .validation_outcomes_for_episode_async(episode.episode_id.clone())
            .await
            .expect("validation outcomes should load");
        let audits = bridge
            .sovereign_audit_records_for_episode_async(episode.episode_id.clone())
            .await
            .expect("sovereign audits should load");

        assert_eq!(routes.len(), 1);
        assert_eq!(remote_episodes.len(), 1);
        assert_eq!(validations.len(), 1);
        assert_eq!(validations[0].decision, ValidationDecision::Accepted);
        assert!(audits.len() >= 3);
    });

    thread::sleep(Duration::from_millis(50));
    let _ = std::fs::remove_dir_all(storage_path);
}

#[test]
fn local_agent_membrane_pass_surfaces_remote_review_required() {
    let locator = unique_locator("emily-local-agent-membrane-remote-review");
    let storage_path = locator.storage_path.clone();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    runtime.block_on(async {
        let bridge = seeded_bridge(locator.clone()).await;
        let prepared = prepare_local_agent_command(
            bridge.clone(),
            seeded_group_orchestrator(),
            "Summarize recent terminal context for the remote review path".to_string(),
        )
        .await;
        let episode = record_episode(bridge.clone(), &prepared, "run-membrane-remote-review").await;
        let registry = registry_with_provider(Arc::new(StubProvider {
            provider_id: "stub-remote",
            result: Ok(ProviderDispatchResult {
                provider_request_id: "unused".to_string(),
                provider_id: "stub-remote".to_string(),
                status: ProviderDispatchStatus::Failed,
                output_text: "Provider completed the dispatch but confidence remains low."
                    .to_string(),
                metadata: serde_json::json!({}),
            }),
        }));

        let membrane = run_local_agent_membrane_pass_with_registry(
            bridge.clone(),
            &episode.episode_id,
            &prepared,
            Some(registry),
            true,
        )
        .await
        .expect("remote membrane review path should succeed");

        assert_eq!(
            membrane.policy_outcome,
            emily_membrane::contracts::RoutingPolicyOutcome::SingleRemote
        );
        assert!(membrane.executed_remote);
        assert!(membrane.caution);
        assert_eq!(
            membrane.validation_disposition,
            Some(emily_membrane::contracts::MembraneValidationDisposition::NeedsReview)
        );

        let validations = bridge
            .validation_outcomes_for_episode_async(episode.episode_id.clone())
            .await
            .expect("validation outcomes should load");

        assert_eq!(validations.len(), 1);
        assert_eq!(validations[0].decision, ValidationDecision::NeedsReview);
    });

    thread::sleep(Duration::from_millis(50));
    let _ = std::fs::remove_dir_all(storage_path);
}

#[test]
fn local_agent_membrane_remote_failure_falls_back_to_local_only() {
    let locator = unique_locator("emily-local-agent-membrane-remote-fallback");
    let storage_path = locator.storage_path.clone();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    runtime.block_on(async {
        let bridge = seeded_bridge(locator.clone()).await;
        let prepared = prepare_local_agent_command(
            bridge.clone(),
            seeded_group_orchestrator(),
            "Summarize recent terminal context for the remote fallback path".to_string(),
        )
        .await;
        let episode = record_episode(bridge.clone(), &prepared, "run-membrane-fallback").await;
        let registry = registry_with_provider(Arc::new(StubProvider {
            provider_id: "stub-remote",
            result: Err(MembraneProviderError::Execution(
                "runtime_timeout: workflow run exceeded timeout_ms 1".to_string(),
            )),
        }));

        let membrane = run_local_agent_membrane_pass_with_registry(
            bridge.clone(),
            &episode.episode_id,
            &prepared,
            Some(registry),
            true,
        )
        .await
        .expect("remote membrane fallback should succeed");

        assert_eq!(
            membrane.policy_outcome,
            emily_membrane::contracts::RoutingPolicyOutcome::LocalOnly
        );
        assert!(!membrane.executed_remote);
        assert!(membrane.fallback_reason.is_some());
        assert_eq!(
            membrane.validation_disposition,
            Some(emily_membrane::contracts::MembraneValidationDisposition::Accepted)
        );
        assert_eq!(
            membrane.route_decision_id.as_deref(),
            Some("local-agent:run-membrane-fallback:local-agent-membrane:local-fallback:route")
        );

        let routes = bridge
            .routing_decisions_for_episode_async(episode.episode_id.clone())
            .await
            .expect("routing decisions should load");
        let remote_episodes = bridge
            .remote_episodes_for_episode_async(episode.episode_id.clone())
            .await
            .expect("remote episodes should load");
        let validations = bridge
            .validation_outcomes_for_episode_async(episode.episode_id.clone())
            .await
            .expect("validation outcomes should load");

        assert_eq!(routes.len(), 2);
        assert_eq!(remote_episodes.len(), 1);
        assert_eq!(remote_episodes[0].state, RemoteEpisodeState::Failed);
        assert_eq!(validations.len(), 1);
        assert_eq!(validations[0].decision, ValidationDecision::Accepted);
    });

    thread::sleep(Duration::from_millis(50));
    let _ = std::fs::remove_dir_all(storage_path);
}

async fn seeded_bridge(locator: DatabaseLocator) -> Arc<EmilyBridge> {
    let emily_runtime = EmilyRuntime::new(Arc::new(SurrealEmilyStore::new()));
    emily_runtime
        .open_db(locator.clone())
        .await
        .expect("db should open");
    let _ = seed_builtin_corpus(&emily_runtime, SYNTHETIC_TERMINAL_DATASET)
        .await
        .expect("terminal dataset should seed");
    emily_runtime
        .close_db()
        .await
        .expect("db should close cleanly");
    Arc::new(EmilyBridge::new(locator))
}

async fn record_episode(
    bridge: Arc<EmilyBridge>,
    prepared: &gestalt::local_agent_context::PreparedLocalAgentCommand,
    run_id: &str,
) -> gestalt::local_agent_episode::LocalAgentEpisodeStatus {
    record_local_agent_episode(
        bridge,
        episode_request_from_prepared_command(
            1,
            "/workspace/demo".to_string(),
            Some(run_id.to_string()),
            prepared,
            1,
            0,
        ),
    )
    .await
    .expect("episode should record")
}

fn registry_with_provider(
    provider: Arc<dyn MembraneProvider>,
) -> Arc<dyn emily_membrane::providers::MembraneProviderRegistry> {
    Arc::new(InMemoryProviderRegistry::single_target(
        RegisteredProviderTarget {
            target: ProviderTarget {
                provider_id: provider.provider_id().to_string(),
                model_id: Some("Qwen3.5-35B-A3B-GGUF".to_string()),
                profile_id: Some("reasoning".to_string()),
                capability_tags: vec!["analysis".to_string(), "reasoning".to_string()],
                metadata: serde_json::json!({"source": "test"}),
            },
            metadata_class: ProviderMetadataClass::Preferred,
            latency_class: emily_membrane::providers::ProviderLatencyClass::Medium,
            cost_class: emily_membrane::providers::ProviderCostClass::Medium,
            validation_compatibility: ProviderValidationCompatibility::ReviewFriendly,
            telemetry: None,
        },
        provider,
    ))
}

fn seeded_group_orchestrator() -> GroupOrchestratorSnapshot {
    GroupOrchestratorSnapshot {
        group_id: 1,
        group_path: "/workspace/demo".to_string(),
        terminals: vec![GroupTerminalState {
            session_id: 1,
            title: "Agent".to_string(),
            role: SessionRole::Agent,
            status: SessionStatus::Idle,
            cwd: "/workspace/demo".to_string(),
            is_selected: true,
            is_focused: true,
            is_runtime_ready: true,
            latest_round: TerminalRound {
                start_row: 1,
                end_row: 2,
                lines: vec!["git status".to_string()],
            },
        }],
    }
}

fn unique_locator(name: &str) -> DatabaseLocator {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let storage_path =
        std::env::temp_dir().join(format!("gestalt-{name}-{nonce}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&storage_path);
    DatabaseLocator {
        storage_path,
        namespace: "gestalt_test".to_string(),
        database: "default".to_string(),
    }
}
