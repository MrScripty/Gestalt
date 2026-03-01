use gestalt::orchestrator;
use gestalt::persistence;
use gestalt::state::{AppState, SessionId};
use gestalt::terminal::{TerminalManager, TerminalSnapshot};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const WARMUP_HISTORY_LINES: usize = 12_000;
const HISTORY_LINE_WIDTH: usize = 96;
const WARMUP_READY_TIMEOUT: Duration = Duration::from_secs(30);
const TYPING_SAMPLES: usize = 320;
const TYPING_INTERVAL: Duration = Duration::from_millis(8);
const ASSERT_LOCK_WAIT_P95_US: u128 = 200;
const ASSERT_RENDER_TOTAL_P95_US: u128 = 1_000;
const ASSERT_FULL_TOTAL_P95_US: u128 = 1_500;
const JSON_PREFIX: &str = "GESTALT_PROFILE_JSON:";
const RENDER_WINDOW_MULTIPLIER: usize = 12;
const RENDER_WINDOW_MIN_ROWS: usize = 512;

fn main() -> Result<(), String> {
    let args = std::env::args().collect::<Vec<_>>();
    let assert_mode = args.iter().any(|arg| arg == "--assert");
    let json_mode = args.iter().any(|arg| arg == "--json");

    let app_state = Arc::new(AppState::default());
    let session_ids = app_state
        .sessions
        .iter()
        .map(|session| session.id)
        .collect::<Vec<_>>();
    let group_id = app_state
        .active_group_id()
        .ok_or_else(|| "missing active group".to_string())?;
    let cwd = app_state.group_path(group_id).unwrap_or(".").to_string();

    let terminal_manager = Arc::new(TerminalManager::new());

    for session_id in &session_ids {
        terminal_manager.ensure_session(*session_id, &cwd)?;
    }

    seed_terminal_output(&terminal_manager, &session_ids)?;

    println!(
        "Profiling keypress latency with {} samples...",
        TYPING_SAMPLES
    );
    println!("Warm terminal history lines: {}", WARMUP_HISTORY_LINES);

    let render_profile = profile_render_hold(&terminal_manager, &app_state, 180);
    let autosave_profile = profile_autosave_hold(&terminal_manager, &app_state, 36);
    let render_hold_stats = stats_from_sorted(&render_profile.hold_times_us);
    let autosave_hold_stats = stats_from_sorted(&autosave_profile.hold_times_us);
    let ui_rows_rendered_stats = stats_from_sorted(&render_profile.ui_rows_rendered);
    let ui_row_render_stats = stats_from_sorted(&render_profile.ui_row_render_us);
    let round_bounds_extract_stats = stats_from_sorted(&render_profile.round_bounds_extract_us);
    let orchestrator_round_extract_stats =
        stats_from_sorted(&render_profile.orchestrator_round_extract_us);
    let autosave_snapshot_lines_stats = stats_from_sorted(&autosave_profile.snapshot_lines_total);

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

    let baseline = profile_typing_latency(&terminal_manager, session_ids[0], TYPING_SAMPLES)?;
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
        profile_typing_latency(&terminal_manager, session_ids[0], TYPING_SAMPLES)?;
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
    let full_contended = profile_typing_latency(&terminal_manager, session_ids[0], TYPING_SAMPLES)?;
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
        warmup_history_lines: WARMUP_HISTORY_LINES,
        typing_samples: TYPING_SAMPLES,
        render_pass: render_hold_stats,
        autosave_pass: autosave_hold_stats,
        ui_rows_rendered_per_refresh: ui_rows_rendered_stats,
        ui_row_render_pass: ui_row_render_stats,
        round_bounds_extract: round_bounds_extract_stats,
        orchestrator_round_extract: orchestrator_round_extract_stats,
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
) -> Result<(), String> {
    let fill = "x".repeat(HISTORY_LINE_WIDTH);
    let completion_marker = format!("{WARMUP_HISTORY_LINES} {fill}");
    let command = format!("for i in $(seq 1 {WARMUP_HISTORY_LINES}); do echo \"$i {fill}\"; done");

    for session_id in session_ids {
        terminal_manager.send_line(*session_id, &command)?;
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
            let _ = persistence::build_workspace_snapshot(&app_state, &terminal_manager);

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
    let _ = orchestrator::snapshot_group_from_runtime(app_state, group_id, None, &runtime_map);
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
        let timings = terminal_manager.send_input_profiled(session_id, b"a")?;

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
    };

    for _ in 0..iterations {
        let started = Instant::now();
        let workspace = persistence::build_workspace_snapshot(app_state, terminal_manager);
        profile.hold_times_us.push(started.elapsed().as_micros());
        let line_total = workspace
            .terminals
            .iter()
            .map(|terminal| terminal.lines.len() as u128)
            .sum();
        profile.snapshot_lines_total.push(line_total);
    }

    profile.hold_times_us.sort_unstable();
    profile.snapshot_lines_total.sort_unstable();
    profile
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
    autosave_snapshot_lines_total_p95: u128,
    autosave_snapshot_build_p95_us: u128,
    baseline_total_send_p95_us: u128,
    render_total_send_p95_us: u128,
    full_total_send_p95_us: u128,
    assert_mode: bool,
    assertions_passed: Option<bool>,
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

fn percentile(sorted_values: &[u128], percentile: usize) -> u128 {
    if sorted_values.is_empty() {
        return 0;
    }

    let clamped = percentile.min(100);
    let idx = ((sorted_values.len().saturating_sub(1)) * clamped) / 100;
    sorted_values[idx]
}
