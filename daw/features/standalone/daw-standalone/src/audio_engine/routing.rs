//! Process-wide, lock-free output routing for the cpal project mixer.
//!
//! The device is opened with its *full* channel count (see
//! `daw-audio-io::open_output`), so a single audio interface can carry the
//! main mix on one output pair, the guide/metronome on another, and a
//! "headphone-check" monitor bus (main + guide summed) on a third — each
//! routable to any channel pair, plus a mute for the main output so you can
//! silence the FOH send while still hearing everything in your IEMs.
//!
//! Written by the control thread (the metronome UI); read once per block by
//! the audio callback using only `Relaxed` atomics — never allocates, never
//! locks.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};

/// Live routing state shared between the UI and the audio callback.
pub struct MixerRouting {
    channel_count: AtomicUsize,
    main_l: AtomicUsize,
    main_r: AtomicUsize,
    main_muted: AtomicBool,
    guide_l: AtomicUsize,
    guide_r: AtomicUsize,
    phones_enabled: AtomicBool,
    phones_l: AtomicUsize,
    phones_r: AtomicUsize,
}

impl MixerRouting {
    /// The one process-wide instance. Mirrors
    /// `duplex_engine::PhonesBus::shared()`.
    pub fn shared() -> &'static MixerRouting {
        static R: MixerRouting = MixerRouting {
            channel_count: AtomicUsize::new(2),
            main_l: AtomicUsize::new(0),
            main_r: AtomicUsize::new(1),
            main_muted: AtomicBool::new(false),
            guide_l: AtomicUsize::new(0),
            guide_r: AtomicUsize::new(1),
            phones_enabled: AtomicBool::new(false),
            phones_l: AtomicUsize::new(2),
            phones_r: AtomicUsize::new(3),
        };
        &R
    }

    // --- control-thread setters -------------------------------------------

    /// Record the device's actual output channel count (set when the stream
    /// opens). The UI clamps its channel pickers to this.
    pub fn set_channel_count(&self, n: usize) {
        self.channel_count.store(n.max(1), Relaxed);
    }
    pub fn set_main_pair(&self, l: usize, r: usize) {
        self.main_l.store(l, Relaxed);
        self.main_r.store(r, Relaxed);
    }
    pub fn set_main_muted(&self, muted: bool) {
        self.main_muted.store(muted, Relaxed);
    }
    pub fn set_guide_pair(&self, l: usize, r: usize) {
        self.guide_l.store(l, Relaxed);
        self.guide_r.store(r, Relaxed);
    }
    pub fn set_phones_enabled(&self, enabled: bool) {
        self.phones_enabled.store(enabled, Relaxed);
    }
    pub fn set_phones_pair(&self, l: usize, r: usize) {
        self.phones_l.store(l, Relaxed);
        self.phones_r.store(r, Relaxed);
    }

    // --- getters ----------------------------------------------------------

    pub fn channel_count(&self) -> usize {
        self.channel_count.load(Relaxed)
    }
    pub fn main_pair(&self) -> (usize, usize) {
        (self.main_l.load(Relaxed), self.main_r.load(Relaxed))
    }
    pub fn main_muted(&self) -> bool {
        self.main_muted.load(Relaxed)
    }
    pub fn guide_pair(&self) -> (usize, usize) {
        (self.guide_l.load(Relaxed), self.guide_r.load(Relaxed))
    }
    pub fn phones_enabled(&self) -> bool {
        self.phones_enabled.load(Relaxed)
    }
    pub fn phones_pair(&self) -> (usize, usize) {
        (self.phones_l.load(Relaxed), self.phones_r.load(Relaxed))
    }

    /// Read every field once for use across a single audio block.
    pub fn snapshot(&self) -> RoutingSnapshot {
        RoutingSnapshot {
            main: self.main_pair(),
            main_muted: self.main_muted(),
            guide: self.guide_pair(),
            phones_enabled: self.phones_enabled(),
            phones: self.phones_pair(),
        }
    }
}

/// An immutable copy of the routing for one audio block — read the atomics
/// once, then apply consistently across every frame.
#[derive(Clone, Copy, Debug)]
pub struct RoutingSnapshot {
    pub main: (usize, usize),
    pub main_muted: bool,
    pub guide: (usize, usize),
    pub phones_enabled: bool,
    pub phones: (usize, usize),
}

/// Zero the interleaved `stage` block and write the stereo project mix onto
/// the main output pair. The guide/aux hook adds onto the guide pair next,
/// then [`finish_routing`] composes the phones sum + main mute.
///
/// `block` is interleaved stereo (`l, r, l, r, …`); `stage` is `num_frames *
/// channels`. Pure and alloc-free.
#[inline]
pub fn stage_main(
    stage: &mut [f32],
    channels: usize,
    num_frames: usize,
    block: &[f32],
    snap: RoutingSnapshot,
) {
    let end = (num_frames * channels).min(stage.len());
    stage[..end].fill(0.0);
    let (ml, mr) = snap.main;
    for frame in 0..num_frames {
        let l = block.get(frame * 2).copied().unwrap_or(0.0);
        let r = block.get(frame * 2 + 1).copied().unwrap_or(0.0);
        let base = frame * channels;
        if ml < channels {
            stage[base + ml] += l;
        }
        if mr < channels {
            stage[base + mr] += r;
        }
    }
}

/// After the guide/aux has been added to the guide pair, compose the
/// headphone-check sum bus (main + guide) onto the phones pair, then apply
/// the main-output mute. Pure and alloc-free over `stage`.
#[inline]
pub fn finish_routing(
    stage: &mut [f32],
    channels: usize,
    num_frames: usize,
    snap: RoutingSnapshot,
) {
    let (ml, mr) = snap.main;
    let (gl, gr) = snap.guide;
    let (pl, pr) = snap.phones;
    // If the guide shares the main pair its signal is already inside the
    // main channels — don't add it twice into the phones sum.
    let guide_separate = (gl, gr) != (ml, mr);

    if snap.phones_enabled {
        for frame in 0..num_frames {
            let base = frame * channels;
            let main_l = if ml < channels { stage[base + ml] } else { 0.0 };
            let main_r = if mr < channels { stage[base + mr] } else { 0.0 };
            let (g_l, g_r) = if guide_separate {
                (
                    if gl < channels { stage[base + gl] } else { 0.0 },
                    if gr < channels { stage[base + gr] } else { 0.0 },
                )
            } else {
                (0.0, 0.0)
            };
            if pl < channels {
                stage[base + pl] += main_l + g_l;
            }
            if pr < channels {
                stage[base + pr] += main_r + g_r;
            }
        }
    }

    if snap.main_muted {
        for frame in 0..num_frames {
            let base = frame * channels;
            if ml < channels {
                stage[base + ml] = 0.0;
            }
            if mr < channels {
                stage[base + mr] = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(
        main: (usize, usize),
        guide: (usize, usize),
        phones: (usize, usize),
        phones_enabled: bool,
        main_muted: bool,
    ) -> RoutingSnapshot {
        RoutingSnapshot {
            main,
            main_muted,
            guide,
            phones_enabled,
            phones,
        }
    }

    #[test]
    fn main_lands_on_configured_pair() {
        let channels = 6;
        let frames = 2;
        let mut stage = vec![9.0; frames * channels]; // garbage to prove zeroing
        let block = [1.0, 2.0, 3.0, 4.0]; // (l,r) x2
        let s = snap((4, 5), (0, 1), (2, 3), false, false);
        stage_main(&mut stage, channels, frames, &block, s);
        // Frame 0: main pair 4/5 = 1,2; everything else zero.
        assert_eq!(stage[4], 1.0);
        assert_eq!(stage[5], 2.0);
        assert_eq!(stage[0], 0.0);
        // Frame 1: 3,4 on 4/5.
        assert_eq!(stage[channels + 4], 3.0);
        assert_eq!(stage[channels + 5], 4.0);
    }

    #[test]
    fn phones_sum_is_main_plus_separate_guide() {
        let channels = 6;
        let frames = 1;
        let mut stage = vec![0.0; frames * channels];
        // main on 0/1, guide on 2/3, phones on 4/5.
        let s = snap((0, 1), (2, 3), (4, 5), true, false);
        // simulate main mix + guide already written into the stage:
        stage[0] = 0.5; // main L
        stage[1] = 0.6; // main R
        stage[2] = 0.1; // guide L
        stage[3] = 0.2; // guide R
        finish_routing(&mut stage, channels, frames, s);
        assert!((stage[4] - 0.6).abs() < 1e-6); // 0.5 + 0.1
        assert!((stage[5] - 0.8).abs() < 1e-6); // 0.6 + 0.2
        // main and guide untouched.
        assert_eq!(stage[0], 0.5);
        assert_eq!(stage[2], 0.1);
    }

    #[test]
    fn phones_sum_does_not_double_count_guide_on_main_pair() {
        let channels = 4;
        let frames = 1;
        let mut stage = vec![0.0; frames * channels];
        // guide shares main pair 0/1; phones on 2/3.
        let s = snap((0, 1), (0, 1), (2, 3), true, false);
        stage[0] = 0.7; // main (already includes guide)
        stage[1] = 0.9;
        finish_routing(&mut stage, channels, frames, s);
        assert!((stage[2] - 0.7).abs() < 1e-6); // not 0.7*2
        assert!((stage[3] - 0.9).abs() < 1e-6);
    }

    #[test]
    fn mute_zeros_main_but_keeps_phones_sum() {
        let channels = 6;
        let frames = 1;
        let mut stage = vec![0.0; frames * channels];
        let s = snap((0, 1), (2, 3), (4, 5), true, true); // muted
        stage[0] = 0.5;
        stage[1] = 0.6;
        stage[2] = 0.1;
        stage[3] = 0.2;
        finish_routing(&mut stage, channels, frames, s);
        // phones still carries the full mix...
        assert!((stage[4] - 0.6).abs() < 1e-6);
        assert!((stage[5] - 0.8).abs() < 1e-6);
        // ...but the device main output is silenced.
        assert_eq!(stage[0], 0.0);
        assert_eq!(stage[1], 0.0);
        // guide send is independent of the main mute.
        assert_eq!(stage[2], 0.1);
    }
}
