use serde::{Deserialize, Serialize};

use crate::types::{Namespace, NodeName, PodName};

/// Opaque handle to a Kubernetes pod.
/// AI agents can only access pod data through MCP tools using this handle.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PodHandle {
    pub namespace: Namespace,
    pub name: PodName,
    pub uid: String,
    pub node_name: Option<NodeName>,
}

impl PodHandle {
    pub fn new(
        namespace: impl Into<Namespace>,
        name: impl Into<PodName>,
        uid: impl Into<String>,
    ) -> Self {
        Self {
            namespace: namespace.into(),
            name: name.into(),
            uid: uid.into(),
            node_name: None,
        }
    }

    pub fn with_node(mut self, node_name: impl Into<NodeName>) -> Self {
        self.node_name = Some(node_name.into());
        self
    }
}

/// Opaque handle to a Linux process.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProcessHandle {
    pub pid: u32,
    pub comm: String,
    pub uid: u32,
}

impl ProcessHandle {
    pub fn new(pid: u32, comm: impl Into<String>, uid: u32) -> Self {
        Self {
            pid,
            comm: comm.into(),
            uid,
        }
    }
}

/// Opaque handle to a Kubernetes node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeHandle {
    pub name: NodeName,
    pub uid: String,
}

impl NodeHandle {
    pub fn new(name: impl Into<NodeName>, uid: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            uid: uid.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pod_handle_creation() {
        let handle = PodHandle::new("default", "my-pod", "uid-123")
            .with_node("node-1");
        assert_eq!(handle.namespace.as_str(), "default");
        assert_eq!(handle.name.as_str(), "my-pod");
        assert_eq!(handle.node_name.as_ref().unwrap().as_str(), "node-1");
    }

    #[test]
    fn test_process_handle() {
        let handle = ProcessHandle::new(1234, "nginx", 0);
        assert_eq!(handle.pid, 1234);
        assert_eq!(handle.comm, "nginx");
    }

    #[test]
    fn test_node_handle() {
        let handle = NodeHandle::new("worker-1", "uid-456");
        assert_eq!(handle.name.as_str(), "worker-1");
    }

    #[test]
    fn test_handle_serde_roundtrip() {
        let handle = PodHandle::new("kube-system", "coredns-abc", "uid-789");
        let json = serde_json::to_string(&handle).unwrap();
        let parsed: PodHandle = serde_json::from_str(&json).unwrap();
        assert_eq!(handle, parsed);
    }
}
