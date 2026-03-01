use super::{PlatformSample, ProcessCounters, SystemCounters};
use std::fs;
use std::process::Command;

pub(super) fn sample() -> Result<PlatformSample, String> {
    let processes = sample_process_counters()?;
    let (memory_used_bytes, memory_total_bytes) =
        sample_memory_counters().unwrap_or((0_u64, 0_u64));
    Ok(PlatformSample {
        processes,
        system: SystemCounters {
            cpu_percent: None,
            memory_used_bytes,
            memory_total_bytes,
        },
    })
}

fn sample_process_counters() -> Result<Vec<ProcessCounters>, String> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,%cpu=,rss="])
        .output()
        .map_err(|error| format!("failed to execute ps: {error}"))?;
    if !output.status.success() {
        return Err("ps command exited with a non-zero status".to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut processes = Vec::<ProcessCounters>::new();
    for line in stdout.lines() {
        if let Some(process) = parse_ps_line(line) {
            processes.push(process);
        }
    }

    Ok(processes)
}

fn sample_memory_counters() -> Result<(u64, u64), String> {
    let meminfo = fs::read_to_string("/proc/meminfo")
        .map_err(|error| format!("failed to read /proc/meminfo: {error}"))?;
    let total_kib = parse_meminfo_kib_value(&meminfo, "MemTotal")
        .ok_or_else(|| "missing MemTotal from /proc/meminfo".to_string())?;
    let available_kib = parse_meminfo_kib_value(&meminfo, "MemAvailable").or_else(|| {
        let free_kib = parse_meminfo_kib_value(&meminfo, "MemFree")?;
        let buffers_kib = parse_meminfo_kib_value(&meminfo, "Buffers").unwrap_or(0);
        let cached_kib = parse_meminfo_kib_value(&meminfo, "Cached").unwrap_or(0);
        Some(
            free_kib
                .saturating_add(buffers_kib)
                .saturating_add(cached_kib),
        )
    });
    let available_kib = available_kib.unwrap_or(0);

    let total_bytes = total_kib.saturating_mul(1_024);
    let available_bytes = available_kib.saturating_mul(1_024);
    let used_bytes = total_bytes.saturating_sub(available_bytes);
    Ok((used_bytes, total_bytes))
}

fn parse_ps_line(line: &str) -> Option<ProcessCounters> {
    let mut fields = line.split_whitespace();
    let pid = fields.next()?.parse::<u32>().ok()?;
    let ppid_raw = fields.next()?.parse::<u32>().ok()?;
    let cpu_percent = parse_decimal(fields.next()?)?;
    let rss_kib = fields.next()?.parse::<u64>().ok()?;
    Some(ProcessCounters {
        pid,
        ppid: if ppid_raw == 0 { None } else { Some(ppid_raw) },
        cpu_percent,
        memory_bytes: rss_kib.saturating_mul(1_024),
    })
}

fn parse_meminfo_kib_value(contents: &str, key: &str) -> Option<u64> {
    let prefix = format!("{key}:");
    contents.lines().find_map(|line| {
        if !line.starts_with(&prefix) {
            return None;
        }
        line.split_whitespace()
            .nth(1)
            .and_then(|value| value.parse::<u64>().ok())
    })
}

fn parse_decimal(value: &str) -> Option<f32> {
    value.replace(',', ".").parse::<f32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ps_line_extracts_pid_parent_cpu_and_memory() {
        let parsed = parse_ps_line("  42  1  12.5  2048").expect("expected a parsed process row");
        assert_eq!(parsed.pid, 42);
        assert_eq!(parsed.ppid, Some(1));
        assert_eq!(parsed.cpu_percent, 12.5);
        assert_eq!(parsed.memory_bytes, 2_097_152);
    }

    #[test]
    fn parse_meminfo_kib_value_reads_named_key() {
        let meminfo = "MemTotal:       32768000 kB\nMemAvailable:   16384000 kB\n";
        assert_eq!(
            parse_meminfo_kib_value(meminfo, "MemTotal"),
            Some(32_768_000)
        );
        assert_eq!(
            parse_meminfo_kib_value(meminfo, "MemAvailable"),
            Some(16_384_000)
        );
        assert_eq!(parse_meminfo_kib_value(meminfo, "Missing"), None);
    }
}
