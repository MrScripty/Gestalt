use std::sync::OnceLock;

use tokio::sync::broadcast;

/// Typed command names emitted for Git orchestration events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitCommandKind {
    StageFiles,
    UnstageFiles,
    CreateCommit,
    UpdateCommitMessage,
    CreateTag,
    CheckoutTarget,
    CreateWorktree,
}

/// Raised when the application executes a Git command through orchestrator APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommandExecuted {
    pub group_path: String,
    pub command: GitCommandKind,
    pub success: bool,
}

/// Raised by active-repository filesystem watchers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoFsChanged {
    pub group_path: String,
}

/// Internal orchestrator event contract used by refresh coordination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrchestratorEvent {
    GitCommandExecuted(GitCommandExecuted),
    RepoFsChanged(RepoFsChanged),
}

/// Lightweight bounded pub/sub hub for orchestrator domain events.
#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<OrchestratorEvent>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl EventBus {
    /// Creates a fresh pub/sub hub.
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(64);
        Self { sender }
    }

    /// Subscribes a new receiver to the event stream.
    pub fn subscribe(&self) -> broadcast::Receiver<OrchestratorEvent> {
        self.sender.subscribe()
    }

    /// Publishes an event to all active subscribers.
    pub fn publish(&self, event: OrchestratorEvent) {
        let _ = self.sender.send(event);
    }
}

static EVENT_BUS: OnceLock<EventBus> = OnceLock::new();

/// Returns the process-wide orchestrator event bus.
pub fn event_bus() -> EventBus {
    EVENT_BUS.get_or_init(EventBus::new).clone()
}

#[cfg(test)]
mod tests {
    use super::{EventBus, GitCommandExecuted, GitCommandKind, OrchestratorEvent};

    #[test]
    fn publish_reaches_all_subscribers() {
        let bus = EventBus::new();
        let mut first = bus.subscribe();
        let mut second = bus.subscribe();

        bus.publish(OrchestratorEvent::GitCommandExecuted(GitCommandExecuted {
            group_path: "/tmp/repo".to_string(),
            command: GitCommandKind::CreateCommit,
            success: true,
        }));

        assert!(first.try_recv().is_ok());
        assert!(second.try_recv().is_ok());
    }

    #[test]
    fn publish_tolerates_dropped_subscribers() {
        let bus = EventBus::new();
        let dropped = bus.subscribe();
        drop(dropped);
        let mut live = bus.subscribe();

        bus.publish(OrchestratorEvent::GitCommandExecuted(GitCommandExecuted {
            group_path: "/tmp/repo".to_string(),
            command: GitCommandKind::CreateTag,
            success: true,
        }));
        bus.publish(OrchestratorEvent::GitCommandExecuted(GitCommandExecuted {
            group_path: "/tmp/repo".to_string(),
            command: GitCommandKind::CheckoutTarget,
            success: true,
        }));

        assert!(live.try_recv().is_ok());
        assert!(live.try_recv().is_ok());
    }
}
