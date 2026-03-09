use gestalt::pantograph_host::validate_embedding_roundtrip_from_env;

const DEFAULT_TEXT: &str =
    "Gestalt Emily Pantograph embedding validation text for Qwen3-Embedding-4B-GGUF.";

fn main() -> Result<(), String> {
    let text = parse_cli(std::env::args().skip(1).collect())?;
    let report = validate_embedding_roundtrip_from_env(&text)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&report)
            .map_err(|error| format!("failed serializing embedding validation report: {error}"))?
    );
    Ok(())
}

fn parse_cli(args: Vec<String>) -> Result<String, String> {
    let mut text = DEFAULT_TEXT.to_string();
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--text" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    return Err("missing value after --text".to_string());
                };
                text = value.clone();
            }
            "--help" | "-h" => return Err(usage()),
            _ => {
                if let Some(value) = arg.strip_prefix("--text=") {
                    text = value.to_string();
                } else {
                    return Err(format!("unrecognized argument '{arg}'\n\n{}", usage()));
                }
            }
        }
        index += 1;
    }

    if text.trim().is_empty() {
        return Err("--text cannot be empty".to_string());
    }

    Ok(text)
}

fn usage() -> String {
    format!(
        "Usage: cargo run --bin emily_pantograph_embedding_probe -- [--text TEXT]\n\
         Default text: {DEFAULT_TEXT}"
    )
}
