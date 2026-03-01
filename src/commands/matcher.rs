use crate::commands::model::{CommandId, InsertCommand};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandMatch {
    pub command_id: CommandId,
    pub score: u16,
    pub name_indices: Vec<usize>,
    pub prompt_indices: Vec<usize>,
}

pub fn rank_commands(commands: &[InsertCommand], query: &str, limit: usize) -> Vec<CommandMatch> {
    if commands.is_empty() || limit == 0 {
        return Vec::new();
    }

    let normalized_query = query.trim().to_lowercase();
    let mut scored = commands
        .iter()
        .filter_map(|command| score_command(command, &normalized_query))
        .collect::<Vec<_>>();

    scored.sort_by(|left, right| {
        left.0
            .score
            .cmp(&right.0.score)
            .then_with(|| right.1.cmp(&left.1))
            .then_with(|| left.2.cmp(&right.2))
    });

    scored
        .into_iter()
        .take(limit)
        .map(|(entry, _, _)| entry)
        .collect()
}

fn score_command(command: &InsertCommand, query: &str) -> Option<(CommandMatch, u64, String)> {
    if query.is_empty() {
        return Some((
            CommandMatch {
                command_id: command.id,
                score: 999,
                name_indices: Vec::new(),
                prompt_indices: Vec::new(),
            },
            command.updated_at_unix,
            command.name.to_lowercase(),
        ));
    }

    let name = command.name.to_lowercase();
    let prompt = command.prompt.to_lowercase();
    let description = command.description.to_lowercase();
    let tags_joined = command
        .tags
        .iter()
        .map(|tag| tag.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");

    let token_prefix = name
        .split_whitespace()
        .any(|token| token.starts_with(query));
    let name_pos = name.find(query);
    let prompt_pos = prompt.find(query);
    let desc_pos = description.find(query);
    let tag_pos = tags_joined.find(query);

    let score = if name == query {
        0
    } else if name.starts_with(query) {
        10
    } else if token_prefix {
        20
    } else if name_pos.is_some() {
        30
    } else if desc_pos.is_some() || tag_pos.is_some() {
        40
    } else if prompt_pos.is_some() {
        50
    } else {
        return None;
    };

    let name_indices = contiguous_indices(name_pos, query);
    let prompt_indices = contiguous_indices(prompt_pos, query);

    Some((
        CommandMatch {
            command_id: command.id,
            score,
            name_indices,
            prompt_indices,
        },
        command.updated_at_unix,
        name,
    ))
}

fn contiguous_indices(start: Option<usize>, needle: &str) -> Vec<usize> {
    let Some(start) = start else {
        return Vec::new();
    };

    let len = needle.chars().count();
    (start..start.saturating_add(len)).collect()
}

#[cfg(test)]
mod tests {
    use super::rank_commands;
    use crate::commands::model::InsertCommand;

    fn cmd(id: u32, name: &str, prompt: &str, updated_at_unix: u64) -> InsertCommand {
        InsertCommand {
            id,
            name: name.to_string(),
            prompt: prompt.to_string(),
            description: String::new(),
            tags: Vec::new(),
            updated_at_unix,
        }
    }

    #[test]
    fn exact_name_ranks_first() {
        let commands = vec![
            cmd(1, "build project", "cargo build", 1),
            cmd(2, "build", "cargo build --release", 2),
        ];

        let matches = rank_commands(&commands, "build", 10);
        assert_eq!(matches[0].command_id, 2);
        assert_eq!(matches[0].score, 0);
    }

    #[test]
    fn recency_breaks_ties() {
        let commands = vec![
            cmd(1, "logs", "tail -f app.log", 10),
            cmd(2, "logs", "journalctl -f", 20),
        ];

        let matches = rank_commands(&commands, "logs", 10);
        assert_eq!(matches[0].command_id, 2);
        assert_eq!(matches[1].command_id, 1);
    }

    #[test]
    fn empty_query_returns_recent_commands() {
        let commands = vec![
            cmd(1, "one", "echo one", 1),
            cmd(2, "two", "echo two", 2),
            cmd(3, "three", "echo three", 3),
        ];

        let matches = rank_commands(&commands, "", 2);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].command_id, 3);
        assert_eq!(matches[1].command_id, 2);
    }
}
