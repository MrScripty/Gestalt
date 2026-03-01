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

#[cfg(test)]
mod tests {
    use super::run_git;
    use crate::git::GitError;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn run_git_maps_non_repo_error() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let path = std::env::temp_dir().join(format!("gestalt-git-nonrepo-{nonce}"));
        std::fs::create_dir_all(&path).expect("temp dir should be created");

        let result = run_git(
            path.to_str().expect("temp path to be valid UTF-8"),
            &["status"],
        );
        let _ = std::fs::remove_dir_all(&path);

        match result {
            Err(GitError::NotRepo { .. }) => {}
            other => panic!("expected NotRepo, got {other:?}"),
        }
    }
}
