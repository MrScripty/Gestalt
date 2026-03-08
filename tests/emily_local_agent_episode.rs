use emily::api::EmilyApi;
use emily::model::{DatabaseLocator, EarlDecision, EpisodeState};
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use gestalt::emily_bridge::EmilyBridge;
use gestalt::emily_seed::{
    SYNTHETIC_RISK_GATED_DATASET, SYNTHETIC_TERMINAL_DATASET, seed_builtin_corpus,
};
use gestalt::local_agent_context::prepare_local_agent_command;
use gestalt::local_agent_episode::{
    LocalAgentEpisodeGate, episode_request_from_prepared_command, inspect_local_agent_episode,
    record_local_agent_episode,
};
use gestalt::orchestrator::{GroupOrchestratorSnapshot, GroupTerminalState, TerminalRound};
use gestalt::state::{SessionRole, SessionStatus};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn local_agent_episode_recording_creates_open_episode_and_links_context() {
    let locator = unique_locator("emily-local-agent-episode-open");
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
        "Summarize recent terminal context".to_string(),
    ));
    let status = runtime
        .block_on(record_local_agent_episode(
            bridge.clone(),
            episode_request_from_prepared_command(
                1,
                "/workspace/demo".to_string(),
                Some("run-open".to_string()),
                &prepared,
                1,
                0,
            ),
        ))
        .expect("episode should record");

    assert_eq!(status.episode_id, "local-agent:run-open");
    assert_eq!(status.state, EpisodeState::Open);
    assert_eq!(status.latest_earl, None);
    assert_eq!(status.gate, LocalAgentEpisodeGate::Proceed);

    drop(bridge);
    thread::sleep(Duration::from_millis(50));
    let _ = std::fs::remove_dir_all(storage_path);
}

#[test]
fn host_gate_reads_seeded_caution_and_blocked_episode_states() {
    let locator = unique_locator("emily-local-agent-episode-risk");
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
        let _ = seed_builtin_corpus(&emily_runtime, SYNTHETIC_RISK_GATED_DATASET)
            .await
            .expect("risk dataset should seed");
        emily_runtime
            .close_db()
            .await
            .expect("db should close cleanly");
    });

    let bridge = Arc::new(EmilyBridge::new(locator));
    let caution = runtime
        .block_on(inspect_local_agent_episode(
            bridge.clone(),
            "seed-episode-risk-caution",
        ))
        .expect("caution episode should inspect");
    assert_eq!(caution.state, EpisodeState::Cautioned);
    assert_eq!(caution.latest_earl, Some(EarlDecision::Caution));
    assert_eq!(caution.gate, LocalAgentEpisodeGate::Caution);

    let blocked = runtime
        .block_on(inspect_local_agent_episode(
            bridge.clone(),
            "seed-episode-risk-reflex",
        ))
        .expect("blocked episode should inspect");
    assert_eq!(blocked.state, EpisodeState::Blocked);
    assert_eq!(blocked.latest_earl, Some(EarlDecision::Reflex));
    assert_eq!(blocked.gate, LocalAgentEpisodeGate::Blocked);

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
