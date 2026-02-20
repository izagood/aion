#![no_std]

/// OOM kill event sent from eBPF kernel program to userspace via perf ring buffer.
///
/// This struct must be `repr(C)` and have fixed-size fields for safe
/// transfer across the kernel/userspace boundary.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OomKillEvent {
    /// PID of the killed process
    pub pid: u32,
    /// UID of the killed process
    pub uid: u32,
    /// Process name (comm), null-terminated
    pub comm: [u8; 16],
    /// Total VM pages of the killed process
    pub total_vm_pages: u64,
    /// RSS pages at time of kill
    pub rss_pages: u64,
    /// OOM score adjustment
    pub oom_score_adj: i32,
    /// Padding to align struct
    pub _pad: u32,
}

impl OomKillEvent {
    /// Extract the process name as a string slice (up to first null byte).
    pub fn comm_str(&self) -> &str {
        let len = self.comm.iter().position(|&b| b == 0).unwrap_or(16);
        // Safety: comm should be valid UTF-8 process names from the kernel
        core::str::from_utf8(&self.comm[..len]).unwrap_or("<invalid>")
    }
}

#[cfg(test)]
mod tests {
    // Tests run in userspace only (cargo test includes std)
}
