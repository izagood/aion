use aion_common::handle::{NodeHandle, PodHandle, ProcessHandle};
use aion_common::types::{AnomalyId, Severity, Timestamp};
use serde::{Deserialize, Serialize};

/// The kind of infrastructure event detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    OomKill,
    CpuThrottle,
    MemoryPressure,
    DiskPressure,
    PodCrashLoop,
    NodeNotReady,
}

impl std::fmt::Display for EventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventKind::OomKill => write!(f, "oom_kill"),
            EventKind::CpuThrottle => write!(f, "cpu_throttle"),
            EventKind::MemoryPressure => write!(f, "memory_pressure"),
            EventKind::DiskPressure => write!(f, "disk_pressure"),
            EventKind::PodCrashLoop => write!(f, "pod_crash_loop"),
            EventKind::NodeNotReady => write!(f, "node_not_ready"),
        }
    }
}

/// A detected infrastructure anomaly event.
///
/// This is the core event type broadcast through the AION pipeline.
/// Created by EventCollectors (eBPF, cgroup, K8s watcher) and consumed
/// by the Agent Mount pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfraEvent {
    pub id: AnomalyId,
    pub kind: EventKind,
    pub severity: Severity,
    pub detected_at: Timestamp,

    /// The node where the event was detected
    pub node: Option<NodeHandle>,

    /// The pod involved (if applicable)
    pub pod: Option<PodHandle>,

    /// The process involved (if applicable, e.g. OOM kill)
    pub process: Option<ProcessHandle>,

    /// Event-specific data
    pub details: EventDetails,

    /// Human-readable description
    pub description: String,
}

/// Event-specific detail data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EventDetails {
    OomKill(OomEventData),
    MemoryPressure(MemoryPressureData),
    PodCrashLoop(CrashLoopData),
    Generic(GenericEventData),
}

/// OOM kill specific data from eBPF tracepoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OomEventData {
    /// Killed process PID
    pub pid: u32,
    /// Killed process name
    pub comm: String,
    /// Total virtual memory pages
    pub total_pages: u64,
    /// Resident set size in pages
    pub rss_pages: u64,
    /// OOM score adjustment
    pub oom_score_adj: i32,
    /// Container memory limit in bytes
    pub memory_limit_bytes: u64,
    /// Container memory usage at kill time
    pub memory_usage_bytes: u64,
}

/// Memory pressure data from cgroup v2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPressureData {
    /// Pressure level: "low", "medium", "critical"
    pub level: String,
    /// Total stall time in microseconds
    pub total_stall_us: u64,
    /// Current memory usage bytes
    pub usage_bytes: u64,
    /// Memory limit bytes
    pub limit_bytes: u64,
}

/// Pod crash loop data from K8s watcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashLoopData {
    pub restart_count: u32,
    pub last_exit_code: i32,
    pub last_reason: String,
}

/// Generic event data for extensibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericEventData {
    pub message: String,
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

impl InfraEvent {
    /// Create a new OOM kill event.
    pub fn oom_kill(
        node: NodeHandle,
        pod: Option<PodHandle>,
        process: ProcessHandle,
        data: OomEventData,
    ) -> Self {
        let description = format!(
            "OOM kill: process '{}' (pid {}) killed on node '{}'",
            data.comm, data.pid, node.name
        );
        Self {
            id: AnomalyId::new(uuid::Uuid::new_v4().to_string()),
            kind: EventKind::OomKill,
            severity: Severity::Critical,
            detected_at: aion_common::types::now(),
            node: Some(node),
            pod,
            process: Some(process),
            details: EventDetails::OomKill(data),
            description,
        }
    }

    /// Create a pod crash loop event.
    pub fn crash_loop(
        node: NodeHandle,
        pod: PodHandle,
        data: CrashLoopData,
    ) -> Self {
        let description = format!(
            "Pod crash loop: '{}' in namespace '{}' restarted {} times",
            pod.name, pod.namespace, data.restart_count
        );
        Self {
            id: AnomalyId::new(uuid::Uuid::new_v4().to_string()),
            kind: EventKind::PodCrashLoop,
            severity: if data.restart_count > 5 {
                Severity::Critical
            } else {
                Severity::Warning
            },
            detected_at: aion_common::types::now(),
            node: Some(node),
            pod: Some(pod),
            process: None,
            details: EventDetails::PodCrashLoop(data),
            description,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aion_common::handle::{NodeHandle, ProcessHandle};

    #[test]
    fn test_oom_event_creation() {
        let node = NodeHandle::new("worker-1", "uid-1");
        let process = ProcessHandle::new(1234, "stress-ng", 1000);
        let data = OomEventData {
            pid: 1234,
            comm: "stress-ng".to_string(),
            total_pages: 262144,
            rss_pages: 131072,
            oom_score_adj: 1000,
            memory_limit_bytes: 536_870_912,
            memory_usage_bytes: 536_870_000,
        };

        let event = InfraEvent::oom_kill(node, None, process, data);
        assert_eq!(event.kind, EventKind::OomKill);
        assert_eq!(event.severity, Severity::Critical);
        assert!(event.description.contains("stress-ng"));
    }

    #[test]
    fn test_crash_loop_event() {
        let node = NodeHandle::new("worker-1", "uid-1");
        let pod = PodHandle::new("default", "my-app-abc", "pod-uid-1");
        let data = CrashLoopData {
            restart_count: 10,
            last_exit_code: 137,
            last_reason: "OOMKilled".to_string(),
        };

        let event = InfraEvent::crash_loop(node, pod, data);
        assert_eq!(event.kind, EventKind::PodCrashLoop);
        assert_eq!(event.severity, Severity::Critical);
    }

    #[test]
    fn test_event_serde_roundtrip() {
        let node = NodeHandle::new("worker-1", "uid-1");
        let process = ProcessHandle::new(42, "nginx", 0);
        let data = OomEventData {
            pid: 42,
            comm: "nginx".to_string(),
            total_pages: 1000,
            rss_pages: 500,
            oom_score_adj: 0,
            memory_limit_bytes: 1_000_000,
            memory_usage_bytes: 999_000,
        };
        let event = InfraEvent::oom_kill(node, None, process, data);
        let json = serde_json::to_string(&event).unwrap();
        let parsed: InfraEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.kind, EventKind::OomKill);
    }

    #[test]
    fn test_event_kind_display() {
        assert_eq!(format!("{}", EventKind::OomKill), "oom_kill");
        assert_eq!(format!("{}", EventKind::PodCrashLoop), "pod_crash_loop");
    }
}
