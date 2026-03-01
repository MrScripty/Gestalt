use super::{PlatformSample, ProcessCounters, SystemCounters};
use std::collections::HashMap;
use std::process::Command;

pub(super) fn sample() -> Result<PlatformSample, String> {
    let processes = sample_process_counters()?;
    let (memory_used_bytes, memory_total_bytes) = sample_memory_counters().unwrap_or_else(|_| {
        let total = 0_u64;
        let used = processes.iter().map(|process| process.memory_bytes).sum();
        (used, total)
    });
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
    let total_output = Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .map_err(|error| format!("failed to query hw.memsize: {error}"))?;
    if !total_output.status.success() {
        return Err("sysctl hw.memsize returned non-zero status".to_string());
    }
    let total_bytes = String::from_utf8_lossy(&total_output.stdout)
        .trim()
        .parse::<u64>()
        .map_err(|error| format!("invalid hw.memsize output: {error}"))?;

    let page_size_output = Command::new("sysctl")
        .args(["-n", "hw.pagesize"])
        .output()
        .map_err(|error| format!("failed to query hw.pagesize: {error}"))?;
    if !page_size_output.status.success() {
        return Err("sysctl hw.pagesize returned non-zero status".to_string());
    }
    let page_size = String::from_utf8_lossy(&page_size_output.stdout)
        .trim()
        .parse::<u64>()
        .map_err(|error| format!("invalid hw.pagesize output: {error}"))?;

    let vm_output = Command::new("vm_stat")
        .output()
        .map_err(|error| format!("failed to execute vm_stat: {error}"))?;
    if !vm_output.status.success() {
        return Err("vm_stat returned non-zero status".to_string());
    }
    let vm_stat = String::from_utf8_lossy(&vm_output.stdout);
    let pages = parse_vm_stat_pages(&vm_stat);

    let reclaimable_pages = pages
        .get("Pages free")
        .copied()
        .unwrap_or(0)
        .saturating_add(pages.get("Pages inactive").copied().unwrap_or(0))
        .saturating_add(pages.get("Pages speculative").copied().unwrap_or(0));
    let reclaimable_bytes = reclaimable_pages.saturating_mul(page_size);
    let used_bytes = total_bytes.saturating_sub(reclaimable_bytes);
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

fn parse_vm_stat_pages(output: &str) -> HashMap<String, u64> {
    let mut pages = HashMap::<String, u64>::new();
    for line in output.lines() {
        if let Some((label, value)) = line.split_once(':') {
            let numeric = value
                .chars()
                .filter(char::is_ascii_digit)
                .collect::<String>();
            if numeric.is_empty() {
                continue;
            }
            if let Ok(parsed) = numeric.parse::<u64>() {
                pages.insert(label.trim().to_string(), parsed);
            }
        }
    }
    pages
}

fn parse_decimal(value: &str) -> Option<f32> {
    value.replace(',', ".").parse::<f32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_vm_stat_pages_extracts_numeric_values() {
        let vm = "Pages free:                               120.\nPages inactive:                            240.\n";
        let pages = parse_vm_stat_pages(vm);
        assert_eq!(pages.get("Pages free"), Some(&120_u64));
        assert_eq!(pages.get("Pages inactive"), Some(&240_u64));
    }
}
