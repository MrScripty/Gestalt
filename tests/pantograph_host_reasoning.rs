use gestalt::pantograph_host::{
    PantographReasoningRuntimeConfig, build_membrane_provider_registry_from_env,
};
use std::path::PathBuf;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

const REASONING_ENV_KEYS: [&str; 10] = [
    "GESTALT_PANTOGRAPH_ROOT",
    "GESTALT_PANTOGRAPH_REASONING_WORKFLOW_ID",
    "GESTALT_PANTOGRAPH_REASONING_TIMEOUT_MS",
    "GESTALT_PANTOGRAPH_REASONING_PROVIDER_ID",
    "GESTALT_PANTOGRAPH_REASONING_MODEL_ID",
    "GESTALT_PANTOGRAPH_REASONING_PROFILE_ID",
    "GESTALT_PANTOGRAPH_REASONING_CAPABILITY_TAGS",
    "GESTALT_PANTOGRAPH_REASONING_TEXT_NODE_ID",
    "GESTALT_PANTOGRAPH_REASONING_TEXT_PORT_ID",
    "GESTALT_PANTOGRAPH_REASONING_OUTPUT_NODE_ID",
];

const REASONING_OUTPUT_PORT_ENV_KEY: &str = "GESTALT_PANTOGRAPH_REASONING_OUTPUT_PORT_ID";

fn pantograph_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../Pantograph")
}

fn with_reasoning_env(entries: &[(&str, &str)], test: impl FnOnce()) {
    let _guard = ENV_LOCK.lock().expect("env lock");
    let previous = REASONING_ENV_KEYS
        .iter()
        .chain([REASONING_OUTPUT_PORT_ENV_KEY].iter())
        .map(|key| ((*key).to_string(), std::env::var(key).ok()))
        .collect::<Vec<_>>();

    for key in REASONING_ENV_KEYS
        .iter()
        .chain([REASONING_OUTPUT_PORT_ENV_KEY].iter())
    {
        unsafe { std::env::remove_var(key) };
    }
    for (key, value) in entries {
        unsafe { std::env::set_var(key, value) };
    }

    test();

    for key in REASONING_ENV_KEYS
        .iter()
        .chain([REASONING_OUTPUT_PORT_ENV_KEY].iter())
    {
        unsafe { std::env::remove_var(key) };
    }
    for (key, value) in previous {
        if let Some(value) = value {
            unsafe { std::env::set_var(key, value) };
        }
    }
}

#[test]
fn reasoning_runtime_config_from_env_applies_expected_defaults() {
    let pantograph_root = pantograph_root();
    let root_string = pantograph_root.display().to_string();

    with_reasoning_env(
        &[
            ("GESTALT_PANTOGRAPH_ROOT", root_string.as_str()),
            ("GESTALT_PANTOGRAPH_REASONING_WORKFLOW_ID", "Qwen Reasoning"),
        ],
        || {
            let config = PantographReasoningRuntimeConfig::from_env()
                .expect("config parse should succeed")
                .expect("workflow id should enable config");

            assert_eq!(config.pantograph_root, pantograph_root);
            assert_eq!(config.workflow_id, "Qwen Reasoning");
            assert_eq!(config.timeout_ms, Some(120_000));
            assert_eq!(config.provider_id, "pantograph-qwen-reasoning");
            assert_eq!(config.model_id, "Qwen3.5-35B-A3B-GGUF");
            assert_eq!(config.profile_id, "reasoning");
            assert_eq!(
                config.capability_tags,
                vec!["analysis".to_string(), "reasoning".to_string()]
            );
            assert_eq!(config.text_input_node_id, None);
            assert_eq!(config.text_input_port_id, None);
            assert_eq!(config.text_output_node_id, None);
            assert_eq!(config.text_output_port_id, None);
        },
    );
}

#[test]
fn reasoning_registry_from_env_registers_expected_target() {
    let root_string = pantograph_root().display().to_string();

    with_reasoning_env(
        &[
            ("GESTALT_PANTOGRAPH_ROOT", root_string.as_str()),
            ("GESTALT_PANTOGRAPH_REASONING_WORKFLOW_ID", "Qwen Reasoning"),
            ("GESTALT_PANTOGRAPH_REASONING_TIMEOUT_MS", "45000"),
            (
                "GESTALT_PANTOGRAPH_REASONING_PROVIDER_ID",
                "pantograph-qwen",
            ),
            (
                "GESTALT_PANTOGRAPH_REASONING_MODEL_ID",
                "Qwen3.5-35B-A3B-GGUF",
            ),
            (
                "GESTALT_PANTOGRAPH_REASONING_PROFILE_ID",
                "remote-reasoning",
            ),
            (
                "GESTALT_PANTOGRAPH_REASONING_CAPABILITY_TAGS",
                "analysis,reasoning",
            ),
            ("GESTALT_PANTOGRAPH_REASONING_TEXT_NODE_ID", "text-input-1"),
            ("GESTALT_PANTOGRAPH_REASONING_TEXT_PORT_ID", "text"),
            (
                "GESTALT_PANTOGRAPH_REASONING_OUTPUT_NODE_ID",
                "text-output-1",
            ),
            ("GESTALT_PANTOGRAPH_REASONING_OUTPUT_PORT_ID", "text"),
        ],
        || {
            let registry = build_membrane_provider_registry_from_env()
                .expect("registry bootstrap should succeed")
                .expect("workflow config should enable registry");
            let targets = registry.targets();

            assert_eq!(targets.len(), 1);
            assert_eq!(targets[0].target.provider_id, "pantograph-qwen");
            assert_eq!(
                targets[0].target.model_id.as_deref(),
                Some("Qwen3.5-35B-A3B-GGUF")
            );
            assert_eq!(
                targets[0].target.profile_id.as_deref(),
                Some("remote-reasoning")
            );
            assert_eq!(
                targets[0].target.capability_tags,
                vec!["analysis".to_string(), "reasoning".to_string()]
            );
            assert_eq!(
                targets[0].target.metadata["source"],
                serde_json::json!("gestalt-pantograph-host")
            );
            assert_eq!(
                targets[0].target.metadata["workflow_id"],
                serde_json::json!("Qwen Reasoning")
            );
            assert!(registry.provider("pantograph-qwen").is_some());
        },
    );
}
