use async_trait::async_trait;
use futures::{StreamExt, TryStreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::Api,
    runtime::watcher::{self, Event as WatcherEvent},
    Client,
};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use aion_common::handle::{NodeHandle, PodHandle};

use crate::collector::EventCollector;
use crate::event::{CrashLoopData, InfraEvent};
use crate::ObserveError;

/// Watches Kubernetes pod events for crash loops and other anomalies.
pub struct KubePodWatcher {
    node_name: String,
    node_uid: String,
    /// Minimum restart count to trigger a crash loop event.
    crash_loop_threshold: u32,
}

impl KubePodWatcher {
    pub fn new(node_name: impl Into<String>, node_uid: impl Into<String>) -> Self {
        Self {
            node_name: node_name.into(),
            node_uid: node_uid.into(),
            crash_loop_threshold: 3,
        }
    }

    pub fn with_crash_loop_threshold(mut self, threshold: u32) -> Self {
        self.crash_loop_threshold = threshold;
        self
    }

    fn check_pod_for_crash_loop(&self, pod: &Pod) -> Option<InfraEvent> {
        let metadata = pod.metadata.clone();
        let status = pod.status.as_ref()?;
        let name = metadata.name.as_deref()?;
        let namespace = metadata.namespace.as_deref().unwrap_or("default");
        let uid = metadata.uid.as_deref().unwrap_or("");

        let container_statuses = status.container_statuses.as_ref()?;

        for cs in container_statuses {
            if cs.restart_count >= self.crash_loop_threshold as i32 {
                let (exit_code, reason) = cs
                    .last_state
                    .as_ref()
                    .and_then(|s| s.terminated.as_ref())
                    .map(|t| {
                        (
                            t.exit_code,
                            t.reason.clone().unwrap_or_default(),
                        )
                    })
                    .unwrap_or((0, String::new()));

                let node = NodeHandle::new(
                    self.node_name.clone(),
                    self.node_uid.clone(),
                );

                let pod_node = status
                    .nominated_node_name
                    .clone()
                    .or_else(|| {
                        pod.spec
                            .as_ref()
                            .and_then(|s| s.node_name.clone())
                    });

                let mut pod_handle = PodHandle::new(namespace, name, uid);
                if let Some(n) = pod_node {
                    pod_handle = pod_handle.with_node(n);
                }

                let data = CrashLoopData {
                    restart_count: cs.restart_count as u32,
                    last_exit_code: exit_code,
                    last_reason: reason,
                };

                return Some(InfraEvent::crash_loop(node, pod_handle, data));
            }
        }

        None
    }
}

#[async_trait]
impl EventCollector for KubePodWatcher {
    fn name(&self) -> &str {
        "kube-pod-watcher"
    }

    async fn run(
        &self,
        sender: broadcast::Sender<InfraEvent>,
        cancel: CancellationToken,
    ) -> Result<(), ObserveError> {
        let client = Client::try_default()
            .await
            .map_err(|e| ObserveError::Kube(format!("failed to create K8s client: {e}")))?;

        let pods: Api<Pod> = Api::all(client);
        let watcher_config = watcher::Config::default();
        let mut stream = watcher::watcher(pods, watcher_config).boxed();

        info!("KubePodWatcher started");

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("KubePodWatcher cancelled");
                    break;
                }
                result = stream.try_next() => {
                    match result {
                        Ok(Some(event)) => {
                            match event {
                                WatcherEvent::Apply(pod) | WatcherEvent::InitApply(pod) => {
                                    if let Some(infra_event) = self.check_pod_for_crash_loop(&pod) {
                                        info!(
                                            event_id = %infra_event.id,
                                            "Crash loop detected"
                                        );
                                        if sender.send(infra_event).is_err() {
                                            warn!("No receivers for crash loop event");
                                        }
                                    }
                                }
                                WatcherEvent::Delete(pod) => {
                                    debug!(
                                        pod = ?pod.metadata.name,
                                        "Pod deleted"
                                    );
                                }
                                _ => {}
                            }
                        }
                        Ok(None) => {
                            info!("KubePodWatcher stream ended");
                            break;
                        }
                        Err(e) => {
                            error!("KubePodWatcher error: {e}");
                            // Retry after a brief delay
                            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kube_watcher_creation() {
        let watcher = KubePodWatcher::new("test-node", "uid-1")
            .with_crash_loop_threshold(5);
        assert_eq!(watcher.node_name, "test-node");
        assert_eq!(watcher.crash_loop_threshold, 5);
    }

    // Integration tests with actual K8s cluster are in tests/integration/
}
