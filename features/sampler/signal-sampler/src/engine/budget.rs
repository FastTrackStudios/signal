//! **Preload memory budget** — one process-wide ceiling on decoded samples.
//!
//! Every sample cache in the process charges its decoded bytes here and
//! releases them on eviction or drop, so the engine's RAM use is bounded no
//! matter how many rigs, layers, modules or packs are open, and no matter
//! which preload profile a caller picked. Preloading is *eager* by nature: a
//! profile that means "load 512 samples per block" is fine for one block and
//! is 11 GB across a full engine. The per-block caps stay (they decide what
//! is worth loading *first*); this is the floor under them that a forgotten
//! cap can't fall through.
//!
//! When the budget is used up, preloads stop and the remaining samples stream
//! from disk on demand — the rig stays playable, it just warms as you play.
//!
//! Override with `FTS_PRELOAD_BUDGET_MB` (0 = unlimited, for offline renders
//! and bounce jobs that genuinely want the whole library resident).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static USED: AtomicU64 = AtomicU64::new(0);
static WARNED: AtomicBool = AtomicBool::new(false);

/// Fallback when the machine's RAM can't be read.
const DEFAULT_BUDGET_MB: u64 = 3072;
/// Never take more than this share of the machine, however big it is — the
/// engine shares the box with a DAW, a browser and the OS page cache.
const MAX_SHARE: f64 = 0.15;
/// …and never less than this, or nothing preloads at all.
const MIN_BUDGET_MB: u64 = 384;
/// …and never more than this either. A 192 GB workstation does not want 28 GB
/// of decoded piano resident; past a few GB the win is gone and the cost is a
/// swapping machine mid-service.
const MAX_BUDGET_MB: u64 = 4096;
/// Phones get a hard, small ceiling: iOS kills a process that grows, it does
/// not swap.
#[cfg(any(target_os = "ios", target_os = "android"))]
const MOBILE_BUDGET_MB: u64 = 384;

/// The ceiling in bytes; `u64::MAX` when explicitly unlimited.
pub fn limit_bytes() -> u64 {
    static LIMIT: OnceLock<u64> = OnceLock::new();
    *LIMIT.get_or_init(|| {
        if let Ok(mb) = std::env::var("FTS_PRELOAD_BUDGET_MB") {
            if let Ok(mb) = mb.trim().parse::<u64>() {
                let limit = if mb == 0 { u64::MAX } else { mb * 1024 * 1024 };
                tracing::info!(
                    budget_mb = if mb == 0 { u64::MAX } else { mb },
                    "sampler: preload budget from FTS_PRELOAD_BUDGET_MB"
                );
                return limit;
            }
        }
        #[cfg(any(target_os = "ios", target_os = "android"))]
        let mb = MOBILE_BUDGET_MB;
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        let mb = match machine_ram_mb() {
            Some(total) => {
                ((total as f64 * MAX_SHARE) as u64).clamp(MIN_BUDGET_MB, MAX_BUDGET_MB)
            }
            None => DEFAULT_BUDGET_MB,
        };
        tracing::info!(budget_mb = mb, "sampler: preload budget");
        mb * 1024 * 1024
    })
}

/// Total RAM in MB, from `/proc/meminfo` (Linux) or `sysctl` (macOS).
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn machine_ram_mb() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let line = text.lines().find(|l| l.starts_with("MemTotal:"))?;
        let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
        return Some(kb / 1024);
    }
    #[cfg(not(target_os = "linux"))]
    {
        let out = std::process::Command::new("sysctl").args(["-n", "hw.memsize"]).output().ok()?;
        let bytes: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        Some(bytes / (1024 * 1024))
    }
}

/// Decoded bytes currently resident across every cache in the process.
pub fn used_bytes() -> u64 {
    USED.load(Ordering::Relaxed)
}

/// Headroom left before preloading stops.
pub fn remaining_bytes() -> u64 {
    limit_bytes().saturating_sub(used_bytes())
}

/// Whether preloading should stop. On-demand loads (a note that needs a
/// sample *now*) ignore this — correctness beats the ceiling for one sample.
pub fn exhausted() -> bool {
    used_bytes() >= limit_bytes()
}

/// Reserve an *estimated* size before decoding. Returns `false` when the
/// reservation would cross the ceiling — the caller skips that sample.
///
/// Preloads decode in parallel, so charging only after the decode lets a
/// whole pool of in-flight samples sail past the limit. Reserving first
/// bounds the overshoot to nothing; the estimate is swapped for the real
/// size at insert time.
pub fn try_reserve(bytes: usize) -> bool {
    let limit = limit_bytes();
    let mut cur = USED.load(Ordering::Relaxed);
    loop {
        let next = cur.saturating_add(bytes as u64);
        if next > limit {
            if limit != u64::MAX && !WARNED.swap(true, Ordering::Relaxed) {
                tracing::warn!(
                    used_mb = cur / (1024 * 1024),
                    budget_mb = limit / (1024 * 1024),
                    "sampler: preload budget reached — remaining samples stream from disk \
                     (raise with FTS_PRELOAD_BUDGET_MB)"
                );
            }
            return false;
        }
        match USED.compare_exchange_weak(cur, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return true,
            Err(actual) => cur = actual,
        }
    }
}

/// Charge decoded bytes as they enter a cache.
pub fn charge(bytes: usize) {
    let used = USED.fetch_add(bytes as u64, Ordering::Relaxed) + bytes as u64;
    let limit = limit_bytes();
    if used >= limit && limit != u64::MAX && !WARNED.swap(true, Ordering::Relaxed) {
        tracing::warn!(
            used_mb = used / (1024 * 1024),
            budget_mb = limit / (1024 * 1024),
            "sampler: preload budget reached — remaining samples stream from disk \
             (raise with FTS_PRELOAD_BUDGET_MB)"
        );
    }
}

/// Release decoded bytes on eviction or cache drop.
pub fn release(bytes: usize) {
    let mut cur = USED.load(Ordering::Relaxed);
    loop {
        let next = cur.saturating_sub(bytes as u64);
        match USED.compare_exchange_weak(cur, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(actual) => cur = actual,
        }
    }
    // Re-arm the warning only after a real drop — a preload that is sitting
    // AT the ceiling releases its per-sample reservation constantly, and
    // re-arming on that turns one warning into hundreds.
    let limit = limit_bytes();
    if limit != u64::MAX && used_bytes() < limit / 4 * 3 {
        WARNED.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charge_and_release_are_symmetric() {
        let before = used_bytes();
        charge(1024);
        assert_eq!(used_bytes(), before + 1024);
        release(1024);
        assert_eq!(used_bytes(), before);
        // Releasing more than was charged floors at zero instead of wrapping.
        release(usize::MAX);
        assert_eq!(used_bytes(), 0);
    }
}
