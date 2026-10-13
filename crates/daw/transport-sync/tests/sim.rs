//! Two engines, simulated: the leader, and a follower whose clock started
//! at a different moment and whose sample clock runs fast — kept in step
//! by estimating the clock offset over a jittery network and correcting
//! drift, as a real pair would be.

#![allow(clippy::unwrap_used, clippy::cast_precision_loss, clippy::float_cmp)]

use std::cell::RefCell;

use daw_transport_sync::{
    AudioSnapshot, BufferClock, ClockEstimator, Correction, Follower, Position, TransportBackend,
};

/// An engine driven by simulated real time `t` (seconds).
struct Sim {
    /// Its clock reads `t * 1e6 + epoch` (µs).
    epoch: f64,
    /// Its device runs this much fast (1.0 = exact).
    speed: f64,
    sample_rate: f64,
    buffer: u32,
    state: RefCell<State>,
}

#[derive(Default)]
struct State {
    t: f64,
    playhead: f64,
    playing: bool,
    rate: f64,
    sequence: u64,
    snapshot: Option<AudioSnapshot>,
    /// A locate waiting for its moment: (clock µs, position, playing, rate).
    armed: Option<(f64, f64, bool, f64)>,
    locates: usize,
    /// The exact real time the last buffer started (the device's truth).
    start_t: f64,
    /// The playhead at that exact start.
    start_playhead: f64,
    /// How the engine stamps buffers: the loop over callback times.
    stamps: BufferClock,
}

impl Sim {
    fn new(epoch: f64, speed: f64) -> Self {
        Self {
            epoch,
            speed,
            sample_rate: 48_000.0,
            buffer: 256,
            state: RefCell::new(State {
                rate: 1.0,
                ..State::default()
            }),
        }
    }

    fn clock(&self, t: f64) -> f64 {
        t.mul_add(1e6, self.epoch)
    }

    /// Real seconds one buffer lasts on this device.
    fn buffer_secs(&self) -> f64 {
        f64::from(self.buffer) / (self.sample_rate * self.speed)
    }

    /// One buffer, starting exactly at real time `start`; its callback
    /// runs `late` seconds after (the scheduler's jitter).
    fn callback(&self, start: f64, late: f64) {
        let mut s = self.state.borrow_mut();
        let entry = self.clock(start + late);
        let stamp = s.stamps.tick(entry, self.buffer, self.sample_rate);
        let span = self.buffer_secs() * 1e6;
        if let Some((at, position, playing, rate)) = s.armed
            && at < stamp + span
        {
            // As the engine sees it: the playhead is `position` at `at`,
            // so at this buffer's (stamped) start it is that, less the
            // time until.
            let ahead = (at - stamp).max(0.0) * 1e-6 * self.speed;
            s.playhead = position - ahead * rate;
            s.playing = playing;
            s.rate = rate;
            s.armed = None;
        }
        s.sequence += 1;
        s.start_t = start;
        s.start_playhead = s.playhead;
        s.snapshot = Some(AudioSnapshot {
            sequence: s.sequence,
            host_micros: stamp,
            playhead_seconds: s.playhead,
            playrate: s.rate * self.speed,
            sample_rate: self.sample_rate,
            buffer_len: self.buffer,
            is_playing: s.playing,
            ..AudioSnapshot::default()
        });
        if s.playing {
            // The engine advances by the buffer's nominal length at its
            // rate; the device plays it in real time a little faster.
            s.playhead += f64::from(self.buffer) / self.sample_rate * s.rate;
        }
    }

    /// Where the playhead truly is at real time `t` — from the exact
    /// buffer start, not the engine's stamp.
    fn truth(&self, t: f64) -> f64 {
        let s = self.state.borrow();
        if s.playing {
            s.start_playhead + (t - s.start_t) * self.speed * s.rate
        } else {
            s.start_playhead
        }
    }
}

impl TransportBackend for Sim {
    fn snapshot(&self) -> Option<AudioSnapshot> {
        self.state.borrow().snapshot
    }
    fn set_rate(&self, rate: f64) {
        self.state.borrow_mut().rate = rate;
    }
    fn locate_at(&self, at_micros: f64, position: f64, playing: bool, rate: f64) {
        let mut s = self.state.borrow_mut();
        s.armed = Some((at_micros, position, playing, rate));
        s.locates += 1;
    }
    fn stop(&self, position: f64) {
        let mut s = self.state.borrow_mut();
        s.playing = false;
        s.playhead = position;
    }
}

/// A deterministic jitter source.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// How a run went, once settled.
struct Run {
    /// The worst gap between the two, samples at 48 kHz.
    worst_samples: f64,
    /// The clock offset estimate's error at the end, µs.
    offset_error: f64,
    /// Jumps once settled (there should be none: rate alone holds it).
    locates_after_settling: usize,
    locates: usize,
}

/// The leader owns the shared clock and plays from 10 s; the follower's
/// clock reads 7.3 s ahead and its device runs `speed` fast. Each way the
/// network takes a base plus up to `jitter_up` / `jitter_down` seconds.
fn run(speed: f64, jitter_up: f64, jitter_down: f64) -> Run {
    let leader = Sim::new(0.0, 1.0);
    let follower = Sim::new(7_300_000.0, speed);
    let mut estimator = ClockEstimator::default();
    let mut follow = Follower::default();
    let mut jitter = Lcg(7);
    leader.state.borrow_mut().playhead = 10.0;
    leader.state.borrow_mut().playing = true;

    let dt = 0.0005;
    let (mut next_leader, mut next_follower, mut next_ping, mut next_tick) =
        (0.0, 0.0003, 0.0, 0.0);
    let settled_at = 12.0;
    let mut out = Run {
        worst_samples: 0.0,
        offset_error: 0.0,
        locates_after_settling: 0,
        locates: 0,
    };
    let mut t = 0.0;
    while t < 40.0 {
        // Each device starts its buffers on its own steady clock; the
        // callbacks come up to 0.8 ms late.
        if t >= next_leader {
            leader.callback(next_leader, 0.0008 * jitter.next());
            next_leader += leader.buffer_secs();
        }
        if t >= next_follower {
            follower.callback(next_follower, 0.0008 * jitter.next());
            next_follower += follower.buffer_secs();
        }
        // 10 Hz pings from the follower to the leader.
        if t >= next_ping {
            let (up, down) = (
                0.0002 + jitter_up * jitter.next(),
                0.0002 + jitter_down * jitter.next(),
            );
            let t1 = follower.clock(t);
            let t2 = leader.clock(t + up);
            let t3 = leader.clock(t + up + 0.00005);
            let t4 = follower.clock(t + up + 0.00005 + down);
            estimator.record(t1, t2, t3, t4);
            next_ping += 0.1;
        }
        // 20 Hz: the leader's latest position (stamped in its clock — the
        // shared one) reaches the follower, which corrects.
        if t >= next_tick && estimator.samples() >= 4 {
            let lead = leader.snapshot().unwrap().position();
            // The follower's offset to the shared clock (the leader's):
            // shared − local, which is what the estimator measures.
            let offset = estimator.offset_micros().unwrap();
            let correction = follow.tick(&follower, &lead, offset, follower.clock(t));
            if t > settled_at && matches!(correction, Correction::Locate { .. }) {
                out.locates_after_settling += 1;
            }
            next_tick += 0.05;
        }
        if t > settled_at {
            let gap = (follower.truth(t) - leader.truth(t)).abs() * 48_000.0;
            out.worst_samples = out.worst_samples.max(gap);
        }
        t += dt;
    }
    out.offset_error = estimator.offset_micros().unwrap() - (leader.epoch - follower.epoch);
    out.locates = follower.state.borrow().locates;
    eprintln!(
        "worst {:.1} samples ({:.0} µs), offset error {:.0} µs, learned mismatch {:.1} ppm",
        out.worst_samples,
        out.worst_samples / 48.0 * 1000.0,
        out.offset_error,
        follow.controller().learned_mismatch() * 1e6
    );
    out
}

#[test]
fn on_a_lan_a_fast_follower_is_held_within_a_few_samples() {
    // Sub-millisecond jitter (a LAN, or two apps on one machine), a device
    // 80 ppm fast.
    let r = run(1.000_08, 0.0008, 0.0003);
    assert!(r.locates >= 1, "it starts with a locate");
    assert_eq!(r.locates_after_settling, 0, "settled, rate alone holds it");
    assert!(
        r.worst_samples < 8.0,
        "{:.1} samples (offset error {:.0} µs)",
        r.worst_samples,
        r.offset_error
    );
}

#[test]
fn across_the_internet_it_stays_under_a_millisecond_without_jumps() {
    // 10 ms of jitter one way, 4 ms the other: the asymmetry no clock
    // protocol can see bounds it, not the controller.
    let r = run(1.000_08, 0.010, 0.004);
    assert_eq!(r.locates_after_settling, 0, "settled, rate alone holds it");
    assert!(
        r.worst_samples < 48.0,
        "{:.1} samples (offset error {:.0} µs)",
        r.worst_samples,
        r.offset_error
    );
}

#[test]
fn the_offset_is_found_through_asymmetric_jitter() {
    let mut estimator = ClockEstimator::default();
    let mut jitter = Lcg(3);
    let offset = 1_234_567.0;
    for i in 0..64 {
        let t1 = f64::from(i) * 100_000.0;
        let (up, down) = (
            500.0 + 8_000.0 * jitter.next(),
            500.0 + 2_000.0 * jitter.next(),
        );
        estimator.record(
            t1,
            t1 + up + offset,
            t1 + up + offset + 50.0,
            t1 + up + 50.0 + down,
        );
    }
    let error = (estimator.offset_micros().unwrap() - offset).abs();
    assert!(error < 1_000.0, "{error} µs");
}

#[test]
fn a_leader_that_stops_stops_the_follower() {
    let follower = Sim::new(0.0, 1.0);
    follower.state.borrow_mut().playing = true;
    follower.callback(0.0, 0.0);
    let mut follow = Follower::default();
    let stopped = Position {
        host_micros: 0.0,
        playhead_seconds: 42.0,
        playrate: 1.0,
        is_playing: false,
    };
    assert_eq!(
        follow.tick(&follower, &stopped, 0.0, 1_000.0),
        Correction::Stop { position: 42.0 }
    );
    assert!(!follower.state.borrow().playing);
}

/// The audio thread writes while another reads: every snapshot read is
/// one the writer wrote whole (the seqlock's fences hold on arm64 too).
#[test]
fn a_snapshot_is_never_read_torn() {
    use std::sync::Arc;
    let cell = Arc::new(daw_transport_sync::SnapshotCell::new());
    let writer = {
        let cell = Arc::clone(&cell);
        std::thread::spawn(move || {
            for i in 0..200_000u64 {
                let x = i as f64;
                cell.store(&AudioSnapshot {
                    sequence: i,
                    host_micros: x * 1000.0,
                    playhead_seconds: x * 0.01,
                    playrate: 1.0 + x,
                    buffer_len: u32::try_from(i % 4096).unwrap(),
                    ..AudioSnapshot::default()
                });
            }
        })
    };
    let mut reads = 0;
    while !writer.is_finished() {
        if let Some(s) = cell.load() {
            let x = s.sequence as f64;
            assert_eq!(s.host_micros, x * 1000.0, "torn: {s:?}");
            assert_eq!(s.playhead_seconds, x * 0.01, "torn: {s:?}");
            assert_eq!(s.playrate, 1.0 + x, "torn: {s:?}");
            reads += 1;
        }
    }
    writer.join().unwrap();
    assert!(reads > 0);
}
