//! Offline blocks end to end: an analysis is deterministic, the apply
//! stage allocates nothing and plays its lanes exactly as an offline
//! application of the same curves (whether Signal plays them or a host's
//! automation does), bypass is exact, and the aligner's delay, polarity
//! and all-pass are what they say.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use signal_core::block::{BlockProcessor, ParamChange};
use signal_core::graph::{self, BuildOptions};
use signal_core::host::NoHost;
use signal_core::offline::align::{self, AlignApply};
use signal_core::offline::level::{self, Amounts, Knobs, Segment};
use signal_core::offline::{Analysis, Curve, Playback, Range, State};
use signal_core::state::{BlockSpec, Preset};
use signal_core::{BlockFactory, BuiltinFactory, Processor};

// ── Counting allocations on this thread ─────────────────────────────────

struct Counting;

thread_local! {
    static COUNT: Cell<Option<usize>> = const { Cell::new(None) };
}

// SAFETY: System's, with a count beside it.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = COUNT.try_with(|c| c.set(c.get().map(|n| n + 1)));
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new: usize) -> *mut u8 {
        let _ = COUNT.try_with(|c| c.set(c.get().map(|n| n + 1)));
        unsafe { System.realloc(ptr, layout, new) }
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

/// How many allocations `f` made on this thread.
fn allocations(f: impl FnOnce()) -> usize {
    COUNT.with(|c| c.set(Some(0)));
    f();
    COUNT.with(|c| c.replace(None)).unwrap_or(0)
}

const SR: f64 = 48_000.0;

fn opts<'a>(factories: &'a [&'a dyn BlockFactory]) -> BuildOptions<'a> {
    BuildOptions {
        sample_rate: 48_000.0,
        max_block: 512,
        port: None,
        factories,
        host: &NoHost,
    }
}

fn vocal(seconds: f64) -> Vec<f64> {
    level::synth_vocal(SR, seconds, 7)
}

fn plan(samples: &[f64]) -> level::Plan {
    level::plan(
        &[Segment {
            start_s: 0.0,
            samples,
            sample_rate: SR,
        }],
        &Knobs::default(),
        &Amounts::default(),
        None,
    )
    .expect("a vocal to level")
}

/// A chain of one `level` block whose analysis is `lanes`, played by
/// `playback`.
fn leveled(lanes: &[(&str, &Curve)], playback: Playback) -> Preset {
    let mut a = Analysis {
        state: State::Fresh,
        playback,
        ..Analysis::default()
    };
    for (id, c) in lanes {
        a.lanes.insert((*id).to_owned(), (*c).clone());
    }
    let mut block = BlockSpec::signal("level", "level");
    block.analysis = Some(a);
    let mut preset = Preset::passthrough();
    preset.lanes[0].blocks = vec![block];
    preset
}

fn processor(preset: &Preset) -> Processor {
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let (g, report) = graph::build(preset, &opts(&factories));
    assert!(report.all_active(), "{report:?}");
    let mut p = Processor::new();
    p.install(g);
    p.set_position(0);
    p
}

/// `input` on both channels through `p`, `block` frames per host call.
fn render(p: &mut Processor, input: &[f32], block: usize) -> Vec<f32> {
    let (mut l, mut r) = (input.to_vec(), input.to_vec());
    let mut at = 0;
    while at < input.len() {
        let end = (at + block).min(input.len());
        let mut io: [&mut [f32]; 2] = [&mut l[at..end], &mut r[at..end]];
        p.process(&mut io, 2, end - at, &[], &[]);
        at = end;
    }
    assert_eq!(l, r);
    l
}

#[test]
fn an_analysis_is_deterministic() {
    let v = vocal(40.0);
    let (a, b) = (plan(&v), plan(&v));
    assert_eq!(a.lanes, b.lanes);
    assert_eq!(a.levels, b.levels);
    assert!(
        a.lanes.gate.points().len() > 4,
        "the gate shut between lines"
    );
    assert!(a.lanes.ess.points().len() > 4, "esses were found");
    assert!(a.lanes.ride.points().len() > 4);
    // The verses (quiet) are ridden up toward the choruses.
    let ride = |t: f64| f64::from(a.lanes.ride.value_at(t).unwrap());
    assert!(
        ride(5.0) > ride(21.0) + 6.0,
        "{} vs {}",
        ride(5.0),
        ride(21.0)
    );
}

#[test]
fn playback_is_the_offline_application_of_the_curves_and_allocates_nothing() {
    let v = vocal(40.0);
    let p = plan(&v);
    // The full-band lanes: the de-ess band is its own test.
    let lanes = [
        ("gate", &p.lanes.gate),
        ("breath", &p.lanes.breath),
        ("ride", &p.lanes.ride),
    ];
    let input: Vec<f32> = v.iter().map(|s| *s as f32).collect();
    let mut proc = processor(&leveled(&lanes, Playback::Signal));
    assert_eq!(proc.graph().unwrap().self_played_lanes(), 3);
    // Warm up one call (the first piece's jump), then count.
    let mut out = Vec::with_capacity(input.len());
    out.extend(render(&mut proc, &input[..1000], 333));
    let mut rest = input[1000..].to_vec();
    let mut r = rest.clone();
    let made = allocations(|| {
        let mut at = 0;
        while at < rest.len() {
            let end = (at + 333).min(rest.len());
            let mut io: [&mut [f32]; 2] = [&mut rest[at..end], &mut r[at..end]];
            proc.process(&mut io, 2, end - at, &[], &[]);
            at = end;
        }
    });
    assert_eq!(made, 0, "the apply stage allocated on the audio path");
    out.extend(rest);

    // The offline application: each sample times the lanes' sum, in dB.
    let mut worst = 0.0_f64;
    for (n, (x, y)) in v.iter().zip(&out).enumerate() {
        let t = n as f64 / SR;
        let db: f64 = lanes
            .iter()
            .map(|(_, c)| f64::from(c.value_at(t).unwrap_or(0.0)))
            .sum();
        let want = x * 10f64.powf(db / 20.0);
        worst = worst.max((f64::from(*y) - want).abs());
    }
    assert!(worst < 2e-6, "played vs applied offline: {worst}");
}

#[test]
fn signal_playing_a_lane_equals_a_host_automating_it_sample_for_sample() {
    let v = vocal(20.0);
    let p = plan(&v);
    let lanes = [("gate", &p.lanes.gate), ("ride", &p.lanes.ride)];
    let input: Vec<f32> = v.iter().map(|s| *s as f32).collect();
    let selfplayed = render(
        &mut processor(&leveled(&lanes, Playback::Signal)),
        &input,
        512,
    );

    // The host's automation: each parameter set to where its lane goes at
    // the end of each piece, pieces ending at every breakpoint.
    let mut host = processor(&leveled(&lanes, Playback::Host));
    let breaks: Vec<usize> = {
        let mut b: Vec<usize> = lanes
            .iter()
            .flat_map(|(_, c)| {
                c.points().iter().flat_map(|(t, _)| {
                    let f = (t * SR).floor() as usize;
                    [f, f + 1]
                })
            })
            .chain((0..input.len()).step_by(512))
            .chain([input.len()])
            .collect();
        b.sort_unstable();
        b.dedup();
        b
    };
    let index = |id: &str| level::PARAMS.iter().position(|s| s.id == id).unwrap();
    let (mut l, mut r) = (input.clone(), input.clone());
    let mut at = 0;
    for &end in breaks.iter().filter(|e| **e > 0) {
        let end = end.min(input.len());
        if end <= at {
            continue;
        }
        for (id, c) in &lanes {
            let t = end as f64 / SR;
            host.apply_param(0, 0, index(id), ParamChange::Set(c.value_at(t).unwrap()));
        }
        let mut io: [&mut [f32]; 2] = [&mut l[at..end], &mut r[at..end]];
        host.process(&mut io, 2, end - at, &[], &[]);
        at = end;
    }
    // From the first piece on: Signal jumps to its first values where a
    // host's first piece ramps to them from the parameter's default.
    let first = breaks.iter().copied().find(|b| *b > 0).unwrap_or(0);
    let worst = selfplayed[first..]
        .iter()
        .zip(&l[first..])
        .fold(0.0_f32, |m, (a, b)| m.max((a - b).abs()));
    assert!(worst < 1e-6, "{worst}");
}

#[test]
fn a_bypassed_offline_block_is_exactly_no_block() {
    let v = vocal(10.0);
    let p = plan(&v);
    let mut preset = leveled(
        &[("ride", &p.lanes.ride), ("gate", &p.lanes.gate)],
        Playback::Signal,
    );
    preset.lanes[0].blocks[0].bypass = true;
    let input: Vec<f32> = v.iter().map(|s| *s as f32).collect();
    let mut proc = processor(&preset);
    assert_eq!(render(&mut proc, &input, 256), input);
}

#[test]
fn a_part_analysed_again_rides_to_the_same_levels() {
    let v = vocal(48.0);
    let whole = plan(&v);
    let from = (16.0 * SR) as usize;
    let part = level::plan(
        &[Segment {
            start_s: 16.0,
            samples: &v[from..],
            sample_rate: SR,
        }],
        &Knobs::default(),
        &Amounts::default(),
        Some(whole.levels),
    )
    .unwrap();
    assert_eq!(part.levels, whole.levels);
    // Where the blocks fall the same, so do the curves, well inside the
    // part — within the decimation's tolerance: the part reads its own
    // noise floor, which moves where a ride's points are kept.
    let range = Range::new(20.0, 44.0);
    for t in [21.0, 25.5, 33.0, 40.25] {
        let (a, b) = (
            whole.lanes.ride.value_at(t).unwrap(),
            part.lanes.ride.value_at(t).unwrap(),
        );
        assert!((a - b).abs() < 0.25, "{t}: {a} vs {b}");
    }
    let spliced = whole
        .lanes
        .ride
        .spliced(range, &part.lanes.ride.within(range));
    assert!(spliced.points().len() > 4);
}

// ── align ───────────────────────────────────────────────────────────────

fn impulse(n: usize) -> Vec<f32> {
    let mut x = vec![0.0; n];
    x[0] = 0.75;
    x
}

fn through(b: &mut AlignApply, x: &[f32]) -> Vec<f32> {
    let (mut l, mut r) = (x.to_vec(), x.to_vec());
    for (cl, cr) in l.chunks_mut(37).zip(r.chunks_mut(37)) {
        b.process(cl, cr);
    }
    assert_eq!(l, r);
    l
}

#[test]
fn align_delays_and_inverts_exactly() {
    let mut b = AlignApply::new(48_000.0, 7.0);
    assert_eq!(b.latency(), 0);
    let y = through(&mut b, &impulse(64));
    assert_eq!(y.iter().position(|s| *s != 0.0), Some(7));
    assert_eq!(y[7], 0.75);

    let mut b = AlignApply::new(48_000.0, 0.0);
    b.set_param(align::INVERT, 1.0);
    let x: Vec<f32> = (0..300)
        .map(|i| ((i * 7919) % 97) as f32 / 97.0 - 0.5)
        .collect();
    let y = through(&mut b, &x);
    assert!(
        y.iter().zip(&x).all(|(a, b)| *a == -*b),
        "polarity is a negation"
    );
}

#[test]
fn an_advance_is_reported_as_latency_and_the_net_shift_is_the_delay() {
    let mut b = AlignApply::new(48_000.0, -5.0);
    assert_eq!(b.latency(), 5, "the advance is the latency");
    let y = through(&mut b, &impulse(32));
    // Delayed by its latency less the advance: net of compensation, 5 early.
    assert_eq!(y.iter().position(|s| *s != 0.0), Some(0));
    b.reset();
    b.set_param(align::DELAY, 3.0);
    let y = through(&mut b, &impulse(32));
    assert_eq!(y.iter().position(|s| *s != 0.0), Some(8), "3 late, net");
    b.set_param(align::DELAY, -40.0);
    b.reset();
    let y = through(&mut b, &impulse(32));
    assert_eq!(
        y.iter().position(|s| *s != 0.0),
        Some(0),
        "no earlier than built for"
    );
}

/// An align block with sections at `(hz, q)` (second order) and `first`.
fn sections(ap: &[(f64, f64)], first: Option<f64>) -> AlignApply {
    let mut b = AlignApply::new(48_000.0, 0.0);
    for (k, (hz, q)) in ap.iter().enumerate() {
        let (ih, iq) = if k == 0 {
            (align::AP1_HZ, align::AP1_Q)
        } else {
            (align::AP2_HZ, align::AP2_Q)
        };
        b.set_param(ih, *hz as f32);
        b.set_param(iq, *q as f32);
    }
    if let Some(hz) = first {
        b.set_param(align::AP0_HZ, hz as f32);
    }
    b
}

/// `b`'s impulse response, `n` frames.
fn impulse_response(b: &mut AlignApply, n: usize) -> Vec<f64> {
    let mut x = vec![0.0_f32; n];
    x[0] = 1.0;
    through(b, &x).into_iter().map(f64::from).collect()
}

/// The DFT of `h` at `hz`: (magnitude, phase in radians).
fn dft_at(h: &[f64], hz: f64) -> (f64, f64) {
    let (mut re, mut im) = (0.0, 0.0);
    for (k, v) in h.iter().enumerate() {
        let w = 2.0 * std::f64::consts::PI * hz * k as f64 / SR;
        re += v * w.cos();
        im -= v * w.sin();
    }
    (re.hypot(im), im.atan2(re))
}

fn wrap(r: f64) -> f64 {
    (r + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI
}

#[test]
fn the_all_passes_are_flat_turn_the_phase_they_should_and_add_no_latency() {
    let cases: [(&[(f64, f64)], Option<f64>); 4] = [
        (&[(50.0, 0.7)], None),
        (&[(80.0, 1.0), (160.0, 2.0)], None),
        (&[(63.0, 0.5)], Some(120.0)),
        (&[], Some(40.0)),
    ];
    let freqs: Vec<f64> = (0..=60)
        .map(|k| 20.0 * 1000f64.powf(f64::from(k) / 60.0))
        .collect();
    for (ap, first) in cases {
        let mut b = sections(ap, first);
        assert_eq!(b.latency(), 0, "no latency without an advance");
        let analytic: Vec<_> = b.sections().copied().collect();
        // Long enough for a 20 Hz section to ring out.
        let h = impulse_response(&mut b, 1 << 18);
        let (mut worst_db, mut worst_phase) = (0.0_f64, 0.0_f64);
        for &f in &freqs {
            let (mag, phase) = dft_at(&h, f);
            worst_db = worst_db.max((20.0 * mag.log10()).abs());
            let want: f64 = analytic.iter().map(|s| s.response(f, SR).1).sum();
            worst_phase = worst_phase.max(wrap(phase - want).abs().to_degrees());
            for s in &analytic {
                assert!(
                    (s.response(f, SR).0 - 1.0).abs() < 1e-12,
                    "|H| from the coefficients"
                );
            }
        }
        eprintln!(
            "sections {ap:?} first {first:?}: flat within {worst_db:.2e} dB, phase within {worst_phase:.2e}° of the analytic all-pass, 20 Hz–20 kHz"
        );
        assert!(worst_db < 0.001, "{worst_db} dB");
        assert!(worst_phase < 0.01, "{worst_phase}°");
    }
    // The analytic phase is the all-pass's: −180° at a second-order
    // section's centre, −90° at a first-order one's.
    let s2 = align::AllPass::second(100.0, 0.7, SR);
    assert!((s2.response(100.0, SR).1.abs().to_degrees() - 180.0).abs() < 1e-6);
    let s1 = align::AllPass::first(100.0, SR);
    assert!((s1.response(100.0, SR).1.to_degrees() + 90.0).abs() < 1e-6);
}

#[test]
fn the_all_passes_allocate_nothing_and_a_bypassed_align_is_no_block() {
    let mut b = sections(&[(50.0, 0.7), (120.0, 1.5)], Some(200.0));
    b.set_param(align::INVERT, 1.0);
    let x: Vec<f32> = (0..4096)
        .map(|i| ((i * 7919) % 97) as f32 / 97.0 - 0.5)
        .collect();
    let (mut l, mut r) = (x.clone(), x.clone());
    let made = allocations(|| {
        for (cl, cr) in l.chunks_mut(64).zip(r.chunks_mut(64)) {
            b.process(cl, cr);
            b.set_param(align::AP1_HZ, 55.0);
        }
    });
    assert_eq!(made, 0);
    let mut spec = BlockSpec::signal("align", "align")
        .param("allpass.0.hz", 50.0)
        .param("delay_samples", 12.0);
    spec.bypass = true;
    let mut preset = Preset::passthrough();
    preset.lanes[0].blocks = vec![spec];
    let mut proc = processor(&preset);
    assert_eq!(render(&mut proc, &x, 128), x, "bypass is exact");
}

#[test]
fn the_aligner_finds_a_shift_and_a_flipped_polarity() {
    let reference: Vec<f64> = vocal(1.0);
    let target: Vec<f64> = (0..reference.len())
        .map(|i| i.checked_sub(23).map_or(0.0, |j| -reference[j]))
        .collect();
    let found = align::find(&reference, &target, 64);
    // The target is 23 late and upside down: delay it by -23 and invert.
    assert_eq!(found.delay_samples, -23);
    assert!(found.invert);
    assert!(found.correlation > 0.99, "{}", found.correlation);
}
