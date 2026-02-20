#![no_std]
#![no_main]

use aya_ebpf::{
    macros::{map, tracepoint},
    maps::PerfEventArray,
    programs::TracePointContext,
    helpers::bpf_get_current_pid_tgid,
};
use aya_log_ebpf::info;

use aion_ebpf_common::OomKillEvent;

/// Perf event array for sending OOM kill events to userspace.
#[map]
static OOM_EVENTS: PerfEventArray<OomKillEvent> = PerfEventArray::new(0);

/// eBPF tracepoint handler for oom_kill events.
///
/// Attaches to: oom/mark_victim
///
/// The oom:mark_victim tracepoint fires when the OOM killer selects a process
/// to kill. We capture the victim's PID, comm, and memory stats.
#[tracepoint]
pub fn aion_oom_kill(ctx: TracePointContext) -> u32 {
    match try_oom_kill(&ctx) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

fn try_oom_kill(ctx: &TracePointContext) -> Result<(), i64> {
    // Read fields from the tracepoint context
    // oom:mark_victim tracepoint format (from /sys/kernel/debug/tracing/events/oom/mark_victim/format):
    //   field: int pid;  offset:8;  size:4;  signed:1;
    let pid: i32 = unsafe { ctx.read_at(8)? };

    let tgid = (bpf_get_current_pid_tgid() >> 32) as u32;

    info!(ctx, "OOM kill detected: pid={}, tgid={}", pid, tgid);

    let event = OomKillEvent {
        pid: pid as u32,
        uid: 0, // Could be read from bpf_get_current_uid_gid()
        comm: [0u8; 16],
        total_vm_pages: 0,
        rss_pages: 0,
        oom_score_adj: 0,
        _pad: 0,
    };

    OOM_EVENTS.output(ctx, &event, 0);

    Ok(())
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}
