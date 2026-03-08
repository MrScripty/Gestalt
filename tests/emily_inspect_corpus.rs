use emily::api::EmilyApi;
use emily::model::{
    DatabaseLocator, RoutingDecision, RoutingDecisionKind, RoutingTarget, ValidationDecision,
    ValidationFinding, ValidationFindingSeverity, ValidationOutcome,
};
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use gestalt::emily_inspect::inspect_seeded_corpus;
use gestalt::emily_seed::{SYNTHETIC_AGENT_ROUND_DATASET, seed_builtin_corpus};
use serde_json::json;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn inspection_snapshot_reads_context_and_sovereign_records() {
    let locator = unique_locator("emily-inspect-corpus");
    let storage_path = locator.storage_path.clone();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    runtime.block_on(async {
        let emily_runtime = EmilyRuntime::new(Arc::new(SurrealEmilyStore::new()));
        emily_runtime
            .open_db(locator.clone())
            .await
            .expect("db should open");

        let _ = seed_builtin_corpus(&emily_runtime, SYNTHETIC_AGENT_ROUND_DATASET)
            .await
            .expect("agent dataset should seed");

        let _ = emily_runtime
            .record_routing_decision(RoutingDecision {
                decision_id: "seed-route-agent-round".to_string(),
                episode_id: "seed-episode-agent-round".to_string(),
                kind: RoutingDecisionKind::SingleRemote,
                decided_at_unix_ms: 1_730_000_106_000,
                rationale: Some("specialized provider selected for debugging".to_string()),
                targets: vec![RoutingTarget {
                    provider_id: "pantograph".to_string(),
                    model_id: Some("qwen3".to_string()),
                    profile_id: Some("reasoning".to_string()),
                    capability_tags: vec!["analysis".to_string(), "code".to_string()],
                    metadata: json!({"priority": 1}),
                }],
                metadata: json!({"dataset": SYNTHETIC_AGENT_ROUND_DATASET}),
            })
            .await
            .expect("routing decision should record");

        let _ = emily_runtime
            .record_validation_outcome(ValidationOutcome {
                validation_id: "seed-validation-agent-round".to_string(),
                episode_id: "seed-episode-agent-round".to_string(),
                remote_episode_id: None,
                decision: ValidationDecision::AcceptedWithCaution,
                validated_at_unix_ms: 1_730_000_107_000,
                findings: vec![ValidationFinding {
                    code: "needs-human-review".to_string(),
                    severity: ValidationFindingSeverity::Warning,
                    message: "debug output should be reviewed before reuse".to_string(),
                }],
                metadata: json!({"dataset": SYNTHETIC_AGENT_ROUND_DATASET}),
            })
            .await
            .expect("validation outcome should record");

        let snapshot = inspect_seeded_corpus(
            &emily_runtime,
            SYNTHETIC_AGENT_ROUND_DATASET,
            4,
            Some("provider registry capability tags"),
            2,
        )
        .await
        .expect("inspection should succeed");

        assert_eq!(snapshot.label, SYNTHETIC_AGENT_ROUND_DATASET);
        assert_eq!(snapshot.streams.len(), 1);
        assert_eq!(snapshot.streams[0].history.items.len(), 4);
        assert!(
            snapshot.streams[0]
                .context
                .as_ref()
                .is_some_and(|packet| !packet.items.is_empty())
        );

        assert_eq!(snapshot.episodes.len(), 1);
        let episode = &snapshot.episodes[0];
        assert_eq!(episode.episode_id, "seed-episode-agent-round");
        assert!(episode.latest_earl.is_none());
        assert_eq!(episode.routing_decisions.len(), 1);
        assert_eq!(episode.validation_outcomes.len(), 1);
        assert!(
            episode.sovereign_audits.len() >= 2,
            "routing and validation writes should generate sovereign audits"
        );

        emily_runtime
            .close_db()
            .await
            .expect("db should close cleanly");
    });

    let _ = std::fs::remove_dir_all(storage_path);
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
