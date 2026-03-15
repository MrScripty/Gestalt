use gestalt::orchestrator;
use gestalt::persistence;
use gestalt::state::{AppState, SessionId};
use gestalt::terminal::{TerminalManager, TerminalSnapshot};
#[cfg(feature = "terminal-native-spike")]
use gestalt::terminal_native::{AlacrittyEmulator, AlacrittyEmulatorConfig, TerminalGpuSceneCache};
use serde::Serialize;
use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};
#[cfg(feature = "terminal-native-spike")]
use vt100::Parser;

const WARMUP_HISTORY_LINES: usize = 12_000;
const HISTORY_LINE_WIDTH: usize = 96;
const WARMUP_READY_TIMEOUT: Duration = Duration::from_secs(30);
const TYPING_SAMPLES: usize = 320;
const TYPING_INTERVAL: Duration = Duration::from_millis(8);
const ASSERT_LOCK_WAIT_P95_US: u128 = 200;
const ASSERT_RENDER_TOTAL_P95_US: u128 = 1_000;
const ASSERT_FULL_TOTAL_P95_US: u128 = 1_500;
const JSON_PREFIX: &str = "GESTALT_PROFILE_JSON:";
const AUTOSAVE_PERSISTED_HISTORY_LINES: usize = 4_000;
const REFRESH_LOOP_INTERVAL_MS: u64 = 33;
const RESIZE_LOOP_INTERVAL_MS: u64 = 180;
const RENDER_WINDOW_MULTIPLIER: usize = 8;
const RENDER_WINDOW_MIN_ROWS: usize = 256;
const REFRESH_PROBE_ITERATIONS: usize = 180;
const GIT_WATCHER_POLL_SAMPLES: usize = 12;
const STARTUP_PROFILE_SAMPLES: usize = 12;
const STARTUP_PROFILE_EXTRA_GROUPS: usize = 4;
const STARTUP_PROFILE_ACTIVE_GROUP_EXTRA_SESSIONS: usize = 2;
#[cfg(feature = "terminal-native-spike")]
const REPLAY_PROFILE_LINES: usize = 3_000;
#[cfg(feature = "terminal-native-spike")]
const REPLAY_PROFILE_ITERATIONS: usize = 24;
#[cfg(feature = "terminal-native-spike")]
const REPLAY_PROFILE_CHUNK_BYTES: usize = 512;
#[cfg(feature = "terminal-native-spike")]
const REPLAY_PROFILE_ROWS: u16 = 42;
#[cfg(feature = "terminal-native-spike")]
const REPLAY_PROFILE_COLS: u16 = 140;
#[cfg(feature = "terminal-native-spike")]
const REPLAY_PROFILE_SCROLLBACK: usize = 12_000;

#[cfg_attr(not(feature = "terminal-native-spike"), allow(dead_code))]
struct ProfileConfig {
    warmup_history_lines: usize,
    typing_samples: usize,
    render_iterations: usize,
    autosave_iterations: usize,
    refresh_iterations: usize,
    git_watcher_poll_samples: usize,
    startup_samples: usize,
    replay_profile_lines: usize,
    replay_profile_iterations: usize,
}

impl ProfileConfig {
    fn from_env() -> Self {
        Self {
            warmup_history_lines: env_usize(
                "GESTALT_PROFILE_WARMUP_HISTORY_LINES",
                WARMUP_HISTORY_LINES,
            ),
            typing_samples: env_usize("GESTALT_PROFILE_TYPING_SAMPLES", TYPING_SAMPLES),
            render_iterations: env_usize("GESTALT_PROFILE_RENDER_ITERATIONS", 180),
            autosave_iterations: env_usize("GESTALT_PROFILE_AUTOSAVE_ITERATIONS", 36),
            refresh_iterations: env_usize(
                "GESTALT_PROFILE_REFRESH_ITERATIONS",
                REFRESH_PROBE_ITERATIONS,
            ),
            git_watcher_poll_samples: env_usize(
                "GESTALT_PROFILE_GIT_WATCHER_SAMPLES",
                GIT_WATCHER_POLL_SAMPLES,
            ),
            startup_samples: env_usize("GESTALT_PROFILE_STARTUP_SAMPLES", STARTUP_PROFILE_SAMPLES),
            replay_profile_lines: env_usize(
                "GESTALT_PROFILE_REPLAY_LINES",
                replay_profile_lines_default(),
            ),
            replay_profile_iterations: env_usize(
                "GESTALT_PROFILE_REPLAY_ITERATIONS",
                replay_profile_iterations_default(),
            ),
        }
    }
}

fn main() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    let assert_mode = args.iter().any(|arg| arg == "--assert");
    let json_mode = args.iter().any(|arg| arg == "--json");
    let replay_only = args.iter().any(|arg| arg == "--replay-only");
    let config = ProfileConfig::from_env();

    if replay_only {
        let replay_benchmark = profile_terminal_replay_benchmark(&config);
        print_replay_benchmark(&replay_benchmark);
        if json_mode {
            let payload = serde_json::to_string(&ReplayOnlyProfileSummary {
                replay_profile_lines: config.replay_profile_lines,
                replay_profile_iterations: config.replay_profile_iterations,
                replay_legacy_snapshot_build: replay_benchmark
                    .as_ref()
                    .map(|benchmark| benchmark.legacy_snapshot_build),
                replay_legacy_row_render: replay_benchmark
                    .as_ref()
                    .map(|benchmark| benchmark.legacy_row_render),
                replay_legacy_round_bounds: replay_benchmark
                    .as_ref()
                    .map(|benchmark| benchmark.legacy_round_bounds),
                replay_native_snapshot_build: replay_benchmark
                    .as_ref()
                    .map(|benchmark| benchmark.native_snapshot_build),
                replay_native_raster_update: replay_benchmark
                    .as_ref()
                    .map(|benchmark| benchmark.native_raster_update),
            })
            .map_err(|error| format!("failed to serialize replay-only summary: {error}"))?;
            println!("{JSON_PREFIX}{payload}");
        }
        return Ok(());
    }

    let app_state = Arc::new(AppState::default());
    let session_ids = app_state
        .sessions()
        .iter()
        .map(|session| session.id)
        .collect::<Vec<_>>();
    let group_id = app_state
        .active_group_id()
        .ok_or_else(|| "missing active group".to_string())?;
    let cwd = app_state.group_path(group_id).unwrap_or(".").to_string();

    let terminal_manager = Arc::new(TerminalManager::new());

    for session_id in &session_ids {
        terminal_manager
            .ensure_session(*session_id, &cwd)
            .map_err(|error| error.to_string())?;
    }

    seed_terminal_output(&terminal_manager, &session_ids, config.warmup_history_lines)?;

    println!(
        "Profiling keypress latency with {} samples...",
        config.typing_samples
    );
    println!(
        "Warm terminal history lines: {}",
        config.warmup_history_lines
    );

    let render_profile =
        profile_render_hold(&terminal_manager, &app_state, config.render_iterations);
    let autosave_profile =
        profile_autosave_hold(&terminal_manager, &app_state, config.autosave_iterations);
    let refresh_profile =
        profile_refresh_loop(&terminal_manager, &app_state, config.refresh_iterations);
    let git_watcher_poll_profile =
        profile_git_watcher_poll_cost(&cwd, config.git_watcher_poll_samples);
    let render_hold_stats = stats_from_sorted(&render_profile.hold_times_us);
    let autosave_hold_stats = stats_from_sorted(&autosave_profile.hold_times_us);
    let ui_rows_rendered_stats = stats_from_sorted(&render_profile.ui_rows_rendered);
    let ui_row_render_stats = stats_from_sorted(&render_profile.ui_row_render_us);
    let round_bounds_extract_stats = stats_from_sorted(&render_profile.round_bounds_extract_us);
    let orchestrator_round_extract_stats =
        stats_from_sorted(&render_profile.orchestrator_round_extract_us);
    let autosave_snapshot_lines_stats = stats_from_sorted(&autosave_profile.snapshot_lines_total);
    let autosave_fingerprint_stats = stats_from_sorted(&autosave_profile.fingerprint_us);
    let refresh_loop_tick_stats = stats_from_sorted(&refresh_profile.tick_us);
    let refresh_loop_state_clone_stats = stats_from_sorted(&refresh_profile.state_clone_us);
    let resize_measure_stats = stats_from_sorted(&refresh_profile.resize_measure_us);
    let resize_measure_calls_per_sec_stats =
        stats_from_sorted(&refresh_profile.resize_measure_calls_per_sec);
    let scroll_observer_callbacks_per_sec_stats =
        stats_from_sorted(&refresh_profile.scroll_observer_callbacks_per_sec);
    let orchestrator_snapshot_build_stats =
        stats_from_sorted(&refresh_profile.orchestrator_snapshot_build_us);
    let git_watcher_poll_cost_stats = stats_from_sorted(&git_watcher_poll_profile);
    let startup_profile = profile_startup_restore(config.startup_samples)?;
    let startup_active_path_group_ready_stats =
        stats_from_sorted(&startup_profile.active_path_group_ready_us);
    let startup_full_restore_stats = stats_from_sorted(&startup_profile.full_restore_us);
    let replay_benchmark = profile_terminal_replay_benchmark(&config);

    println!();
    println!("Mutex hold timings for heavy operations");
    println!(
        "  render pass us: avg={} p50={} p95={} p99={} max={}",
        render_hold_stats.avg_us,
        render_hold_stats.p50_us,
        render_hold_stats.p95_us,
        render_hold_stats.p99_us,
        render_hold_stats.max_us
    );
    println!(
        "  autosave pass us: avg={} p50={} p95={} p99={} max={}",
        autosave_hold_stats.avg_us,
        autosave_hold_stats.p50_us,
        autosave_hold_stats.p95_us,
        autosave_hold_stats.p99_us,
        autosave_hold_stats.max_us
    );
    println!();
    println!("Render suspect timings");
    println!(
        "  ui rows rendered per refresh: avg={} p50={} p95={} p99={} max={}",
        ui_rows_rendered_stats.avg_us,
        ui_rows_rendered_stats.p50_us,
        ui_rows_rendered_stats.p95_us,
        ui_rows_rendered_stats.p99_us,
        ui_rows_rendered_stats.max_us
    );
    println!(
        "  ui row render pass us: avg={} p50={} p95={} p99={} max={}",
        ui_row_render_stats.avg_us,
        ui_row_render_stats.p50_us,
        ui_row_render_stats.p95_us,
        ui_row_render_stats.p99_us,
        ui_row_render_stats.max_us
    );
    println!(
        "  round bounds extract us: avg={} p50={} p95={} p99={} max={}",
        round_bounds_extract_stats.avg_us,
        round_bounds_extract_stats.p50_us,
        round_bounds_extract_stats.p95_us,
        round_bounds_extract_stats.p99_us,
        round_bounds_extract_stats.max_us
    );
    println!(
        "  orchestrator round extract us: avg={} p50={} p95={} p99={} max={}",
        orchestrator_round_extract_stats.avg_us,
        orchestrator_round_extract_stats.p50_us,
        orchestrator_round_extract_stats.p95_us,
        orchestrator_round_extract_stats.p99_us,
        orchestrator_round_extract_stats.max_us
    );
    println!(
        "  autosave snapshot lines total: avg={} p50={} p95={} p99={} max={}",
        autosave_snapshot_lines_stats.avg_us,
        autosave_snapshot_lines_stats.p50_us,
        autosave_snapshot_lines_stats.p95_us,
        autosave_snapshot_lines_stats.p99_us,
        autosave_snapshot_lines_stats.max_us
    );
    println!(
        "  autosave fingerprint us: avg={} p50={} p95={} p99={} max={}",
        autosave_fingerprint_stats.avg_us,
        autosave_fingerprint_stats.p50_us,
        autosave_fingerprint_stats.p95_us,
        autosave_fingerprint_stats.p99_us,
        autosave_fingerprint_stats.max_us
    );
    println!();
    println!("Refresh and watcher suspect timings");
    println!(
        "  refresh loop tick us: avg={} p50={} p95={} p99={} max={}",
        refresh_loop_tick_stats.avg_us,
        refresh_loop_tick_stats.p50_us,
        refresh_loop_tick_stats.p95_us,
        refresh_loop_tick_stats.p99_us,
        refresh_loop_tick_stats.max_us
    );
    println!(
        "  refresh loop state clone us: avg={} p50={} p95={} p99={} max={}",
        refresh_loop_state_clone_stats.avg_us,
        refresh_loop_state_clone_stats.p50_us,
        refresh_loop_state_clone_stats.p95_us,
        refresh_loop_state_clone_stats.p99_us,
        refresh_loop_state_clone_stats.max_us
    );
    println!(
        "  resize measure us: avg={} p50={} p95={} p99={} max={}",
        resize_measure_stats.avg_us,
        resize_measure_stats.p50_us,
        resize_measure_stats.p95_us,
        resize_measure_stats.p99_us,
        resize_measure_stats.max_us
    );
    println!(
        "  resize measure calls/sec: avg={} p50={} p95={} p99={} max={}",
        resize_measure_calls_per_sec_stats.avg_us,
        resize_measure_calls_per_sec_stats.p50_us,
        resize_measure_calls_per_sec_stats.p95_us,
        resize_measure_calls_per_sec_stats.p99_us,
        resize_measure_calls_per_sec_stats.max_us
    );
    println!(
        "  scroll observer callbacks/sec (estimated): avg={} p50={} p95={} p99={} max={}",
        scroll_observer_callbacks_per_sec_stats.avg_us,
        scroll_observer_callbacks_per_sec_stats.p50_us,
        scroll_observer_callbacks_per_sec_stats.p95_us,
        scroll_observer_callbacks_per_sec_stats.p99_us,
        scroll_observer_callbacks_per_sec_stats.max_us
    );
    println!(
        "  orchestrator snapshot build us: avg={} p50={} p95={} p99={} max={}",
        orchestrator_snapshot_build_stats.avg_us,
        orchestrator_snapshot_build_stats.p50_us,
        orchestrator_snapshot_build_stats.p95_us,
        orchestrator_snapshot_build_stats.p99_us,
        orchestrator_snapshot_build_stats.max_us
    );
    println!(
        "  git watcher poll cost us: avg={} p50={} p95={} p99={} max={}",
        git_watcher_poll_cost_stats.avg_us,
        git_watcher_poll_cost_stats.p50_us,
        git_watcher_poll_cost_stats.p95_us,
        git_watcher_poll_cost_stats.p99_us,
        git_watcher_poll_cost_stats.max_us
    );
    println!();
    println!("Startup suspect timings");
    println!(
        "  startup profile sessions: active_visible={} total={}",
        startup_profile.active_path_group_visible_sessions, startup_profile.total_sessions
    );
    println!(
        "  active path group ready us: avg={} p50={} p95={} p99={} max={}",
        startup_active_path_group_ready_stats.avg_us,
        startup_active_path_group_ready_stats.p50_us,
        startup_active_path_group_ready_stats.p95_us,
        startup_active_path_group_ready_stats.p99_us,
        startup_active_path_group_ready_stats.max_us
    );
    println!(
        "  full restore us: avg={} p50={} p95={} p99={} max={}",
        startup_full_restore_stats.avg_us,
        startup_full_restore_stats.p50_us,
        startup_full_restore_stats.p95_us,
        startup_full_restore_stats.p99_us,
        startup_full_restore_stats.max_us
    );
    print_replay_benchmark(&replay_benchmark);

    let baseline =
        profile_typing_latency(&terminal_manager, session_ids[0], config.typing_samples)?;
    println!();
    println!("Scenario: baseline");
    baseline.print();

    let render_stop = Arc::new(AtomicBool::new(false));
    let render_worker = spawn_render_worker(
        terminal_manager.clone(),
        app_state.clone(),
        render_stop.clone(),
    );
    thread::sleep(Duration::from_millis(250));
    let render_contended =
        profile_typing_latency(&terminal_manager, session_ids[0], config.typing_samples)?;
    render_stop.store(true, Ordering::Relaxed);
    let _ = render_worker.join();

    println!();
    println!("Scenario: render loop contention (33ms snapshots + orchestrator)");
    render_contended.print();

    let render_stop = Arc::new(AtomicBool::new(false));
    let autosave_stop = Arc::new(AtomicBool::new(false));
    let render_worker = spawn_render_worker(
        terminal_manager.clone(),
        app_state.clone(),
        render_stop.clone(),
    );
    let autosave_worker = spawn_autosave_worker(
        terminal_manager.clone(),
        app_state.clone(),
        autosave_stop.clone(),
    );
    thread::sleep(Duration::from_millis(250));
    let full_contended =
        profile_typing_latency(&terminal_manager, session_ids[0], config.typing_samples)?;
    render_stop.store(true, Ordering::Relaxed);
    autosave_stop.store(true, Ordering::Relaxed);
    let _ = render_worker.join();
    let _ = autosave_worker.join();

    println!();
    println!("Scenario: render + autosave contention");
    full_contended.print();

    let mut assertion_failures = Vec::new();
    let mut assertions_passed = None;
    if assert_mode {
        if render_contended.lock_wait_p95() > ASSERT_LOCK_WAIT_P95_US {
            assertion_failures.push(format!(
                "render contention lock-wait p95 {}us exceeds {}us",
                render_contended.lock_wait_p95(),
                ASSERT_LOCK_WAIT_P95_US
            ));
        }
        if full_contended.lock_wait_p95() > ASSERT_LOCK_WAIT_P95_US {
            assertion_failures.push(format!(
                "render+autosave lock-wait p95 {}us exceeds {}us",
                full_contended.lock_wait_p95(),
                ASSERT_LOCK_WAIT_P95_US
            ));
        }
        if render_contended.total_send_p95() > ASSERT_RENDER_TOTAL_P95_US {
            assertion_failures.push(format!(
                "render contention total-send p95 {}us exceeds {}us",
                render_contended.total_send_p95(),
                ASSERT_RENDER_TOTAL_P95_US
            ));
        }
        if full_contended.total_send_p95() > ASSERT_FULL_TOTAL_P95_US {
            assertion_failures.push(format!(
                "render+autosave total-send p95 {}us exceeds {}us",
                full_contended.total_send_p95(),
                ASSERT_FULL_TOTAL_P95_US
            ));
        }

        assertions_passed = Some(assertion_failures.is_empty());
        if assertion_failures.is_empty() {
            println!();
            println!("Perf assertions passed.");
        } else {
            println!();
            println!("Perf assertions failed:");
            for failure in &assertion_failures {
                println!("  - {failure}");
            }
        }
    }

    let summary = ProfileSummary {
        warmup_history_lines: config.warmup_history_lines,
        typing_samples: config.typing_samples,
        render_pass: render_hold_stats,
        autosave_pass: autosave_hold_stats,
        ui_rows_rendered_per_refresh: ui_rows_rendered_stats,
        ui_row_render_pass: ui_row_render_stats,
        round_bounds_extract: round_bounds_extract_stats,
        orchestrator_round_extract: orchestrator_round_extract_stats,
        refresh_loop_tick: refresh_loop_tick_stats,
        refresh_loop_state_clone: refresh_loop_state_clone_stats,
        resize_measure: resize_measure_stats,
        resize_measure_calls_per_sec: resize_measure_calls_per_sec_stats,
        scroll_observer_callbacks_per_sec: scroll_observer_callbacks_per_sec_stats,
        orchestrator_snapshot_build: orchestrator_snapshot_build_stats,
        autosave_fingerprint: autosave_fingerprint_stats,
        git_watcher_poll_cost: git_watcher_poll_cost_stats,
        startup_active_path_group_ready: startup_active_path_group_ready_stats,
        startup_full_restore: startup_full_restore_stats,
        replay_legacy_snapshot_build: replay_benchmark
            .as_ref()
            .map(|benchmark| benchmark.legacy_snapshot_build),
        replay_legacy_row_render: replay_benchmark
            .as_ref()
            .map(|benchmark| benchmark.legacy_row_render),
        replay_legacy_round_bounds: replay_benchmark
            .as_ref()
            .map(|benchmark| benchmark.legacy_round_bounds),
        replay_native_snapshot_build: replay_benchmark
            .as_ref()
            .map(|benchmark| benchmark.native_snapshot_build),
        replay_native_raster_update: replay_benchmark
            .as_ref()
            .map(|benchmark| benchmark.native_raster_update),
        autosave_snapshot_lines_total: autosave_snapshot_lines_stats,
        baseline_lock_wait: baseline.lock_wait_stats(),
        baseline_total_send: baseline.total_send_stats(),
        render_lock_wait: render_contended.lock_wait_stats(),
        render_total_send: render_contended.total_send_stats(),
        full_lock_wait: full_contended.lock_wait_stats(),
        full_total_send: full_contended.total_send_stats(),
        render_pass_p95_us: render_hold_stats.p95_us,
        autosave_pass_p95_us: autosave_hold_stats.p95_us,
        ui_rows_rendered_per_refresh_p95: ui_rows_rendered_stats.p95_us,
        ui_row_render_pass_p95_us: ui_row_render_stats.p95_us,
        round_bounds_extract_p95_us: round_bounds_extract_stats.p95_us,
        orchestrator_round_extract_p95_us: orchestrator_round_extract_stats.p95_us,
        refresh_loop_tick_p95_us: refresh_loop_tick_stats.p95_us,
        refresh_loop_state_clone_p95_us: refresh_loop_state_clone_stats.p95_us,
        resize_measure_p95_us: resize_measure_stats.p95_us,
        resize_measure_calls_per_sec_p95: resize_measure_calls_per_sec_stats.p95_us,
        scroll_observer_callbacks_per_sec_p95: scroll_observer_callbacks_per_sec_stats.p95_us,
        orchestrator_snapshot_build_p95_us: orchestrator_snapshot_build_stats.p95_us,
        autosave_fingerprint_p95_us: autosave_fingerprint_stats.p95_us,
        git_watcher_poll_cost_p95_us: git_watcher_poll_cost_stats.p95_us,
        startup_active_path_group_ready_p95_us: startup_active_path_group_ready_stats.p95_us,
        startup_full_restore_p95_us: startup_full_restore_stats.p95_us,
        replay_legacy_snapshot_build_p95_us: replay_benchmark
            .as_ref()
            .map(|benchmark| benchmark.legacy_snapshot_build.p95_us),
        replay_legacy_row_render_p95_us: replay_benchmark
            .as_ref()
            .map(|benchmark| benchmark.legacy_row_render.p95_us),
        replay_legacy_round_bounds_p95_us: replay_benchmark
            .as_ref()
            .map(|benchmark| benchmark.legacy_round_bounds.p95_us),
        replay_native_snapshot_build_p95_us: replay_benchmark
            .as_ref()
            .map(|benchmark| benchmark.native_snapshot_build.p95_us),
        replay_native_raster_update_p95_us: replay_benchmark
            .as_ref()
            .map(|benchmark| benchmark.native_raster_update.p95_us),
        autosave_snapshot_lines_total_p95: autosave_snapshot_lines_stats.p95_us,
        autosave_snapshot_build_p95_us: autosave_hold_stats.p95_us,
        baseline_total_send_p95_us: baseline.total_send_p95(),
        render_total_send_p95_us: render_contended.total_send_p95(),
        full_total_send_p95_us: full_contended.total_send_p95(),
        assert_mode,
        assertions_passed,
    };

    if json_mode {
        let payload = serde_json::to_string(&summary)
            .map_err(|error| format!("failed to serialize profile summary: {error}"))?;
        println!("{JSON_PREFIX}{payload}");
    }

    if assert_mode && !assertion_failures.is_empty() {
        return Err("terminal performance assertion failed".to_string());
    }

    Ok(())
}

fn seed_terminal_output(
    terminal_manager: &Arc<TerminalManager>,
    session_ids: &[SessionId],
    warmup_history_lines: usize,
) -> Result<(), String> {
    let fill = "x".repeat(HISTORY_LINE_WIDTH);
    let completion_marker = format!("{warmup_history_lines} {fill}");
    let command = format!("for i in $(seq 1 {warmup_history_lines}); do echo \"$i {fill}\"; done");

    for session_id in session_ids {
        terminal_manager
            .send_line(*session_id, &command)
            .map_err(|error| error.to_string())?;
    }

    let deadline = Instant::now() + WARMUP_READY_TIMEOUT;
    loop {
        let ready_count = session_ids
            .iter()
            .filter(|session_id| {
                terminal_manager
                    .snapshot(**session_id)
                    .map(|snapshot| {
                        snapshot
                            .lines
                            .iter()
                            .any(|line| line.contains(&completion_marker))
                    })
                    .unwrap_or(false)
            })
            .count();

        if ready_count == session_ids.len() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "warmup timed out: {ready_count}/{} sessions reached completion marker within {:?}",
                session_ids.len(),
                WARMUP_READY_TIMEOUT
            ));
        }

        thread::sleep(Duration::from_millis(25));
    }
}

fn spawn_render_worker(
    terminal_manager: Arc<TerminalManager>,
    app_state: Arc<AppState>,
    stop: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let started = Instant::now();
            simulate_render_pass(&app_state, &terminal_manager);

            let spent = started.elapsed();
            if spent < Duration::from_millis(33) {
                thread::sleep(Duration::from_millis(33) - spent);
            }
        }
    })
}

fn spawn_autosave_worker(
    terminal_manager: Arc<TerminalManager>,
    app_state: Arc<AppState>,
    stop: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let tick = Instant::now();
            let _ = persistence::build_workspace_snapshot_with_history_limit(
                &app_state,
                &terminal_manager,
                AUTOSAVE_PERSISTED_HISTORY_LINES,
            );

            let spent = tick.elapsed();
            if spent < Duration::from_millis(1_200) {
                thread::sleep(Duration::from_millis(1_200) - spent);
            }
        }
    })
}

#[derive(Default)]
struct RenderPassProbe {
    ui_rows_rendered: u128,
    ui_row_render_us: u128,
    orchestrator_round_extract_us: u128,
}

#[derive(Default)]
struct RenderProfile {
    hold_times_us: Vec<u128>,
    ui_rows_rendered: Vec<u128>,
    ui_row_render_us: Vec<u128>,
    round_bounds_extract_us: Vec<u128>,
    orchestrator_round_extract_us: Vec<u128>,
}

#[derive(Default)]
struct AutosaveProfile {
    hold_times_us: Vec<u128>,
    snapshot_lines_total: Vec<u128>,
    fingerprint_us: Vec<u128>,
}

#[derive(Default)]
struct RefreshLoopProfile {
    tick_us: Vec<u128>,
    state_clone_us: Vec<u128>,
    resize_measure_us: Vec<u128>,
    resize_measure_calls_per_sec: Vec<u128>,
    scroll_observer_callbacks_per_sec: Vec<u128>,
    orchestrator_snapshot_build_us: Vec<u128>,
}

fn simulate_render_pass(app_state: &AppState, runtime: &TerminalManager) -> RenderPassProbe {
    let mut probe = RenderPassProbe::default();
    let Some(group_id) = app_state.active_group_id() else {
        return probe;
    };

    let (agents, runner) = app_state.workspace_sessions_for_group(group_id);
    let mut sessions = agents;
    if let Some(runner) = runner {
        sessions.push(runner);
    }

    struct RuntimePane {
        session_id: SessionId,
        lines: Vec<String>,
        cwd: String,
        is_runtime_ready: bool,
    }
    let mut runtime_panes = Vec::<RuntimePane>::new();

    for session in &sessions {
        let runtime_snapshot = runtime.snapshot(session.id);
        let is_runtime_ready = runtime_snapshot.is_some();
        let snapshot = runtime_snapshot.unwrap_or_else(empty_terminal_snapshot);
        let row_render_started = Instant::now();
        let rows_rendered = emulate_terminal_row_render_work(snapshot.rows, &snapshot.lines);
        probe.ui_rows_rendered = probe.ui_rows_rendered.saturating_add(rows_rendered as u128);
        probe.ui_row_render_us = probe
            .ui_row_render_us
            .saturating_add(row_render_started.elapsed().as_micros());

        let cwd = runtime.session_cwd(session.id).unwrap_or_else(|| {
            app_state
                .group_path(session.group_id)
                .map(ToString::to_string)
                .unwrap_or_else(|| ".".to_string())
        });

        runtime_panes.push(RuntimePane {
            session_id: session.id,
            lines: snapshot.lines,
            cwd,
            is_runtime_ready,
        });
    }

    let runtime_map = runtime_panes
        .iter()
        .map(|pane| {
            (
                pane.session_id,
                orchestrator::SessionRuntimeView {
                    lines: &pane.lines,
                    cwd: pane.cwd.as_str(),
                    is_runtime_ready: pane.is_runtime_ready,
                },
            )
        })
        .collect::<HashMap<SessionId, orchestrator::SessionRuntimeView>>();
    let orchestrator_started = Instant::now();
    let _ = orchestrator::snapshot_group_from_runtime(
        app_state.workspace_state(),
        group_id,
        None,
        &runtime_map,
    );
    probe.orchestrator_round_extract_us = orchestrator_started.elapsed().as_micros();
    probe
}

fn profile_typing_latency(
    terminal_manager: &Arc<TerminalManager>,
    session_id: SessionId,
    samples: usize,
) -> Result<LatencyProfile, String> {
    let mut lock_waits = Vec::with_capacity(samples);
    let mut totals = Vec::with_capacity(samples);

    for _ in 0..samples {
        let timings = terminal_manager
            .send_input_profiled(session_id, b"a")
            .map_err(|error| error.to_string())?;

        lock_waits.push(timings.lock_wait);
        totals.push(timings.total);
        thread::sleep(TYPING_INTERVAL);
    }

    Ok(LatencyProfile::new(lock_waits, totals))
}

fn profile_render_hold(
    terminal_manager: &Arc<TerminalManager>,
    app_state: &AppState,
    iterations: usize,
) -> RenderProfile {
    let mut profile = RenderProfile {
        hold_times_us: Vec::with_capacity(iterations),
        ui_rows_rendered: Vec::with_capacity(iterations),
        ui_row_render_us: Vec::with_capacity(iterations),
        round_bounds_extract_us: Vec::with_capacity(iterations),
        orchestrator_round_extract_us: Vec::with_capacity(iterations),
    };

    for _ in 0..iterations {
        let render_started = Instant::now();
        let probe = simulate_render_pass(app_state, terminal_manager);
        profile
            .hold_times_us
            .push(render_started.elapsed().as_micros());

        let round_started = Instant::now();
        probe_round_bounds_extract(app_state, terminal_manager);
        let round_bounds_extract_us = round_started.elapsed().as_micros();

        profile.ui_rows_rendered.push(probe.ui_rows_rendered);
        profile.ui_row_render_us.push(probe.ui_row_render_us);
        profile
            .round_bounds_extract_us
            .push(round_bounds_extract_us);
        profile
            .orchestrator_round_extract_us
            .push(probe.orchestrator_round_extract_us);
    }

    profile.hold_times_us.sort_unstable();
    profile.ui_rows_rendered.sort_unstable();
    profile.ui_row_render_us.sort_unstable();
    profile.round_bounds_extract_us.sort_unstable();
    profile.orchestrator_round_extract_us.sort_unstable();
    profile
}

fn profile_autosave_hold(
    terminal_manager: &Arc<TerminalManager>,
    app_state: &AppState,
    iterations: usize,
) -> AutosaveProfile {
    let mut profile = AutosaveProfile {
        hold_times_us: Vec::with_capacity(iterations),
        snapshot_lines_total: Vec::with_capacity(iterations),
        fingerprint_us: Vec::with_capacity(iterations),
    };

    for _ in 0..iterations {
        let started = Instant::now();
        let workspace = persistence::build_workspace_snapshot_with_history_limit(
            app_state,
            terminal_manager,
            AUTOSAVE_PERSISTED_HISTORY_LINES,
        );
        profile.hold_times_us.push(started.elapsed().as_micros());
        // Fingerprint hashing now runs in the autosave worker thread, not the UI hold path.
        profile.fingerprint_us.push(0);
        let line_total = workspace
            .terminals
            .iter()
            .map(|terminal| terminal.lines.len() as u128)
            .sum();
        profile.snapshot_lines_total.push(line_total);
    }

    profile.hold_times_us.sort_unstable();
    profile.snapshot_lines_total.sort_unstable();
    profile.fingerprint_us.sort_unstable();
    profile
}

fn profile_refresh_loop(
    terminal_manager: &Arc<TerminalManager>,
    app_state: &AppState,
    iterations: usize,
) -> RefreshLoopProfile {
    let mut profile = RefreshLoopProfile {
        tick_us: Vec::with_capacity(iterations),
        state_clone_us: Vec::with_capacity(iterations),
        resize_measure_us: Vec::with_capacity(iterations),
        resize_measure_calls_per_sec: Vec::with_capacity(iterations),
        scroll_observer_callbacks_per_sec: Vec::with_capacity(iterations),
        orchestrator_snapshot_build_us: Vec::with_capacity(iterations),
    };
    let mut last_revisions = Vec::<(SessionId, u64)>::new();
    let mut last_sizes: HashMap<SessionId, (u16, u16)> = HashMap::new();
    let mut resize_dirty = HashMap::<SessionId, bool>::new();

    for _ in 0..iterations {
        let tick_started = Instant::now();

        let mut resize_measure_elapsed = 0_u128;
        let mut resize_calls = 0_u128;
        let mut scroll_callbacks_est = 0_u128;
        let mut orchestrator_snapshot_elapsed = 0_u128;
        let clone_elapsed = 0_u128;

        if let Some(group_id) = app_state.active_group_id() {
            let mut revisions = app_state
                .session_ids_in_group(group_id)
                .into_iter()
                .map(|session_id| {
                    (
                        session_id,
                        terminal_manager
                            .session_snapshot_revision(session_id)
                            .unwrap_or(0),
                    )
                })
                .collect::<Vec<_>>();
            revisions.sort_unstable_by_key(|(session_id, _)| *session_id);
            if revisions != last_revisions {
                last_revisions = revisions;
            }

            let active_session_ids = app_state.workspace_session_ids_for_group(group_id);
            let active_session_set = active_session_ids
                .iter()
                .copied()
                .collect::<std::collections::HashSet<_>>();
            last_sizes.retain(|session_id, _| active_session_set.contains(session_id));
            resize_dirty.retain(|session_id, _| active_session_set.contains(session_id));

            let resize_started = Instant::now();
            for session_id in &active_session_ids {
                let should_measure = resize_dirty.entry(*session_id).or_insert(true);
                if !*should_measure {
                    continue;
                }

                let body_id = format!("terminal-body-{session_id}");
                let fallback = empty_terminal_snapshot();
                let snapshot = terminal_manager
                    .snapshot_shared(*session_id)
                    .unwrap_or_else(|| Arc::new(fallback));
                let (rows, cols) =
                    emulate_resize_measure_work(&body_id, snapshot.rows, snapshot.cols);
                resize_calls = resize_calls.saturating_add(1);
                *should_measure = false;

                if last_sizes.get(session_id).copied() != Some((rows, cols)) {
                    last_sizes.insert(*session_id, (rows, cols));
                }
            }
            resize_measure_elapsed = resize_started.elapsed().as_micros();
            let resize_hz = (1_000_u128 / u128::from(RESIZE_LOOP_INTERVAL_MS.max(1))).max(1);
            let refresh_hz = (1_000_u128 / u128::from(REFRESH_LOOP_INTERVAL_MS.max(1))).max(1);
            profile
                .resize_measure_calls_per_sec
                .push(resize_calls.saturating_mul(resize_hz));
            let coalesced_callbacks_per_session_hz = (refresh_hz / 2).max(1);
            scroll_callbacks_est = u128::from(active_session_ids.len() as u64)
                .saturating_mul(coalesced_callbacks_per_session_hz);

            let orchestrator_started = Instant::now();
            probe_orchestrator_snapshot_build(app_state, group_id, terminal_manager);
            orchestrator_snapshot_elapsed = orchestrator_started.elapsed().as_micros();
        } else {
            profile.resize_measure_calls_per_sec.push(0);
        }

        profile.tick_us.push(tick_started.elapsed().as_micros());
        profile.state_clone_us.push(clone_elapsed);
        profile.resize_measure_us.push(resize_measure_elapsed);
        profile
            .scroll_observer_callbacks_per_sec
            .push(scroll_callbacks_est);
        profile
            .orchestrator_snapshot_build_us
            .push(orchestrator_snapshot_elapsed);
    }

    profile.tick_us.sort_unstable();
    profile.state_clone_us.sort_unstable();
    profile.resize_measure_us.sort_unstable();
    profile.resize_measure_calls_per_sec.sort_unstable();
    profile.scroll_observer_callbacks_per_sec.sort_unstable();
    profile.orchestrator_snapshot_build_us.sort_unstable();
    profile
}

struct StartupProfile {
    active_path_group_visible_sessions: usize,
    total_sessions: usize,
    active_path_group_ready_us: Vec<u128>,
    full_restore_us: Vec<u128>,
}

fn profile_startup_restore(samples: usize) -> Result<StartupProfile, String> {
    let state = build_startup_profile_state();
    let active_group_id = state
        .active_group_id()
        .ok_or_else(|| "missing active group for startup profile".to_string())?;
    let active_group_path = state
        .group_path(active_group_id)
        .ok_or_else(|| "missing active group path for startup profile".to_string())?
        .to_string();
    let active_group_session_ids = state.workspace_session_ids_for_group(active_group_id);
    let all_sessions = state
        .sessions()
        .iter()
        .map(|session| {
            (
                session.id,
                state
                    .group_path(session.group_id)
                    .unwrap_or(".")
                    .to_string(),
            )
        })
        .collect::<Vec<_>>();

    let mut active_path_group_ready_us = Vec::with_capacity(samples);
    let mut full_restore_us = Vec::with_capacity(samples);

    for _ in 0..samples {
        let terminal_manager = TerminalManager::new();
        let started = Instant::now();
        for session_id in &active_group_session_ids {
            terminal_manager
                .ensure_session(*session_id, &active_group_path)
                .map_err(|error| error.to_string())?;
        }
        active_path_group_ready_us.push(started.elapsed().as_micros());
        drop(terminal_manager);

        let terminal_manager = TerminalManager::new();
        let started = Instant::now();
        for (session_id, path) in &all_sessions {
            terminal_manager
                .ensure_session(*session_id, path)
                .map_err(|error| error.to_string())?;
        }
        full_restore_us.push(started.elapsed().as_micros());
        drop(terminal_manager);
    }

    active_path_group_ready_us.sort_unstable();
    full_restore_us.sort_unstable();

    Ok(StartupProfile {
        active_path_group_visible_sessions: active_group_session_ids.len(),
        total_sessions: all_sessions.len(),
        active_path_group_ready_us,
        full_restore_us,
    })
}

fn build_startup_profile_state() -> AppState {
    let mut state = AppState::default();
    let active_group_id = state.active_group_id().expect("default active group");
    for _ in 0..STARTUP_PROFILE_ACTIVE_GROUP_EXTRA_SESSIONS {
        let _ = state.add_session(active_group_id);
    }
    for index in 0..STARTUP_PROFILE_EXTRA_GROUPS {
        let path = format!("/tmp/gestalt-startup-profile-{index}");
        let _ = state.create_group_with_defaults(path);
    }
    let first_session_id = state
        .sessions()
        .iter()
        .find(|session| session.group_id == active_group_id)
        .map(|session| session.id)
        .expect("active group session");
    state.select_session(first_session_id);
    state
}

fn profile_git_watcher_poll_cost(group_path: &str, samples: usize) -> Vec<u128> {
    let mut samples_us = Vec::with_capacity(samples);
    let Ok(repo_root) = gestalt::git::repo_root(group_path) else {
        return samples_us;
    };
    for _ in 0..samples {
        let started = Instant::now();
        let _ = gestalt::git::repo_change_fingerprint_from_root(&repo_root);
        samples_us.push(started.elapsed().as_micros());
    }
    samples_us.sort_unstable();
    samples_us
}

fn probe_orchestrator_snapshot_build(
    app_state: &AppState,
    group_id: u32,
    terminal_manager: &TerminalManager,
) {
    struct RuntimePane {
        session_id: SessionId,
        snapshot: Arc<TerminalSnapshot>,
        cwd: String,
        is_runtime_ready: bool,
    }

    let (agents, runner) = app_state.workspace_sessions_for_group(group_id);
    let mut sessions = agents;
    if let Some(runner) = runner {
        sessions.push(runner);
    }

    let mut runtime_panes = Vec::with_capacity(sessions.len());
    for session in &sessions {
        let runtime_snapshot = terminal_manager.snapshot_shared(session.id);
        let is_runtime_ready = runtime_snapshot.is_some();
        let snapshot = runtime_snapshot.unwrap_or_else(|| Arc::new(empty_terminal_snapshot()));
        let cwd = terminal_manager.session_cwd(session.id).unwrap_or_else(|| {
            app_state
                .group_path(session.group_id)
                .map(ToString::to_string)
                .unwrap_or_else(|| ".".to_string())
        });
        runtime_panes.push(RuntimePane {
            session_id: session.id,
            snapshot,
            cwd,
            is_runtime_ready,
        });
    }

    let runtime_map = runtime_panes
        .iter()
        .map(|pane| {
            (
                pane.session_id,
                orchestrator::SessionRuntimeView {
                    lines: &pane.snapshot.lines,
                    cwd: pane.cwd.as_str(),
                    is_runtime_ready: pane.is_runtime_ready,
                },
            )
        })
        .collect::<HashMap<SessionId, orchestrator::SessionRuntimeView>>();
    let _ = orchestrator::snapshot_group_from_runtime(
        app_state.workspace_state(),
        group_id,
        None,
        &runtime_map,
    );
}

fn emulate_resize_measure_work(terminal_body_id: &str, rows: u16, cols: u16) -> (u16, u16) {
    let checksum = terminal_body_id
        .bytes()
        .fold(0_u64, |acc, byte| acc.wrapping_add(u64::from(byte)));
    black_box(checksum);
    (rows.max(2), cols.max(8))
}

fn empty_terminal_snapshot() -> TerminalSnapshot {
    TerminalSnapshot {
        lines: vec![String::new()],
        rows: 42,
        cols: 140,
        cursor_row: 0,
        cursor_col: 0,
        hide_cursor: false,
        bracketed_paste: false,
    }
}

fn emulate_terminal_row_render_work(rows: u16, lines: &[String]) -> usize {
    let render_window_rows = usize::from(rows)
        .saturating_mul(RENDER_WINDOW_MULTIPLIER)
        .max(RENDER_WINDOW_MIN_ROWS);
    let window_start = lines.len().saturating_sub(render_window_rows);
    let rendered = &lines[window_start..];

    for line in rendered {
        let _ = split_prompt_prefix(line);
        let _ = char_index_to_byte(line, 0);
    }

    rendered.len()
}

fn probe_round_bounds_extract(app_state: &AppState, runtime: &TerminalManager) {
    let Some(group_id) = app_state.active_group_id() else {
        return;
    };
    let (agents, runner) = app_state.workspace_sessions_for_group(group_id);
    let mut sessions = agents;
    if let Some(runner) = runner {
        sessions.push(runner);
    }

    for session in &sessions {
        if let Some(snapshot) = runtime.snapshot(session.id) {
            let _ = terminal_round_bounds(&snapshot.lines, snapshot.cursor_row);
        }
    }
}

fn terminal_round_bounds(lines: &[String], cursor_row: u16) -> Option<(u16, u16)> {
    if lines.is_empty() {
        return None;
    }

    let cursor_idx = usize::from(cursor_row).min(lines.len().saturating_sub(1));

    let start_idx = (0..=cursor_idx)
        .rev()
        .find(|idx| {
            split_prompt_prefix(
                lines
                    .get(*idx)
                    .map(|line| line.as_str())
                    .unwrap_or_default(),
            )
            .is_some()
        })
        .unwrap_or(0);

    let next_prompt_idx = (start_idx + 1..lines.len()).find(|idx| {
        split_prompt_prefix(
            lines
                .get(*idx)
                .map(|line| line.as_str())
                .unwrap_or_default(),
        )
        .is_some()
    });

    let last_non_empty = lines
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .unwrap_or(cursor_idx);

    let mut end_idx = next_prompt_idx
        .map(|idx| idx.saturating_sub(1))
        .unwrap_or(last_non_empty.max(cursor_idx));
    if end_idx < start_idx {
        end_idx = start_idx;
    }

    let start = u16::try_from(start_idx).ok()?;
    let end = u16::try_from(end_idx).ok()?;
    Some((start, end))
}

fn char_index_to_byte(input: &str, char_index: usize) -> usize {
    if char_index == 0 {
        return 0;
    }

    input
        .char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or(input.len())
}

fn split_prompt_prefix(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return None;
    }

    let leading = line.len().saturating_sub(trimmed.len());

    if trimmed.starts_with("$ ") || trimmed.starts_with("# ") {
        let end = leading + 2;
        return Some((&line[..end], &line[end..]));
    }

    if trimmed == "$" || trimmed == "#" {
        return Some((line, ""));
    }

    if (trimmed.ends_with('$') || trimmed.ends_with('#'))
        && (trimmed.contains('@') || trimmed.contains(':'))
    {
        return Some((line, ""));
    }

    let marker = trimmed.find("$ ").or_else(|| trimmed.find("# "))?;
    let end = leading + marker + 2;
    let prefix = &line[..end];
    if !prefix.contains('@') || !prefix.contains(':') {
        return None;
    }

    Some((prefix, &line[end..]))
}

#[derive(Clone, Copy, Serialize)]
struct DistributionStats {
    avg_us: u128,
    p50_us: u128,
    p95_us: u128,
    p99_us: u128,
    max_us: u128,
}

impl DistributionStats {
    fn from_sorted(sorted_values: &[u128]) -> Self {
        Self {
            avg_us: average(sorted_values),
            p50_us: percentile(sorted_values, 50),
            p95_us: percentile(sorted_values, 95),
            p99_us: percentile(sorted_values, 99),
            max_us: sorted_values.last().copied().unwrap_or_default(),
        }
    }
}

#[derive(Serialize)]
struct ProfileSummary {
    warmup_history_lines: usize,
    typing_samples: usize,
    render_pass: DistributionStats,
    autosave_pass: DistributionStats,
    ui_rows_rendered_per_refresh: DistributionStats,
    ui_row_render_pass: DistributionStats,
    round_bounds_extract: DistributionStats,
    orchestrator_round_extract: DistributionStats,
    refresh_loop_tick: DistributionStats,
    refresh_loop_state_clone: DistributionStats,
    resize_measure: DistributionStats,
    resize_measure_calls_per_sec: DistributionStats,
    scroll_observer_callbacks_per_sec: DistributionStats,
    orchestrator_snapshot_build: DistributionStats,
    autosave_fingerprint: DistributionStats,
    git_watcher_poll_cost: DistributionStats,
    startup_active_path_group_ready: DistributionStats,
    startup_full_restore: DistributionStats,
    replay_legacy_snapshot_build: Option<DistributionStats>,
    replay_legacy_row_render: Option<DistributionStats>,
    replay_legacy_round_bounds: Option<DistributionStats>,
    replay_native_snapshot_build: Option<DistributionStats>,
    replay_native_raster_update: Option<DistributionStats>,
    autosave_snapshot_lines_total: DistributionStats,
    baseline_lock_wait: DistributionStats,
    baseline_total_send: DistributionStats,
    render_lock_wait: DistributionStats,
    render_total_send: DistributionStats,
    full_lock_wait: DistributionStats,
    full_total_send: DistributionStats,
    render_pass_p95_us: u128,
    autosave_pass_p95_us: u128,
    ui_rows_rendered_per_refresh_p95: u128,
    ui_row_render_pass_p95_us: u128,
    round_bounds_extract_p95_us: u128,
    orchestrator_round_extract_p95_us: u128,
    refresh_loop_tick_p95_us: u128,
    refresh_loop_state_clone_p95_us: u128,
    resize_measure_p95_us: u128,
    resize_measure_calls_per_sec_p95: u128,
    scroll_observer_callbacks_per_sec_p95: u128,
    orchestrator_snapshot_build_p95_us: u128,
    autosave_fingerprint_p95_us: u128,
    git_watcher_poll_cost_p95_us: u128,
    startup_active_path_group_ready_p95_us: u128,
    startup_full_restore_p95_us: u128,
    replay_legacy_snapshot_build_p95_us: Option<u128>,
    replay_legacy_row_render_p95_us: Option<u128>,
    replay_legacy_round_bounds_p95_us: Option<u128>,
    replay_native_snapshot_build_p95_us: Option<u128>,
    replay_native_raster_update_p95_us: Option<u128>,
    autosave_snapshot_lines_total_p95: u128,
    autosave_snapshot_build_p95_us: u128,
    baseline_total_send_p95_us: u128,
    render_total_send_p95_us: u128,
    full_total_send_p95_us: u128,
    assert_mode: bool,
    assertions_passed: Option<bool>,
}

#[derive(Serialize)]
struct ReplayOnlyProfileSummary {
    replay_profile_lines: usize,
    replay_profile_iterations: usize,
    replay_legacy_snapshot_build: Option<DistributionStats>,
    replay_legacy_row_render: Option<DistributionStats>,
    replay_legacy_round_bounds: Option<DistributionStats>,
    replay_native_snapshot_build: Option<DistributionStats>,
    replay_native_raster_update: Option<DistributionStats>,
}

struct LatencyProfile {
    lock_wait_micros: Vec<u128>,
    total_micros: Vec<u128>,
}

impl LatencyProfile {
    fn new(lock_waits: Vec<Duration>, totals: Vec<Duration>) -> Self {
        Self {
            lock_wait_micros: to_micros_sorted(lock_waits),
            total_micros: to_micros_sorted(totals),
        }
    }

    fn print(&self) {
        let lock_wait_stats = self.lock_wait_stats();
        let total_send_stats = self.total_send_stats();

        println!(
            "  lock wait us: avg={} p50={} p95={} p99={} max={}",
            lock_wait_stats.avg_us,
            lock_wait_stats.p50_us,
            lock_wait_stats.p95_us,
            lock_wait_stats.p99_us,
            lock_wait_stats.max_us
        );
        println!(
            "  total send us: avg={} p50={} p95={} p99={} max={}",
            total_send_stats.avg_us,
            total_send_stats.p50_us,
            total_send_stats.p95_us,
            total_send_stats.p99_us,
            total_send_stats.max_us
        );
    }

    fn lock_wait_stats(&self) -> DistributionStats {
        DistributionStats::from_sorted(&self.lock_wait_micros)
    }

    fn total_send_stats(&self) -> DistributionStats {
        DistributionStats::from_sorted(&self.total_micros)
    }

    fn lock_wait_p95(&self) -> u128 {
        percentile(&self.lock_wait_micros, 95)
    }

    fn total_send_p95(&self) -> u128 {
        percentile(&self.total_micros, 95)
    }
}

fn stats_from_sorted(values: &[u128]) -> DistributionStats {
    DistributionStats::from_sorted(values)
}

#[derive(Clone, Copy, Serialize)]
struct ReplayBenchmarkSummary {
    legacy_snapshot_build: DistributionStats,
    legacy_row_render: DistributionStats,
    legacy_round_bounds: DistributionStats,
    native_snapshot_build: DistributionStats,
    native_raster_update: DistributionStats,
}

#[cfg(feature = "terminal-native-spike")]
fn profile_terminal_replay_benchmark(config: &ProfileConfig) -> Option<ReplayBenchmarkSummary> {
    let transcript = build_replay_transcript(config.replay_profile_lines);
    let chunks = transcript
        .chunks(REPLAY_PROFILE_CHUNK_BYTES)
        .map(|chunk| chunk.to_vec())
        .collect::<Vec<_>>();

    let mut legacy_snapshot_build =
        Vec::with_capacity(config.replay_profile_iterations * chunks.len());
    let mut legacy_row_render = Vec::with_capacity(config.replay_profile_iterations * chunks.len());
    let mut legacy_round_bounds =
        Vec::with_capacity(config.replay_profile_iterations * chunks.len());
    let mut native_snapshot_build =
        Vec::with_capacity(config.replay_profile_iterations * chunks.len());
    let mut native_raster_update =
        Vec::with_capacity(config.replay_profile_iterations * chunks.len());

    for _ in 0..config.replay_profile_iterations {
        let mut legacy_parser = Parser::new(
            REPLAY_PROFILE_ROWS,
            REPLAY_PROFILE_COLS,
            REPLAY_PROFILE_SCROLLBACK,
        );
        let mut legacy_scrollback = BenchScrollback::default();
        let mut native_emulator = AlacrittyEmulator::new(AlacrittyEmulatorConfig {
            rows: REPLAY_PROFILE_ROWS,
            cols: REPLAY_PROFILE_COLS,
            scrollback: REPLAY_PROFILE_SCROLLBACK,
        });
        let mut scene_cache = TerminalGpuSceneCache::new();

        for chunk in &chunks {
            legacy_parser.process(chunk);
            legacy_scrollback.process_bytes(chunk);

            let legacy_started = Instant::now();
            let legacy_snapshot =
                benchmark_terminal_snapshot_from_parser(&legacy_parser, &legacy_scrollback.lines);
            legacy_snapshot_build.push(legacy_started.elapsed().as_micros());

            let legacy_render_started = Instant::now();
            let _ = emulate_terminal_row_render_work(legacy_snapshot.rows, &legacy_snapshot.lines);
            legacy_row_render.push(legacy_render_started.elapsed().as_micros());

            let legacy_round_started = Instant::now();
            let _ = terminal_round_bounds(&legacy_snapshot.lines, legacy_snapshot.cursor_row);
            legacy_round_bounds.push(legacy_round_started.elapsed().as_micros());

            let native_snapshot_started = Instant::now();
            native_emulator.ingest(chunk);
            let frame = native_emulator.snapshot();
            native_snapshot_build.push(native_snapshot_started.elapsed().as_micros());

            let native_raster_started = Instant::now();
            let _ = scene_cache.prepare(
                &frame,
                u32::from(frame.cols).saturating_mul(9),
                u32::from(frame.rows).saturating_mul(18),
            );
            native_raster_update.push(native_raster_started.elapsed().as_micros());
        }
    }

    legacy_snapshot_build.sort_unstable();
    legacy_row_render.sort_unstable();
    legacy_round_bounds.sort_unstable();
    native_snapshot_build.sort_unstable();
    native_raster_update.sort_unstable();

    Some(ReplayBenchmarkSummary {
        legacy_snapshot_build: stats_from_sorted(&legacy_snapshot_build),
        legacy_row_render: stats_from_sorted(&legacy_row_render),
        legacy_round_bounds: stats_from_sorted(&legacy_round_bounds),
        native_snapshot_build: stats_from_sorted(&native_snapshot_build),
        native_raster_update: stats_from_sorted(&native_raster_update),
    })
}

#[cfg(not(feature = "terminal-native-spike"))]
fn profile_terminal_replay_benchmark(_config: &ProfileConfig) -> Option<ReplayBenchmarkSummary> {
    None
}

fn print_replay_benchmark(summary: &Option<ReplayBenchmarkSummary>) {
    println!();
    println!("Replay benchmark: legacy vs native terminal path");
    let Some(summary) = summary else {
        println!("  skipped: build without terminal-native-spike feature");
        return;
    };

    println!(
        "  legacy snapshot build us: avg={} p50={} p95={} p99={} max={}",
        summary.legacy_snapshot_build.avg_us,
        summary.legacy_snapshot_build.p50_us,
        summary.legacy_snapshot_build.p95_us,
        summary.legacy_snapshot_build.p99_us,
        summary.legacy_snapshot_build.max_us
    );
    println!(
        "  legacy row render us: avg={} p50={} p95={} p99={} max={}",
        summary.legacy_row_render.avg_us,
        summary.legacy_row_render.p50_us,
        summary.legacy_row_render.p95_us,
        summary.legacy_row_render.p99_us,
        summary.legacy_row_render.max_us
    );
    println!(
        "  legacy round bounds us: avg={} p50={} p95={} p99={} max={}",
        summary.legacy_round_bounds.avg_us,
        summary.legacy_round_bounds.p50_us,
        summary.legacy_round_bounds.p95_us,
        summary.legacy_round_bounds.p99_us,
        summary.legacy_round_bounds.max_us
    );
    println!(
        "  native snapshot build us: avg={} p50={} p95={} p99={} max={}",
        summary.native_snapshot_build.avg_us,
        summary.native_snapshot_build.p50_us,
        summary.native_snapshot_build.p95_us,
        summary.native_snapshot_build.p99_us,
        summary.native_snapshot_build.max_us
    );
    println!(
        "  native render prepare us: avg={} p50={} p95={} p99={} max={}",
        summary.native_raster_update.avg_us,
        summary.native_raster_update.p50_us,
        summary.native_raster_update.p95_us,
        summary.native_raster_update.p99_us,
        summary.native_raster_update.max_us
    );
}

#[cfg(feature = "terminal-native-spike")]
fn build_replay_transcript(line_count: usize) -> Vec<u8> {
    let mut transcript = Vec::with_capacity(line_count * (HISTORY_LINE_WIDTH + 48));
    let payload = "x".repeat(HISTORY_LINE_WIDTH);

    for index in 0..line_count {
        transcript.extend_from_slice(
            format!(
                "\x1b[32mjeremy@gestalt:/repo$ \x1b[0mecho block-{index}\r\n\
                 \x1b[36m{index:04}\x1b[0m {payload}\r\n"
            )
            .as_bytes(),
        );
    }

    transcript
}

#[derive(Default)]
#[cfg(feature = "terminal-native-spike")]
struct BenchScrollback {
    lines: Vec<String>,
    pending: Vec<u8>,
    escape_state: BenchEscapeState,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
#[cfg(feature = "terminal-native-spike")]
enum BenchEscapeState {
    #[default]
    Normal,
    Esc,
    Csi,
    Osc,
    OscEsc,
}

#[cfg(feature = "terminal-native-spike")]
impl BenchScrollback {
    fn process_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            match self.escape_state {
                BenchEscapeState::Normal => match byte {
                    0x1b => self.escape_state = BenchEscapeState::Esc,
                    b'\n' => {
                        let line = String::from_utf8_lossy(&self.pending)
                            .trim_end_matches('\r')
                            .to_string();
                        self.pending.clear();
                        self.lines.push(line);
                    }
                    b'\r' => {}
                    0x08 => {
                        let _ = self.pending.pop();
                    }
                    value if value >= 0x20 || value == b'\t' => self.pending.push(value),
                    _ => {}
                },
                BenchEscapeState::Esc => match byte {
                    b'[' => self.escape_state = BenchEscapeState::Csi,
                    b']' => self.escape_state = BenchEscapeState::Osc,
                    _ => self.escape_state = BenchEscapeState::Normal,
                },
                BenchEscapeState::Csi => {
                    if (0x40..=0x7e).contains(&byte) {
                        self.escape_state = BenchEscapeState::Normal;
                    }
                }
                BenchEscapeState::Osc => {
                    if byte == 0x07 {
                        self.escape_state = BenchEscapeState::Normal;
                    } else if byte == 0x1b {
                        self.escape_state = BenchEscapeState::OscEsc;
                    }
                }
                BenchEscapeState::OscEsc => {
                    self.escape_state = if byte == b'\\' {
                        BenchEscapeState::Normal
                    } else {
                        BenchEscapeState::Osc
                    };
                }
            }
        }
    }
}

#[cfg(feature = "terminal-native-spike")]
fn benchmark_terminal_snapshot_from_parser(
    parser: &Parser,
    scrollback_lines: &[String],
) -> TerminalSnapshot {
    let screen = parser.screen();
    let (rows, cols) = screen.size();
    let (cursor_row_rel, cursor_col) = screen.cursor_position();
    let visible_lines = screen.rows(0, cols).collect::<Vec<_>>();
    let lines = benchmark_merge_scrollback_with_visible(scrollback_lines, &visible_lines);
    let visible_start = lines.len().saturating_sub(visible_lines.len());
    let cursor_row = visible_start
        .saturating_add(usize::from(cursor_row_rel))
        .min(lines.len().saturating_sub(1));

    TerminalSnapshot {
        lines,
        rows,
        cols,
        cursor_row: u16::try_from(cursor_row).unwrap_or(u16::MAX),
        cursor_col,
        hide_cursor: screen.hide_cursor(),
        bracketed_paste: screen.bracketed_paste(),
    }
}

#[cfg(feature = "terminal-native-spike")]
fn benchmark_merge_scrollback_with_visible(
    scrollback: &[String],
    visible: &[String],
) -> Vec<String> {
    let max_overlap = scrollback.len().min(visible.len());
    let overlap = (0..=max_overlap)
        .rev()
        .find(|overlap_len| {
            scrollback[scrollback.len().saturating_sub(*overlap_len)..] == visible[..*overlap_len]
        })
        .unwrap_or(0);

    let keep = scrollback.len().saturating_sub(overlap);
    let mut lines = Vec::with_capacity(keep + visible.len());
    lines.extend(scrollback.iter().take(keep).cloned());
    lines.extend(visible.iter().cloned());
    lines
}

fn to_micros_sorted(values: Vec<Duration>) -> Vec<u128> {
    let mut micros = values
        .into_iter()
        .map(|duration| duration.as_micros())
        .collect::<Vec<_>>();
    micros.sort_unstable();
    micros
}

fn average(sorted_values: &[u128]) -> u128 {
    if sorted_values.is_empty() {
        return 0;
    }
    let sum: u128 = sorted_values.iter().copied().sum();
    sum / (sorted_values.len() as u128)
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

#[cfg(feature = "terminal-native-spike")]
fn replay_profile_lines_default() -> usize {
    REPLAY_PROFILE_LINES
}

#[cfg(not(feature = "terminal-native-spike"))]
fn replay_profile_lines_default() -> usize {
    1
}

#[cfg(feature = "terminal-native-spike")]
fn replay_profile_iterations_default() -> usize {
    REPLAY_PROFILE_ITERATIONS
}

#[cfg(not(feature = "terminal-native-spike"))]
fn replay_profile_iterations_default() -> usize {
    1
}

fn percentile(sorted_values: &[u128], percentile: usize) -> u128 {
    if sorted_values.is_empty() {
        return 0;
    }

    let clamped = percentile.min(100);
    let idx = ((sorted_values.len().saturating_sub(1)) * clamped) / 100;
    sorted_values[idx]
}
