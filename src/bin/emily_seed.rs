use emily::api::EmilyApi;
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use gestalt::emily_seed::{builtin_dataset_labels, seed_builtin_corpus};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

const DEFAULT_STORAGE_DIR: &str = "gestalt-emily-seed";
const DEFAULT_NAMESPACE: &str = "gestalt_seed";
const DEFAULT_DATABASE: &str = "default";

struct SeedCli {
    datasets: Vec<String>,
    storage_path: PathBuf,
    namespace: String,
    database: String,
    reset: bool,
}

fn main() -> Result<(), String> {
    let cli = parse_cli(std::env::args().skip(1).collect())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed building Tokio runtime: {error}"))?;

    runtime.block_on(async move {
        let emily_runtime = EmilyRuntime::new(Arc::new(SurrealEmilyStore::new()));
        let locator = emily::DatabaseLocator {
            storage_path: cli.storage_path.clone(),
            namespace: cli.namespace.clone(),
            database: cli.database.clone(),
        };

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
        emily_runtime
            .open_db(locator)
            .await
            .map_err(|error| format!("failed opening Emily database: {error}"))?;

        for dataset in &cli.datasets {
            let report = seed_builtin_corpus(&emily_runtime, dataset)
                .await
                .map_err(|error| error.to_string())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&report)
                    .map_err(|error| format!("failed serializing seed report: {error}"))?
            );
        }

        let _ = emily_runtime.close_db().await;
        Ok(())
    })
}

fn parse_cli(args: Vec<String>) -> Result<SeedCli, String> {
    let mut datasets = Vec::new();
    let mut storage_path = std::env::temp_dir().join(DEFAULT_STORAGE_DIR);
    let mut namespace = DEFAULT_NAMESPACE.to_string();
    let mut database = DEFAULT_DATABASE.to_string();
    let mut reset = false;
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
            "--reset" => {
                reset = true;
            }
            "--help" | "-h" => {
                return Err(usage());
            }
            _ => {
                if let Some(value) = arg.strip_prefix("--dataset=") {
                    datasets.push(value.to_string());
                } else if let Some(value) = arg.strip_prefix("--storage-path=") {
                    storage_path = PathBuf::from(value);
                } else if let Some(value) = arg.strip_prefix("--namespace=") {
                    namespace = value.to_string();
                } else if let Some(value) = arg.strip_prefix("--database=") {
                    database = value.to_string();
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

    Ok(SeedCli {
        datasets,
        storage_path,
        namespace,
        database,
        reset,
    })
}

fn usage() -> String {
    format!(
        "Usage: cargo run --bin emily_seed -- [--dataset LABEL] [--storage-path PATH] [--namespace NS] [--database DB] [--reset]\n\
         Available datasets: {}\n\
         Default storage path: {}",
        builtin_dataset_labels().join(", "),
        std::env::temp_dir().join(DEFAULT_STORAGE_DIR).display()
    )
}
