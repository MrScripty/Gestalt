use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

pub type CommandId = u32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InsertCommand {
    pub id: CommandId,
    pub name: String,
    pub prompt: String,
    pub description: String,
    pub tags: Vec<String>,
    pub updated_at_unix: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CommandLibrary {
    pub commands: Vec<InsertCommand>,
    pub next_command_id: CommandId,
}

impl CommandLibrary {
    pub fn create(
        &mut self,
        name: String,
        prompt: String,
        description: String,
        tags: Vec<String>,
    ) -> CommandId {
        self.ensure_next_id_initialized();
        let id = self.next_command_id;
        self.next_command_id = self.next_command_id.saturating_add(1);

        self.commands.push(InsertCommand {
            id,
            name,
            prompt,
            description,
            tags,
            updated_at_unix: unix_timestamp_seconds(),
        });

        id
    }

    pub fn update(
        &mut self,
        id: CommandId,
        name: String,
        prompt: String,
        description: String,
        tags: Vec<String>,
    ) -> bool {
        let Some(command) = self.commands.iter_mut().find(|command| command.id == id) else {
            return false;
        };

        let changed = command.name != name
            || command.prompt != prompt
            || command.description != description
            || command.tags != tags;
        if !changed {
            return false;
        }

        command.name = name;
        command.prompt = prompt;
        command.description = description;
        command.tags = tags;
        command.updated_at_unix = unix_timestamp_seconds();
        true
    }

    pub fn delete(&mut self, id: CommandId) -> bool {
        let before = self.commands.len();
        self.commands.retain(|command| command.id != id);
        before != self.commands.len()
    }

    pub fn command(&self, id: CommandId) -> Option<&InsertCommand> {
        self.commands.iter().find(|command| command.id == id)
    }

    pub fn repair_after_restore(&mut self) {
        self.ensure_next_id_initialized();

        let mut seen = HashSet::new();
        self.commands.retain(|command| seen.insert(command.id));

        let max_id = self
            .commands
            .iter()
            .map(|command| command.id)
            .max()
            .unwrap_or(0);
        self.next_command_id = self.next_command_id.max(max_id.saturating_add(1));
    }

    fn ensure_next_id_initialized(&mut self) {
        if self.next_command_id == 0 {
            let max_id = self
                .commands
                .iter()
                .map(|command| command.id)
                .max()
                .unwrap_or(0);
            self.next_command_id = max_id.saturating_add(1);
        }
    }
}

fn unix_timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::{CommandLibrary, InsertCommand};

    #[test]
    fn repair_initializes_next_id_for_empty_library() {
        let mut library = CommandLibrary::default();
        library.repair_after_restore();
        assert_eq!(library.next_command_id, 1);
    }

    #[test]
    fn repair_drops_duplicate_ids() {
        let mut library = CommandLibrary {
            commands: vec![
                InsertCommand {
                    id: 7,
                    name: "one".to_string(),
                    prompt: "echo one".to_string(),
                    description: String::new(),
                    tags: vec![],
                    updated_at_unix: 1,
                },
                InsertCommand {
                    id: 7,
                    name: "two".to_string(),
                    prompt: "echo two".to_string(),
                    description: String::new(),
                    tags: vec![],
                    updated_at_unix: 2,
                },
            ],
            next_command_id: 0,
        };

        library.repair_after_restore();
        assert_eq!(library.commands.len(), 1);
        assert_eq!(library.commands[0].name, "one");
        assert_eq!(library.next_command_id, 8);
    }
}
