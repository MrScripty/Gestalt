use crate::contracts::{
    MembraneProtectedKind, MembraneProtectedReference, MembraneProtectionDisposition,
};
use std::collections::BTreeMap;

const SECRET_MARKERS: &[&str] = &[
    "api_key",
    "password",
    "token",
    "secret",
    "authorization:",
    "-----begin",
    "sk-",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProtectedTextReport {
    pub outbound_text: String,
    pub protected_references: Vec<MembraneProtectedReference>,
}

pub(super) fn protect_outbound_text(reference_id: &str, text: &str) -> ProtectedTextReport {
    if contains_secret_marker(text) {
        let placeholder = format!("[BLOCKED_SECRET:{}]", reference_id);
        return ProtectedTextReport {
            outbound_text: placeholder.clone(),
            protected_references: vec![MembraneProtectedReference {
                reference_id: reference_id.to_string(),
                kind: MembraneProtectedKind::Secret,
                disposition: MembraneProtectionDisposition::Blocked,
                placeholder,
                local_text: None,
            }],
        };
    }

    let mut outbound = text.to_string();
    let mut protected_references = Vec::new();
    let mut email_placeholders: BTreeMap<String, String> = BTreeMap::new();
    let mut path_placeholders: BTreeMap<String, String> = BTreeMap::new();

    let tokens: Vec<&str> = text.split_whitespace().collect();
    let mut index = 0;

    while index < tokens.len() {
        let token = tokens[index];
        let normalized = normalize_token(token);
        if normalized.is_empty() {
            index += 1;
            continue;
        }

        if is_email_token(normalized) {
            let placeholder = if let Some(existing) = email_placeholders.get(normalized) {
                existing.clone()
            } else {
                let created = format!(
                    "EMAIL_HANDLE_{}",
                    email_placeholders.len().saturating_add(1)
                );
                email_placeholders.insert(normalized.to_string(), created.clone());
                created
            };
            outbound = outbound.replace(normalized, &placeholder);
            protected_references.push(MembraneProtectedReference {
                reference_id: reference_id.to_string(),
                kind: MembraneProtectedKind::PersonalIdentifier,
                disposition: MembraneProtectionDisposition::Transformed,
                placeholder,
                local_text: Some(normalized.to_string()),
            });
            index += 1;
            continue;
        }

        if is_path_token(normalized) {
            let path_text = collect_path_text(&tokens, index);
            let placeholder = if let Some(existing) = path_placeholders.get(path_text.as_str()) {
                existing.clone()
            } else {
                let created = format!("PATH_HANDLE_{}", path_placeholders.len().saturating_add(1));
                path_placeholders.insert(path_text.clone(), created.clone());
                created
            };
            outbound = outbound.replace(&path_text, &placeholder);
            protected_references.push(MembraneProtectedReference {
                reference_id: reference_id.to_string(),
                kind: MembraneProtectedKind::FilesystemPath,
                disposition: MembraneProtectionDisposition::Transformed,
                placeholder,
                local_text: Some(path_text),
            });
            index += 1;
            continue;
        }

        index += 1;
    }

    ProtectedTextReport {
        outbound_text: outbound,
        protected_references,
    }
}

fn contains_secret_marker(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    SECRET_MARKERS.iter().any(|marker| lowered.contains(marker))
}

fn normalize_token(token: &str) -> &str {
    token.trim_matches(|character: char| {
        matches!(
            character,
            ',' | '.' | ';' | ':' | '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}'
        )
    })
}

fn is_email_token(token: &str) -> bool {
    let mut parts = token.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    !local.is_empty()
        && !domain.is_empty()
        && parts.next().is_none()
        && domain.contains('.')
        && !domain.ends_with('.')
}

fn is_path_token(token: &str) -> bool {
    token.starts_with("/home/")
        || token.starts_with("/media/")
        || token.starts_with("/tmp/")
        || token.starts_with("~/")
}

fn collect_path_text(tokens: &[&str], start_index: usize) -> String {
    let mut path_parts = vec![normalize_token(tokens[start_index]).to_string()];
    let mut cursor = start_index + 1;

    while cursor < tokens.len() {
        let normalized = normalize_token(tokens[cursor]);
        if normalized.is_empty() || !normalized.contains('/') {
            break;
        }
        path_parts.push(normalized.to_string());
        cursor += 1;
    }

    path_parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::protect_outbound_text;
    use crate::contracts::{MembraneProtectedKind, MembraneProtectionDisposition};

    #[test]
    fn protect_outbound_text_blocks_secret_markers() {
        let report = protect_outbound_text("task-1", "Use API_KEY=abcd1234 for the request.");

        assert_eq!(report.outbound_text, "[BLOCKED_SECRET:task-1]");
        assert_eq!(report.protected_references.len(), 1);
        assert_eq!(
            report.protected_references[0].kind,
            MembraneProtectedKind::Secret
        );
        assert_eq!(
            report.protected_references[0].disposition,
            MembraneProtectionDisposition::Blocked
        );
        assert_eq!(report.protected_references[0].local_text, None);
    }

    #[test]
    fn protect_outbound_text_transforms_emails_and_paths() {
        let report = protect_outbound_text(
            "ctx-1",
            "Email jeremy@example.com about /media/jeremy/OrangeCream/Linux Software/Gestalt.",
        );

        assert!(report.outbound_text.contains("EMAIL_HANDLE_1"));
        assert!(report.outbound_text.contains("PATH_HANDLE_1"));
        assert_eq!(report.protected_references.len(), 2);
        assert!(report.protected_references.iter().any(|reference| {
            reference.kind == MembraneProtectedKind::PersonalIdentifier
                && reference.disposition == MembraneProtectionDisposition::Transformed
                && reference.local_text.as_deref() == Some("jeremy@example.com")
        }));
        assert!(report.protected_references.iter().any(|reference| {
            reference.kind == MembraneProtectedKind::FilesystemPath
                && reference.disposition == MembraneProtectionDisposition::Transformed
                && reference.local_text.as_deref()
                    == Some("/media/jeremy/OrangeCream/Linux Software/Gestalt")
        }));
    }
}
