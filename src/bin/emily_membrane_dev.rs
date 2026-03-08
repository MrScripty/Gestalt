use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use gestalt::emily_membrane_dev::{
    EmilyMembraneDevRequest, assert_membrane_dev_enabled, membrane_dev_toggle_env,
    run_membrane_dev_scenario,
};
use gestalt::emily_seed::SYNTHETIC_AGENT_ROUND_DATASET;
use std::path::PathBuf;
use std::sync::Arc;

const DEFAULT_STORAGE_DIR: &str = "gestalt-emily-membrane-dev";
const DEFAULT_NAMESPACE: &str = "gestalt_membrane_dev";
const DEFAULT_DATABASE: &str = "default";
const DEFAULT_TOP_K: usize = 3;
const DEFAULT_TASK_TEXT: &str =
    "Summarize the locally available provider-registry context for the debugging task.";

struct MembraneDevCli {
    dataset: String,
    storage_path: PathBuf,
    namespace: String,
    database: String,
    task_text: String,
    query_text: String,
    top_k: usize,
    reset: bool,
    reseed: bool,
}

fn main() -> Result<(), String> {
    assert_membrane_dev_enabled().map_err(|error| error.to_string())?;
    let cli = parse_cli(std::env::args().skip(1).collect())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed building Tokio runtime: {error}"))?;

    runtime.block_on(async move {
        let emily_runtime = Arc::new(EmilyRuntime::new(Arc::new(SurrealEmilyStore::new())));
        let snapshot = run_membrane_dev_scenario(
            emily_runtime,
            EmilyMembraneDevRequest {
                dataset: cli.dataset,
                storage_path: cli.storage_path,
                namespace: cli.namespace,
                database: cli.database,
                task_text: cli.task_text,
                query_text: cli.query_text,
                top_k: cli.top_k,
                reset: cli.reset,
                reseed: cli.reseed,
            },
        )
        .await
        .map_err(|error| error.to_string())?;
        println!(
            "{}",
            serde_json::to_string_pretty(&snapshot)
                .map_err(|error| format!("failed serializing membrane snapshot: {error}"))?
        );
        Ok(())
    })
}

fn parse_cli(args: Vec<String>) -> Result<MembraneDevCli, String> {
    let mut dataset = SYNTHETIC_AGENT_ROUND_DATASET.to_string();
    let mut storage_path = std::env::temp_dir().join(DEFAULT_STORAGE_DIR);
    let mut namespace = DEFAULT_NAMESPACE.to_string();
    let mut database = DEFAULT_DATABASE.to_string();
    let mut task_text = DEFAULT_TASK_TEXT.to_string();
    let mut query_text = DEFAULT_TASK_TEXT.to_string();
    let mut top_k = DEFAULT_TOP_K;
    let mut reset = false;
    let mut reseed = false;
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--dataset" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value after --dataset".to_string());
                };
                dataset = value.clone();
            }
            "--storage-path" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value after --storage-path".to_string());
                };
                storage_path = PathBuf::from(value);
            }
            "--namespace" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value after --namespace".to_string());
                };
                namespace = value.clone();
            }
            "--database" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value after --database".to_string());
                };
                database = value.clone();
            }
            "--task" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value after --task".to_string());
                };
                task_text = value.clone();
                query_text = value.clone();
            }
            "--query" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value after --query".to_string());
                };
                query_text = value.clone();
            }
            "--top-k" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value after --top-k".to_string());
                };
                top_k = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --top-k value '{value}': {error}"))?;
            }
            "--reset" => reset = true,
            "--reseed" => reseed = true,
            "--help" | "-h" => return Err(usage()),
            _ => {
                if let Some(value) = arg.strip_prefix("--dataset=") {
                    dataset = value.to_string();
                } else if let Some(value) = arg.strip_prefix("--storage-path=") {
                    storage_path = PathBuf::from(value);
                } else if let Some(value) = arg.strip_prefix("--namespace=") {
                    namespace = value.to_string();
                } else if let Some(value) = arg.strip_prefix("--database=") {
                    database = value.to_string();
                } else if let Some(value) = arg.strip_prefix("--task=") {
                    task_text = value.to_string();
                    query_text = value.to_string();
                } else if let Some(value) = arg.strip_prefix("--query=") {
                    query_text = value.to_string();
                } else if let Some(value) = arg.strip_prefix("--top-k=") {
                    top_k = value
                        .parse::<usize>()
                        .map_err(|error| format!("invalid --top-k value '{value}': {error}"))?;
                } else {
                    return Err(format!("unrecognized argument '{arg}'\n\n{}", usage()));
                }
            }
        }
        index += 1;
    }

    if top_k == 0 {
        return Err("--top-k must be greater than zero".to_string());
    }

    Ok(MembraneDevCli {
        dataset,
        storage_path,
        namespace,
        database,
        task_text,
        query_text,
        top_k,
        reset,
        reseed,
    })
}

fn usage() -> String {
    format!(
        "Usage: {env}=1 cargo run --bin emily_membrane_dev -- [--dataset LABEL] [--task TEXT] [--query TEXT] [--top-k N] [--reseed] [--reset] [--storage-path PATH] [--namespace NS] [--database DB]\n\
         Default dataset: {dataset}\n\
         Default storage path: {path}",
        env = membrane_dev_toggle_env(),
        dataset = SYNTHETIC_AGENT_ROUND_DATASET,
        path = std::env::temp_dir().join(DEFAULT_STORAGE_DIR).display(),
    )
}
