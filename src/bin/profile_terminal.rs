use gestalt::orchestrator;
use gestalt::persistence;
use gestalt::state::{AppState, SessionId};
use gestalt::terminal::TerminalManager;
use serde::Serialize;
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

    let render_hold = profile_render_hold(&terminal_manager, &app_state, 180);
    let autosave_hold = profile_autosave_hold(&terminal_manager, &app_state, 36);
    let render_hold_stats = stats_from_sorted(&render_hold);
    let autosave_hold_stats = stats_from_sorted(&autosave_hold);

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
        baseline_lock_wait: baseline.lock_wait_stats(),
        baseline_total_send: baseline.total_send_stats(),
        render_lock_wait: render_contended.lock_wait_stats(),
        render_total_send: render_contended.total_send_stats(),
        full_lock_wait: full_contended.lock_wait_stats(),
        full_total_send: full_contended.total_send_stats(),
        render_pass_p95_us: render_hold_stats.p95_us,
        autosave_pass_p95_us: autosave_hold_stats.p95_us,
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

fn simulate_render_pass(app_state: &AppState, runtime: &TerminalManager) {
    let Some(group_id) = app_state.active_group_id() else {
        return;
    };

    let (agents, runner) = app_state.workspace_sessions_for_group(group_id);
    let mut sessions = agents;
    if let Some(runner) = runner {
        sessions.push(runner);
    }

    for session in &sessions {
        let _ = runtime.snapshot(session.id);
        let _ = runtime.session_cwd(session.id).or_else(|| {
            app_state
                .group_path(session.group_id)
                .map(ToString::to_string)
        });
    }

    let _ = orchestrator::snapshot_group(app_state, runtime, group_id, None);
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
) -> Vec<u128> {
    let mut hold_times = Vec::with_capacity(iterations);

    for _ in 0..iterations {
        let started = Instant::now();
        simulate_render_pass(app_state, terminal_manager);
        hold_times.push(started.elapsed().as_micros());
    }

    hold_times.sort_unstable();
    hold_times
}

fn profile_autosave_hold(
    terminal_manager: &Arc<TerminalManager>,
    app_state: &AppState,
    iterations: usize,
) -> Vec<u128> {
    let mut hold_times = Vec::with_capacity(iterations);

    for _ in 0..iterations {
        let started = Instant::now();
        let _ = persistence::build_workspace_snapshot(app_state, terminal_manager);
        hold_times.push(started.elapsed().as_micros());
    }

    hold_times.sort_unstable();
    hold_times
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
    baseline_lock_wait: DistributionStats,
    baseline_total_send: DistributionStats,
    render_lock_wait: DistributionStats,
    render_total_send: DistributionStats,
    full_lock_wait: DistributionStats,
    full_total_send: DistributionStats,
    render_pass_p95_us: u128,
    autosave_pass_p95_us: u128,
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
