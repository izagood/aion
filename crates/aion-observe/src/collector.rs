use async_trait::async_trait;
use tokio::sync::broadcast;

use crate::event::InfraEvent;

/// Trait for all infrastructure event collectors.
///
/// Implementations include:
/// - OomWatcher (eBPF perf events)
/// - KubePodWatcher (K8s API watch)
/// - CgroupCollector (cgroup v2 memory pressure)
#[async_trait]
pub trait EventCollector: Send + Sync {
    /// Human-readable name of this collector.
    fn name(&self) -> &str;

    /// Start collecting events and broadcasting them.
    ///
    /// This method runs indefinitely until the cancellation token is triggered.
    /// Detected events are sent via the broadcast sender.
    async fn run(
        &self,
        sender: broadcast::Sender<InfraEvent>,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<(), crate::ObserveError>;
}

/// Central event bus that collects from multiple sources and fans out to consumers.
pub struct EventBus {
    sender: broadcast::Sender<InfraEvent>,
    _receiver: broadcast::Receiver<InfraEvent>,
}

impl EventBus {
    /// Create a new event bus with the specified channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (sender, _receiver) = broadcast::channel(capacity);
        Self { sender, _receiver }
    }

    /// Get a sender for collectors to broadcast events.
    pub fn sender(&self) -> broadcast::Sender<InfraEvent> {
        self.sender.clone()
    }

    /// Subscribe to receive events.
    pub fn subscribe(&self) -> broadcast::Receiver<InfraEvent> {
        self.sender.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(256)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aion_common::handle::{NodeHandle, ProcessHandle};
    use crate::event::{InfraEvent, OomEventData};

    #[tokio::test]
    async fn test_event_bus_broadcast() {
        let bus = EventBus::new(16);
        let sender = bus.sender();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        let node = NodeHandle::new("node-1", "uid-1");
        let process = ProcessHandle::new(1, "test", 0);
        let data = OomEventData {
            pid: 1,
            comm: "test".to_string(),
            total_pages: 100,
            rss_pages: 50,
            oom_score_adj: 0,
            memory_limit_bytes: 1000,
            memory_usage_bytes: 999,
        };
        let event = InfraEvent::oom_kill(node, None, process, data);

        sender.send(event).unwrap();

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        assert_eq!(e1.id, e2.id);
    }

    #[tokio::test]
    async fn test_event_bus_no_subscribers() {
        let bus = EventBus::new(16);
        let sender = bus.sender();

        let node = NodeHandle::new("node-1", "uid-1");
        let process = ProcessHandle::new(1, "test", 0);
        let data = OomEventData {
            pid: 1,
            comm: "test".to_string(),
            total_pages: 100,
            rss_pages: 50,
            oom_score_adj: 0,
            memory_limit_bytes: 1000,
            memory_usage_bytes: 999,
        };
        let event = InfraEvent::oom_kill(node, None, process, data);

        // EventBus keeps an internal _receiver, so send() will succeed even
        // without explicit subscribers. The key property is: no panic occurs,
        // and the event is silently consumed by the internal receiver.
        let result = sender.send(event);
        assert!(result.is_ok(), "send should succeed because EventBus holds an internal receiver");
    }

    #[tokio::test]
    async fn test_event_bus_subscribe_after_send() {
        let bus = EventBus::new(16);
        let sender = bus.sender();

        let node = NodeHandle::new("node-1", "uid-1");
        let process = ProcessHandle::new(1, "test", 0);
        let data = OomEventData {
            pid: 1,
            comm: "test".to_string(),
            total_pages: 100,
            rss_pages: 50,
            oom_score_adj: 0,
            memory_limit_bytes: 1000,
            memory_usage_bytes: 999,
        };
        let event = InfraEvent::oom_kill(node, None, process, data);

        // Subscribe first to make send succeed, then subscribe after
        let _rx_before = bus.subscribe();
        sender.send(event).unwrap();

        // Subscribe after send — should not receive the past event
        let mut rx_after = bus.subscribe();

        // Use try_recv to check that no event is available
        let result = rx_after.try_recv();
        assert!(result.is_err()); // No message available
    }

    #[tokio::test]
    async fn test_event_bus_multiple_subscribers() {
        let bus = EventBus::new(16);
        let sender = bus.sender();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();
        let mut rx3 = bus.subscribe();

        let node = NodeHandle::new("node-1", "uid-1");
        let process = ProcessHandle::new(42, "nginx", 0);
        let data = OomEventData {
            pid: 42,
            comm: "nginx".to_string(),
            total_pages: 200,
            rss_pages: 100,
            oom_score_adj: 0,
            memory_limit_bytes: 2000,
            memory_usage_bytes: 1999,
        };
        let event = InfraEvent::oom_kill(node, None, process, data);

        sender.send(event).unwrap();

        let e1 = rx1.recv().await.unwrap();
        let e2 = rx2.recv().await.unwrap();
        let e3 = rx3.recv().await.unwrap();

        // All subscribers should receive the same event
        assert_eq!(e1.id, e2.id);
        assert_eq!(e2.id, e3.id);
        assert_eq!(e1.kind, crate::event::EventKind::OomKill);
    }
}
