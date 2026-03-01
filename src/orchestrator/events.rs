use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;

/// Typed command names emitted for Git orchestration events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitCommandKind {
    StageFiles,
    UnstageFiles,
    CreateCommit,
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
#[derive(Clone, Default)]
pub struct EventBus {
    subscribers: Arc<Mutex<Vec<Sender<OrchestratorEvent>>>>,
}

impl EventBus {
    /// Creates a fresh pub/sub hub.
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribes a new receiver to the event stream.
    pub fn subscribe(&self) -> Receiver<OrchestratorEvent> {
        let (sender, receiver) = channel();
        self.subscribers.lock().push(sender);
        receiver
    }

    /// Publishes an event to all active subscribers.
    pub fn publish(&self, event: OrchestratorEvent) {
        let mut subscribers = self.subscribers.lock();
        subscribers.retain(|sender| sender.send(event.clone()).is_ok());
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
        let first = bus.subscribe();
        let second = bus.subscribe();

        bus.publish(OrchestratorEvent::GitCommandExecuted(GitCommandExecuted {
            group_path: "/tmp/repo".to_string(),
            command: GitCommandKind::CreateCommit,
            success: true,
        }));

        assert!(first.recv().is_ok());
        assert!(second.recv().is_ok());
    }

    #[test]
    fn publish_prunes_dropped_subscribers() {
        let bus = EventBus::new();
        let dropped = bus.subscribe();
        drop(dropped);
        let live = bus.subscribe();

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

        assert!(live.recv().is_ok());
        assert!(live.recv().is_ok());
    }
}
