use super::*;
use std::collections::HashSet;

/// Durable notes, snippets, and embedding metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeState {
    #[serde(default)]
    pub(crate) notes: Vec<NoteDocument>,
    #[serde(default)]
    pub(crate) snippets: Vec<Snippet>,
    #[serde(default)]
    selected_note_id: Option<NoteId>,
    #[serde(default = "default_next_note_id")]
    next_note_id: NoteId,
    #[serde(default = "default_next_snippet_id")]
    next_snippet_id: SnippetId,
}

impl Default for KnowledgeState {
    fn default() -> Self {
        Self {
            notes: Vec::new(),
            snippets: Vec::new(),
            selected_note_id: None,
            next_note_id: 1,
            next_snippet_id: 1,
        }
    }
}

impl KnowledgeState {
    pub(crate) fn repair_after_restore(
        &mut self,
        valid_group_ids: &HashSet<GroupId>,
        fallback_group_id: GroupId,
    ) {
        for note in &mut self.notes {
            if note.group_id == 0 || !valid_group_ids.contains(&note.group_id) {
                note.group_id = fallback_group_id;
            }
            if note.title.trim().is_empty() {
                note.title = default_note_title();
            }
        }
        self.notes.retain(|note| !note.title.trim().is_empty());
        if self
            .selected_note_id
            .is_some_and(|selected| !self.notes.iter().any(|note| note.id == selected))
        {
            self.selected_note_id = None;
        }

        for snippet in &mut self.snippets {
            if snippet.log_ref.end_offset < snippet.log_ref.start_offset {
                std::mem::swap(
                    &mut snippet.log_ref.start_offset,
                    &mut snippet.log_ref.end_offset,
                );
            }
            if snippet.log_ref.end_row < snippet.log_ref.start_row {
                std::mem::swap(&mut snippet.log_ref.start_row, &mut snippet.log_ref.end_row);
            }
        }
        self.snippets
            .retain(|snippet| !snippet.text_snapshot_plain.trim().is_empty());

        let max_note = self.notes.iter().map(|note| note.id).max().unwrap_or(0);
        let max_snippet = self
            .snippets
            .iter()
            .map(|snippet| snippet.id)
            .max()
            .unwrap_or(0);
        self.next_note_id = self.next_note_id.max(max_note.saturating_add(1));
        self.next_snippet_id = self.next_snippet_id.max(max_snippet.saturating_add(1));
    }

    pub(crate) fn remove_notes_for_group(&mut self, group_id: GroupId) {
        self.notes.retain(|note| note.group_id != group_id);
    }

    pub fn notes(&self) -> &[NoteDocument] {
        &self.notes
    }

    pub fn notes_for_group(&self, group_id: GroupId) -> Vec<&NoteDocument> {
        self.notes
            .iter()
            .filter(|note| note.group_id == group_id)
            .collect()
    }

    pub fn note_by_id(&self, note_id: NoteId) -> Option<&NoteDocument> {
        self.notes.iter().find(|note| note.id == note_id)
    }

    pub fn selected_note_id(&self) -> Option<NoteId> {
        self.selected_note_id
    }

    pub fn selected_note_id_for_group(&self, group_id: GroupId) -> Option<NoteId> {
        if let Some(selected) = self.selected_note_id
            && self
                .notes
                .iter()
                .any(|note| note.id == selected && note.group_id == group_id)
        {
            return Some(selected);
        }
        self.notes
            .iter()
            .find(|note| note.group_id == group_id)
            .map(|note| note.id)
    }

    pub fn select_note(&mut self, note_id: NoteId) -> bool {
        if self.selected_note_id == Some(note_id) {
            return false;
        }
        if !self.notes.iter().any(|note| note.id == note_id) {
            return false;
        }
        self.selected_note_id = Some(note_id);
        true
    }

    pub fn snippets(&self) -> &[Snippet] {
        &self.snippets
    }

    pub fn snippet_by_id(&self, snippet_id: SnippetId) -> Option<&Snippet> {
        self.snippets
            .iter()
            .find(|snippet| snippet.id == snippet_id)
    }

    pub fn snippets_for_session(&self, session_id: SessionId) -> Vec<&Snippet> {
        self.snippets
            .iter()
            .filter(|snippet| snippet.log_ref.session_id == session_id)
            .collect()
    }

    pub fn create_note_for_group(
        &mut self,
        group_id: GroupId,
        title: String,
        updated_at_unix_ms: i64,
    ) -> NoteId {
        let trimmed = title.trim();
        let note_id = self.next_note_id;
        self.next_note_id = self.next_note_id.saturating_add(1);
        self.notes.push(NoteDocument {
            id: note_id,
            group_id,
            title: if trimmed.is_empty() {
                default_note_title()
            } else {
                trimmed.to_string()
            },
            markdown: String::new(),
            updated_at_unix_ms,
        });
        self.selected_note_id = Some(note_id);
        note_id
    }

    pub fn update_note_markdown(
        &mut self,
        note_id: NoteId,
        markdown: String,
        updated_at_unix_ms: i64,
    ) -> bool {
        let Some(note) = self.notes.iter_mut().find(|note| note.id == note_id) else {
            return false;
        };
        if note.markdown == markdown && note.updated_at_unix_ms == updated_at_unix_ms {
            return false;
        }
        note.markdown = markdown;
        note.updated_at_unix_ms = updated_at_unix_ms;
        true
    }

    pub fn append_note_snippet_reference(
        &mut self,
        note_id: NoteId,
        snippet_id: SnippetId,
        updated_at_unix_ms: i64,
    ) -> bool {
        let Some(note) = self.notes.iter_mut().find(|note| note.id == note_id) else {
            return false;
        };
        let token = snippet_reference_token(snippet_id);
        if !note.markdown.is_empty() && !note.markdown.ends_with('\n') {
            note.markdown.push('\n');
        }
        note.markdown.push_str(&token);
        note.markdown.push('\n');
        note.updated_at_unix_ms = updated_at_unix_ms;
        true
    }

    pub fn create_snippet(&mut self, new_snippet: NewSnippet) -> SnippetId {
        let snippet_id = self.next_snippet_id;
        self.next_snippet_id = self.next_snippet_id.saturating_add(1);
        let log_ref = SnippetLogRef {
            session_id: new_snippet.source_session_id,
            stream_id: new_snippet.source_stream_id,
            start_offset: new_snippet.start_offset.min(new_snippet.end_offset),
            end_offset: new_snippet.start_offset.max(new_snippet.end_offset),
            start_row: new_snippet.start_row.min(new_snippet.end_row),
            end_row: new_snippet.start_row.max(new_snippet.end_row),
        };
        self.snippets.insert(
            0,
            Snippet {
                id: snippet_id,
                created_at_unix_ms: new_snippet.created_at_unix_ms,
                source_cwd: new_snippet.source_cwd,
                text_snapshot_plain: new_snippet.text_snapshot_plain,
                log_ref,
                embedding_status: SnippetEmbeddingStatus::Pending,
                embedding_object_id: None,
                embedding_profile_id: None,
                embedding_dimensions: None,
                embedding_error: None,
            },
        );
        snippet_id
    }

    pub fn promote_snippet(&mut self, snippet_id: SnippetId) -> bool {
        let Some(index) = self
            .snippets
            .iter()
            .position(|snippet| snippet.id == snippet_id)
        else {
            return false;
        };
        if index == 0 {
            return false;
        }
        let snippet = self.snippets.remove(index);
        self.snippets.insert(0, snippet);
        true
    }

    pub fn delete_snippet(&mut self, snippet_id: SnippetId) -> bool {
        let before_len = self.snippets.len();
        self.snippets.retain(|snippet| snippet.id != snippet_id);
        before_len != self.snippets.len()
    }

    pub fn set_snippet_embedding_processing(&mut self, snippet_id: SnippetId) -> bool {
        let Some(snippet) = self
            .snippets
            .iter_mut()
            .find(|snippet| snippet.id == snippet_id)
        else {
            return false;
        };
        if snippet.embedding_status == SnippetEmbeddingStatus::Processing {
            return false;
        }
        snippet.embedding_status = SnippetEmbeddingStatus::Processing;
        snippet.embedding_error = None;
        true
    }

    pub fn set_snippet_embedding_ready(
        &mut self,
        snippet_id: SnippetId,
        embedding_object_id: String,
        embedding_profile_id: Option<String>,
        embedding_dimensions: Option<usize>,
    ) -> bool {
        let Some(snippet) = self
            .snippets
            .iter_mut()
            .find(|snippet| snippet.id == snippet_id)
        else {
            return false;
        };
        snippet.embedding_status = SnippetEmbeddingStatus::Ready;
        snippet.embedding_object_id = Some(embedding_object_id);
        snippet.embedding_profile_id = embedding_profile_id;
        snippet.embedding_dimensions = embedding_dimensions;
        snippet.embedding_error = None;
        true
    }

    pub fn set_snippet_embedding_failed(&mut self, snippet_id: SnippetId, error: String) -> bool {
        let Some(snippet) = self
            .snippets
            .iter_mut()
            .find(|snippet| snippet.id == snippet_id)
        else {
            return false;
        };
        snippet.embedding_status = SnippetEmbeddingStatus::Failed;
        snippet.embedding_error = Some(error);
        true
    }
}

impl AppState {
    /// Returns all notes in insertion order.
    pub fn notes(&self) -> &[NoteDocument] {
        self.knowledge.notes()
    }

    /// Returns notes for one path group in insertion order.
    pub fn notes_for_group(&self, group_id: GroupId) -> Vec<&NoteDocument> {
        self.knowledge.notes_for_group(group_id)
    }

    /// Returns a note by identifier.
    pub fn note_by_id(&self, note_id: NoteId) -> Option<&NoteDocument> {
        self.knowledge.note_by_id(note_id)
    }

    /// Returns the selected note identifier.
    pub fn selected_note_id(&self) -> Option<NoteId> {
        self.knowledge.selected_note_id()
    }

    /// Returns selected note for one group, falling back to first note in group.
    pub fn selected_note_id_for_group(&self, group_id: GroupId) -> Option<NoteId> {
        self.knowledge.selected_note_id_for_group(group_id)
    }

    /// Selects the active note.
    pub fn select_note(&mut self, note_id: NoteId) {
        if self.knowledge.select_note(note_id) {
            self.mark_dirty();
        }
    }

    /// Returns all snippets in display order (newest and promoted first).
    pub fn snippets(&self) -> &[Snippet] {
        self.knowledge.snippets()
    }

    /// Returns a snippet by identifier.
    pub fn snippet_by_id(&self, snippet_id: SnippetId) -> Option<&Snippet> {
        self.knowledge.snippet_by_id(snippet_id)
    }

    /// Returns snippets originating from one terminal session.
    pub fn snippets_for_session(&self, session_id: SessionId) -> Vec<&Snippet> {
        self.knowledge.snippets_for_session(session_id)
    }

    /// Creates an empty note and returns its identifier.
    pub fn create_note_for_group(
        &mut self,
        group_id: GroupId,
        title: String,
        updated_at_unix_ms: i64,
    ) -> NoteId {
        let note_id = self
            .knowledge
            .create_note_for_group(group_id, title, updated_at_unix_ms);
        self.mark_dirty();
        note_id
    }

    /// Creates an empty note in the active path group.
    pub fn create_note(&mut self, title: String, updated_at_unix_ms: i64) -> Option<NoteId> {
        let group_id = self.active_group_id()?;
        Some(self.create_note_for_group(group_id, title, updated_at_unix_ms))
    }

    /// Updates note markdown and touched timestamp.
    pub fn update_note_markdown(
        &mut self,
        note_id: NoteId,
        markdown: String,
        updated_at_unix_ms: i64,
    ) -> bool {
        let updated = self
            .knowledge
            .update_note_markdown(note_id, markdown, updated_at_unix_ms);
        if updated {
            self.mark_dirty();
        }
        updated
    }

    /// Appends a markdown snippet reference token to a note.
    pub fn append_note_snippet_reference(
        &mut self,
        note_id: NoteId,
        snippet_id: SnippetId,
        updated_at_unix_ms: i64,
    ) -> bool {
        let updated =
            self.knowledge
                .append_note_snippet_reference(note_id, snippet_id, updated_at_unix_ms);
        if updated {
            self.mark_dirty();
        }
        updated
    }

    /// Creates a snippet and returns its identifier.
    pub fn create_snippet(&mut self, new_snippet: NewSnippet) -> SnippetId {
        let snippet_id = self.knowledge.create_snippet(new_snippet);
        self.mark_dirty();
        snippet_id
    }

    /// Promotes one snippet to the top of the snippets list.
    pub fn promote_snippet(&mut self, snippet_id: SnippetId) -> bool {
        let promoted = self.knowledge.promote_snippet(snippet_id);
        if promoted {
            self.mark_dirty();
        }
        promoted
    }

    /// Deletes one snippet object by identifier.
    pub fn delete_snippet(&mut self, snippet_id: SnippetId) -> bool {
        let deleted = self.knowledge.delete_snippet(snippet_id);
        if deleted {
            self.mark_dirty();
        }
        deleted
    }

    /// Marks snippet embedding as processing.
    pub fn set_snippet_embedding_processing(&mut self, snippet_id: SnippetId) -> bool {
        let updated = self.knowledge.set_snippet_embedding_processing(snippet_id);
        if updated {
            self.mark_dirty();
        }
        updated
    }

    /// Marks snippet embedding as ready.
    pub fn set_snippet_embedding_ready(
        &mut self,
        snippet_id: SnippetId,
        embedding_object_id: String,
        embedding_profile_id: Option<String>,
        embedding_dimensions: Option<usize>,
    ) -> bool {
        let updated = self.knowledge.set_snippet_embedding_ready(
            snippet_id,
            embedding_object_id,
            embedding_profile_id,
            embedding_dimensions,
        );
        if updated {
            self.mark_dirty();
        }
        updated
    }

    /// Marks snippet embedding as failed.
    pub fn set_snippet_embedding_failed(&mut self, snippet_id: SnippetId, error: String) -> bool {
        let updated = self
            .knowledge
            .set_snippet_embedding_failed(snippet_id, error);
        if updated {
            self.mark_dirty();
        }
        updated
    }
}
