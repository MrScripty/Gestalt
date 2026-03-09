use emily::api::EmilyApi;
use emily::model::{DatabaseLocator, ValidationDecision};
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use gestalt::emily_bridge::EmilyBridge;
use gestalt::emily_seed::{SYNTHETIC_TERMINAL_DATASET, seed_builtin_corpus};
use gestalt::local_agent_context::prepare_local_agent_command;
use gestalt::local_agent_episode::{
    episode_request_from_prepared_command, record_local_agent_episode,
};
use gestalt::local_agent_membrane::run_local_agent_membrane_pass;
use gestalt::orchestrator::{GroupOrchestratorSnapshot, GroupTerminalState, TerminalRound};
use gestalt::state::{SessionRole, SessionStatus};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn local_agent_membrane_pass_records_local_only_sovereign_artifacts() {
    let locator = unique_locator("emily-local-agent-membrane");
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
        let _ = seed_builtin_corpus(&emily_runtime, SYNTHETIC_TERMINAL_DATASET)
            .await
            .expect("terminal dataset should seed");
        emily_runtime
            .close_db()
            .await
            .expect("db should close cleanly");
    });

    let bridge = Arc::new(EmilyBridge::new(locator));
    let prepared = runtime.block_on(prepare_local_agent_command(
        bridge.clone(),
        seeded_group_orchestrator(),
        "Summarize recent terminal context for the local-only membrane path".to_string(),
    ));
    let episode = runtime
        .block_on(record_local_agent_episode(
            bridge.clone(),
            episode_request_from_prepared_command(
                1,
                "/workspace/demo".to_string(),
                Some("run-membrane".to_string()),
                &prepared,
                1,
                0,
            ),
        ))
        .expect("episode should record");
    let membrane = runtime
        .block_on(run_local_agent_membrane_pass(
            bridge.clone(),
            &episode.episode_id,
            &prepared,
        ))
        .expect("local-only membrane pass should succeed");

    assert_eq!(
        membrane.policy_outcome,
        emily_membrane::contracts::RoutingPolicyOutcome::LocalOnly
    );
    assert_eq!(
        membrane.validation_disposition,
        Some(emily_membrane::contracts::MembraneValidationDisposition::Accepted)
    );
    assert!(membrane.reference_count >= 2);
    assert_eq!(
        membrane.route_decision_id.as_deref(),
        Some("local-agent:run-membrane:local-agent-membrane:route")
    );
    assert_eq!(
        membrane.validation_id.as_deref(),
        Some("local-agent:run-membrane:local-agent-membrane:validation")
    );

    let routes = runtime
        .block_on(bridge.routing_decisions_for_episode_async(episode.episode_id.clone()))
        .expect("routing decisions should load");
    let validations = runtime
        .block_on(bridge.validation_outcomes_for_episode_async(episode.episode_id.clone()))
        .expect("validation outcomes should load");
    let audits = runtime
        .block_on(bridge.sovereign_audit_records_for_episode_async(episode.episode_id.clone()))
        .expect("sovereign audits should load");

    assert_eq!(routes.len(), 1);
    assert_eq!(validations.len(), 1);
    assert_eq!(validations[0].decision, ValidationDecision::Accepted);
    assert_eq!(audits.len(), 2);

    drop(bridge);
    thread::sleep(Duration::from_millis(50));
    let _ = std::fs::remove_dir_all(storage_path);
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
