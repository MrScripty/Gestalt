use super::*;
use std::collections::HashSet;

/// Durable workspace topology and selection state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceState {
    pub(crate) sessions: Vec<Session>,
    pub(crate) groups: Vec<TabGroup>,
    #[serde(default = "default_ui_scale")]
    ui_scale: f64,
    #[serde(default)]
    auxiliary_panels: AuxiliaryPanelLayout,
    pub(crate) selected_session: Option<SessionId>,
    next_session_id: SessionId,
    next_group_id: GroupId,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            sessions: Vec::new(),
            groups: Vec::new(),
            ui_scale: UI_SCALE_DEFAULT,
            auxiliary_panels: AuxiliaryPanelLayout::default(),
            selected_session: None,
            next_session_id: 1,
            next_group_id: 1,
        }
    }
}

impl WorkspaceState {
    const PALETTE: [&'static str; 8] = [
        "#f4a261", "#2a9d8f", "#457b9d", "#e76f51", "#8ab17d", "#e9c46a", "#264653", "#219ebc",
    ];

    pub fn sessions(&self) -> &[Session] {
        &self.sessions
    }

    pub fn groups(&self) -> &[TabGroup] {
        &self.groups
    }

    pub fn selected_session(&self) -> Option<SessionId> {
        self.selected_session
    }

    pub(crate) fn valid_group_ids(&self) -> HashSet<GroupId> {
        self.groups.iter().map(|group| group.id).collect()
    }

    pub(crate) fn repair_after_restore(&mut self) {
        self.ui_scale = clamp_ui_scale(self.ui_scale);
        self.auxiliary_panels = self.auxiliary_panels.clone().normalized();
        self.groups.retain(|group| !group.path.trim().is_empty());
        for group in &mut self.groups {
            group.layout = group.layout.normalized();
        }

        if self.groups.is_empty() {
            let (_, ids) = self.create_group_with_defaults(".".to_string());
            self.selected_session = ids.first().copied();
            return;
        }

        let valid_group_ids = self.valid_group_ids();
        self.sessions
            .retain(|session| valid_group_ids.contains(&session.group_id));

        if self.sessions.is_empty() {
            let fallback_group = self.groups[0].id;
            let ids = [
                self.add_session_with_title_and_role(
                    fallback_group,
                    "Agent A".to_string(),
                    SessionRole::Agent,
                ),
                self.add_session_with_title_and_role(
                    fallback_group,
                    "Agent B".to_string(),
                    SessionRole::Agent,
                ),
                self.add_session_with_title_and_role(
                    fallback_group,
                    "Run / Compile".to_string(),
                    SessionRole::Runner,
                ),
            ];
            self.selected_session = ids.first().copied();
        }

        if let Some(selected) = self.selected_session {
            let selection_is_valid = self.sessions.iter().any(|session| session.id == selected);
            if !selection_is_valid {
                self.selected_session = self.sessions.first().map(|session| session.id);
            }
        } else {
            self.selected_session = self.sessions.first().map(|session| session.id);
        }

        let max_group = self.groups.iter().map(|group| group.id).max().unwrap_or(0);
        let max_session = self
            .sessions
            .iter()
            .map(|session| session.id)
            .max()
            .unwrap_or(0);
        self.next_group_id = self.next_group_id.max(max_group.saturating_add(1));
        self.next_session_id = self.next_session_id.max(max_session.saturating_add(1));
    }

    pub fn create_group_with_defaults(&mut self, path: String) -> (GroupId, Vec<SessionId>) {
        let group_id = self.add_group_with_path(path);
        let ids = vec![
            self.add_session_with_title_and_role(
                group_id,
                "Agent A".to_string(),
                SessionRole::Agent,
            ),
            self.add_session_with_title_and_role(
                group_id,
                "Agent B".to_string(),
                SessionRole::Agent,
            ),
            self.add_session_with_title_and_role(
                group_id,
                "Run / Compile".to_string(),
                SessionRole::Runner,
            ),
        ];
        (group_id, ids)
    }

    pub fn add_group_with_path(&mut self, path: String) -> GroupId {
        let id = self.next_group_id;
        self.next_group_id = self.next_group_id.saturating_add(1);
        let normalized = if path.trim().is_empty() {
            ".".to_string()
        } else {
            path.trim().to_string()
        };
        let color = Self::PALETTE[(id as usize - 1) % Self::PALETTE.len()].to_string();
        self.groups.push(TabGroup {
            id,
            path: normalized,
            color,
            layout: GroupLayout::default(),
        });
        id
    }

    pub fn remove_group(&mut self, group_id: GroupId) -> Vec<SessionId> {
        let group_exists = self.groups.iter().any(|group| group.id == group_id);
        if !group_exists {
            return Vec::new();
        }

        self.groups.retain(|group| group.id != group_id);
        let removed_session_ids = self
            .sessions
            .iter()
            .filter(|session| session.group_id == group_id)
            .map(|session| session.id)
            .collect::<Vec<_>>();

        if !removed_session_ids.is_empty() {
            let removed_ids: HashSet<SessionId> = removed_session_ids.iter().copied().collect();
            self.sessions
                .retain(|session| !removed_ids.contains(&session.id));

            if self
                .selected_session
                .is_some_and(|selected| removed_ids.contains(&selected))
            {
                self.selected_session = self.sessions.first().map(|session| session.id);
            }
        }

        removed_session_ids
    }

    pub fn add_session(&mut self, group_id: GroupId) -> SessionId {
        let title = format!("Agent {:02}", self.next_session_id);
        self.add_session_with_title_and_role(group_id, title, SessionRole::Agent)
    }

    pub fn add_session_with_title_and_role(
        &mut self,
        group_id: GroupId,
        title: String,
        role: SessionRole,
    ) -> SessionId {
        let id = self.next_session_id;
        self.next_session_id = self.next_session_id.saturating_add(1);
        self.sessions.push(Session {
            id,
            title,
            group_id,
            role,
            status: SessionStatus::Idle,
        });

        if self.selected_session.is_none() {
            self.selected_session = Some(id);
        }

        id
    }

    pub fn remove_session(&mut self, session_id: SessionId) -> bool {
        let Some(remove_idx) = self
            .sessions
            .iter()
            .position(|session| session.id == session_id)
        else {
            return false;
        };

        let removed_group_id = self.sessions[remove_idx].group_id;
        self.sessions.remove(remove_idx);

        if self.selected_session == Some(session_id) {
            self.selected_session = self
                .sessions
                .iter()
                .find(|session| session.group_id == removed_group_id)
                .map(|session| session.id)
                .or_else(|| self.sessions.first().map(|session| session.id));
        }

        true
    }

    pub fn rename_session(&mut self, session_id: SessionId, title: String) -> bool {
        let trimmed = title.trim();
        if trimmed.is_empty() {
            return false;
        }

        let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        else {
            return false;
        };
        if session.title == trimmed {
            return false;
        }

        session.title = trimmed.to_string();
        true
    }

    pub fn select_session(&mut self, session_id: SessionId) -> bool {
        if self.selected_session == Some(session_id) {
            return false;
        }
        self.selected_session = Some(session_id);
        true
    }

    pub fn cycle_session_status(&mut self, session_id: SessionId) -> bool {
        let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        else {
            return false;
        };
        session.status = session.status.next();
        true
    }

    pub fn set_session_status(&mut self, session_id: SessionId, status: SessionStatus) -> bool {
        let Some(session) = self
            .sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        else {
            return false;
        };
        if session.status == status {
            return false;
        }
        session.status = status;
        true
    }

    pub fn move_session_before(&mut self, source_id: SessionId, target_id: SessionId) -> bool {
        if source_id == target_id {
            return false;
        }

        let Some(source_idx) = self
            .sessions
            .iter()
            .position(|session| session.id == source_id)
        else {
            return false;
        };
        let mut source = self.sessions.remove(source_idx);

        let Some(target_idx) = self
            .sessions
            .iter()
            .position(|session| session.id == target_id)
        else {
            self.sessions.push(source);
            return true;
        };

        source.group_id = self.sessions[target_idx].group_id;
        self.sessions.insert(target_idx, source);
        true
    }

    pub fn move_session_to_group_end(&mut self, source_id: SessionId, group_id: GroupId) -> bool {
        let Some(source_idx) = self
            .sessions
            .iter()
            .position(|session| session.id == source_id)
        else {
            return false;
        };

        let mut source = self.sessions.remove(source_idx);
        source.group_id = group_id;
        let insert_idx = self
            .sessions
            .iter()
            .rposition(|session| session.group_id == group_id)
            .map_or(self.sessions.len(), |idx| idx + 1);
        self.sessions.insert(insert_idx, source);
        true
    }

    pub fn swap_session_with_visible_agent_slot(
        &mut self,
        source_id: SessionId,
        slot: VisibleAgentSlot,
    ) -> bool {
        let Some(source_idx) = self
            .sessions
            .iter()
            .position(|session| session.id == source_id)
        else {
            return false;
        };

        let source_group_id = self.sessions[source_idx].group_id;
        let Some(target_id) = self.visible_agent_slot_session_id(source_group_id, slot) else {
            return false;
        };
        let Some(target_idx) = self
            .sessions
            .iter()
            .position(|session| session.id == target_id)
        else {
            return false;
        };

        let mut changed = false;
        if source_idx != target_idx {
            self.sessions.swap(source_idx, target_idx);
            changed = true;
        }
        if self.selected_session != Some(source_id) {
            self.selected_session = Some(source_id);
            changed = true;
        }
        changed
    }

    fn visible_agent_slot_session_id(
        &self,
        group_id: GroupId,
        slot: VisibleAgentSlot,
    ) -> Option<SessionId> {
        let mut runner_seen = false;
        let mut visible_agents = Vec::with_capacity(2);

        for session in self
            .sessions
            .iter()
            .filter(|session| session.group_id == group_id)
        {
            if session.role.is_runner() && !runner_seen {
                runner_seen = true;
                continue;
            }

            visible_agents.push(session.id);
            if visible_agents.len() == 2 {
                break;
            }
        }

        match slot {
            VisibleAgentSlot::Top => visible_agents.first().copied(),
            VisibleAgentSlot::Bottom => visible_agents
                .get(1)
                .copied()
                .or_else(|| visible_agents.first().copied()),
        }
    }

    pub fn active_group_id(&self) -> Option<GroupId> {
        if let Some(selected) = self.selected_session
            && let Some(session) = self.sessions.iter().find(|session| session.id == selected)
        {
            return Some(session.group_id);
        }

        self.groups.first().map(|group| group.id)
    }

    pub fn sessions_in_group(&self, group_id: GroupId) -> Vec<Session> {
        self.sessions
            .iter()
            .filter(|session| session.group_id == group_id)
            .cloned()
            .collect()
    }

    pub fn session_ids_in_group(&self, group_id: GroupId) -> Vec<SessionId> {
        self.sessions
            .iter()
            .filter(|session| session.group_id == group_id)
            .map(|session| session.id)
            .collect()
    }

    pub fn workspace_sessions_for_group(
        &self,
        group_id: GroupId,
    ) -> (Vec<Session>, Option<Session>) {
        let mut agents = Vec::with_capacity(2);
        let mut runner = None;

        for session in self
            .sessions
            .iter()
            .filter(|session| session.group_id == group_id)
        {
            if session.role.is_runner() && runner.is_none() {
                runner = Some(session.clone());
                continue;
            }

            if agents.len() < 2 {
                agents.push(session.clone());
            }
        }

        if runner.is_none() {
            runner = agents.pop();
        }

        (agents, runner)
    }

    pub fn workspace_session_ids_for_group(&self, group_id: GroupId) -> Vec<SessionId> {
        let mut session_ids = Vec::with_capacity(3);
        let mut runner_id = None;

        for session in self
            .sessions
            .iter()
            .filter(|session| session.group_id == group_id)
        {
            if session.role.is_runner() && runner_id.is_none() {
                runner_id = Some(session.id);
                continue;
            }

            if session_ids.len() < 2 {
                session_ids.push(session.id);
            }
        }

        if let Some(runner_id) = runner_id {
            session_ids.push(runner_id);
        }

        session_ids
    }

    pub fn group_path(&self, group_id: GroupId) -> Option<&str> {
        self.groups
            .iter()
            .find(|group| group.id == group_id)
            .map(|group| group.path.as_str())
    }

    pub fn group_layout(&self, group_id: GroupId) -> GroupLayout {
        self.groups
            .iter()
            .find(|group| group.id == group_id)
            .map(|group| group.layout.normalized())
            .unwrap_or_default()
    }

    pub fn set_group_runner_width_px(&mut self, group_id: GroupId, width: i32) -> bool {
        let next = clamp_group_runner_width_px(width);
        self.update_group_layout(group_id, |layout| layout.runner_width_px = next)
    }

    pub fn set_group_agent_top_ratio(&mut self, group_id: GroupId, ratio: f64) -> bool {
        let next = clamp_group_split_ratio(ratio);
        self.update_group_layout(group_id, |layout| layout.agent_top_ratio = next)
    }

    pub fn set_group_runner_top_ratio(&mut self, group_id: GroupId, ratio: f64) -> bool {
        let next = clamp_group_split_ratio(ratio);
        self.update_group_layout(group_id, |layout| layout.runner_top_ratio = next)
    }

    fn update_group_layout(
        &mut self,
        group_id: GroupId,
        update: impl FnOnce(&mut GroupLayout),
    ) -> bool {
        let Some(group) = self.groups.iter_mut().find(|group| group.id == group_id) else {
            return false;
        };
        let before = group.layout.normalized();
        let mut next = before;
        update(&mut next);
        next = next.normalized();
        if before == next {
            return false;
        }
        group.layout = next;
        true
    }

    pub fn session_count_by_status(&self, status: SessionStatus) -> usize {
        self.sessions
            .iter()
            .filter(|session| session.status == status)
            .count()
    }

    pub fn group_label(&self, group_id: GroupId) -> String {
        self.groups
            .iter()
            .find(|group| group.id == group_id)
            .map(TabGroup::label)
            .unwrap_or_else(|| "Unknown".to_string())
    }

    pub fn ui_scale(&self) -> f64 {
        clamp_ui_scale(self.ui_scale)
    }

    pub fn set_ui_scale(&mut self, scale: f64) -> bool {
        let normalized = clamp_ui_scale(scale);
        if (self.ui_scale - normalized).abs() < f64::EPSILON {
            return false;
        }
        self.ui_scale = normalized;
        true
    }

    pub fn auxiliary_panel_layout(&self) -> AuxiliaryPanelLayout {
        self.auxiliary_panels.clone()
    }

    pub fn active_auxiliary_panel(&self, host: AuxiliaryPanelHost) -> Option<AuxiliaryPanelKind> {
        self.auxiliary_panels.active_tab(host)
    }

    pub fn auxiliary_panel_tabs(&self, host: AuxiliaryPanelHost) -> Vec<AuxiliaryPanelKind> {
        self.auxiliary_panels.tabs(host).to_vec()
    }

    pub fn set_active_auxiliary_panel(
        &mut self,
        host: AuxiliaryPanelHost,
        panel: AuxiliaryPanelKind,
    ) -> bool {
        self.auxiliary_panels.set_active_tab(host, panel)
    }

    pub fn move_auxiliary_panel_before(
        &mut self,
        source: AuxiliaryPanelKind,
        target: AuxiliaryPanelKind,
    ) -> bool {
        self.auxiliary_panels.move_panel_before(source, target)
    }

    pub fn move_auxiliary_panel_to_host_end(
        &mut self,
        panel: AuxiliaryPanelKind,
        host: AuxiliaryPanelHost,
    ) -> bool {
        self.auxiliary_panels.move_panel_to_host_end(panel, host)
    }
}

impl AppState {
    /// Creates a group and seeds default Agent/Runner sessions.
    pub fn create_group_with_defaults(&mut self, path: String) -> (GroupId, Vec<SessionId>) {
        let result = self.workspace.create_group_with_defaults(path);
        self.mark_dirty();
        result
    }

    /// Adds a group for the provided path and returns its identifier.
    pub fn add_group_with_path(&mut self, path: String) -> GroupId {
        let group_id = self.workspace.add_group_with_path(path);
        self.mark_dirty();
        group_id
    }

    /// Removes a group and every session assigned to it.
    pub fn remove_group(&mut self, group_id: GroupId) -> Vec<SessionId> {
        let removed_session_ids = self.workspace.remove_group(group_id);
        if removed_session_ids.is_empty() {
            return removed_session_ids;
        }
        self.knowledge.remove_notes_for_group(group_id);
        self.mark_dirty();
        removed_session_ids
    }

    /// Adds a new agent session with an auto-generated title.
    pub fn add_session(&mut self, group_id: GroupId) -> SessionId {
        let session_id = self.workspace.add_session(group_id);
        self.mark_dirty();
        session_id
    }

    /// Adds a session with explicit title and role.
    pub fn add_session_with_title_and_role(
        &mut self,
        group_id: GroupId,
        title: String,
        role: SessionRole,
    ) -> SessionId {
        let session_id = self
            .workspace
            .add_session_with_title_and_role(group_id, title, role);
        self.mark_dirty();
        session_id
    }

    /// Removes a session by identifier.
    pub fn remove_session(&mut self, session_id: SessionId) -> bool {
        let removed = self.workspace.remove_session(session_id);
        if removed {
            self.mark_dirty();
        }
        removed
    }

    /// Renames a session when the provided title is non-empty.
    pub fn rename_session(&mut self, session_id: SessionId, title: String) {
        if self.workspace.rename_session(session_id, title) {
            self.mark_dirty();
        }
    }

    /// Marks a session as selected.
    pub fn select_session(&mut self, session_id: SessionId) {
        if self.workspace.select_session(session_id) {
            self.mark_dirty();
        }
    }

    /// Cycles the selected session status forward.
    pub fn cycle_session_status(&mut self, session_id: SessionId) {
        if self.workspace.cycle_session_status(session_id) {
            self.mark_dirty();
        }
    }

    /// Sets a session status to an explicit value.
    pub fn set_session_status(&mut self, session_id: SessionId, status: SessionStatus) {
        if self.workspace.set_session_status(session_id, status) {
            self.mark_dirty();
        }
    }

    /// Moves one session before another and aligns group membership.
    pub fn move_session_before(&mut self, source_id: SessionId, target_id: SessionId) {
        if self.workspace.move_session_before(source_id, target_id) {
            self.mark_dirty();
        }
    }

    /// Moves a session to the end of a target group.
    pub fn move_session_to_group_end(&mut self, source_id: SessionId, group_id: GroupId) {
        if self
            .workspace
            .move_session_to_group_end(source_id, group_id)
        {
            self.mark_dirty();
        }
    }

    /// Swaps a session with one of the currently visible center-stack agent panes.
    pub fn swap_session_with_visible_agent_slot(
        &mut self,
        source_id: SessionId,
        slot: VisibleAgentSlot,
    ) {
        if self
            .workspace
            .swap_session_with_visible_agent_slot(source_id, slot)
        {
            self.mark_dirty();
        }
    }

    /// Returns the active group based on selected session fallback.
    pub fn active_group_id(&self) -> Option<GroupId> {
        self.workspace.active_group_id()
    }

    /// Returns all sessions currently belonging to a group.
    pub fn sessions_in_group(&self, group_id: GroupId) -> Vec<Session> {
        self.workspace.sessions_in_group(group_id)
    }

    /// Returns session identifiers in insertion order for a group.
    pub fn session_ids_in_group(&self, group_id: GroupId) -> Vec<SessionId> {
        self.workspace.session_ids_in_group(group_id)
    }

    /// Returns center-stack agent sessions and optional runner for UI layout.
    pub fn workspace_sessions_for_group(
        &self,
        group_id: GroupId,
    ) -> (Vec<Session>, Option<Session>) {
        self.workspace.workspace_sessions_for_group(group_id)
    }

    /// Returns center-stack agent identifiers plus optional runner identifier.
    pub fn workspace_session_ids_for_group(&self, group_id: GroupId) -> Vec<SessionId> {
        self.workspace.workspace_session_ids_for_group(group_id)
    }

    /// Returns the configured path for a group identifier.
    pub fn group_path(&self, group_id: GroupId) -> Option<&str> {
        self.workspace.group_path(group_id)
    }

    /// Returns layout controls for a group identifier.
    pub fn group_layout(&self, group_id: GroupId) -> GroupLayout {
        self.workspace.group_layout(group_id)
    }

    /// Updates the run sidebar width for a group.
    pub fn set_group_runner_width_px(&mut self, group_id: GroupId, width: i32) {
        if self.workspace.set_group_runner_width_px(group_id, width) {
            self.mark_dirty();
        }
    }

    /// Updates the agent stack split ratio for a group.
    pub fn set_group_agent_top_ratio(&mut self, group_id: GroupId, ratio: f64) {
        if self.workspace.set_group_agent_top_ratio(group_id, ratio) {
            self.mark_dirty();
        }
    }

    /// Updates the run/local-agent split ratio for a group.
    pub fn set_group_runner_top_ratio(&mut self, group_id: GroupId, ratio: f64) {
        if self.workspace.set_group_runner_top_ratio(group_id, ratio) {
            self.mark_dirty();
        }
    }

    /// Counts sessions in a given status.
    pub fn session_count_by_status(&self, status: SessionStatus) -> usize {
        self.workspace.session_count_by_status(status)
    }

    /// Returns a display label for the given group identifier.
    pub fn group_label(&self, group_id: GroupId) -> String {
        self.workspace.group_label(group_id)
    }

    /// Returns the persisted GUI font scale.
    pub fn ui_scale(&self) -> f64 {
        self.workspace.ui_scale()
    }

    /// Sets the GUI font scale and marks the state dirty when changed.
    pub fn set_ui_scale(&mut self, scale: f64) {
        if self.workspace.set_ui_scale(scale) {
            self.mark_dirty();
        }
    }

    /// Returns the durable auxiliary panel layout.
    pub fn auxiliary_panel_layout(&self) -> AuxiliaryPanelLayout {
        self.workspace.auxiliary_panel_layout()
    }

    /// Returns the active panel for the requested auxiliary host.
    pub fn active_auxiliary_panel(&self, host: AuxiliaryPanelHost) -> Option<AuxiliaryPanelKind> {
        self.workspace.active_auxiliary_panel(host)
    }

    /// Returns the ordered tabs for one auxiliary host.
    pub fn auxiliary_panel_tabs(&self, host: AuxiliaryPanelHost) -> Vec<AuxiliaryPanelKind> {
        self.workspace.auxiliary_panel_tabs(host)
    }

    /// Selects the active panel in one auxiliary host.
    pub fn set_active_auxiliary_panel(
        &mut self,
        host: AuxiliaryPanelHost,
        panel: AuxiliaryPanelKind,
    ) {
        if self.workspace.set_active_auxiliary_panel(host, panel) {
            self.mark_dirty();
        }
    }

    /// Reorders or moves one auxiliary panel before another tab.
    pub fn move_auxiliary_panel_before(
        &mut self,
        source: AuxiliaryPanelKind,
        target: AuxiliaryPanelKind,
    ) {
        if self.workspace.move_auxiliary_panel_before(source, target) {
            self.mark_dirty();
        }
    }

    /// Moves one auxiliary panel to the end of a host.
    pub fn move_auxiliary_panel_to_host_end(
        &mut self,
        panel: AuxiliaryPanelKind,
        host: AuxiliaryPanelHost,
    ) {
        if self.workspace.move_auxiliary_panel_to_host_end(panel, host) {
            self.mark_dirty();
        }
    }
}
