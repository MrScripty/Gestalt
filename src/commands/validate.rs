use std::collections::HashSet;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommandValidationError {
    #[error("Command name cannot be empty.")]
    EmptyName,
    #[error("Command prompt cannot be empty.")]
    EmptyPrompt,
}

pub fn validate_command_name(name: &str) -> Result<(), CommandValidationError> {
    if name.trim().is_empty() {
        return Err(CommandValidationError::EmptyName);
    }

    Ok(())
}

pub fn validate_command_prompt(prompt: &str) -> Result<(), CommandValidationError> {
    if prompt.trim().is_empty() {
        return Err(CommandValidationError::EmptyPrompt);
    }

    Ok(())
}

pub fn parse_tags_csv(tags_csv: &str) -> Vec<String> {
    let mut seen = HashSet::new();

    tags_csv
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .filter(|tag| seen.insert(tag.to_lowercase()))
        .map(|tag| tag.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{parse_tags_csv, validate_command_name, validate_command_prompt};

    #[test]
    fn parse_tags_trims_and_deduplicates() {
        let tags = parse_tags_csv(" git, deploy,git , release ");
        assert_eq!(tags, vec!["git", "deploy", "release"]);
    }

    #[test]
    fn validates_required_fields() {
        assert!(validate_command_name("  ").is_err());
        assert!(validate_command_prompt("").is_err());
        assert!(validate_command_name("Build").is_ok());
        assert!(validate_command_prompt("cargo build").is_ok());
    }
}
