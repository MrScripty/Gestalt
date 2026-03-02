use crate::git::{BranchInfo, CommitInfo, FileChange, GitError, TagInfo};
use std::collections::HashSet;

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
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }

            let fields = trimmed.split('\t').collect::<Vec<_>>();
            if fields.len() == 2 {
                return Some(TagInfo {
                    name: fields[0].trim().to_string(),
                    target_sha: fields[1].trim().to_string(),
                    annotated: false,
                });
            }
            if fields.len() >= 3 {
                let object_name = fields[1].trim();
                let peeled_name = fields[2].trim();
                let annotated = !peeled_name.is_empty();
                let target_sha = if annotated { peeled_name } else { object_name };
                return Some(TagInfo {
                    name: fields[0].trim().to_string(),
                    target_sha: target_sha.to_string(),
                    annotated,
                });
            }

            Some(TagInfo {
                name: trimmed.to_string(),
                target_sha: String::new(),
                annotated: false,
            })
        })
        .collect()
}

pub(crate) fn parse_status_porcelain(output: &str) -> Vec<FileChange> {
    output
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_end();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
                return None;
            }

            if let Some(path) = trimmed.strip_prefix("?? ") {
                return Some(FileChange {
                    path: path.trim().to_string(),
                    code: "??".to_string(),
                    is_staged: false,
                    is_unstaged: false,
                    is_untracked: true,
                });
            }
            if let Some(path) = trimmed.strip_prefix("? ") {
                return Some(FileChange {
                    path: path.trim().to_string(),
                    code: "??".to_string(),
                    is_staged: false,
                    is_unstaged: false,
                    is_untracked: true,
                });
            }

            if let Some(rest) = trimmed
                .strip_prefix("1 ")
                .or_else(|| trimmed.strip_prefix("2 "))
            {
                let fields = rest.split_whitespace().collect::<Vec<_>>();
                if fields.len() < 2 {
                    return None;
                }

                let code = fields[0];
                let mut code_chars = code.chars();
                let x = code_chars.next().unwrap_or(' ');
                let y = code_chars.next().unwrap_or(' ');
                let path = if trimmed.starts_with("2 ") && fields.len() >= 2 {
                    let idx = fields.len().saturating_sub(2);
                    rename_destination(fields[idx])
                } else {
                    fields
                        .last()
                        .map_or_else(String::new, |raw| rename_destination(raw))
                };

                return Some(FileChange {
                    path,
                    code: code.to_string(),
                    is_staged: x != '.' && x != ' ' && x != '?',
                    is_unstaged: y != '.' && y != ' ',
                    is_untracked: false,
                });
            }

            if trimmed.len() < 3 {
                return None;
            }

            let mut chars = trimmed.chars();
            let x = chars.next()?;
            let y = chars.next()?;
            let path_raw = trimmed.get(3..)?.trim();
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
            body_preview: fields[4].to_string(),
            decorations,
            graph_prefix,
        });
    }

    Ok(commits)
}

pub(crate) fn parse_status_with_ignored(output: &str) -> (HashSet<String>, HashSet<String>) {
    let mut modified_paths = HashSet::new();
    let mut ignored_paths = HashSet::new();

    for line in output.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() || trimmed.starts_with("##") {
            continue;
        }

        if let Some(path) = trimmed.strip_prefix("!! ") {
            let normalized = normalize_status_path(path);
            if !normalized.is_empty() {
                ignored_paths.insert(normalized);
            }
            continue;
        }

        if let Some(path) = trimmed.strip_prefix("?? ") {
            let normalized = normalize_status_path(path);
            if !normalized.is_empty() {
                modified_paths.insert(normalized);
            }
            continue;
        }

        if trimmed.len() < 4 {
            continue;
        }

        let status = &trimmed[..2];
        if status == "  " {
            continue;
        }

        let raw_path = trimmed[3..].trim();
        if raw_path.is_empty() {
            continue;
        }

        let normalized = normalize_status_path(raw_path);
        if !normalized.is_empty() {
            modified_paths.insert(normalized);
        }
    }

    (modified_paths, ignored_paths)
}

pub(crate) fn normalize_status_path(raw_path: &str) -> String {
    let renamed = raw_path
        .rsplit_once(" -> ")
        .map(|(_, destination)| destination)
        .unwrap_or(raw_path);

    renamed
        .trim()
        .trim_matches('"')
        .trim_end_matches('/')
        .replace('\\', "/")
}

fn rename_destination(path_raw: &str) -> String {
    path_raw
        .rsplit_once(" -> ")
        .map(|(_, destination)| destination.to_string())
        .unwrap_or_else(|| path_raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_status_path, parse_branches, parse_graph_commits, parse_status_porcelain,
        parse_status_with_ignored, parse_tags,
    };

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
        assert_eq!(commits[0].body_preview, "feat: add panel");
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

    #[test]
    fn parse_branches_handles_current_remote_and_symbolic_refs() {
        let input =
            "* main\n  feature/a\n  remotes/origin/main\n  remotes/origin/HEAD -> origin/main\n";
        let branches = parse_branches(input);

        assert_eq!(branches.len(), 3);
        assert!(branches[0].is_current);
        assert_eq!(branches[0].name, "main");
        assert!(!branches[0].is_remote);
        assert!(branches[2].is_remote);
        assert_eq!(branches[2].name, "origin/main");
    }

    #[test]
    fn parse_tags_skips_blank_lines() {
        let input = concat!(
            "v1.0.0\taaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\t\n",
            "v1.1.0\tbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\tcccccccccccccccccccccccccccccccccccccccc\n"
        );
        let tags = parse_tags(input);
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].name, "v1.0.0");
        assert_eq!(tags[1].name, "v1.1.0");
        assert!(!tags[0].annotated);
        assert!(tags[1].annotated);
        assert_eq!(tags[1].target_sha.len(), 40);
    }

    #[test]
    fn parse_status_with_ignored_tracks_modified_and_ignored_paths() {
        let input = concat!(
            "## main\n",
            " M src/main.rs\n",
            "?? src/new.rs\n",
            "!! target/\n",
            "R  old/name.rs -> src/new_name.rs\n"
        );
        let (modified, ignored) = parse_status_with_ignored(input);

        assert!(modified.contains("src/main.rs"));
        assert!(modified.contains("src/new.rs"));
        assert!(modified.contains("src/new_name.rs"));
        assert!(ignored.contains("target"));
    }

    #[test]
    fn normalize_status_path_extracts_rename_destination() {
        assert_eq!(
            normalize_status_path("old/path.rs -> src/new/path.rs"),
            "src/new/path.rs"
        );
        assert_eq!(normalize_status_path("target/"), "target");
    }
}
