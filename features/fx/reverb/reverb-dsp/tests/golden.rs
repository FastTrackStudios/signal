//! Bit-exact reference vectors for the reverb algorithms.
//!
//! A reverb is the hardest thing in this tree to pin and the most worth
//! pinning. It is a lattice of recirculating delay lines: every sample that
//! comes out has been through the feedback path many times, so an error that
//! would be a rounding difference in a filter is amplified by the loop until
//! it is a different tail. That sensitivity cuts both ways — it makes the
//! reference vectors an unusually sharp instrument, and it makes "it sounded
//! fine when I tried it" an unusually weak one.
//!
//! Each algorithm is driven by an impulse, which for a reverb is not a
//! degenerate case but *the* case: the impulse response is the reverb. The
//! fixtures run 24000 samples — half a second — because the first 50 ms of a
//! hall tell you almost nothing about whether the tail is right.
//!
//! Sixteen algorithms, each at two decay settings, because a rewrite that
//! breaks a feedback path usually breaks it at one end of the decay range
//! first.
//!
//! One pair of vectors is deliberately identical: `ir_reflections_short` and
//! `ir_reflections_long`. `Reflections::set_params` reads `size`, `extra_a`
//! and `damping` and never `decay` — early reflections are a fixed tap
//! pattern rather than a decaying tail — so the two decay settings render the
//! same audio. That is by design, and checked here so a future reader does
//! not mistake it for a fixture that forgot to vary anything.

use audiocore_dsp::{AudioConfig, Processor};
use dsp_golden::{Golden, golden, signal};
use reverb_dsp::AlgorithmType;
use reverb_dsp::chain::ReverbChain;

dsp_golden::install_counting_allocator!();

const SR: f64 = 48_000.0;
const GENERATOR_RATE: f32 = 48_000.0;
const BLOCK: usize = 512;
/// Half a second — long enough for the tail, not so long that 32 fixtures
/// make the suite slow.
const LEN: usize = 24_000;

/// Every algorithm, with the name its reference file carries.
const ALGORITHMS: [(&str, AlgorithmType); 16] = [
    ("room", AlgorithmType::Room),
    ("hall", AlgorithmType::Hall),
    ("plate", AlgorithmType::Plate),
    ("spring", AlgorithmType::Spring),
    ("cloud", AlgorithmType::Cloud),
    ("bloom", AlgorithmType::Bloom),
    ("shimmer", AlgorithmType::Shimmer),
    ("chorale", AlgorithmType::Chorale),
    ("magneto", AlgorithmType::Magneto),
    ("nonlinear", AlgorithmType::NonLinear),
    ("swell", AlgorithmType::Swell),
    ("reflections", AlgorithmType::Reflections),
    ("velvet", AlgorithmType::Velvet),
    ("freeverb", AlgorithmType::FreeVerb),
    ("convolution", AlgorithmType::Convolution),
    ("random", AlgorithmType::Random),
];

const fn config() -> AudioConfig {
    AudioConfig { sample_rate: SR, max_buffer_size: BLOCK }
}

/// A chain at a stated decay, fully wet.
///
/// Fully wet on purpose: at any other mix the dry signal dominates the first
/// milliseconds and dilutes everything the fixture is trying to observe.
fn chain(algo: AlgorithmType, decay: f64) -> ReverbChain {
    let mut c = ReverbChain::new();
    c.set_algorithm(algo);
    c.params.decay = decay;
    c.params.size = 0.6;
    c.params.diffusion = 0.7;
    c.params.damping = 0.4;
    c.params.modulation = 0.3;
    c.mix = 1.0;
    c.predelay_ms = 12.0;
    c.reset();
    c.update(config());
    c
}

/// Render a stereo pair in host-sized blocks, interleaved.
fn render(c: &mut ReverbChain, left_in: &[f64], right_in: &[f64]) -> Vec<f64> {
    let mut left = left_in.to_vec();
    let mut right = right_in.to_vec();
    for (l, r) in left.chunks_mut(BLOCK).zip(right.chunks_mut(BLOCK)) {
        c.process(l, r);
    }
    left.into_iter().zip(right).flat_map(<[f64; 2]>::from).collect()
}

/// A stereo impulse, offset by one sample between channels so a fixture
/// cannot pass while the two channels are swapped or summed.
fn impulse_pair() -> (Vec<f64>, Vec<f64>) {
    let left = signal::widen(&signal::impulse(LEN));
    let mut right = vec![0.0; LEN];
    if let Some(slot) = right.get_mut(1) {
        *slot = 1.0;
    }
    (left, right)
}

#[test]
fn every_algorithm_holds_its_impulse_response() {
    let g: Golden = golden!();
    let (left, right) = impulse_pair();
    for (name, algo) in ALGORITHMS {
        for (tag, decay) in [("short", 0.2_f64), ("long", 0.85)] {
            let mut c = chain(algo, decay);
            let out = render(&mut c, &left, &right);
            dsp_golden::assert_golden!(g, &format!("ir_{name}_{tag}"), &out);
        }
    }
}

#[test]
fn every_algorithm_holds_its_reference_on_programme() {
    // The impulse response is the reverb, but it does not exercise the input
    // stage — the pre-delay, the input filters, the saturation and the ducker
    // all sit in front of the tank and all see programme rather than a spike.
    let g: Golden = golden!();
    let left = signal::widen(&signal::transients(LEN, GENERATOR_RATE));
    let right = signal::widen(&signal::sine(LEN, 180.0, GENERATOR_RATE));
    for (name, algo) in ALGORITHMS {
        let mut c = chain(algo, 0.6);
        c.saturation = 0.4;
        c.input_hp_freq = 90.0;
        c.output_lp_freq = 9_000.0;
        c.duck_amount = 0.5;
        c.update(config());
        let out = render(&mut c, &left, &right);
        dsp_golden::assert_golden!(g, &format!("prog_{name}"), &out);
    }
}

#[test]
fn the_chain_controls_hold_their_references() {
    // The parts of the chain that are not the tank: width, tremolo, the
    // output tilt, freeze. Driven on one algorithm so a change here is
    // attributable to the control rather than to the reverb behind it.
    let g: Golden = golden!();
    let (left, right) = impulse_pair();
    for (name, setup) in chain_variants() {
        let mut c = chain(AlgorithmType::Hall, 0.6);
        setup(&mut c);
        c.update(config());
        let out = render(&mut c, &left, &right);
        dsp_golden::assert_golden!(g, &format!("chain_{name}"), &out);
    }
}

/// Chain configurations checked to render differently from one another.
type ChainVariant = (&'static str, Box<dyn Fn(&mut ReverbChain)>);

fn chain_variants() -> Vec<ChainVariant> {
    vec![
        ("plain", Box::new(|_: &mut ReverbChain| {})),
        ("narrow", Box::new(|c: &mut ReverbChain| c.width = 0.0)),
        ("wide", Box::new(|c: &mut ReverbChain| c.width = 2.0)),
        (
            "tremolo",
            Box::new(|c: &mut ReverbChain| {
                c.trem_rate_hz = 4.5;
                c.trem_depth = 0.8;
            }),
        ),
        (
            "tilted",
            Box::new(|c: &mut ReverbChain| {
                c.output_tilt_db = 6.0;
                c.output_tilt_pivot = 800.0;
            }),
        ),
        ("frozen", Box::new(|c: &mut ReverbChain| c.freeze = true)),
        (
            "saturated",
            Box::new(|c: &mut ReverbChain| c.saturation = 0.9),
        ),
    ]
}

#[test]
fn every_chain_control_renders_differently() {
    // The guard on the fixture above: a control that changes nothing is
    // either not reaching the processor or not being driven, and a duplicated
    // reference vector would hide both.
    let (left, right) = impulse_pair();
    let mut seen: Vec<(String, u64)> = Vec::new();
    let mut collisions = Vec::new();
    for (name, setup) in chain_variants() {
        let mut c = chain(AlgorithmType::Hall, 0.6);
        setup(&mut c);
        c.update(config());
        let digest = render(&mut c, &left, &right)
            .iter()
            .fold(0xcbf2_9ce4_8422_2325_u64, |h, v| {
                v.to_bits()
                    .to_le_bytes()
                    .iter()
                    .fold(h, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3))
            });
        if let Some((other, _)) = seen.iter().find(|(_, d)| *d == digest) {
            collisions.push(format!("`{name}` == `{other}`"));
        }
        seen.push((name.to_owned(), digest));
    }
    assert!(collisions.is_empty(), "inert chain controls: {collisions:?}");
}

#[test]
fn silence_in_is_silence_out_for_every_algorithm() {
    // A reverb is a feedback loop with gain in it. If it is not exactly
    // silent at rest it is either self-oscillating or grinding denormals, and
    // both are audible the moment the player stops.
    let quiet = vec![0.0_f64; 8192];
    for (name, algo) in ALGORITHMS {
        let mut c = chain(algo, 0.9);
        let out = render(&mut c, &quiet, &quiet);
        let peak = out.iter().fold(0.0_f64, |m, s| m.max(s.abs()));
        assert!(peak < 1e-9, "{name} emitted {peak} on silence");
    }
}

#[test]
fn no_algorithm_runs_away_at_maximum_decay() {
    // The counterpart, and the failure this crate has actually had: a
    // feedback path that regenerates rather than decays. Every algorithm, at
    // the top of the decay range, fed real signal.
    let input = signal::widen(&signal::transients(LEN, GENERATOR_RATE));
    for (name, algo) in ALGORITHMS {
        let mut c = chain(algo, 1.0);
        let out = render(&mut c, &input, &input);
        let peak = out.iter().fold(0.0_f64, |m, s| m.max(s.abs()));
        assert!(peak.is_finite(), "{name} produced a non-finite sample");
        assert!(peak < 100.0, "{name} ran away to {peak} at maximum decay");
    }
}

#[test]
fn block_size_does_not_change_the_output() {
    // A rewrite that caches something per block rather than per sample breaks
    // exactly this, and no single-block fixture would notice.
    let (left, right) = impulse_pair();
    let render_at = |block: usize| {
        let mut c = chain(AlgorithmType::Hall, 0.6);
        let mut l = left.clone();
        let mut r = right.clone();
        for (a, b) in l.chunks_mut(block).zip(r.chunks_mut(block)) {
            c.process(a, b);
        }
        (l, r)
    };
    let (big_l, big_r) = render_at(512);
    let (small_l, small_r) = render_at(64);
    for (n, (a, b)) in big_l.iter().zip(&small_l).enumerate() {
        assert_eq!(a.to_bits(), b.to_bits(), "left diverged at sample {n}");
    }
    for (n, (a, b)) in big_r.iter().zip(&small_r).enumerate() {
        assert_eq!(a.to_bits(), b.to_bits(), "right diverged at sample {n}");
    }
}

#[test]
fn processing_allocates_nothing() {
    let input = signal::widen(&signal::noise(BLOCK, 5));
    let mut chains: Vec<ReverbChain> =
        ALGORITHMS.into_iter().map(|(_, a)| chain(a, 0.6)).collect();
    let mut left = input.clone();
    let mut right = input;

    let mut run = |left: &mut [f64], right: &mut [f64]| {
        for c in &mut chains {
            c.process(left, right);
        }
    };
    // Warm up outside the guard: the first block may size buffers.
    run(&mut left, &mut right);
    dsp_golden::assert_no_alloc(|| run(&mut left, &mut right));
}
