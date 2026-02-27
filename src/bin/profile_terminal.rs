use gestalt::orchestrator;
use gestalt::persistence;
use gestalt::state::{AppState, SessionId};
use gestalt::terminal::TerminalManager;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const WARMUP_HISTORY_LINES: usize = 12_000;
const HISTORY_LINE_WIDTH: usize = 96;
const TYPING_SAMPLES: usize = 320;
const TYPING_INTERVAL: Duration = Duration::from_millis(8);
const ASSERT_LOCK_WAIT_P95_US: u128 = 200;
const ASSERT_RENDER_TOTAL_P95_US: u128 = 1_000;
const ASSERT_FULL_TOTAL_P95_US: u128 = 1_500;

fn main() -> Result<(), String> {
    let assert_mode = std::env::args().any(|arg| arg == "--assert");
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
    println!();
    println!("Mutex hold timings for heavy operations");
    println!(
        "  render pass us: avg={} p50={} p95={} p99={} max={}",
        average(&render_hold),
        percentile(&render_hold, 50),
        percentile(&render_hold, 95),
        percentile(&render_hold, 99),
        render_hold.last().copied().unwrap_or_default()
    );
    println!(
        "  autosave pass us: avg={} p50={} p95={} p99={} max={}",
        average(&autosave_hold),
        percentile(&autosave_hold, 50),
        percentile(&autosave_hold, 95),
        percentile(&autosave_hold, 99),
        autosave_hold.last().copied().unwrap_or_default()
    );

    let baseline = profile_typing_latency(&terminal_manager, session_ids[0], TYPING_SAMPLES);
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
        profile_typing_latency(&terminal_manager, session_ids[0], TYPING_SAMPLES);
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
    let full_contended = profile_typing_latency(&terminal_manager, session_ids[0], TYPING_SAMPLES);
    render_stop.store(true, Ordering::Relaxed);
    autosave_stop.store(true, Ordering::Relaxed);
    let _ = render_worker.join();
    let _ = autosave_worker.join();

    println!();
    println!("Scenario: render + autosave contention");
    full_contended.print();

    if assert_mode {
        let mut failures = Vec::new();

        if render_contended.lock_wait_p95() > ASSERT_LOCK_WAIT_P95_US {
            failures.push(format!(
                "render contention lock-wait p95 {}us exceeds {}us",
                render_contended.lock_wait_p95(),
                ASSERT_LOCK_WAIT_P95_US
            ));
        }
        if full_contended.lock_wait_p95() > ASSERT_LOCK_WAIT_P95_US {
            failures.push(format!(
                "render+autosave lock-wait p95 {}us exceeds {}us",
                full_contended.lock_wait_p95(),
                ASSERT_LOCK_WAIT_P95_US
            ));
        }
        if render_contended.total_send_p95() > ASSERT_RENDER_TOTAL_P95_US {
            failures.push(format!(
                "render contention total-send p95 {}us exceeds {}us",
                render_contended.total_send_p95(),
                ASSERT_RENDER_TOTAL_P95_US
            ));
        }
        if full_contended.total_send_p95() > ASSERT_FULL_TOTAL_P95_US {
            failures.push(format!(
                "render+autosave total-send p95 {}us exceeds {}us",
                full_contended.total_send_p95(),
                ASSERT_FULL_TOTAL_P95_US
            ));
        }

        if failures.is_empty() {
            println!();
            println!("Perf assertions passed.");
        } else {
            println!();
            println!("Perf assertions failed:");
            for failure in failures {
                println!("  - {failure}");
            }
            return Err("terminal performance assertion failed".to_string());
        }
    }

    Ok(())
}

fn seed_terminal_output(
    terminal_manager: &Arc<TerminalManager>,
    session_ids: &[SessionId],
) -> Result<(), String> {
    let fill = "x".repeat(HISTORY_LINE_WIDTH);
    let command = format!("for i in $(seq 1 {WARMUP_HISTORY_LINES}); do echo \"$i {fill}\"; done");

    for session_id in session_ids {
        terminal_manager.send_line(*session_id, &command)?;
    }

    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let ready = {
            session_ids.iter().all(|session_id| {
                terminal_manager
                    .snapshot(*session_id)
                    .map(|snapshot| snapshot.lines.iter().any(|line| line.contains(&fill)))
                    .unwrap_or(false)
            })
        };

        if ready || Instant::now() >= deadline {
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }

    Ok(())
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
) -> LatencyProfile {
    let mut lock_waits = Vec::with_capacity(samples);
    let mut totals = Vec::with_capacity(samples);

    for _ in 0..samples {
        let started = Instant::now();
        let lock_acquired = Instant::now();
        let _ = terminal_manager.send_input(session_id, b"a");
        let ended = Instant::now();

        lock_waits.push(lock_acquired.duration_since(started));
        totals.push(ended.duration_since(started));
        thread::sleep(TYPING_INTERVAL);
    }

    LatencyProfile::new(lock_waits, totals)
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
        println!(
            "  lock wait us: avg={} p50={} p95={} p99={} max={}",
            average(&self.lock_wait_micros),
            percentile(&self.lock_wait_micros, 50),
            percentile(&self.lock_wait_micros, 95),
            percentile(&self.lock_wait_micros, 99),
            self.lock_wait_micros.last().copied().unwrap_or_default()
        );
        println!(
            "  total send us: avg={} p50={} p95={} p99={} max={}",
            average(&self.total_micros),
            percentile(&self.total_micros, 50),
            percentile(&self.total_micros, 95),
            percentile(&self.total_micros, 99),
            self.total_micros.last().copied().unwrap_or_default()
        );
    }

    fn lock_wait_p95(&self) -> u128 {
        percentile(&self.lock_wait_micros, 95)
    }

    fn total_send_p95(&self) -> u128 {
        percentile(&self.total_micros, 95)
    }
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
