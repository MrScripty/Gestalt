use crate::git::{BranchInfo, CommitInfo, FileChange, GitError, TagInfo};

const LOG_FIELD_DELIMITER: char = '\u{1f}';
const LOG_GRAPH_DELIMITER: char = '\0';

pub(crate) fn parse_branches(output: &str) -> Vec<BranchInfo> {
    output
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                return None;
            }

            let is_current = trimmed.starts_with('*');
            let raw_name = trimmed.trim_start_matches(['*', ' ']).trim().to_string();
            if raw_name.is_empty() || raw_name.contains(" -> ") {
                return None;
            }

            let is_remote = raw_name.starts_with("remotes/");
            let name = raw_name
                .strip_prefix("remotes/")
                .unwrap_or(&raw_name)
                .to_string();

            Some(BranchInfo {
                name,
                is_current,
                is_remote,
            })
        })
        .collect()
}

pub(crate) fn parse_tags(output: &str) -> Vec<TagInfo> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| TagInfo {
            name: line.to_string(),
        })
        .collect()
}

pub(crate) fn parse_status_porcelain(output: &str) -> Vec<FileChange> {
    output
        .lines()
        .filter_map(|line| {
            if line.len() < 3 {
                return None;
            }

            if let Some(path) = line.strip_prefix("?? ") {
                return Some(FileChange {
                    path: path.trim().to_string(),
                    code: "??".to_string(),
                    is_staged: false,
                    is_unstaged: false,
                    is_untracked: true,
                });
            }

            let mut chars = line.chars();
            let x = chars.next()?;
            let y = chars.next()?;
            let path_raw = line.get(3..)?.trim();
            let path = rename_destination(path_raw);

            let is_staged = x != ' ' && x != '?';
            let is_unstaged = y != ' ';
            Some(FileChange {
                path,
                code: format!("{x}{y}"),
                is_staged,
                is_unstaged,
                is_untracked: false,
            })
        })
        .collect()
}

pub(crate) fn parse_graph_commits(output: &str) -> Result<Vec<CommitInfo>, GitError> {
    let mut commits = Vec::new();

    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let Some(split_idx) = line.find(LOG_GRAPH_DELIMITER) else {
            continue;
        };

        let graph_prefix = line[..split_idx].to_string();
        let payload = &line[split_idx + 1..];
        let fields = payload.split(LOG_FIELD_DELIMITER).collect::<Vec<_>>();
        if fields.len() < 6 {
            return Err(GitError::ParseError {
                command: "git log --graph".to_string(),
                details: format!("Expected 6 fields, found {} in line '{line}'", fields.len()),
            });
        }

        let decorations = fields[5]
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>();

        commits.push(CommitInfo {
            sha: fields[0].to_string(),
            short_sha: fields[1].to_string(),
            author: fields[2].to_string(),
            authored_at: fields[3].to_string(),
            subject: fields[4].to_string(),
            decorations,
            graph_prefix,
        });
    }

    Ok(commits)
}

fn rename_destination(path_raw: &str) -> String {
    path_raw
        .rsplit_once(" -> ")
        .map(|(_, destination)| destination.to_string())
        .unwrap_or_else(|| path_raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::{parse_graph_commits, parse_status_porcelain};

    #[test]
    fn parse_graph_commits_extracts_graph_prefix_and_fields() {
        let input = concat!(
            "* \0aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\u{1f}aaaaaaaa\u{1f}alice\u{1f}2026-03-01T10:00:00+00:00\u{1f}feat: add panel\u{1f}HEAD -> main\n",
            "| * \0bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\u{1f}bbbbbbbb\u{1f}bob\u{1f}2026-02-28T09:00:00+00:00\u{1f}fix: layout\u{1f}tag: v1.0.0\n"
        );

        let commits = parse_graph_commits(input).expect("parse should succeed");
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].graph_prefix, "* ");
        assert_eq!(commits[0].sha.len(), 40);
        assert_eq!(commits[1].graph_prefix, "| * ");
        assert_eq!(commits[1].decorations[0], "tag: v1.0.0");
    }

    #[test]
    fn parse_status_porcelain_maps_stage_bits() {
        let input = "M  src/ui.rs\n M src/state.rs\n?? docs/new.md\nR  old.rs -> new.rs\n";
        let changes = parse_status_porcelain(input);

        assert_eq!(changes.len(), 4);
        assert!(changes[0].is_staged);
        assert!(!changes[0].is_unstaged);
        assert!(!changes[2].is_staged);
        assert!(changes[2].is_untracked);
        assert_eq!(changes[3].path, "new.rs");
    }
}
