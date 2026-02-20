use async_trait::async_trait;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use aion_common::handle::{NodeHandle, ProcessHandle};
use aion_ebpf_common::OomKillEvent;

use crate::collector::EventCollector;
use crate::event::{InfraEvent, OomEventData};
use crate::ObserveError;

/// Configuration for the OOM watcher.
#[derive(Debug, Clone)]
pub struct OomWatcherConfig {
    /// Path to the compiled eBPF object file.
    pub ebpf_obj_path: Option<String>,
    /// Node name this watcher is running on.
    pub node_name: String,
    /// Node UID.
    pub node_uid: String,
    /// Whether to use mock mode (no actual eBPF).
    pub mock_mode: bool,
}

impl Default for OomWatcherConfig {
    fn default() -> Self {
        Self {
            ebpf_obj_path: None,
            node_name: "unknown".to_string(),
            node_uid: "unknown".to_string(),
            mock_mode: true,
        }
    }
}

/// OOM kill event watcher using eBPF tracepoints.
///
/// In production mode, attaches an eBPF program to the `oom:mark_victim`
/// tracepoint and receives events via a perf ring buffer.
///
/// In mock mode (for testing and environments without eBPF support),
/// provides a method to inject synthetic events.
pub struct OomWatcher {
    config: OomWatcherConfig,
    /// For mock mode: channel to inject synthetic events
    mock_sender: Option<tokio::sync::mpsc::Sender<OomKillEvent>>,
    mock_receiver: Option<tokio::sync::Mutex<tokio::sync::mpsc::Receiver<OomKillEvent>>>,
}

impl OomWatcher {
    pub fn new(config: OomWatcherConfig) -> Self {
        if config.mock_mode {
            let (tx, rx) = tokio::sync::mpsc::channel(64);
            Self {
                config,
                mock_sender: Some(tx),
                mock_receiver: Some(tokio::sync::Mutex::new(rx)),
            }
        } else {
            Self {
                config,
                mock_sender: None,
                mock_receiver: None,
            }
        }
    }

    /// Get a handle to inject mock OOM events (only in mock mode).
    pub fn mock_injector(&self) -> Option<MockOomInjector> {
        self.mock_sender.as_ref().map(|s| MockOomInjector {
            sender: s.clone(),
        })
    }

    fn convert_event(&self, raw: &OomKillEvent) -> InfraEvent {
        let node = NodeHandle::new(
            self.config.node_name.clone(),
            self.config.node_uid.clone(),
        );
        let process = ProcessHandle::new(raw.pid, raw.comm_str(), raw.uid);

        let data = OomEventData {
            pid: raw.pid,
            comm: raw.comm_str().to_string(),
            total_pages: raw.total_vm_pages,
            rss_pages: raw.rss_pages,
            oom_score_adj: raw.oom_score_adj,
            memory_limit_bytes: 0, // Will be enriched by K8s metadata
            memory_usage_bytes: 0, // Will be enriched by K8s metadata
        };

        InfraEvent::oom_kill(node, None, process, data)
    }

    /// Run in mock mode, reading from the mock channel.
    async fn run_mock(
        &self,
        sender: broadcast::Sender<InfraEvent>,
        cancel: CancellationToken,
    ) -> Result<(), ObserveError> {
        let rx = self.mock_receiver.as_ref()
            .ok_or(ObserveError::Ebpf("mock receiver not available".into()))?;
        let mut rx = rx.lock().await;

        info!("OomWatcher running in mock mode");

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("OomWatcher mock mode cancelled");
                    break;
                }
                event = rx.recv() => {
                    match event {
                        Some(raw) => {
                            let infra_event = self.convert_event(&raw);
                            info!(
                                event_id = %infra_event.id,
                                pid = raw.pid,
                                comm = raw.comm_str(),
                                "OOM kill event detected (mock)"
                            );
                            if sender.send(infra_event).is_err() {
                                warn!("No receivers for OOM event");
                            }
                        }
                        None => {
                            info!("Mock channel closed");
                            break;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Run in production mode with actual eBPF.
    async fn run_ebpf(
        &self,
        _sender: broadcast::Sender<InfraEvent>,
        cancel: CancellationToken,
    ) -> Result<(), ObserveError> {
        // In production, this would:
        // 1. Load eBPF object from ebpf_obj_path
        // 2. Attach to oom:mark_victim tracepoint
        // 3. Create AsyncPerfEventArray
        // 4. Poll events in a loop
        //
        // Skeleton:
        // let mut bpf = aya::Ebpf::load_file(path)?;
        // let program: &mut TracePoint = bpf.program_mut("aion_oom_kill")?.try_into()?;
        // program.load()?;
        // program.attach("oom", "mark_victim")?;
        // let mut perf_array = AsyncPerfEventArray::try_from(bpf.take_map("OOM_EVENTS")?)?;
        // ... poll loop ...

        warn!("eBPF mode not fully implemented yet — waiting for cancellation");
        cancel.cancelled().await;
        Ok(())
    }
}

#[async_trait]
impl EventCollector for OomWatcher {
    fn name(&self) -> &str {
        "oom-watcher"
    }

    async fn run(
        &self,
        sender: broadcast::Sender<InfraEvent>,
        cancel: CancellationToken,
    ) -> Result<(), ObserveError> {
        if self.config.mock_mode {
            self.run_mock(sender, cancel).await
        } else {
            self.run_ebpf(sender, cancel).await
        }
    }
}

/// Handle for injecting mock OOM events (testing only).
#[derive(Clone)]
pub struct MockOomInjector {
    sender: tokio::sync::mpsc::Sender<OomKillEvent>,
}

impl MockOomInjector {
    /// Inject a synthetic OOM kill event.
    pub async fn inject(&self, event: OomKillEvent) -> Result<(), ObserveError> {
        self.sender
            .send(event)
            .await
            .map_err(|e| ObserveError::Ebpf(format!("failed to inject mock event: {e}")))
    }

    /// Create a simple test event.
    pub fn test_event(pid: u32, comm: &str) -> OomKillEvent {
        let mut event = OomKillEvent {
            pid,
            uid: 1000,
            comm: [0u8; 16],
            total_vm_pages: 262144,
            rss_pages: 131072,
            oom_score_adj: 1000,
            _pad: 0,
        };
        let bytes = comm.as_bytes();
        let len = bytes.len().min(15);
        event.comm[..len].copy_from_slice(&bytes[..len]);
        event
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::EventBus;
    use crate::event::EventKind;

    #[tokio::test]
    async fn test_mock_oom_watcher() {
        let config = OomWatcherConfig {
            node_name: "test-node".to_string(),
            node_uid: "test-uid".to_string(),
            mock_mode: true,
            ..Default::default()
        };
        let watcher = OomWatcher::new(config);
        let injector = watcher.mock_injector().unwrap();

        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();

        // Run watcher in background
        let sender = bus.sender();
        let handle = tokio::spawn(async move {
            watcher.run(sender, cancel_clone).await
        });

        // Inject a mock OOM event
        let mock_event = MockOomInjector::test_event(1234, "stress-ng");
        injector.inject(mock_event).await.unwrap();

        // Receive the converted InfraEvent
        let event = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            rx.recv(),
        )
        .await
        .expect("timeout waiting for event")
        .expect("channel error");

        assert_eq!(event.kind, EventKind::OomKill);
        assert!(event.process.is_some());
        assert_eq!(event.process.as_ref().unwrap().pid, 1234);
        assert_eq!(event.node.as_ref().unwrap().name.as_str(), "test-node");

        // Cleanup
        cancel.cancel();
        handle.await.unwrap().unwrap();
    }

    #[test]
    fn test_mock_injector_test_event() {
        let event = MockOomInjector::test_event(42, "nginx");
        assert_eq!(event.pid, 42);
        assert_eq!(event.comm_str(), "nginx");
    }
}
