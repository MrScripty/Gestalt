use emily::api::EmilyApi;
use emily::inference::EmbeddingProvider;
use emily::model::{DatabaseLocator, EmbeddingProviderStatus, VectorizationConfigPatch};
use emily::runtime::EmilyRuntime;
use emily::store::surreal::SurrealEmilyStore;
use gestalt::emily_bridge::EmilyBridge;
use gestalt::emily_seed::{SYNTHETIC_SEMANTIC_CONTEXT_DATASET, seed_builtin_corpus};
use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct FixedEmbeddingProvider {
    vectors_by_text: HashMap<String, Vec<f32>>,
    default_vector: Vec<f32>,
}

#[async_trait::async_trait]
impl EmbeddingProvider for FixedEmbeddingProvider {
    async fn embed_text(&self, text: &str) -> Result<Vec<f32>, emily::error::EmilyError> {
        Ok(self
            .vectors_by_text
            .get(text)
            .cloned()
            .unwrap_or_else(|| self.default_vector.clone()))
    }

    async fn status(&self) -> Option<EmbeddingProviderStatus> {
        Some(EmbeddingProviderStatus {
            state: "ready".to_string(),
            session_id: None,
            queued_runs: None,
            queue_items: None,
            keep_alive: Some(true),
            last_error: None,
        })
    }
}

#[test]
fn emily_bridge_returns_semantic_context_when_vectors_are_available() {
    let locator = unique_locator("emily-semantic-retrieval");
    let storage_path = locator.storage_path.clone();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    let provider = Arc::new(FixedEmbeddingProvider {
        vectors_by_text: HashMap::from([
            (
                "provider capability metadata must match target routing requirements before remote dispatch"
                    .to_string(),
                vec![1.0, 0.0, 0.0],
            ),
            (
                "repository clean after the last cargo test run".to_string(),
                vec![0.0, 1.0, 0.0],
            ),
            (
                "recent session output: membrane validation remained within review thresholds"
                    .to_string(),
                vec![0.0, 0.0, 1.0],
            ),
            (
                "why did target selection fail for the remote provider".to_string(),
                vec![1.0, 0.0, 0.0],
            ),
        ]),
        default_vector: vec![0.0, 0.0, 1.0],
    });

    runtime.block_on(async {
        let emily_runtime = EmilyRuntime::with_embedding_provider(
            Arc::new(SurrealEmilyStore::new()),
            Some(provider.clone()),
        );
        emily_runtime
            .open_db(locator.clone())
            .await
            .expect("db should open");
        emily_runtime
            .update_vectorization_config(VectorizationConfigPatch {
                enabled: Some(true),
                expected_dimensions: Some(3),
                profile_id: Some("Qwen3-Embedding-4B-GGUF".to_string()),
            })
            .await
            .expect("vectorization should enable");
        let _ = seed_builtin_corpus(&emily_runtime, SYNTHETIC_SEMANTIC_CONTEXT_DATASET)
            .await
            .expect("semantic dataset should seed");
        emily_runtime
            .close_db()
            .await
            .expect("db should close cleanly");
    });

    let bridge = Arc::new(EmilyBridge::with_embedding_provider(
        locator,
        Some(provider),
    ));
    let packet = runtime
        .block_on(bridge.query_context_async(
            41,
            "why did target selection fail for the remote provider".to_string(),
            2,
        ))
        .expect("semantic query should succeed");

    assert_eq!(packet.items.len(), 2);
    assert_eq!(
        packet.items[0].object.text,
        "provider capability metadata must match target routing requirements before remote dispatch"
    );

    drop(bridge);
    thread::sleep(Duration::from_millis(50));
    let _ = std::fs::remove_dir_all(storage_path);
}

#[test]
fn emily_bridge_falls_back_to_lexical_context_when_vectors_are_unavailable() {
    let locator = unique_locator("emily-lexical-retrieval");
    let storage_path = locator.storage_path.clone();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime should build");

    runtime.block_on(async {
        let emily_runtime = EmilyRuntime::new(Arc::new(SurrealEmilyStore::new()));
        emily_runtime
            .open_db(locator.clone())
            .await
            .expect("db should open");
        let _ = seed_builtin_corpus(&emily_runtime, SYNTHETIC_SEMANTIC_CONTEXT_DATASET)
            .await
            .expect("semantic dataset should seed");
        emily_runtime
            .close_db()
            .await
            .expect("db should close cleanly");
    });

    let bridge = Arc::new(EmilyBridge::new(locator));
    let packet = runtime
        .block_on(bridge.query_context_async(41, "repository clean".to_string(), 1))
        .expect("lexical query should succeed");

    assert_eq!(packet.items.len(), 1);
    assert_eq!(
        packet.items[0].object.text,
        "repository clean after the last cargo test run"
    );

    drop(bridge);
    thread::sleep(Duration::from_millis(50));
    let _ = std::fs::remove_dir_all(storage_path);
}

fn unique_locator(name: &str) -> DatabaseLocator {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let storage_path =
        std::env::temp_dir().join(format!("gestalt-{name}-{nonce}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&storage_path);
    DatabaseLocator {
        storage_path,
        namespace: "gestalt_test".to_string(),
        database: "default".to_string(),
    }
}
