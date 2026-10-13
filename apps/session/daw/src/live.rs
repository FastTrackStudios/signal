//! What moves in a rack, and the two things that are expensive to know.
//!
//! The rack's rule is: record what is constant, re-emit what varies,
//! cache what varies sometimes. This module is the third bin. Two
//! pictures in the rack depend on settings alone but cost milliseconds
//! to work out — the saturator's harmonic ladder (a probe through the
//! stage) and the reverb's tail (the algorithm's own impulse response,
//! rendered) — and neither may run inside a frame. Both are memoised
//! here on what they were built from, and the reverb's goes to a worker
//! so a drag on its decay never waits for it.
//!
//! It also holds [`Meters`]: the per-track payload the engine publishes
//! at meter rate, which is everything in a rack that moves with the
//! audio. One struct, so that a unit gaining a live element is a field
//! here and a read in its drawing, not a new channel.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};

use reverb_dsp::algorithm::{AlgorithmParams, AlgorithmType};
use saturate_dsp::preamp::ClassAPreamp;

/// Everything in one track's rack that moves with the audio.
///
/// Published per meter frame, not per sample; drawn into the rack's
/// cached scene once per publish. Empty (`spectrum.is_empty()`) when
/// nothing is playing — a rack with nothing moving stays in the
/// recording.
///
/// Every field is what the ENGINE knows, not what a display worked out
/// from the spectrum: the suppressors' reduction is the spectral
/// engine's own per-bin gain, so the ribbon in the strip is the ribbon
/// the plugin would draw. (Until the rack is bound to a chain,
/// [`crate::simulate::meters`] stands in and says so.)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meters {
    /// The analyser's bins, in dB, log-spaced across the audible range.
    pub spectrum: Vec<f32>,
    /// The peak going INTO the saturator, linear — where on its curve
    /// the signal reaches this frame.
    pub sat_peak: f32,
    /// The de-esser's reduction, per spectrum bin, in dB (positive).
    pub deess_db: Vec<f32>,
    /// The level in the de-esser's band this frame, in dB on the
    /// analyser's scale — the "S" as a number.
    pub ess_db: f32,
    /// And the level of the material around the band — the octave
    /// either side — which is what the threshold stands above.
    pub ess_ref_db: f32,
    /// The delay's wet return, linear peak.
    pub delay_wet: f32,
    /// The reverb's wet return, linear peak.
    pub reverb_wet: f32,
}

impl Meters {
    /// Whether there is anything moving at all.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.spectrum.is_empty()
    }

    /// The deepest the de-esser is cutting this frame, in dB.
    #[must_use]
    pub fn deess_deepest(&self) -> f32 {
        self.deess_db.iter().copied().fold(0.0, f32::max)
    }
}

// ── The saturator's ladder ───────────────────────────────────────────

/// How many harmonics the ladder shows: H2 through H8.
pub const RUNGS: usize = 7;

/// The input levels the ladder is measured at, in dBFS.
///
/// Three, and interpolated between by the live peak: a static
/// nonlinearity adds different harmonics at −18 than at 0, and a
/// ladder measured only at full scale would claim a quiet passage is
/// as coloured as a loud one. Three probes is the fewest that catch the
/// knee — below the first the stage is linear and the ladder is
/// nothing, which is the right answer.
pub const LEVELS: [f32; 3] = [-18.0, -6.0, 0.0];

/// H2..H8 as a share of H1, at each of [`LEVELS`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ladder {
    rungs: [[f32; RUNGS]; 3],
}

impl Ladder {
    /// The rungs at `level_dbfs`, interpolated between the measured
    /// levels and clamped to them.
    #[must_use]
    pub fn at(&self, level_dbfs: f32) -> [f32; RUNGS] {
        let level = level_dbfs.clamp(LEVELS[0], LEVELS[2]);
        let (lo, hi, t) = if level <= LEVELS[1] {
            (0, 1, (level - LEVELS[0]) / (LEVELS[1] - LEVELS[0]))
        } else {
            (1, 2, (level - LEVELS[1]) / (LEVELS[2] - LEVELS[1]))
        };
        let mut out = [0.0_f32; RUNGS];
        let (below, above) = (
            self.rungs.get(lo).copied().unwrap_or_default(),
            self.rungs.get(hi).copied().unwrap_or_default(),
        );
        for ((slot, a), b) in out.iter_mut().zip(below).zip(above) {
            *slot = a.mul_add(1.0 - t, b * t);
        }
        out
    }

    /// The rungs at full scale — what a recording with no signal shows.
    #[must_use]
    pub const fn full(&self) -> [f32; RUNGS] {
        self.rungs[2]
    }

    /// How much of what is added is even-order, 0..1.
    ///
    /// The one number that separates a valve from a transistor by ear,
    /// and the one the header prints.
    #[must_use]
    pub fn even_share(&self) -> f32 {
        let rungs = self.full();
        let total: f32 = rungs.iter().sum();
        if total <= f32::EPSILON {
            return 0.0;
        }
        // Rung k is harmonic k+2, so even harmonics are the even k.
        let even: f32 = rungs.iter().step_by(2).sum();
        even / total
    }
}

/// The stage's settings, quantised to what changes the ladder.
///
/// The memo's key. Quantised because a drag produces hundreds of
/// drives a pixel apart, and a probe per pixel is the cost this cache
/// exists to avoid: a hundredth of a drive unit is well under what the
/// ladder can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SatKey {
    drive: i32,
    q_point: i32,
    skew: i32,
    headroom: i32,
    knee: i32,
    crossover: i32,
    sag: i32,
    tilt: i32,
    positive: u8,
    negative: u8,
}

impl SatKey {
    fn of(pre: &ClassAPreamp) -> Self {
        let q = |v: f32| crate::num::quantise(f64::from(v), 100.0);
        Self {
            drive: q(pre.drive),
            q_point: q(pre.q_point),
            skew: q(pre.skew),
            headroom: q(pre.headroom),
            knee: q(pre.knee),
            crossover: q(pre.crossover),
            sag: q(pre.sag),
            tilt: q(pre.tilt_db()),
            positive: shaper_index(pre.positive),
            negative: shaper_index(pre.negative),
        }
    }
}

const fn shaper_index(shaper: saturate_dsp::preamp::SideShaper) -> u8 {
    use saturate_dsp::preamp::SideShaper as S;
    match shaper {
        S::Clean => 0,
        S::OpAmp => 1,
        S::Tube => 2,
        S::Transformer => 3,
        S::Diode => 4,
        S::Hard => 5,
    }
}

/// How many ladders the memo keeps before it starts over.
///
/// A mixer's worth of distinct stages is a few dozen; a long session
/// of drags is thousands. Starting over at this many costs one probe
/// per panel on the next frame, which is the same as a project open.
const LADDERS: usize = 512;

thread_local! {
    static LADDER_MEMO: std::cell::RefCell<HashMap<SatKey, Ladder>> =
        std::cell::RefCell::new(HashMap::new());
}

/// The harmonic ladder for this stage, measured once per distinct
/// setting.
///
/// Three probes through `saturate_dsp::preamp::analysis` on a miss —
/// about a third of a millisecond — and a hash lookup on a hit. The
/// memo is per thread because the probe is pure and the racks are
/// drawn from one; sharing it across threads would buy a lock for
/// nothing.
#[must_use]
pub fn ladder(pre: &ClassAPreamp) -> Ladder {
    let key = SatKey::of(pre);
    LADDER_MEMO.with(|memo| {
        if let Some(found) = memo.borrow().get(&key) {
            return *found;
        }
        let mut rungs = [[0.0_f32; RUNGS]; 3];
        for (level, row) in LEVELS.iter().zip(rungs.iter_mut()) {
            // H1..H8; the ladder drops H1, which is the signal itself.
            let mut all = [0.0_f32; RUNGS + 1];
            saturate_dsp::preamp::analysis::harmonic_spectrum_at(pre, *level, &mut all);
            for (slot, value) in row.iter_mut().zip(all.iter().skip(1)) {
                *slot = value.clamp(0.0, 1.0);
            }
        }
        let built = Ladder { rungs };
        let mut memo = memo.borrow_mut();
        if memo.len() >= LADDERS {
            memo.clear();
        }
        memo.insert(key, built);
        built
    })
}

// ── The reverb's tail ────────────────────────────────────────────────

/// How many slices of time a tail is folded into.
///
/// Half a pixel per slice at a working width, which is finer than the
/// picture and coarse enough that the render is a handful of
/// kilobytes per distinct setting.
pub const TAIL_BINS: usize = 64;

/// A reverb's envelope over its own decay window, in dB below its peak.
#[derive(Clone, Debug, PartialEq)]
pub struct Tail {
    /// One value per slice, [`reverb_dsp::analysis::FLOOR_DB`]..0.
    pub envelope: Arc<[f32; TAIL_BINS]>,
    /// Whether this is the algorithm's own response, or the exponential
    /// standing in for it while the worker renders. A display draws the
    /// stand-in dashed: a guess that looked like a measurement would be
    /// believed.
    pub exact: bool,
}

/// What a reverb's tail is rendered from.
///
/// The parameters that change the impulse response, quantised to the
/// worker's resolution. Not the whole [`crate::tone::Room`]: predelay
/// shifts the tail and mix scales it, and both are applied where it is
/// drawn rather than baked into the render.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RoomKey {
    algorithm: usize,
    /// Seconds to −60 dB, in twentieths.
    decay: i32,
    size: i32,
    damping: i32,
}

impl RoomKey {
    /// The key for these settings.
    #[must_use]
    pub fn of(algorithm: AlgorithmType, decay_s: f32, size: f32, damping: f32) -> Self {
        Self {
            algorithm: algorithm.index(),
            decay: crate::num::quantise(f64::from(decay_s), 20.0).max(1),
            size: crate::num::quantise(f64::from(size), 20.0),
            damping: crate::num::quantise(f64::from(damping), 20.0),
        }
    }

    fn algorithm(self) -> AlgorithmType {
        AlgorithmType::from_index(self.algorithm)
    }

    /// The decay this key stands for, in seconds.
    #[must_use]
    pub fn decay_s(self) -> f64 {
        f64::from(self.decay) / 20.0
    }

    fn params(self) -> AlgorithmParams {
        let algorithm = self.algorithm();
        let decay_s = self.decay_s();
        // Through the engine's own time model where it has one, so
        // "1.8 seconds" is 1.8 seconds whichever algorithm is chosen.
        // Engines without one set a feedback coefficient directly; a
        // log map over a plausible range is the best a display can do.
        let decay = algorithm.t60_range(0).map_or_else(
            || (decay_s.clamp(0.1, 20.0) / 0.1).log(200.0),
            |(lo, hi)| reverb_dsp::algorithm::t60_to_decay(decay_s, lo, hi),
        );
        AlgorithmParams {
            decay,
            size: f64::from(self.size) / 20.0,
            damping: f64::from(self.damping) / 20.0,
            ..AlgorithmParams::default()
        }
    }

    /// Render this key's envelope, on the calling thread.
    fn render(self) -> [f32; TAIL_BINS] {
        let mut out = [0.0_f32; TAIL_BINS];
        // The window is the decay: a tail that reaches −60 dB at the
        // panel's right edge is a tail drawn to its own setting.
        reverb_dsp::analysis::impulse_envelope(
            self.algorithm(),
            0,
            &self.params(),
            self.decay_s(),
            &mut out,
        );
        out
    }

    /// The exponential a decay knob implies — the stand-in while the
    /// real one renders. The same for every key: the window IS the
    /// decay, so the guess reaches −60 dB at the right edge whatever
    /// the setting.
    fn estimate() -> [f32; TAIL_BINS] {
        let mut out = [0.0_f32; TAIL_BINS];
        for (i, slot) in out.iter_mut().enumerate() {
            let t = crate::num::coord(i) / crate::num::coord(TAIL_BINS - 1);
            *slot = crate::mcp::f64_to_f32(-60.0 * t);
        }
        out
    }
}

struct Tails {
    done: Mutex<HashMap<RoomKey, Arc<[f32; TAIL_BINS]>>>,
    pending: Mutex<std::collections::HashSet<RoomKey>>,
    requests: Mutex<mpsc::Sender<RoomKey>>,
    /// Bumped when a render lands, so a window can notice without
    /// asking for every key.
    generation: AtomicU64,
}

fn tails() -> &'static Tails {
    static TAILS: OnceLock<Tails> = OnceLock::new();
    TAILS.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<RoomKey>();
        // One worker for the process: renders are independent and a
        // few milliseconds each, and a drag queues dozens — a second
        // thread would finish them no sooner than this one does in
        // order, since each later key makes the earlier ones moot.
        std::thread::Builder::new()
            .name("reverb-tails".into())
            .spawn(move || {
                while let Ok(key) = rx.recv() {
                    let rendered = key.render();
                    let store = tails();
                    if let Ok(mut done) = store.done.lock() {
                        done.insert(key, Arc::new(rendered));
                    }
                    if let Ok(mut pending) = store.pending.lock() {
                        pending.remove(&key);
                    }
                    store.generation.fetch_add(1, Ordering::Release);
                }
            })
            .ok();
        Tails {
            done: Mutex::new(HashMap::new()),
            pending: Mutex::new(std::collections::HashSet::new()),
            requests: Mutex::new(tx),
            generation: AtomicU64::new(0),
        }
    })
}

/// The tail for these settings: the rendered one if it is ready, else
/// the estimate — with the render requested, so the next frame after
/// the worker finishes gets the real thing.
#[must_use]
pub fn tail(key: RoomKey) -> Tail {
    let store = tails();
    if let Ok(done) = store.done.lock()
        && let Some(found) = done.get(&key)
    {
        return Tail {
            envelope: Arc::clone(found),
            exact: true,
        };
    }
    if let Ok(mut pending) = store.pending.lock()
        && pending.insert(key)
        && let Ok(tx) = store.requests.lock()
    {
        // A send can only fail if the worker is gone, and then the
        // estimate is all there will ever be.
        tx.send(key).ok();
    }
    Tail {
        envelope: Arc::new(RoomKey::estimate()),
        exact: false,
    }
}

/// Render these settings now, on this thread, and remember the result.
///
/// For a bench or a test that has to be deterministic: a worker that
/// finished between two renders of the same frame would make them
/// differ, and `daw-verify` compares them byte for byte.
pub fn render_tail_now(key: RoomKey) {
    let store = tails();
    if store.done.lock().is_ok_and(|done| done.contains_key(&key)) {
        return;
    }
    let rendered = key.render();
    if let Ok(mut done) = store.done.lock() {
        done.insert(key, Arc::new(rendered));
    }
    store.generation.fetch_add(1, Ordering::Release);
}

/// How many renders have landed so far.
///
/// A window keeps the last value it saw and re-records when this moves:
/// a tail that arrived after the recording is a tail the recording does
/// not have.
#[must_use]
pub fn tails_generation() -> u64 {
    tails().generation.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::{LEVELS, RUNGS, RoomKey, TAIL_BINS, ladder, render_tail_now, tail};
    use reverb_dsp::algorithm::AlgorithmType;
    use saturate_dsp::preamp::{ClassAPreamp, SideShaper};

    fn biased_triode() -> ClassAPreamp {
        let mut pre = ClassAPreamp::new(48_000.0);
        pre.positive = SideShaper::Tube;
        pre.negative = SideShaper::Transformer;
        pre.drive = 4.0;
        pre.q_point = 0.25;
        pre
    }

    /// A biased single-ended stage is mostly even; a symmetric op-amp is
    /// mostly odd. The ladder is what tells the two apart.
    #[test]
    fn a_biased_stage_is_even_and_a_symmetric_one_is_odd() {
        let valve = ladder(&biased_triode());
        let mut rails = ClassAPreamp::new(48_000.0);
        rails.positive = SideShaper::OpAmp;
        rails.negative = SideShaper::OpAmp;
        rails.drive = 4.0;
        let solid = ladder(&rails);
        // A single-ended stage still makes a third — iron on the
        // bottom half does — so the test is the CONTRAST, which is
        // what the ladder's colours show.
        assert!(
            valve.even_share() > 0.25,
            "valve even share {}",
            valve.even_share()
        );
        assert!(
            solid.even_share() < 0.1,
            "solid even share {}",
            solid.even_share()
        );
        assert!(valve.even_share() > solid.even_share() + 0.2);
    }

    /// The ladder breathes: a quiet probe adds less than a loud one.
    #[test]
    fn a_quiet_signal_climbs_less_of_the_ladder() {
        let rungs = ladder(&biased_triode());
        let quiet: f32 = rungs.at(LEVELS[0]).iter().sum();
        let loud: f32 = rungs.at(LEVELS[2]).iter().sum();
        assert!(quiet < loud, "quiet {quiet} vs loud {loud}");
        assert_eq!(rungs.at(-3.0).len(), RUNGS);
        // Between two measured levels is between their ladders.
        let mid: f32 = rungs.at(-12.0).iter().sum();
        assert!(
            mid > quiet && mid < loud,
            "mid {mid} outside {quiet}..{loud}"
        );
    }

    /// The first ask for a tail is the estimate, and after a synchronous
    /// render the same key comes back exact.
    #[test]
    fn a_tail_arrives_exact_once_rendered() {
        let key = RoomKey::of(AlgorithmType::Plate, 0.6, 0.5, 0.3);
        render_tail_now(key);
        let got = tail(key);
        assert!(got.exact);
        assert_eq!(got.envelope.len(), TAIL_BINS);
        let peak = got.envelope.iter().copied().fold(f32::MIN, f32::max);
        assert!(peak.abs() < 1e-4, "the peak slice is 0 dB: {peak}");
    }

    /// Quantisation: two decays a hair apart share a render, and two
    /// clearly different ones do not.
    #[test]
    fn keys_quantise_to_the_workers_resolution() {
        let a = RoomKey::of(AlgorithmType::Hall, 1.80, 0.5, 0.3);
        let b = RoomKey::of(AlgorithmType::Hall, 1.81, 0.5, 0.3);
        let c = RoomKey::of(AlgorithmType::Hall, 2.30, 0.5, 0.3);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert!((a.decay_s() - 1.8).abs() < 1e-9);
    }
}
