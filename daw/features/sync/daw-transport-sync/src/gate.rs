//! Which snapshots are worth sending.
//!
//! An audio thread writes a snapshot every buffer (a few hundred a
//! second); a carrier moving positions to followers needs far fewer. A
//! follower projects the last position it has, so a new one only tells
//! it something when the projection would now be wrong — the transport
//! started, stopped, jumped, or changed rate — or when it has gone a
//! while without hearing (a keepalive, so it knows the leader is there
//! and its projection is fresh). [`PublishGate`] is that decision, for
//! one transport.

use crate::AudioSnapshot;

/// Decides, snapshot by snapshot, whether to publish.
#[derive(Clone, Copy, Debug)]
pub struct PublishGate {
    last: Option<AudioSnapshot>,
    keepalive_micros: f64,
    jump_seconds: f64,
}

impl Default for PublishGate {
    /// A keepalive every 20 ms; a jump is anything over 250 µs (12
    /// samples at 48 kHz) from where the last published position
    /// projects.
    fn default() -> Self {
        Self::new(20_000.0, 250e-6)
    }
}

impl PublishGate {
    /// Publish at least every `keepalive_micros` (of the snapshots' own
    /// clock), and whenever the playhead is more than `jump_seconds`
    /// from where the last published position projects.
    #[must_use]
    pub const fn new(keepalive_micros: f64, jump_seconds: f64) -> Self {
        Self {
            last: None,
            keepalive_micros,
            jump_seconds,
        }
    }

    /// The last snapshot this gate let through.
    #[must_use]
    pub const fn last(&self) -> Option<&AudioSnapshot> {
        self.last.as_ref()
    }

    /// Forget what was published (a new subscriber should hear at once).
    pub fn reset(&mut self) {
        self.last = None;
    }

    /// Whether `snap` should be published; if so, it becomes the one
    /// later snapshots are judged against. The same snapshot twice is
    /// never published twice.
    pub fn offer(&mut self, snap: &AudioSnapshot) -> bool {
        let Some(last) = self.last else {
            self.last = Some(*snap);
            return true;
        };
        if snap.sequence == last.sequence
            && snap.host_micros.to_bits() == last.host_micros.to_bits()
        {
            return false;
        }
        let changed = snap.is_playing != last.is_playing
            || snap.playrate.to_bits() != last.playrate.to_bits()
            || snap.sequence < last.sequence
            || snap.host_micros < last.host_micros
            || (snap.playhead_seconds - last.position().at(snap.host_micros)).abs()
                > self.jump_seconds;
        let due = snap.host_micros - last.host_micros >= self.keepalive_micros;
        if changed || due {
            self.last = Some(*snap);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playing(sequence: u64, host_micros: f64) -> AudioSnapshot {
        AudioSnapshot {
            sequence,
            host_micros,
            playhead_seconds: host_micros * 1e-6,
            playrate: 1.0,
            is_playing: true,
            ..AudioSnapshot::default()
        }
    }

    #[test]
    fn a_steady_transport_is_published_at_the_keepalive() {
        let mut gate = PublishGate::default();
        // 256-frame buffers at 48 kHz: 5.33 ms apart.
        let period = 256.0 / 48_000.0 * 1e6;
        let sent: Vec<u64> = (0..40_u32)
            .map(|b| playing(u64::from(b), f64::from(b) * period))
            .filter(|s| gate.offer(s))
            .map(|s| s.sequence)
            .collect();
        // The first, then every fourth buffer (21.3 ms ≥ 20 ms).
        assert_eq!(sent, vec![0, 4, 8, 12, 16, 20, 24, 28, 32, 36]);
    }

    #[test]
    fn a_change_is_published_at_once() {
        let mut gate = PublishGate::default();
        assert!(gate.offer(&playing(0, 0.0)));
        assert!(!gate.offer(&playing(1, 5_000.0)));
        // Stopped.
        assert!(gate.offer(&AudioSnapshot {
            is_playing: false,
            ..playing(2, 10_000.0)
        }));
        // Playing again, located.
        assert!(gate.offer(&AudioSnapshot {
            playhead_seconds: 30.0,
            ..playing(3, 15_000.0)
        }));
        // Rate.
        assert!(gate.offer(&AudioSnapshot {
            playhead_seconds: 30.005,
            playrate: 1.001,
            ..playing(4, 20_000.0)
        }));
    }

    #[test]
    fn the_same_snapshot_is_published_once() {
        let mut gate = PublishGate::new(0.0, 1.0);
        let s = playing(7, 1_000.0);
        assert!(gate.offer(&s));
        assert!(!gate.offer(&s));
    }
}
