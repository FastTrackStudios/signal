//! Bit-exact reference vectors for the delay engines.
//!
//! A delay is a ring buffer plus everything wrapped around it, and almost all
//! of that is state: the write cursor, a feedback path that feeds its own
//! output back in, modulation LFOs with running phase, filters in the feedback
//! loop, and — in the tape and BBD models — a read position that moves
//! fractionally and interpolates between samples. Every one of those is a place
//! where an off-by-one in an index rewrite is inaudible for the first buffer
//! and unmistakable by the fourth repeat.
//!
//! So the fixtures are long enough to hear the tail: 24000 samples at 48 kHz is
//! half a second, which at the 150 ms delay time used here is three full
//! repeats through the feedback path.
//!
//! The excitation is a transient burst rather than a sine, deliberately. A
//! delay fed a steady tone produces a steady tone, and the repeats hide inside
//! it; fed an impulse-like hit, each repeat is separately visible in the
//! output, so a reference vector that changes tells you *which* repeat moved.

use delay_dsp::chain::{DelayChain, StereoMode};
use delay_dsp::engine::{DelayEngine, DelayStyle};
use dsp_golden::{Golden, golden, signal};

use audiocore_dsp::{AudioConfig, Processor};

dsp_golden::install_counting_allocator!();

const SAMPLE_RATE: f64 = 48_000.0;
const GENERATOR_RATE: f32 = 48_000.0;
/// Half a second — three repeats at the 150 ms time used below.
const LEN: usize = 24_000;

const fn config() -> AudioConfig {
    AudioConfig {
        sample_rate: SAMPLE_RATE,
        max_buffer_size: 512,
    }
}

/// Transient bursts with a little tone under them: each repeat stays
/// individually visible in the output instead of merging into a drone.
fn program(len: usize) -> Vec<f64> {
    let hits = signal::widen(&signal::transients(len, GENERATOR_RATE));
    let tone = signal::widen(&signal::sine(len, 220.0, GENERATOR_RATE));
    hits.into_iter()
        .zip(tone)
        .map(|(hit, t)| hit.mul_add(0.8, t * 0.1))
        .collect()
}

/// Every style, by name, so a failure names the engine rather than an index.
fn styles() -> Vec<(String, DelayStyle)> {
    (0..DelayStyle::COUNT)
        .map(DelayStyle::from_index)
        .map(|style| (style.label().to_lowercase().replace(' ', "_"), style))
        .collect()
}

/// A style set to the same musically-sensible place every time. Times are
/// clamped into each style's own supported range, so this asks every engine
/// for the same thing rather than asking some of them for the impossible.
fn engine(style: DelayStyle) -> DelayEngine {
    let mut engine = DelayEngine::new();
    engine.set_style(style);
    let (min_ms, max_ms) = style.time_range_ms();
    engine.time_ms = 150.0_f64.clamp(min_ms, max_ms);
    engine.feedback = 0.45;
    engine.update(SAMPLE_RATE);
    engine
}

#[test]
fn every_delay_style_holds_its_reference() {
    let g: Golden = golden!();
    let input = program(LEN);
    for (name, style) in styles() {
        let mut engine = engine(style);
        let out: Vec<f64> = input
            .iter()
            .enumerate()
            .map(|(n, x)| engine.tick(*x, n % 2))
            .collect();
        dsp_golden::assert_golden!(g, &format!("engine_{name}"), &out);
    }
}

#[test]
fn every_style_holds_its_reference_under_modulation() {
    // Wow, flutter and the BBD clock jitter all drive a fractional read
    // position through an interpolator. That is the single most fragile thing
    // in this crate to an index rewrite, and it is idle at the default
    // settings the test above uses.
    let g: Golden = golden!();
    let input = program(LEN);
    for (name, style) in styles() {
        let mut engine = engine(style);
        engine.wow_depth = 0.6;
        engine.wow_rate = 1.7;
        engine.flutter_depth = 0.4;
        engine.flutter_rate = 7.3;
        engine.bbd_clock_jitter = 0.5;
        engine.drive = 0.5;
        engine.update(SAMPLE_RATE);
        let out: Vec<f64> = input
            .iter()
            .enumerate()
            .map(|(n, x)| engine.tick(*x, n % 2))
            .collect();
        dsp_golden::assert_golden!(g, &format!("modulated_{name}"), &out);
    }
}

#[test]
fn the_time_modulation_entry_point_holds_its_reference() {
    // `tick_at` is the path the chain actually drives, with a per-sample time
    // rather than the stored parameter. It reads the ring at a moving
    // fractional offset, which `tick` never does.
    let g: Golden = golden!();
    let input = program(LEN);
    for (name, style) in styles() {
        let mut engine = engine(style);
        let (min_ms, max_ms) = style.time_range_ms();
        let out: Vec<f64> = input
            .iter()
            .enumerate()
            .map(|(n, x)| {
                // A vibrato around a musical base time, NOT a sweep across the
                // style's full range. A sweep to 2500 ms over a half-second
                // buffer outruns the buffer — the read position never catches
                // up, nothing ever comes back out, and nine of the fourteen
                // styles then rendered byte-identical dry signal. A modest
                // wobble keeps the read position genuinely moving, which is
                // the thing this fixture exists to pin.
                let base = 150.0_f64.clamp(min_ms, max_ms);
                let seconds = f64::from(dsp_golden::num::count_to_f32(n)) / SAMPLE_RATE;
                let wobble = (core::f64::consts::TAU * 2.0 * seconds).sin();
                engine.tick_at(
                    *x,
                    n % 2,
                    (base * wobble.mul_add(0.15, 1.0)).clamp(min_ms, max_ms),
                )
            })
            .collect();
        dsp_golden::assert_golden!(g, &format!("tick_at_{name}"), &out);
    }
}

/// A named chain configuration: the setter that distinguishes it.
type ChainVariant = (&'static str, Box<dyn Fn(&mut DelayChain)>);

/// Chain configurations that provably render differently from one another.
///
/// Each was checked to produce a distinct output before being added — a
/// reference vector that duplicates its neighbour pins the same code twice and
/// tests nothing extra.
fn chain_variants() -> Vec<ChainVariant> {
    vec![
        ("plain", Box::new(|_: &mut DelayChain| {})),
        (
            "pingpong",
            Box::new(|c: &mut DelayChain| {
                // `pingpong_feedback` alone does nothing — the stereo mode is
                // what selects the ping-pong topology.
                c.stereo_mode = StereoMode::PingPong;
                c.pingpong_feedback = 0.8;
            }),
        ),
        (
            "mono",
            Box::new(|c: &mut DelayChain| c.stereo_mode = StereoMode::Mono),
        ),
        ("frozen", Box::new(|c: &mut DelayChain| c.freeze = true)),
        (
            "lr_offset",
            Box::new(|c: &mut DelayChain| c.lr_offset_ms = 12.0),
        ),
        (
            "diffused",
            Box::new(|c: &mut DelayChain| {
                c.diffusion_enabled = true;
                c.diffusion_size = 0.7;
                c.diffusion_smear = 0.6;
            }),
        ),
        (
            "ducked",
            Box::new(|c: &mut DelayChain| {
                c.ducking_enabled = true;
                c.ducker.amount = 0.8;
                c.ducker.threshold = 0.05;
            }),
        ),
        ("wide", Box::new(|c: &mut DelayChain| c.width = 2.0)),
    ]
}

/// Chain parameters that currently do nothing, as measured.
///
/// Two configurations rendering identically means either the parameter never
/// reaches the processor or the fixture fails to drive it. Both are worth
/// knowing, and a duplicated reference vector would hide both, so the set is
/// pinned exactly rather than tolerated:
///
/// - `mono` == `pingpong`: `StereoMode::PingPong` mixes to mono and adds a
///   cross-fed `last_feedback()` term, so it should differ from plain mono.
/// - `lr_offset` == `plain`: `lr_offset_ms` is smoothed and sizes a delay line
///   in `update()`, but changes nothing audible at 12 ms.
///
/// Neither is fixed here — this pass is a refactor, and both are behaviour
/// changes. If one is fixed, this test fails and the vectors get re-recorded.
const KNOWN_INERT: [(&str, &str); 2] = [("mono", "pingpong"), ("lr_offset", "plain")];

#[test]
fn only_the_known_chain_parameters_are_inert() {
    let mut seen: Vec<(String, u64)> = Vec::new();
    let mut collisions: Vec<(String, String)> = Vec::new();
    for (name, setup) in chain_variants() {
        let mut chain = DelayChain::new();
        chain.delay_l.time_ms = 150.0;
        chain.delay_r.time_ms = 190.0;
        chain.delay_l.feedback = 0.45;
        chain.delay_r.feedback = 0.45;
        chain.mix = 0.6;
        setup(&mut chain);
        chain.reset();
        chain.update(config());

        let mut left = program(LEN);
        let mut right = signal::widen(&signal::noise(LEN, 0x00D1_5EA5));
        for (l, r) in left.chunks_mut(512).zip(right.chunks_mut(512)) {
            chain.process(l, r);
        }
        let digest = left
            .iter()
            .chain(right.iter())
            .fold(0xcbf2_9ce4_8422_2325_u64, |h, v| {
                v.to_bits().to_le_bytes().iter().fold(h, |h, b| {
                    (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3)
                })
            });
        if let Some((other, _)) = seen.iter().find(|(_, d)| *d == digest) {
            collisions.push((name.to_owned(), other.clone()));
        }
        seen.push((name.to_owned(), digest));
    }

    let found: Vec<(&str, &str)> = collisions
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    assert_eq!(
        found,
        KNOWN_INERT.to_vec(),
        "the set of chain parameters that render identically changed.\n  \
         A NEW pair means a refactor stopped a parameter reaching the \
         processor.\n  A MISSING pair means one was fixed — good; delete it \
         from KNOWN_INERT and re-record the chain vectors."
    );
}

#[test]
fn the_stereo_chain_holds_its_reference() {
    let g: Golden = golden!();
    let left_in = program(LEN);
    let right_in = signal::widen(&signal::noise(LEN, 0x00D1_5EA5));

    // Order matters: `reset()` BEFORE `update()`. See
    // [`reset_must_precede_update`] — the other way round leaves the tape
    // motor stopped and the wet path near-silent, and every configuration
    // below then renders identically.
    let configure = |chain: &mut DelayChain| {
        chain.delay_l.time_ms = 150.0;
        chain.delay_r.time_ms = 190.0; // deliberately unequal: catches L/R swaps
        chain.delay_l.feedback = 0.45;
        chain.delay_r.feedback = 0.45;
        chain.mix = 0.6;
    };

    for (name, setup) in chain_variants() {
        let mut chain = DelayChain::new();
        configure(&mut chain);
        setup(&mut chain);
        chain.reset();
        chain.update(config());

        let mut left = left_in.clone();
        let mut right = right_in.clone();
        // In blocks, as a host would: a chain that only works when handed the
        // whole signal at once has a buffer-boundary bug this would miss.
        for (l, r) in left.chunks_mut(512).zip(right.chunks_mut(512)) {
            chain.process(l, r);
        }
        let interleaved: Vec<f64> = left
            .into_iter()
            .zip(right)
            .flat_map(<[f64; 2]>::from)
            .collect();
        dsp_golden::assert_golden!(g, &format!("chain_{name}"), &interleaved);
    }
}

#[test]
fn block_size_does_not_change_the_output() {
    // An invariant, not a reference. The chain must produce the same samples
    // whether the host hands it 512 frames or 64, and a rewrite that caches
    // something per block rather than per sample breaks exactly this.
    let render = |block: usize| {
        let mut chain = DelayChain::new();
        chain.delay_l.time_ms = 150.0;
        chain.delay_r.time_ms = 190.0;
        chain.mix = 0.6;
        chain.reset();
        chain.update(config());
        let mut left = program(LEN);
        let mut right = signal::widen(&signal::noise(LEN, 7));
        for (l, r) in left.chunks_mut(block).zip(right.chunks_mut(block)) {
            chain.process(l, r);
        }
        (left, right)
    };
    let (big_l, big_r) = render(512);
    let (small_l, small_r) = render(64);
    for (n, (a, b)) in big_l.iter().zip(&small_l).enumerate() {
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "left channel diverged at sample {n}"
        );
    }
    for (n, (a, b)) in big_r.iter().zip(&small_r).enumerate() {
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "right channel diverged at sample {n}"
        );
    }
}

/// `Processor::reset()` must be followed by `update()`, or the delay goes
/// nearly silent. This documents that requirement rather than fixing it.
///
/// `TapeDelay::reset` parks `speed_smoother` at a literal `0.0` — a stopped
/// tape motor — while every other smoother on the lines around it resets to
/// its live parameter (`self.feedback`, `self.drive`, `self.hicut_freq`, ...).
/// The motor speed is `head_cells / delay_samples`, and `update()` is the only
/// place that seeds it, so a chain that is reset and then run without a further
/// `update()` reads a tape that is barely moving.
///
/// Measured at 80 ms, no feedback, on a 100-sample burst over 19200 samples:
///
///     update() only      wet energy 99.6
///     update(), reset()  wet energy  1.0
///     reset(), update()  wet energy 99.6
///
/// This may be deliberate — the code calls the 120 ms glide "motor inertia ...
/// the stored audio repitches through the glide (the tape feel)", and a
/// stopped motor is a reasonable rest state for that. But `Processor::reset`
/// is documented as "reset all internal state", hosts call it on transport
/// stop, and nothing says a caller must re-`update()` afterwards. Whether the
/// fix is to seed the speed in `reset()` or to document the ordering is a
/// judgement about the tape model, so it is left alone and pinned here.
#[test]
fn reset_must_precede_update() {
    let burst: Vec<f64> = (0..19_200)
        .map(|i| if i < 100 { 1.0 } else { 0.0 })
        .collect();
    let wet_energy = |reset_first: bool| {
        let mut chain = DelayChain::new();
        chain.delay_l.time_ms = 80.0;
        chain.delay_r.time_ms = 80.0;
        chain.delay_l.feedback = 0.0;
        chain.delay_r.feedback = 0.0;
        chain.mix = 1.0;
        if reset_first {
            chain.reset();
            chain.update(config());
        } else {
            chain.update(config());
            chain.reset();
        }
        let mut left = burst.clone();
        let mut right = vec![0.0_f64; burst.len()];
        chain.process(&mut left, &mut right);
        left.iter().skip(200).map(|x| x * x).sum::<f64>()
    };
    let healthy = wet_energy(true);
    let starved = wet_energy(false);
    assert!(
        healthy > 50.0,
        "reset-then-update should delay normally: {healthy}"
    );
    assert!(
        starved < healthy / 10.0,
        "update-then-reset is expected to starve the wet path (see the doc \
         comment); if this now passes, the tape motor seeding was fixed — \
         delete this test and re-record the chain vectors. healthy={healthy} \
         starved={starved}"
    );
}

#[test]
fn silence_in_is_silence_out_for_every_style() {
    // A delay with feedback is a loop with gain in it: if it is not exactly
    // stable at rest it either self-oscillates or grinds denormals, and both
    // are audible the moment the player stops.
    let quiet = vec![0.0_f64; 4096];
    for (name, style) in styles() {
        let mut engine = engine(style);
        engine.feedback = 0.95; // near the edge on purpose
        engine.update(SAMPLE_RATE);
        let peak = quiet
            .iter()
            .enumerate()
            .map(|(n, x)| engine.tick(*x, n % 2).abs())
            .fold(0.0_f64, f64::max);
        assert!(peak < 1e-9, "{name} emitted {peak} on silence");
    }
}

#[test]
fn no_style_blows_up_at_maximum_feedback() {
    // The counterpart: fed real signal at the top of the feedback range, the
    // loop must stay bounded. This is the failure the Optical compressor style
    // in comp-dsp turned out to have, and it is worth asking of anything with
    // a feedback path.
    let input = program(LEN);
    for (name, style) in styles() {
        let mut engine = engine(style);
        engine.feedback = 1.0;
        engine.update(SAMPLE_RATE);
        let mut peak = 0.0_f64;
        for (n, x) in input.iter().enumerate() {
            let y = engine.tick(*x, n % 2);
            assert!(y.is_finite(), "{name} produced {y} at sample {n}");
            peak = peak.max(y.abs());
        }
        assert!(peak < 100.0, "{name} ran away to {peak} at unity feedback");
    }
}

#[test]
fn processing_allocates_nothing() {
    let input = program(2048);
    let mut chain = DelayChain::new();
    chain.update(config());
    let mut engines: Vec<DelayEngine> = styles().into_iter().map(|(_, s)| engine(s)).collect();

    let mut left = vec![0.0_f64; 512];
    let mut right = vec![0.0_f64; 512];
    let mut sink = 0.0_f64;

    let mut run = |sink: &mut f64, left: &mut [f64], right: &mut [f64]| {
        for engine in &mut engines {
            for (n, x) in input.iter().enumerate() {
                *sink += engine.tick(*x, n % 2);
            }
        }
        chain.process(left, right);
    };
    // Warm up outside the guard: the first block may size buffers.
    run(&mut sink, &mut left, &mut right);
    dsp_golden::assert_no_alloc(|| run(&mut sink, &mut left, &mut right));
    assert!(sink.is_finite());
}
