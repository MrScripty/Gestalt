use emily::api::EmilyApi;
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use gestalt::emily_inspect::inspect_seeded_corpus;
use gestalt::emily_seed::{builtin_dataset_labels, seed_builtin_corpus};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

const DEFAULT_STORAGE_DIR: &str = "gestalt-emily-seed";
const DEFAULT_NAMESPACE: &str = "gestalt_seed";
const DEFAULT_DATABASE: &str = "default";
const DEFAULT_HISTORY_LIMIT: usize = 8;
const DEFAULT_CONTEXT_TOP_K: usize = 3;

struct InspectCli {
    datasets: Vec<String>,
    storage_path: PathBuf,
    namespace: String,
    database: String,
    history_limit: usize,
    context_top_k: usize,
    query_text: Option<String>,
    reset: bool,
    reseed: bool,
}

fn main() -> Result<(), String> {
    let cli = parse_cli(std::env::args().skip(1).collect())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed building Tokio runtime: {error}"))?;

    runtime.block_on(async move {
        if cli.reset {
            match std::fs::remove_dir_all(&cli.storage_path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "failed resetting Emily storage path {}: {error}",
                        cli.storage_path.display()
                    ));
                }
            }
        }

        let emily_runtime = EmilyRuntime::new(Arc::new(SurrealEmilyStore::new()));
        emily_runtime
            .open_db(emily::DatabaseLocator {
                storage_path: cli.storage_path.clone(),
                namespace: cli.namespace.clone(),
                database: cli.database.clone(),
            })
            .await
            .map_err(|error| format!("failed opening Emily database: {error}"))?;

        if cli.reseed {
            for dataset in &cli.datasets {
                let _ = seed_builtin_corpus(&emily_runtime, dataset)
                    .await
                    .map_err(|error| error.to_string())?;
            }
        }

        for dataset in &cli.datasets {
            let snapshot = inspect_seeded_corpus(
                &emily_runtime,
                dataset,
                cli.history_limit,
                cli.query_text.as_deref(),
                cli.context_top_k,
            )
            .await
            .map_err(|error| error.to_string())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&snapshot)
                    .map_err(|error| format!("failed serializing inspection snapshot: {error}"))?
            );
        }

        let _ = emily_runtime.close_db().await;
        Ok(())
    })
}

fn parse_cli(args: Vec<String>) -> Result<InspectCli, String> {
    let mut datasets = Vec::new();
    let mut storage_path = std::env::temp_dir().join(DEFAULT_STORAGE_DIR);
    let mut namespace = DEFAULT_NAMESPACE.to_string();
    let mut database = DEFAULT_DATABASE.to_string();
    let mut history_limit = DEFAULT_HISTORY_LIMIT;
    let mut context_top_k = DEFAULT_CONTEXT_TOP_K;
    let mut query_text = None;
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
                datasets.push(value.clone());
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
            "--history-limit" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value after --history-limit".to_string());
                };
                history_limit = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --history-limit value '{value}': {error}"))?;
            }
            "--top-k" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value after --top-k".to_string());
                };
                context_top_k = value
                    .parse::<usize>()
                    .map_err(|error| format!("invalid --top-k value '{value}': {error}"))?;
            }
            "--query" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value after --query".to_string());
                };
                query_text = Some(value.clone());
            }
            "--reset" => reset = true,
            "--reseed" => reseed = true,
            "--help" | "-h" => return Err(usage()),
            _ => {
                if let Some(value) = arg.strip_prefix("--dataset=") {
                    datasets.push(value.to_string());
                } else if let Some(value) = arg.strip_prefix("--storage-path=") {
                    storage_path = PathBuf::from(value);
                } else if let Some(value) = arg.strip_prefix("--namespace=") {
                    namespace = value.to_string();
                } else if let Some(value) = arg.strip_prefix("--database=") {
                    database = value.to_string();
                } else if let Some(value) = arg.strip_prefix("--history-limit=") {
                    history_limit = value.parse::<usize>().map_err(|error| {
                        format!("invalid --history-limit value '{value}': {error}")
                    })?;
                } else if let Some(value) = arg.strip_prefix("--top-k=") {
                    context_top_k = value
                        .parse::<usize>()
                        .map_err(|error| format!("invalid --top-k value '{value}': {error}"))?;
                } else if let Some(value) = arg.strip_prefix("--query=") {
                    query_text = Some(value.to_string());
                } else {
                    return Err(format!("unrecognized argument '{arg}'\n\n{}", usage()));
                }
            }
        }
        index += 1;
    }

    if datasets.is_empty() {
        datasets.extend(
            builtin_dataset_labels()
                .iter()
                .map(|label| (*label).to_string()),
        );
    }

    if history_limit == 0 {
        return Err("--history-limit must be greater than zero".to_string());
    }
    if query_text.is_some() && context_top_k == 0 {
        return Err("--top-k must be greater than zero when --query is used".to_string());
    }

    Ok(InspectCli {
        datasets,
        storage_path,
        namespace,
        database,
        history_limit,
        context_top_k,
        query_text,
        reset,
        reseed,
    })
}

fn usage() -> String {
    format!(
        "Usage: cargo run --bin emily_inspect -- [--dataset LABEL] [--query TEXT] [--top-k N] [--history-limit N] [--reseed] [--reset] [--storage-path PATH] [--namespace NS] [--database DB]\n\
         Available datasets: {}\n\
         Default storage path: {}",
        builtin_dataset_labels().join(", "),
        std::env::temp_dir().join(DEFAULT_STORAGE_DIR).display()
    )
}
