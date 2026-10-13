//! SMPTE 330M UMID generation for freshly created AAF Mob objects.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::parse::auid::MobId;

/// Monotonic counter to ensure uniqueness within a single process run.
static COUNTER: AtomicU64 = AtomicU64::new(1);

/// SMPTE 330M "packed" UMID label prefix (12 bytes).
const UMID_LABEL: [u8; 12] = [
    0x06, 0x0A, 0x2B, 0x34, // ISO/SMPTE OID prefix
    0x01, 0x01, 0x01, 0x05, // AAF category + version
    0x01, 0x01, 0x0D, 0x00, // local use
];

/// Generate a fresh SMPTE 330M Type 1 UMID suitable as an AAF Mob ID.
///
/// Uses a mix of wall-clock nanoseconds and an atomic counter so that two
/// calls within the same nanosecond still produce distinct IDs.
pub fn new_mob_id() -> MobId {
    let now_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);

    let mut bytes = [0u8; 32];

    // ── UMID prefix (bytes 0–12) ─────────────────────────────────────────────
    bytes[0..12].copy_from_slice(&UMID_LABEL);

    // Length byte: 0x13 (19 remaining bytes after this position)
    bytes[12] = 0x13;

    // Instance number (3 bytes) — lower 3 bytes of the atomic counter.
    bytes[13] = (seq >> 16) as u8;
    bytes[14] = (seq >> 8) as u8;
    bytes[15] = seq as u8;

    // ── Material number (bytes 16–31) ────────────────────────────────────────
    // Mix time × counter with two multiplicative hashes to spread entropy.
    let a = now_ns.wrapping_add(seq.wrapping_mul(0x517C_C1B7_2722_0A95));
    let b = seq.wrapping_add(now_ns.wrapping_mul(0x9E37_79B9_7F4A_7C15));

    bytes[16..24].copy_from_slice(&a.to_le_bytes());
    bytes[24..32].copy_from_slice(&b.to_le_bytes());

    MobId(bytes)
}
