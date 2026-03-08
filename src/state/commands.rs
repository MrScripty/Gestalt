use super::*;
use crate::commands::CommandLibrary;

/// Durable insert-command state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommandState {
    #[serde(default)]
    pub(crate) command_library: CommandLibrary,
}

impl CommandState {
    pub(crate) fn repair_after_restore(&mut self) {
        self.command_library.repair_after_restore();
    }

    pub fn commands(&self) -> &[InsertCommand] {
        &self.command_library.commands
    }

    pub fn command_by_id(&self, command_id: CommandId) -> Option<&InsertCommand> {
        self.command_library.command(command_id)
    }

    pub fn create_insert_command(
        &mut self,
        name: String,
        prompt: String,
        description: String,
        tags: Vec<String>,
    ) -> CommandId {
        self.command_library.create(name, prompt, description, tags)
    }

    pub fn update_insert_command(
        &mut self,
        command_id: CommandId,
        name: String,
        prompt: String,
        description: String,
        tags: Vec<String>,
    ) -> bool {
        self.command_library
            .update(command_id, name, prompt, description, tags)
    }

    pub fn delete_insert_command(&mut self, command_id: CommandId) -> bool {
        self.command_library.delete(command_id)
    }
}

impl AppState {
    /// Returns true when the insert-command library has at least one command.
    pub fn has_commands(&self) -> bool {
        !self.commands().is_empty()
    }

    pub(crate) fn seed_commands_from(&mut self, other: &AppState) -> bool {
        if self.has_commands() || !other.has_commands() {
            return false;
        }

        self.commands.command_library = other.commands.command_library.clone();
        true
    }

    /// Returns all insert commands in insertion order.
    pub fn commands(&self) -> &[InsertCommand] {
        self.commands.commands()
    }

    /// Returns a command by identifier.
    pub fn command_by_id(&self, command_id: CommandId) -> Option<&InsertCommand> {
        self.commands.command_by_id(command_id)
    }

    /// Creates an insert command and returns its identifier.
    pub fn create_insert_command(
        &mut self,
        name: String,
        prompt: String,
        description: String,
        tags: Vec<String>,
    ) -> CommandId {
        let id = self
            .commands
            .create_insert_command(name, prompt, description, tags);
        self.mark_dirty();
        id
    }

    /// Updates an existing insert command. Returns true on mutation.
    pub fn update_insert_command(
        &mut self,
        command_id: CommandId,
        name: String,
        prompt: String,
        description: String,
        tags: Vec<String>,
    ) -> bool {
        let updated =
            self.commands
                .update_insert_command(command_id, name, prompt, description, tags);
        if updated {
            self.mark_dirty();
        }
        updated
    }

    /// Deletes an existing insert command. Returns true when removed.
    pub fn delete_insert_command(&mut self, command_id: CommandId) -> bool {
        let removed = self.commands.delete_insert_command(command_id);
        if removed {
            self.mark_dirty();
        }
        removed
    }
}
