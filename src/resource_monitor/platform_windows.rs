use super::{PlatformSample, ProcessCounters, SystemCounters};
use serde_json::Value;
use std::process::Command;

pub(super) fn sample() -> Result<PlatformSample, String> {
    let output = run_powershell(
        "$cpu=(Get-Counter '\\Processor(_Total)\\% Processor Time').CounterSamples[0].CookedValue;\
         $os=Get-CimInstance Win32_OperatingSystem | Select-Object TotalVisibleMemorySize,FreePhysicalMemory;\
         $procs=Get-CimInstance Win32_PerfFormattedData_PerfProc_Process | Select-Object IDProcess,CreatingProcessID,PercentProcessorTime,WorkingSetPrivate,WorkingSet;\
         @{cpu=$cpu;total_kib=$os.TotalVisibleMemorySize;free_kib=$os.FreePhysicalMemory;processes=$procs} | ConvertTo-Json -Compress",
    )?;

    let value: Value = serde_json::from_str(&output)
        .map_err(|error| format!("invalid powershell json: {error}"))?;
    let system_cpu = value
        .get("cpu")
        .and_then(parse_json_f32)
        .map(|value| value.clamp(0.0, 100.0));
    let total_kib = value
        .get("total_kib")
        .and_then(parse_json_u64)
        .unwrap_or(0_u64);
    let free_kib = value
        .get("free_kib")
        .and_then(parse_json_u64)
        .unwrap_or(0_u64);
    let total_bytes = total_kib.saturating_mul(1_024);
    let free_bytes = free_kib.saturating_mul(1_024);
    let used_bytes = total_bytes.saturating_sub(free_bytes);

    let processes = value
        .get("processes")
        .map(parse_process_rows)
        .unwrap_or_default();

    Ok(PlatformSample {
        processes,
        system: SystemCounters {
            cpu_percent: system_cpu,
            memory_used_bytes: used_bytes,
            memory_total_bytes: total_bytes,
        },
    })
}

fn run_powershell(script: &str) -> Result<String, String> {
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .output()
        .map_err(|error| format!("failed to execute powershell: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "powershell exited with non-zero status: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn parse_process_rows(value: &Value) -> Vec<ProcessCounters> {
    let rows = match value {
        Value::Array(values) => values.iter().collect::<Vec<_>>(),
        Value::Object(_) => vec![value],
        _ => return Vec::new(),
    };
    let mut processes = Vec::<ProcessCounters>::new();
    for row in rows {
        let pid = row.get("IDProcess").and_then(parse_json_u64).unwrap_or(0);
        if pid == 0 || pid > u64::from(u32::MAX) {
            continue;
        }

        let ppid = row
            .get("CreatingProcessID")
            .and_then(parse_json_u64)
            .filter(|value| *value > 0 && *value <= u64::from(u32::MAX))
            .map(|value| value as u32);
        let cpu_percent = row
            .get("PercentProcessorTime")
            .and_then(parse_json_f32)
            .unwrap_or(0.0);
        let memory_bytes = row
            .get("WorkingSetPrivate")
            .and_then(parse_json_u64)
            .or_else(|| row.get("WorkingSet").and_then(parse_json_u64))
            .unwrap_or(0_u64);

        processes.push(ProcessCounters {
            pid: pid as u32,
            ppid,
            cpu_percent,
            memory_bytes,
        });
    }
    processes
}

fn parse_json_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(string) => string.trim().parse::<u64>().ok(),
        _ => None,
    }
}

fn parse_json_f32(value: &Value) -> Option<f32> {
    match value {
        Value::Number(number) => number.as_f64().map(|value| value as f32),
        Value::String(string) => string.replace(',', ".").parse::<f32>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_process_rows_accepts_single_object_shape() {
        let value = serde_json::json!({
            "IDProcess": 1234,
            "CreatingProcessID": 77,
            "PercentProcessorTime": 50.5,
            "WorkingSetPrivate": 1024
        });
        let rows = parse_process_rows(&value);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].pid, 1234);
        assert_eq!(rows[0].ppid, Some(77));
    }
}
