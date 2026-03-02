mod matcher;
mod model;
mod validate;

pub use matcher::{CommandMatch, rank_commands};
pub use model::{CommandId, CommandLibrary, InsertCommand};
pub use validate::{
    CommandValidationError, parse_tags_csv, validate_command_name, validate_command_prompt,
};
