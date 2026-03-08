use super::test_support::{MockStore, locator};
use super::*;
use crate::api::EmilyApi;
use crate::model::{
    AppendSovereignAuditRecordRequest, AuditRecordKind, CreateEpisodeRequest, EpisodeState,
    RemoteEpisodeRequest, RemoteEpisodeState, RoutingDecision, RoutingDecisionKind, RoutingTarget,
    SovereignAuditMetadata, UpdateRemoteEpisodeStateRequest, ValidationDecision, ValidationFinding,
    ValidationFindingSeverity, ValidationOutcome,
};
use serde_json::json;
use std::sync::Arc;

fn episode_request() -> CreateEpisodeRequest {
    CreateEpisodeRequest {
        episode_id: "ep-sovereign".to_string(),
        stream_id: Some("stream-a".to_string()),
        source_kind: "terminal".to_string(),
        episode_kind: "command_round".to_string(),
        started_at_unix_ms: 1,
        intent: Some("route task".to_string()),
        metadata: json!({"cwd": "/tmp"}),
    }
}

fn routing_decision() -> RoutingDecision {
    RoutingDecision {
        decision_id: "route-1".to_string(),
        episode_id: "ep-sovereign".to_string(),
        kind: RoutingDecisionKind::SingleRemote,
        decided_at_unix_ms: 2,
        rationale: Some("specialized reasoning".to_string()),
        targets: vec![RoutingTarget {
            provider_id: "provider-a".to_string(),
            model_id: Some("model-x".to_string()),
            profile_id: Some("reasoning".to_string()),
            capability_tags: vec!["analysis".to_string()],
            metadata: json!({"priority": 1}),
        }],
        metadata: json!({"source": "planner"}),
    }
}

fn local_only_routing_decision() -> RoutingDecision {
    RoutingDecision {
        decision_id: "route-local".to_string(),
        episode_id: "ep-sovereign".to_string(),
        kind: RoutingDecisionKind::LocalOnly,
        decided_at_unix_ms: 2,
        rationale: Some("stay local".to_string()),
        targets: Vec::new(),
        metadata: json!({"source": "planner"}),
    }
}

fn rejected_routing_decision() -> RoutingDecision {
    RoutingDecision {
        decision_id: "route-rejected".to_string(),
        episode_id: "ep-sovereign".to_string(),
        kind: RoutingDecisionKind::Rejected,
        decided_at_unix_ms: 2,
        rationale: Some("boundary violation".to_string()),
        targets: Vec::new(),
        metadata: json!({"source": "planner"}),
    }
}

#[tokio::test]
async fn record_routing_decision_persists_and_is_idempotent() {
    let store = Arc::new(MockStore::default());
    let runtime = EmilyRuntime::new(store.clone());
    runtime.open_db(locator()).await.expect("open");
    runtime
        .create_episode(episode_request())
        .await
        .expect("create episode");

    let first = runtime
        .record_routing_decision(routing_decision())
        .await
        .expect("record route");
    let second = runtime
        .record_routing_decision(routing_decision())
        .await
        .expect("record route idempotent");

    assert_eq!(first, second);
    assert_eq!(store.routing_decisions.lock().await.len(), 1);
    assert_eq!(store.audits.lock().await.len(), 1);
}

#[tokio::test]
async fn create_remote_episode_requires_matching_route_decision() {
    let store = Arc::new(MockStore::default());
    let runtime = EmilyRuntime::new(store);
    runtime.open_db(locator()).await.expect("open");
    runtime
        .create_episode(episode_request())
        .await
        .expect("create episode");

    let error = runtime
        .create_remote_episode(RemoteEpisodeRequest {
            remote_episode_id: "remote-1".to_string(),
            episode_id: "ep-sovereign".to_string(),
            route_decision_id: Some("missing-route".to_string()),
            dispatch_kind: "bounded_program".to_string(),
            dispatched_at_unix_ms: 3,
            metadata: json!({"provider": "provider-a"}),
        })
        .await
        .expect_err("missing route decision should fail");

    assert!(matches!(error, EmilyError::InvalidRequest(_)));
}

#[tokio::test]
async fn create_remote_episode_rejects_local_only_route() {
    let store = Arc::new(MockStore::default());
    let runtime = EmilyRuntime::new(store);
    runtime.open_db(locator()).await.expect("open");
    runtime
        .create_episode(episode_request())
        .await
        .expect("create episode");
    runtime
        .record_routing_decision(local_only_routing_decision())
        .await
        .expect("record local route");

    let error = runtime
        .create_remote_episode(RemoteEpisodeRequest {
            remote_episode_id: "remote-1".to_string(),
            episode_id: "ep-sovereign".to_string(),
            route_decision_id: Some("route-local".to_string()),
            dispatch_kind: "bounded_program".to_string(),
            dispatched_at_unix_ms: 3,
            metadata: json!({"provider": "provider-a"}),
        })
        .await
        .expect_err("local-only route should not allow remote dispatch");

    assert!(matches!(error, EmilyError::InvalidRequest(_)));
}

#[tokio::test]
async fn record_validation_outcome_requires_matching_remote_episode() {
    let store = Arc::new(MockStore::default());
    let runtime = EmilyRuntime::new(store.clone());
    runtime.open_db(locator()).await.expect("open");
    runtime
        .create_episode(episode_request())
        .await
        .expect("create episode");
    runtime
        .record_routing_decision(routing_decision())
        .await
        .expect("record route");
    runtime
        .create_remote_episode(RemoteEpisodeRequest {
            remote_episode_id: "remote-1".to_string(),
            episode_id: "ep-sovereign".to_string(),
            route_decision_id: Some("route-1".to_string()),
            dispatch_kind: "bounded_program".to_string(),
            dispatched_at_unix_ms: 3,
            metadata: json!({"provider": "provider-a"}),
        })
        .await
        .expect("create remote episode");

    let outcome = runtime
        .record_validation_outcome(ValidationOutcome {
            validation_id: "validation-1".to_string(),
            episode_id: "ep-sovereign".to_string(),
            remote_episode_id: Some("remote-1".to_string()),
            decision: ValidationDecision::AcceptedWithCaution,
            validated_at_unix_ms: 4,
            findings: vec![ValidationFinding {
                code: "uncertain_reference".to_string(),
                severity: ValidationFindingSeverity::Warning,
                message: "cross-check before use".to_string(),
            }],
            metadata: json!({"checker": "eccr"}),
        })
        .await
        .expect("record validation");

    assert_eq!(outcome.validation_id, "validation-1");
    assert_eq!(store.audits.lock().await.len(), 3);
    assert_eq!(
        runtime
            .remote_episode("remote-1")
            .await
            .expect("get remote")
            .expect("remote")
            .state,
        RemoteEpisodeState::Succeeded
    );
    assert_eq!(
        runtime
            .remote_episode("remote-1")
            .await
            .expect("get remote")
            .expect("remote")
            .completed_at_unix_ms,
        Some(4)
    );
    assert_eq!(
        store.episodes.lock().await[0].state,
        EpisodeState::Cautioned
    );
}

#[tokio::test]
async fn rejected_routing_decision_blocks_episode() {
    let store = Arc::new(MockStore::default());
    let runtime = EmilyRuntime::new(store.clone());
    runtime.open_db(locator()).await.expect("open");
    runtime
        .create_episode(episode_request())
        .await
        .expect("create episode");

    runtime
        .record_routing_decision(rejected_routing_decision())
        .await
        .expect("record rejected route");

    assert_eq!(store.episodes.lock().await[0].state, EpisodeState::Blocked);
}

#[tokio::test]
async fn rejected_validation_rejects_remote_episode_and_blocks_episode() {
    let store = Arc::new(MockStore::default());
    let runtime = EmilyRuntime::new(store.clone());
    runtime.open_db(locator()).await.expect("open");
    runtime
        .create_episode(episode_request())
        .await
        .expect("create episode");
    runtime
        .record_routing_decision(routing_decision())
        .await
        .expect("record route");
    runtime
        .create_remote_episode(RemoteEpisodeRequest {
            remote_episode_id: "remote-1".to_string(),
            episode_id: "ep-sovereign".to_string(),
            route_decision_id: Some("route-1".to_string()),
            dispatch_kind: "bounded_program".to_string(),
            dispatched_at_unix_ms: 3,
            metadata: json!({"provider": "provider-a"}),
        })
        .await
        .expect("create remote episode");

    runtime
        .record_validation_outcome(ValidationOutcome {
            validation_id: "validation-1".to_string(),
            episode_id: "ep-sovereign".to_string(),
            remote_episode_id: Some("remote-1".to_string()),
            decision: ValidationDecision::Rejected,
            validated_at_unix_ms: 4,
            findings: vec![ValidationFinding {
                code: "policy_violation".to_string(),
                severity: ValidationFindingSeverity::Error,
                message: "output crossed the approved boundary".to_string(),
            }],
            metadata: json!({"checker": "eccr"}),
        })
        .await
        .expect("record rejected validation");

    let remote_episode = runtime
        .remote_episode("remote-1")
        .await
        .expect("get remote episode")
        .expect("remote episode");
    assert_eq!(remote_episode.state, RemoteEpisodeState::Rejected);
    assert_eq!(remote_episode.completed_at_unix_ms, Some(4));
    assert_eq!(store.episodes.lock().await[0].state, EpisodeState::Blocked);
}

#[tokio::test]
async fn explicit_remote_state_transition_marks_failed_and_cautions_episode() {
    let store = Arc::new(MockStore::default());
    let runtime = EmilyRuntime::new(store.clone());
    runtime.open_db(locator()).await.expect("open");
    runtime
        .create_episode(episode_request())
        .await
        .expect("create episode");
    runtime
        .record_routing_decision(routing_decision())
        .await
        .expect("record route");
    runtime
        .create_remote_episode(RemoteEpisodeRequest {
            remote_episode_id: "remote-1".to_string(),
            episode_id: "ep-sovereign".to_string(),
            route_decision_id: Some("route-1".to_string()),
            dispatch_kind: "bounded_program".to_string(),
            dispatched_at_unix_ms: 3,
            metadata: json!({"provider": "provider-a"}),
        })
        .await
        .expect("create remote episode");

    let remote_episode = runtime
        .update_remote_episode_state(UpdateRemoteEpisodeStateRequest {
            remote_episode_id: "remote-1".to_string(),
            next_state: RemoteEpisodeState::Failed,
            transitioned_at_unix_ms: 5,
            summary: Some("provider timeout".to_string()),
            metadata: json!({"origin": "host"}),
        })
        .await
        .expect("mark remote episode failed");

    assert_eq!(remote_episode.state, RemoteEpisodeState::Failed);
    assert_eq!(remote_episode.completed_at_unix_ms, Some(5));
    assert_eq!(
        store.episodes.lock().await[0].state,
        EpisodeState::Cautioned
    );
    assert_eq!(store.audits.lock().await.len(), 3);
}

#[tokio::test]
async fn explicit_remote_state_transition_is_idempotent() {
    let store = Arc::new(MockStore::default());
    let runtime = EmilyRuntime::new(store.clone());
    runtime.open_db(locator()).await.expect("open");
    runtime
        .create_episode(episode_request())
        .await
        .expect("create episode");
    runtime
        .record_routing_decision(routing_decision())
        .await
        .expect("record route");
    runtime
        .create_remote_episode(RemoteEpisodeRequest {
            remote_episode_id: "remote-1".to_string(),
            episode_id: "ep-sovereign".to_string(),
            route_decision_id: Some("route-1".to_string()),
            dispatch_kind: "bounded_program".to_string(),
            dispatched_at_unix_ms: 3,
            metadata: json!({"provider": "provider-a"}),
        })
        .await
        .expect("create remote episode");

    let first = runtime
        .update_remote_episode_state(UpdateRemoteEpisodeStateRequest {
            remote_episode_id: "remote-1".to_string(),
            next_state: RemoteEpisodeState::Cancelled,
            transitioned_at_unix_ms: 5,
            summary: Some("host cancelled remote work".to_string()),
            metadata: json!({"origin": "host"}),
        })
        .await
        .expect("cancel remote episode");
    let second = runtime
        .update_remote_episode_state(UpdateRemoteEpisodeStateRequest {
            remote_episode_id: "remote-1".to_string(),
            next_state: RemoteEpisodeState::Cancelled,
            transitioned_at_unix_ms: 5,
            summary: Some("host cancelled remote work".to_string()),
            metadata: json!({"origin": "host"}),
        })
        .await
        .expect("replay cancel remote episode");

    assert_eq!(first, second);
    assert_eq!(store.remote_episodes.lock().await.len(), 1);
    assert_eq!(store.audits.lock().await.len(), 3);
}

#[tokio::test]
async fn explicit_remote_state_transition_rejects_conflicting_terminal_change() {
    let store = Arc::new(MockStore::default());
    let runtime = EmilyRuntime::new(store);
    runtime.open_db(locator()).await.expect("open");
    runtime
        .create_episode(episode_request())
        .await
        .expect("create episode");
    runtime
        .record_routing_decision(routing_decision())
        .await
        .expect("record route");
    runtime
        .create_remote_episode(RemoteEpisodeRequest {
            remote_episode_id: "remote-1".to_string(),
            episode_id: "ep-sovereign".to_string(),
            route_decision_id: Some("route-1".to_string()),
            dispatch_kind: "bounded_program".to_string(),
            dispatched_at_unix_ms: 3,
            metadata: json!({"provider": "provider-a"}),
        })
        .await
        .expect("create remote episode");
    runtime
        .update_remote_episode_state(UpdateRemoteEpisodeStateRequest {
            remote_episode_id: "remote-1".to_string(),
            next_state: RemoteEpisodeState::Succeeded,
            transitioned_at_unix_ms: 5,
            summary: Some("remote work completed".to_string()),
            metadata: json!({"origin": "host"}),
        })
        .await
        .expect("mark remote episode succeeded");

    let error = runtime
        .update_remote_episode_state(UpdateRemoteEpisodeStateRequest {
            remote_episode_id: "remote-1".to_string(),
            next_state: RemoteEpisodeState::Failed,
            transitioned_at_unix_ms: 6,
            summary: Some("conflicting failure".to_string()),
            metadata: json!({"origin": "host"}),
        })
        .await
        .expect_err("conflicting terminal change should fail");

    assert!(matches!(error, EmilyError::InvalidRequest(_)));
}

#[tokio::test]
async fn append_sovereign_audit_record_persists_structured_metadata() {
    let store = Arc::new(MockStore::default());
    let runtime = EmilyRuntime::new(store.clone());
    runtime.open_db(locator()).await.expect("open");
    runtime
        .create_episode(episode_request())
        .await
        .expect("create episode");
    runtime
        .record_routing_decision(routing_decision())
        .await
        .expect("record route");

    let audit = runtime
        .append_sovereign_audit_record(AppendSovereignAuditRecordRequest {
            audit_id: "audit-sovereign-1".to_string(),
            episode_id: "ep-sovereign".to_string(),
            kind: AuditRecordKind::RoutingDecided,
            ts_unix_ms: 5,
            summary: "route selected".to_string(),
            metadata: SovereignAuditMetadata {
                remote_episode_id: None,
                route_decision_id: Some("route-1".to_string()),
                validation_id: None,
                boundary_profile: Some("bounded-v1".to_string()),
                metadata: json!({"origin": "test"}),
            },
        })
        .await
        .expect("append sovereign audit");

    assert_eq!(audit.kind, AuditRecordKind::RoutingDecided);
    assert_eq!(
        audit.metadata["sovereign"]["route_decision_id"],
        json!("route-1")
    );
    assert_eq!(store.audits.lock().await.len(), 2);
}

#[tokio::test]
async fn sovereign_read_api_returns_persisted_records() {
    let store = Arc::new(MockStore::default());
    let runtime = EmilyRuntime::new(store.clone());
    runtime.open_db(locator()).await.expect("open");
    runtime
        .create_episode(episode_request())
        .await
        .expect("create episode");
    runtime
        .record_routing_decision(routing_decision())
        .await
        .expect("record route");
    runtime
        .create_remote_episode(RemoteEpisodeRequest {
            remote_episode_id: "remote-1".to_string(),
            episode_id: "ep-sovereign".to_string(),
            route_decision_id: Some("route-1".to_string()),
            dispatch_kind: "bounded_program".to_string(),
            dispatched_at_unix_ms: 3,
            metadata: json!({"provider": "provider-a"}),
        })
        .await
        .expect("create remote episode");
    runtime
        .record_validation_outcome(ValidationOutcome {
            validation_id: "validation-1".to_string(),
            episode_id: "ep-sovereign".to_string(),
            remote_episode_id: Some("remote-1".to_string()),
            decision: ValidationDecision::AcceptedWithCaution,
            validated_at_unix_ms: 4,
            findings: vec![ValidationFinding {
                code: "uncertain_reference".to_string(),
                severity: ValidationFindingSeverity::Warning,
                message: "cross-check before use".to_string(),
            }],
            metadata: json!({"checker": "eccr"}),
        })
        .await
        .expect("record validation");
    runtime
        .append_sovereign_audit_record(AppendSovereignAuditRecordRequest {
            audit_id: "audit-sovereign-1".to_string(),
            episode_id: "ep-sovereign".to_string(),
            kind: AuditRecordKind::RoutingDecided,
            ts_unix_ms: 5,
            summary: "route selected".to_string(),
            metadata: SovereignAuditMetadata {
                remote_episode_id: Some("remote-1".to_string()),
                route_decision_id: Some("route-1".to_string()),
                validation_id: Some("validation-1".to_string()),
                boundary_profile: Some("bounded-v1".to_string()),
                metadata: json!({"origin": "test"}),
            },
        })
        .await
        .expect("append sovereign audit");

    assert_eq!(
        runtime
            .routing_decision("route-1")
            .await
            .expect("get route")
            .expect("route")
            .decision_id,
        "route-1"
    );
    assert_eq!(
        runtime
            .routing_decisions_for_episode("ep-sovereign")
            .await
            .expect("list routes")
            .len(),
        1
    );
    assert_eq!(
        runtime
            .remote_episode("remote-1")
            .await
            .expect("get remote episode")
            .expect("remote episode")
            .id,
        "remote-1"
    );
    assert_eq!(
        runtime
            .remote_episodes_for_episode("ep-sovereign")
            .await
            .expect("list remote episodes")
            .len(),
        1
    );
    assert_eq!(
        runtime
            .validation_outcome("validation-1")
            .await
            .expect("get validation")
            .expect("validation")
            .validation_id,
        "validation-1"
    );
    assert_eq!(
        runtime
            .validation_outcomes_for_episode("ep-sovereign")
            .await
            .expect("list validation")
            .len(),
        1
    );
    assert_eq!(
        runtime
            .sovereign_audit_records_for_episode("ep-sovereign")
            .await
            .expect("list sovereign audits")
            .len(),
        4
    );
}

#[tokio::test]
async fn automatic_sovereign_audits_are_replay_safe() {
    let store = Arc::new(MockStore::default());
    let runtime = EmilyRuntime::new(store.clone());
    runtime.open_db(locator()).await.expect("open");
    runtime
        .create_episode(episode_request())
        .await
        .expect("create episode");

    runtime
        .record_routing_decision(routing_decision())
        .await
        .expect("record route");
    runtime
        .record_routing_decision(routing_decision())
        .await
        .expect("record route replay");

    runtime
        .create_remote_episode(RemoteEpisodeRequest {
            remote_episode_id: "remote-1".to_string(),
            episode_id: "ep-sovereign".to_string(),
            route_decision_id: Some("route-1".to_string()),
            dispatch_kind: "bounded_program".to_string(),
            dispatched_at_unix_ms: 3,
            metadata: json!({"provider": "provider-a"}),
        })
        .await
        .expect("create remote episode");
    runtime
        .create_remote_episode(RemoteEpisodeRequest {
            remote_episode_id: "remote-1".to_string(),
            episode_id: "ep-sovereign".to_string(),
            route_decision_id: Some("route-1".to_string()),
            dispatch_kind: "bounded_program".to_string(),
            dispatched_at_unix_ms: 3,
            metadata: json!({"provider": "provider-a"}),
        })
        .await
        .expect("replay remote episode");

    runtime
        .record_validation_outcome(ValidationOutcome {
            validation_id: "validation-1".to_string(),
            episode_id: "ep-sovereign".to_string(),
            remote_episode_id: Some("remote-1".to_string()),
            decision: ValidationDecision::AcceptedWithCaution,
            validated_at_unix_ms: 4,
            findings: vec![ValidationFinding {
                code: "uncertain_reference".to_string(),
                severity: ValidationFindingSeverity::Warning,
                message: "cross-check before use".to_string(),
            }],
            metadata: json!({"checker": "eccr"}),
        })
        .await
        .expect("record validation");
    runtime
        .record_validation_outcome(ValidationOutcome {
            validation_id: "validation-1".to_string(),
            episode_id: "ep-sovereign".to_string(),
            remote_episode_id: Some("remote-1".to_string()),
            decision: ValidationDecision::AcceptedWithCaution,
            validated_at_unix_ms: 4,
            findings: vec![ValidationFinding {
                code: "uncertain_reference".to_string(),
                severity: ValidationFindingSeverity::Warning,
                message: "cross-check before use".to_string(),
            }],
            metadata: json!({"checker": "eccr"}),
        })
        .await
        .expect("replay validation");

    let audits = runtime
        .sovereign_audit_records_for_episode("ep-sovereign")
        .await
        .expect("list sovereign audits");
    assert_eq!(audits.len(), 3);
}
