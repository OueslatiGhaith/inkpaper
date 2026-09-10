use dhat::{Alloc, HeapStats};

#[global_allocator]
static ALLOCATOR: Alloc = Alloc;

/// reports allocations tracked by the active DHAT profiler. Peak and total cover the entire
/// run. Delta is relative to the supplied checkpoint. These are requested rust heap
/// bytes, not process RSS or stack usage
pub fn report(label: &str, baseline: usize) -> usize {
    let stats = HeapStats::get();
    let delta = stats.curr_bytes as i128 - baseline as i128;

    eprintln!(
        "heap: {label} live={} peak={} total={} delta={delta:+} bytes",
        stats.curr_bytes, stats.max_bytes, stats.total_bytes,
    );

    stats.curr_bytes
}
