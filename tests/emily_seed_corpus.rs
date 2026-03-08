use emily::api::EmilyApi;
use emily::model::{ContextQuery, DatabaseLocator, EarlDecision, EpisodeState, HistoryPageRequest};
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use gestalt::emily_seed::{
    SYNTHETIC_AGENT_ROUND_DATASET, SYNTHETIC_RISK_GATED_DATASET,
    SYNTHETIC_SEMANTIC_CONTEXT_DATASET, SYNTHETIC_TERMINAL_DATASET, seed_builtin_corpus,
};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn seeded_corpora_roundtrip_through_public_emily_facade() {
    let locator = unique_locator("emily-seed-corpus");
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

        let terminal_report = seed_builtin_corpus(&emily_runtime, SYNTHETIC_TERMINAL_DATASET)
            .await
            .expect("terminal dataset should seed");
        assert_eq!(terminal_report.text_objects_seeded, 5);

        let semantic_report =
            seed_builtin_corpus(&emily_runtime, SYNTHETIC_SEMANTIC_CONTEXT_DATASET)
                .await
                .expect("semantic dataset should seed");
        assert_eq!(semantic_report.text_objects_seeded, 3);

        let agent_report = seed_builtin_corpus(&emily_runtime, SYNTHETIC_AGENT_ROUND_DATASET)
            .await
            .expect("agent dataset should seed");
        assert_eq!(agent_report.episodes_seeded, 1);

        let risk_report = seed_builtin_corpus(&emily_runtime, SYNTHETIC_RISK_GATED_DATASET)
            .await
            .expect("risk dataset should seed");
        assert_eq!(risk_report.earl_evaluations_seeded, 2);

        let history = emily_runtime
            .page_history_before(HistoryPageRequest {
                stream_id: "terminal:1".to_string(),
                before_sequence: None,
                limit: 8,
            })
            .await
            .expect("history should load");
        assert_eq!(history.items.len(), 5);
        assert_eq!(history.items[0].sequence, 5);
        assert!(
            history.items[0]
                .metadata
                .get("dataset")
                .and_then(|value| value.as_str())
                .is_some_and(|value| value == SYNTHETIC_TERMINAL_DATASET)
        );

        let context = emily_runtime
            .query_context(ContextQuery {
                stream_id: Some("seed:agent:round-1".to_string()),
                query_text: "provider registry capability tags".to_string(),
                top_k: 2,
                neighbor_depth: 1,
            })
            .await
            .expect("context query should succeed");
        assert!(
            !context.items.is_empty(),
            "seeded context should be queryable"
        );
        assert!(
            context
                .items
                .iter()
                .any(|item| item.object.text.contains("capability tags were registered"))
        );

        let agent_episode = emily_runtime
            .episode("seed-episode-agent-round")
            .await
            .expect("episode read should succeed")
            .expect("agent episode should exist");
        assert_eq!(agent_episode.state, EpisodeState::Completed);

        let caution_earl = emily_runtime
            .latest_earl_evaluation_for_episode("seed-episode-risk-caution")
            .await
            .expect("caution EARL should load")
            .expect("caution EARL should exist");
        assert_eq!(caution_earl.decision, EarlDecision::Caution);

        let reflex_episode = emily_runtime
            .episode("seed-episode-risk-reflex")
            .await
            .expect("reflex episode read should succeed")
            .expect("reflex episode should exist");
        assert_eq!(reflex_episode.state, EpisodeState::Blocked);

        let reflex_earl = emily_runtime
            .latest_earl_evaluation_for_episode("seed-episode-risk-reflex")
            .await
            .expect("reflex EARL should load")
            .expect("reflex EARL should exist");
        assert_eq!(reflex_earl.decision, EarlDecision::Reflex);

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
