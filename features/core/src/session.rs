//! One Signal session shared by the instances on a kit's tracks.
//!
//! In the per-track mode Signal sits on every track of a kit (Sum, Sample,
//! Sub, Verb, each mic) and each instance is a *port* of one session. What
//! crosses between them is data, never audio: the host's processing order
//! isn't Signal's, and pulling audio across would add a block of latency.
//!
//! - **Audio**: each instance processes its own track. The host routes and
//!   sums.
//! - **Data**: hits and triggers (a `Trig` keying a `Sample`), gate keys,
//!   the shared preset and its macros, analysis. Hits go through a
//!   [`HitBus`], lock-free, so one instance's audio thread can publish
//!   what another's reads.
//!
//! Instances find each other by the [`SessionId`] saved in their state:
//! in one process through [`Registry`] (the core library's own state);
//! across processes (a bridged or sandboxed plugin) through a socket or
//! shared memory, later, with the degradation shown in the UI.
//!
//! This is the first cut: the ids, the hit bus and the in-process
//! registry. The kit's preset revisions, macro sharing and the
//! cross-process link come with the drum submixer (signal#26 phase 4).

use alloc::string::String;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// A session's id, saved in each instance's state. Instances with the same
/// id in one process share one session.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub String);

/// Which port of the session an instance is: its track's role in the kit
/// (`Kick/In`, `Kick/Sum`, `Snare/Trig`, `Rooms`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PortName(pub String);

/// What an instance in the per-track mode saves: its session, and its port.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLink {
    pub session: SessionId,
    pub port: PortName,
}

/// A hit one port published: a piece was struck.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SessionHit {
    /// The piece, as the session numbers them (the preset's lane index).
    pub piece: u16,
    /// 0–1.
    pub velocity: f32,
    /// The song position in frames at which it happened.
    pub frame: u64,
    /// Played as a note (MIDI: the host's hit notes, a drum track's), not
    /// detected on a trigger mic. A piece with notes is keyed by them; its
    /// trigger's detection is the fallback.
    pub midi: bool,
}

/// The piece's top bit, in a slot: the hit came from MIDI.
const MIDI_BIT: u32 = 1 << 31;

/// How many hits the bus holds before the oldest is overwritten. At a
/// 64-frame block and 48 kHz that is more than a second of a blast beat on
/// every piece, longer than any reader goes between blocks.
pub const HIT_BUS_SLOTS: usize = 256;

/// A many-writer, many-reader ring of hits, without locks or allocation.
///
/// Every slot carries a sequence number: a writer claims the next index,
/// marks the slot busy (odd), writes, then publishes it (even, derived from
/// the index). A reader keeps its own [`HitCursor`] and takes what was
/// published since; a slot that was overwritten under it is skipped, and
/// [`HitCursor::missed`] counts what it lost.
pub struct HitBus {
    next: AtomicU64,
    slots: [Slot; HIT_BUS_SLOTS],
}

struct Slot {
    seq: AtomicU64,
    frame: AtomicU64,
    /// piece (high 16 bits) and velocity (low 16 bits, 0–65535).
    data: AtomicU32,
}

impl Default for HitBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Where one reader is in a [`HitBus`].
#[derive(Clone, Copy, Debug, Default)]
pub struct HitCursor {
    next: u64,
    missed: u64,
}

impl HitCursor {
    /// Hits that were overwritten before this reader took them.
    #[must_use]
    pub const fn missed(&self) -> u64 {
        self.missed
    }
}

const SLOTS_U64: u64 = 256;
const _: () = assert!(SLOTS_U64 == 256 && HIT_BUS_SLOTS == 256);

impl HitBus {
    #[must_use]
    pub fn new() -> Self {
        Self {
            next: AtomicU64::new(0),
            slots: core::array::from_fn(|_| Slot {
                seq: AtomicU64::new(u64::MAX),
                frame: AtomicU64::new(0),
                data: AtomicU32::new(0),
            }),
        }
    }

    fn slot(&self, index: u64) -> Option<&Slot> {
        self.slots.get(usize::try_from(index % SLOTS_U64).ok()?)
    }

    /// Publish a hit. Real-time safe.
    pub fn publish(&self, hit: SessionHit) {
        let index = self.next.fetch_add(1, Ordering::AcqRel);
        let Some(slot) = self.slot(index) else { return };
        let busy = index.wrapping_mul(2).wrapping_add(1);
        slot.seq.store(busy, Ordering::Release);
        slot.frame.store(hit.frame, Ordering::Relaxed);
        let vel =
            signal_dsp_core::num::f64_to_u32(f64::from(hit.velocity.clamp(0.0, 1.0)) * 65535.0)
                .min(65535);
        let midi = if hit.midi { MIDI_BIT } else { 0 };
        slot.data.store(
            midi | (u32::from(hit.piece & 0x7fff) << 16) | vel,
            Ordering::Relaxed,
        );
        slot.seq.store(index.wrapping_mul(2), Ordering::Release);
    }

    /// A cursor that reads only what is published from now on.
    #[must_use]
    pub fn cursor(&self) -> HitCursor {
        HitCursor {
            next: self.next.load(Ordering::Acquire),
            missed: 0,
        }
    }

    /// Hand every hit published since `cursor` to `each`, oldest first.
    /// Real-time safe.
    pub fn drain(&self, cursor: &mut HitCursor, mut each: impl FnMut(SessionHit)) {
        let end = self.next.load(Ordering::Acquire);
        if end.saturating_sub(cursor.next) > SLOTS_U64 {
            let skip = end.saturating_sub(SLOTS_U64);
            cursor.missed = cursor
                .missed
                .saturating_add(skip.saturating_sub(cursor.next));
            cursor.next = skip;
        }
        while cursor.next < end {
            let index = cursor.next;
            cursor.next = index.saturating_add(1);
            let Some(slot) = self.slot(index) else {
                continue;
            };
            let want = index.wrapping_mul(2);
            if slot.seq.load(Ordering::Acquire) != want {
                // Still being written, or already overwritten.
                cursor.missed = cursor.missed.saturating_add(1);
                continue;
            }
            let frame = slot.frame.load(Ordering::Relaxed);
            let data = slot.data.load(Ordering::Relaxed);
            if slot.seq.load(Ordering::Acquire) != want {
                cursor.missed = cursor.missed.saturating_add(1);
                continue;
            }
            let piece = u16::try_from((data >> 16) & 0x7fff).unwrap_or(0);
            let vel = f32::from(u16::try_from(data & 0xffff).unwrap_or(0)) / 65535.0;
            each(SessionHit {
                piece,
                velocity: vel,
                frame,
                midi: data & MIDI_BIT != 0,
            });
        }
    }
}

/// One session, as the instances in it share it.
pub struct Session {
    pub id: SessionId,
    /// Hits and triggers between the session's ports.
    pub hits: HitBus,
}

#[cfg(feature = "std")]
pub use registry::Registry;

#[cfg(feature = "std")]
mod registry {
    use super::{HitBus, Session, SessionId};
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

    /// The process's sessions: the core library's own state, so every
    /// instance of the CLAP in one process (and the linked core in
    /// Studio's engine) finds the same one by id. A session lives while
    /// an instance holds it.
    pub struct Registry;

    fn sessions() -> &'static Mutex<BTreeMap<SessionId, Weak<Session>>> {
        static SESSIONS: OnceLock<Mutex<BTreeMap<SessionId, Weak<Session>>>> = OnceLock::new();
        SESSIONS.get_or_init(|| Mutex::new(BTreeMap::new()))
    }

    impl Registry {
        /// Join the session `id`, opening it if no instance has. Control
        /// thread only (it takes the registry's lock).
        #[must_use]
        #[allow(
            clippy::disallowed_methods,
            reason = "control thread: joining a session"
        )]
        pub fn join(id: &SessionId) -> Arc<Session> {
            let mut map = sessions().lock().unwrap_or_else(PoisonError::into_inner);
            map.retain(|_, s| s.strong_count() > 0);
            if let Some(s) = map.get(id).and_then(Weak::upgrade) {
                return s;
            }
            let s = Arc::new(Session {
                id: id.clone(),
                hits: HitBus::new(),
            });
            map.insert(id.clone(), Arc::downgrade(&s));
            tracing::debug!(session = %id.0, "signal session opened");
            s
        }

        /// Sessions open in this process.
        #[must_use]
        #[allow(clippy::disallowed_methods, reason = "control thread")]
        pub fn open_count() -> usize {
            let map = sessions().lock().unwrap_or_else(PoisonError::into_inner);
            map.values().filter(|s| s.strong_count() > 0).count()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hit_one_port_publishes_another_reads() {
        let bus = HitBus::new();
        let mut reader = bus.cursor();
        bus.publish(SessionHit {
            piece: 3,
            velocity: 0.5,
            frame: 4800,
            midi: false,
        });
        bus.publish(SessionHit {
            piece: 1,
            velocity: 1.0,
            frame: 4810,
            midi: false,
        });
        let mut got = alloc::vec::Vec::new();
        bus.drain(&mut reader, |h| got.push(h));
        assert_eq!(got.len(), 2);
        assert_eq!(got.first().map(|h| (h.piece, h.frame)), Some((3, 4800)));
        assert!((got.first().map_or(0.0, |h| h.velocity) - 0.5).abs() < 1e-4);
        // Nothing twice.
        bus.drain(&mut reader, |_| panic!("read twice"));
    }

    #[test]
    fn a_slow_reader_skips_what_was_overwritten() {
        let bus = HitBus::new();
        let mut reader = bus.cursor();
        for i in 0..300u64 {
            bus.publish(SessionHit {
                piece: 0,
                velocity: 1.0,
                frame: i,
                midi: false,
            });
        }
        let mut first = None;
        let mut n = 0;
        bus.drain(&mut reader, |h| {
            first.get_or_insert(h.frame);
            n += 1;
        });
        assert_eq!(n, HIT_BUS_SLOTS);
        assert_eq!(first, Some(44));
        assert_eq!(reader.missed(), 44);
    }

    #[cfg(feature = "std")]
    #[test]
    fn instances_with_one_id_share_one_session() {
        let id = SessionId("kit-test".into());
        let a = Registry::join(&id);
        let b = Registry::join(&id);
        let mut from_b = b.hits.cursor();
        a.hits.publish(SessionHit {
            piece: 0,
            velocity: 1.0,
            frame: 1,
            midi: false,
        });
        let mut n = 0;
        b.hits.drain(&mut from_b, |_| n += 1);
        assert_eq!(n, 1);
        assert!(std::sync::Arc::ptr_eq(&a, &b));
        let other = Registry::join(&SessionId("other-kit".into()));
        assert!(!std::sync::Arc::ptr_eq(&a, &other));
    }
}
