use gestalt::pantograph_reasoning_probe::{PantographReasoningProbeRequest, run_reasoning_probe};
use std::path::PathBuf;

fn main() -> Result<(), String> {
    let request = parse_cli(std::env::args().skip(1).collect())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed creating tokio runtime: {error}"))?;
    let report = runtime
        .block_on(run_reasoning_probe(request))
        .map_err(|error| error.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report)
            .map_err(|error| format!("failed serializing reasoning validation report: {error}"))?
    );
    Ok(())
}

fn parse_cli(args: Vec<String>) -> Result<PantographReasoningProbeRequest, String> {
    let mut request = PantographReasoningProbeRequest::default();
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--dataset" => {
                index += 1;
                request.dataset = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| "missing value after --dataset".to_string())?;
            }
            "--task" => {
                index += 1;
                request.task_text = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| "missing value after --task".to_string())?;
            }
            "--query" => {
                index += 1;
                request.query_text = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| "missing value after --query".to_string())?;
            }
            "--top-k" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "missing value after --top-k".to_string())?;
                request.top_k = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --top-k value '{value}': {error}"))?;
            }
            "--storage-path" => {
                index += 1;
                request.storage_path = PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "missing value after --storage-path".to_string())?,
                );
            }
            "--namespace" => {
                index += 1;
                request.namespace = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| "missing value after --namespace".to_string())?;
            }
            "--database" => {
                index += 1;
                request.database = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| "missing value after --database".to_string())?;
            }
            "--reset" => request.reset = true,
            "--reseed" => request.reseed = true,
            "--help" | "-h" => return Err(usage()),
            _ => {
                if let Some(value) = arg.strip_prefix("--dataset=") {
                    request.dataset = value.to_string();
                } else if let Some(value) = arg.strip_prefix("--task=") {
                    request.task_text = value.to_string();
                } else if let Some(value) = arg.strip_prefix("--query=") {
                    request.query_text = value.to_string();
                } else if let Some(value) = arg.strip_prefix("--top-k=") {
                    request.top_k = value
                        .parse::<usize>()
                        .map_err(|error| format!("invalid --top-k value '{value}': {error}"))?;
                } else if let Some(value) = arg.strip_prefix("--storage-path=") {
                    request.storage_path = PathBuf::from(value);
                } else if let Some(value) = arg.strip_prefix("--namespace=") {
                    request.namespace = value.to_string();
                } else if let Some(value) = arg.strip_prefix("--database=") {
                    request.database = value.to_string();
                } else {
                    return Err(format!("unrecognized argument '{arg}'\n\n{}", usage()));
                }
            }
        }
        index += 1;
    }

    if request.dataset.trim().is_empty() {
        return Err("--dataset cannot be empty".to_string());
    }
    if request.task_text.trim().is_empty() {
        return Err("--task cannot be empty".to_string());
    }
    if request.query_text.trim().is_empty() {
        return Err("--query cannot be empty".to_string());
    }
    if request.top_k == 0 {
        return Err("--top-k must be greater than zero".to_string());
    }

    Ok(request)
}

fn usage() -> String {
    "Usage: cargo run --bin emily_pantograph_reasoning_probe -- [--dataset LABEL] [--task TEXT] [--query TEXT] [--top-k N] [--storage-path PATH] [--namespace NS] [--database DB] [--reset] [--reseed]\n\
     Requires GESTALT_PANTOGRAPH_REASONING_WORKFLOW_ID and related Pantograph host env vars when needed."
        .to_string()
}
