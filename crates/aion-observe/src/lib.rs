pub mod collector;
pub mod event;
pub mod kube_watcher;
pub mod oom_watcher;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ObserveError {
    #[error("eBPF error: {0}")]
    Ebpf(String),

    #[error("kubernetes error: {0}")]
    Kube(String),

    #[error("cgroup error: {0}")]
    Cgroup(String),

    #[error("collector '{name}' failed: {reason}")]
    Collector { name: String, reason: String },

    #[error(transparent)]
    Io(#[from] std::io::Error),
}
