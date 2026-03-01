use crate::state::SessionId;
use std::collections::{HashMap, HashSet, VecDeque};

#[cfg(target_os = "linux")]
mod platform_linux;
#[cfg(target_os = "macos")]
mod platform_macos;
#[cfg(target_os = "windows")]
mod platform_windows;

#[cfg(target_os = "linux")]
use platform_linux as platform;
#[cfg(target_os = "macos")]
use platform_macos as platform;
#[cfg(target_os = "windows")]
use platform_windows as platform;

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod platform {
    use super::{PlatformSample, ProcessCounters, SystemCounters};

    pub(super) fn sample() -> Result<PlatformSample, String> {
        Ok(PlatformSample {
            processes: Vec::<ProcessCounters>::new(),
            system: SystemCounters::default(),
        })
    }
}

const HOT_TAB_CPU_PERCENT: f32 = 35.0;
const WARM_TAB_CPU_PERCENT: f32 = 12.0;
const HOT_TAB_MEMORY_BYTES: u64 = 1_200 * 1_024 * 1_024;
const WARM_TAB_MEMORY_BYTES: u64 = 512 * 1_024 * 1_024;

/// Stable poll cadence used by UI collectors.
pub const RESOURCE_POLL_MS: u64 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionLoadLevel {
    #[default]
    Normal,
    Warm,
    Hot,
}

impl SessionLoadLevel {
    pub fn css_class(self) -> &'static str {
        match self {
            Self::Normal => "load-normal",
            Self::Warm => "load-warm",
            Self::Hot => "load-hot",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SessionLoadSnapshot {
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub level: SessionLoadLevel,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResourceSnapshot {
    pub system_cpu_percent: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub session_loads: HashMap<SessionId, SessionLoadSnapshot>,
}

#[derive(Debug, Clone)]
struct ProcessCounters {
    pid: u32,
    ppid: Option<u32>,
    cpu_percent: f32,
    memory_bytes: u64,
}

#[derive(Debug, Clone, Default)]
struct SystemCounters {
    cpu_percent: Option<f32>,
    memory_used_bytes: u64,
    memory_total_bytes: u64,
}

#[derive(Debug, Clone, Default)]
struct PlatformSample {
    processes: Vec<ProcessCounters>,
    system: SystemCounters,
}

/// Captures a point-in-time resource snapshot for system and terminal sessions.
pub fn sample_resource_snapshot(session_roots: &[(SessionId, u32)]) -> ResourceSnapshot {
    let platform_sample = platform::sample().unwrap_or_default();
    let process_by_pid = platform_sample
        .processes
        .iter()
        .map(|process| (process.pid, process))
        .collect::<HashMap<u32, &ProcessCounters>>();
    let mut children_by_pid = HashMap::<u32, Vec<u32>>::new();
    for process in &platform_sample.processes {
        if let Some(ppid) = process.ppid {
            children_by_pid.entry(ppid).or_default().push(process.pid);
        }
    }

    let mut session_loads = HashMap::<SessionId, SessionLoadSnapshot>::new();
    for &(session_id, root_pid) in session_roots {
        let (cpu_percent, memory_bytes) =
            aggregate_process_tree(root_pid, &process_by_pid, &children_by_pid);
        session_loads.insert(
            session_id,
            SessionLoadSnapshot {
                cpu_percent,
                memory_bytes,
                level: classify_session_load(cpu_percent, memory_bytes),
            },
        );
    }

    let total_process_cpu = platform_sample
        .processes
        .iter()
        .map(|process| process.cpu_percent.max(0.0))
        .sum::<f32>();
    let cpu_divisor = std::thread::available_parallelism()
        .map(|count| count.get() as f32)
        .unwrap_or(1.0)
        .max(1.0);
    let fallback_system_cpu = (total_process_cpu / cpu_divisor).clamp(0.0, 100.0);

    ResourceSnapshot {
        system_cpu_percent: round_percent(
            platform_sample
                .system
                .cpu_percent
                .unwrap_or(fallback_system_cpu),
        ),
        memory_used_bytes: platform_sample.system.memory_used_bytes,
        memory_total_bytes: platform_sample.system.memory_total_bytes,
        session_loads,
    }
}

fn aggregate_process_tree(
    root_pid: u32,
    process_by_pid: &HashMap<u32, &ProcessCounters>,
    children_by_pid: &HashMap<u32, Vec<u32>>,
) -> (f32, u64) {
    let mut queue = VecDeque::from([root_pid]);
    let mut visited = HashSet::<u32>::new();
    let mut cpu_percent = 0.0_f32;
    let mut memory_bytes = 0_u64;

    while let Some(pid) = queue.pop_front() {
        if !visited.insert(pid) {
            continue;
        }

        if let Some(process) = process_by_pid.get(&pid) {
            cpu_percent += process.cpu_percent.max(0.0);
            memory_bytes = memory_bytes.saturating_add(process.memory_bytes);
        }

        if let Some(children) = children_by_pid.get(&pid) {
            for child in children {
                queue.push_back(*child);
            }
        }
    }

    (round_percent(cpu_percent), memory_bytes)
}

fn classify_session_load(cpu_percent: f32, memory_bytes: u64) -> SessionLoadLevel {
    if cpu_percent >= HOT_TAB_CPU_PERCENT || memory_bytes >= HOT_TAB_MEMORY_BYTES {
        SessionLoadLevel::Hot
    } else if cpu_percent >= WARM_TAB_CPU_PERCENT || memory_bytes >= WARM_TAB_MEMORY_BYTES {
        SessionLoadLevel::Warm
    } else {
        SessionLoadLevel::Normal
    }
}

fn round_percent(value: f32) -> f32 {
    (value * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_process_tree_rolls_up_descendants() {
        let processes = vec![
            ProcessCounters {
                pid: 10,
                ppid: None,
                cpu_percent: 4.7,
                memory_bytes: 200,
            },
            ProcessCounters {
                pid: 11,
                ppid: Some(10),
                cpu_percent: 12.3,
                memory_bytes: 400,
            },
            ProcessCounters {
                pid: 12,
                ppid: Some(11),
                cpu_percent: 5.0,
                memory_bytes: 700,
            },
        ];

        let process_by_pid = processes
            .iter()
            .map(|process| (process.pid, process))
            .collect::<HashMap<u32, &ProcessCounters>>();
        let children_by_pid = HashMap::from([(10_u32, vec![11_u32]), (11_u32, vec![12_u32])]);

        let (cpu_percent, memory_bytes) =
            aggregate_process_tree(10, &process_by_pid, &children_by_pid);
        assert_eq!(cpu_percent, 22.0);
        assert_eq!(memory_bytes, 1_300);
    }

    #[test]
    fn classify_session_load_marks_hot_and_warm_thresholds() {
        assert_eq!(
            classify_session_load(HOT_TAB_CPU_PERCENT, 0),
            SessionLoadLevel::Hot
        );
        assert_eq!(
            classify_session_load(0.0, HOT_TAB_MEMORY_BYTES),
            SessionLoadLevel::Hot
        );
        assert_eq!(
            classify_session_load(WARM_TAB_CPU_PERCENT, 0),
            SessionLoadLevel::Warm
        );
        assert_eq!(
            classify_session_load(0.0, WARM_TAB_MEMORY_BYTES),
            SessionLoadLevel::Warm
        );
        assert_eq!(classify_session_load(2.0, 100), SessionLoadLevel::Normal);
    }
}
