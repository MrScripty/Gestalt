use crate::git::GitError;
use std::process::Command;

#[derive(Debug, Clone)]
pub(crate) struct CommandOutput {
    pub stdout: String,
}

pub(crate) fn run_git<S>(cwd: &str, args: &[S]) -> Result<CommandOutput, GitError>
where
    S: AsRef<str>,
{
    let args_owned = args
        .iter()
        .map(|arg| arg.as_ref().to_string())
        .collect::<Vec<_>>();
    let mut command = Command::new("git");
    command.current_dir(cwd).args(&args_owned);

    let output = command.output().map_err(|error| GitError::Io {
        details: error.to_string(),
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if output.status.success() {
        return Ok(CommandOutput { stdout });
    }

    let command_text = command_text(&args_owned);
    if is_not_repo_error(&stderr) {
        return Err(GitError::NotRepo {
            path: cwd.to_string(),
        });
    }

    Err(GitError::CommandFailed {
        command: command_text,
        code: output.status.code(),
        stderr,
    })
}

fn command_text(args: &[String]) -> String {
    if args.is_empty() {
        return "git".to_string();
    }

    format!("git {}", args.join(" "))
}

fn is_not_repo_error(stderr: &str) -> bool {
    let normalized = stderr.to_lowercase();
    normalized.contains("not a git repository")
}
