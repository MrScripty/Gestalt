use serde::{Deserialize, Serialize};

/// Durable auxiliary panel tabs that can be docked into either sidebar host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum AuxiliaryPanelKind {
    LocalAgent,
    RunReview,
    Notes,
    Commands,
    Git,
    Files,
}

impl AuxiliaryPanelKind {
    pub const ALL: [Self; 6] = [
        Self::LocalAgent,
        Self::RunReview,
        Self::Notes,
        Self::Commands,
        Self::Git,
        Self::Files,
    ];

    /// Returns the visible label used in tab chrome.
    pub fn label(self) -> &'static str {
        match self {
            Self::LocalAgent => "Local Agent",
            Self::RunReview => "Run Review",
            Self::Notes => "Notes",
            Self::Commands => "Commands",
            Self::Git => "Git",
            Self::Files => "Files",
        }
    }

    pub(crate) fn default_host(self) -> AuxiliaryPanelHost {
        match self {
            Self::LocalAgent | Self::RunReview | Self::Notes => AuxiliaryPanelHost::RunSidebar,
            Self::Commands | Self::Git | Self::Files => AuxiliaryPanelHost::SidePanel,
        }
    }
}

/// Identifies one of the two auxiliary sidebar hosts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum AuxiliaryPanelHost {
    RunSidebar,
    SidePanel,
}

/// Durable order, placement, and active tab selection for auxiliary panels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuxiliaryPanelLayout {
    #[serde(default = "default_run_sidebar_panels")]
    run_sidebar: Vec<AuxiliaryPanelKind>,
    #[serde(default = "default_side_panel_panels")]
    side_panel: Vec<AuxiliaryPanelKind>,
    #[serde(default = "default_run_sidebar_active_panel")]
    active_run_sidebar: Option<AuxiliaryPanelKind>,
    #[serde(default = "default_side_panel_active_panel")]
    active_side_panel: Option<AuxiliaryPanelKind>,
}

impl Default for AuxiliaryPanelLayout {
    fn default() -> Self {
        Self {
            run_sidebar: default_run_sidebar_panels(),
            side_panel: default_side_panel_panels(),
            active_run_sidebar: default_run_sidebar_active_panel(),
            active_side_panel: default_side_panel_active_panel(),
        }
    }
}

impl AuxiliaryPanelLayout {
    pub(crate) fn normalized(self) -> Self {
        let mut seen = Vec::with_capacity(AuxiliaryPanelKind::ALL.len());
        let mut run_sidebar = collect_unique_panels(self.run_sidebar, &mut seen);
        let mut side_panel = collect_unique_panels(self.side_panel, &mut seen);

        for panel in AuxiliaryPanelKind::ALL {
            if seen.contains(&panel) {
                continue;
            }

            match panel.default_host() {
                AuxiliaryPanelHost::RunSidebar => run_sidebar.push(panel),
                AuxiliaryPanelHost::SidePanel => side_panel.push(panel),
            }
        }

        let active_run_sidebar = normalize_active_panel(self.active_run_sidebar, &run_sidebar);
        let active_side_panel = normalize_active_panel(self.active_side_panel, &side_panel);

        Self {
            run_sidebar,
            side_panel,
            active_run_sidebar,
            active_side_panel,
        }
    }

    pub fn tabs(&self, host: AuxiliaryPanelHost) -> &[AuxiliaryPanelKind] {
        match host {
            AuxiliaryPanelHost::RunSidebar => &self.run_sidebar,
            AuxiliaryPanelHost::SidePanel => &self.side_panel,
        }
    }

    pub fn active_tab(&self, host: AuxiliaryPanelHost) -> Option<AuxiliaryPanelKind> {
        match host {
            AuxiliaryPanelHost::RunSidebar => self.active_run_sidebar,
            AuxiliaryPanelHost::SidePanel => self.active_side_panel,
        }
    }

    pub fn host_for_panel(&self, panel: AuxiliaryPanelKind) -> Option<AuxiliaryPanelHost> {
        if self.run_sidebar.contains(&panel) {
            return Some(AuxiliaryPanelHost::RunSidebar);
        }
        if self.side_panel.contains(&panel) {
            return Some(AuxiliaryPanelHost::SidePanel);
        }
        None
    }

    pub fn set_active_tab(&mut self, host: AuxiliaryPanelHost, panel: AuxiliaryPanelKind) -> bool {
        let tabs = self.tabs(host);
        if !tabs.contains(&panel) {
            return false;
        }

        let active = match host {
            AuxiliaryPanelHost::RunSidebar => &mut self.active_run_sidebar,
            AuxiliaryPanelHost::SidePanel => &mut self.active_side_panel,
        };
        if *active == Some(panel) {
            return false;
        }

        *active = Some(panel);
        true
    }

    pub fn move_panel_before(
        &mut self,
        source: AuxiliaryPanelKind,
        target: AuxiliaryPanelKind,
    ) -> bool {
        if source == target {
            return false;
        }

        let Some(source_host) = self.host_for_panel(source) else {
            return false;
        };
        let Some(target_host) = self.host_for_panel(target) else {
            return false;
        };

        self.remove_panel(source, source_host);
        let target_tabs = self.tabs_mut(target_host);
        let Some(target_idx) = target_tabs.iter().position(|panel| *panel == target) else {
            return false;
        };
        target_tabs.insert(target_idx, source);

        if source_host != target_host {
            self.set_active_for_host_after_removal(source_host);
            self.set_active_for_host(target_host, Some(source));
        }
        true
    }

    pub fn move_panel_to_host_end(
        &mut self,
        panel: AuxiliaryPanelKind,
        destination_host: AuxiliaryPanelHost,
    ) -> bool {
        let Some(source_host) = self.host_for_panel(panel) else {
            return false;
        };

        let already_last = source_host == destination_host
            && self
                .tabs(destination_host)
                .last()
                .is_some_and(|last| *last == panel);
        if already_last {
            return false;
        }

        self.remove_panel(panel, source_host);
        self.tabs_mut(destination_host).push(panel);

        if source_host != destination_host {
            self.set_active_for_host_after_removal(source_host);
            self.set_active_for_host(destination_host, Some(panel));
        }
        true
    }

    fn tabs_mut(&mut self, host: AuxiliaryPanelHost) -> &mut Vec<AuxiliaryPanelKind> {
        match host {
            AuxiliaryPanelHost::RunSidebar => &mut self.run_sidebar,
            AuxiliaryPanelHost::SidePanel => &mut self.side_panel,
        }
    }

    fn remove_panel(&mut self, panel: AuxiliaryPanelKind, host: AuxiliaryPanelHost) -> bool {
        let tabs = self.tabs_mut(host);
        let Some(idx) = tabs.iter().position(|candidate| *candidate == panel) else {
            return false;
        };
        tabs.remove(idx);
        true
    }

    fn set_active_for_host_after_removal(&mut self, host: AuxiliaryPanelHost) {
        let next_active = self.tabs(host).first().copied();
        self.set_active_for_host(host, next_active);
    }

    fn set_active_for_host(&mut self, host: AuxiliaryPanelHost, panel: Option<AuxiliaryPanelKind>) {
        match host {
            AuxiliaryPanelHost::RunSidebar => self.active_run_sidebar = panel,
            AuxiliaryPanelHost::SidePanel => self.active_side_panel = panel,
        }
    }
}

fn collect_unique_panels(
    panels: Vec<AuxiliaryPanelKind>,
    seen: &mut Vec<AuxiliaryPanelKind>,
) -> Vec<AuxiliaryPanelKind> {
    let mut normalized = Vec::with_capacity(panels.len());
    for panel in panels {
        if seen.contains(&panel) {
            continue;
        }
        seen.push(panel);
        normalized.push(panel);
    }
    normalized
}

fn normalize_active_panel(
    active_panel: Option<AuxiliaryPanelKind>,
    tabs: &[AuxiliaryPanelKind],
) -> Option<AuxiliaryPanelKind> {
    active_panel
        .filter(|panel| tabs.contains(panel))
        .or_else(|| tabs.first().copied())
}

fn default_run_sidebar_panels() -> Vec<AuxiliaryPanelKind> {
    vec![
        AuxiliaryPanelKind::LocalAgent,
        AuxiliaryPanelKind::RunReview,
        AuxiliaryPanelKind::Notes,
    ]
}

fn default_side_panel_panels() -> Vec<AuxiliaryPanelKind> {
    vec![
        AuxiliaryPanelKind::Commands,
        AuxiliaryPanelKind::Git,
        AuxiliaryPanelKind::Files,
    ]
}

fn default_run_sidebar_active_panel() -> Option<AuxiliaryPanelKind> {
    Some(AuxiliaryPanelKind::LocalAgent)
}

fn default_side_panel_active_panel() -> Option<AuxiliaryPanelKind> {
    Some(AuxiliaryPanelKind::Commands)
}
