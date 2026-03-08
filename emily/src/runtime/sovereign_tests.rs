use super::test_support::{MockStore, locator};
use super::*;
use crate::api::EmilyApi;
use crate::model::{
    AppendSovereignAuditRecordRequest, AuditRecordKind, CreateEpisodeRequest, RemoteEpisodeRequest,
    RoutingDecision, RoutingDecisionKind, RoutingTarget, SovereignAuditMetadata,
    ValidationDecision, ValidationFinding, ValidationFindingSeverity, ValidationOutcome,
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
async fn record_validation_outcome_requires_matching_remote_episode() {
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
    assert_eq!(store.audits.lock().await.len(), 1);
}
