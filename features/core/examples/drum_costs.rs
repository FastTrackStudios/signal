//! What a drum mix costs: the Unbreakable kit (`rigs::drum_kit`, every
//! lane, its verbs and parallel compressors) over twenty mics of
//! synthetic drum hits, and each block kind alone, per 512-frame block at
//! 48 kHz.
//!
//! `cargo run --release -p signal-core --example drum_costs [-- --seconds S]
//! [--dump FILE] [--compare FILE]`: `--dump` writes the kit's output and
//! its phase meters, `--compare` nulls a run against such a file (the
//! largest difference, sample and meter).

use std::time::Instant;

use signal_core::graph::{self, BuildOptions};
use signal_core::host::NoHost;
use signal_core::state::{BlockSpec, LaneSpec, Phase, Preset};
use signal_core::{BlockFactory, BuiltinFactory, Processor, rigs};

const SR: f32 = 48_000.0;
const BLOCK: usize = 512;
const MICS: usize = 20;

/// A deterministic noise source (xorshift), -1..1.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Mic `c` of a kit playing eighth notes at 120 BPM: its drum's hits
/// (a decaying tone plus noise) and the other drums' bleed, quieter.
fn mic(c: usize, frames: usize) -> Vec<f32> {
    let mut noise = Noise(0x9e37_79b9 ^ (c as u32 + 1) * 7919);
    let eighth = (SR * 0.25) as usize;
    let freq = 50.0 + 40.0 * c as f32;
    let decay = (-1.0 / (0.08 * SR)).exp();
    let (mut env, mut bleed, mut phase) = (0.0_f32, 0.0_f32, 0.0_f32);
    (0..frames)
        .map(|i| {
            if i % eighth == (c * 331) % eighth && (i / eighth + c) % 3 != 0 {
                env = 0.7;
            }
            if i % eighth == 0 {
                bleed = 0.1;
            }
            env *= decay;
            bleed *= decay;
            phase += freq / SR;
            let tone = (phase * core::f32::consts::TAU).sin();
            env * (0.7 * tone + 0.3 * noise.next()) + bleed * noise.next() + 1e-4 * noise.next()
        })
        .collect()
}

fn options<'a>(factories: &'a [&'a dyn BlockFactory]) -> BuildOptions<'a> {
    BuildOptions {
        sample_rate: SR,
        max_block: BLOCK,
        port: None,
        factories,
        host: &NoHost,
    }
}

/// Play `preset` over `inputs`, two channels out; the output, the phase
/// meters, and the seconds it took.
fn render(preset: &Preset, inputs: &[Vec<f32>]) -> (Vec<Vec<f32>>, Vec<f32>, f64) {
    let factories: [&dyn BlockFactory; 1] = [&BuiltinFactory];
    let (g, report) = graph::build(preset, &options(&factories));
    assert!(report.all_active(), "{report:?}");
    let mut proc = Processor::new();
    let _ = proc.install(g);
    let frames = inputs[0].len();
    let channels = inputs.len().max(2);
    let mut chans: Vec<Vec<f32>> = (0..channels)
        .map(|c| inputs.get(c).cloned().unwrap_or_else(|| inputs[0].clone()))
        .collect();
    let began = Instant::now();
    let mut start = 0;
    while start < frames {
        let end = (start + BLOCK).min(frames);
        let mut io: Vec<&mut [f32]> = chans.iter_mut().map(|c| &mut c[start..end]).collect();
        proc.process(&mut io, inputs.len(), end - start, &[], &[]);
        start = end;
    }
    let took = began.elapsed().as_secs_f64();
    let levels = proc
        .graph()
        .map(graph::Graph::phase_levels)
        .unwrap_or_default()
        .iter()
        .flat_map(|l| [l.in_db, l.out_db])
        .collect();
    chans.truncate(2);
    (chans, levels, took)
}

fn per_block(seconds: f64, frames: usize) -> f64 {
    seconds * 1e6 / (frames as f64 / BLOCK as f64)
}

fn one_block(spec: BlockSpec) -> Preset {
    Preset {
        name: "one".into(),
        inputs: 1,
        outputs: 2,
        lanes: vec![LaneSpec {
            name: "Lane".into(),
            input: vec![0],
            blocks: vec![spec],
            ..LaneSpec::default()
        }],
        macros: vec![],
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let seconds: f32 = arg("--seconds")
        .and_then(|s| s.parse().ok())
        .unwrap_or(20.0);
    let frames = (seconds * SR) as usize;
    let inputs: Vec<Vec<f32>> = (0..MICS).map(|c| mic(c, frames)).collect();

    let kit = rigs::drum_kit(&rigs::KitPieces::default());
    let (out, levels, took) = render(&kit, &inputs);
    println!(
        "the kit, {seconds:.0} s of {MICS} mics: {:.0} ms ({:.1} µs per 512-frame block, {:.1}x realtime)",
        took * 1e3,
        per_block(took, frames),
        f64::from(seconds) / took
    );

    if let Some(path) = arg("--dump") {
        let mut bytes = Vec::new();
        for x in out.iter().flatten().chain(&levels) {
            bytes.extend_from_slice(&x.to_le_bytes());
        }
        std::fs::write(&path, bytes).expect("write the dump");
        println!("  dumped to {path}");
    }
    if let Some(path) = arg("--compare") {
        let bytes = std::fs::read(&path).expect("read the dump");
        let theirs: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        let ours: Vec<f32> = out.iter().flatten().chain(&levels).copied().collect();
        assert_eq!(theirs.len(), ours.len(), "a dump of another length");
        let n = out.iter().map(Vec::len).sum::<usize>();
        let worst = |a: &[f32], b: &[f32]| {
            a.iter()
                .zip(b)
                .map(|(x, y)| (x - y).abs())
                .fold(0.0_f32, f32::max)
        };
        let peak = ours[..n].iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        let audio = worst(&ours[..n], &theirs[..n]);
        println!(
            "  null against {path}: audio {audio:e} ({:.1} dB under the peak {peak}), meters {:e} dB",
            20.0 * (peak / audio.max(1e-30)).log10(),
            worst(&ours[n..], &theirs[n..])
        );
    }

    println!("each block alone, one stereo lane (µs per 512-frame block):");
    let input = vec![inputs[0].clone()];
    let blocks = [
        (
            "gate (drum, self-keyed)",
            BlockSpec::signal("gate", "gate")
                .param("mode", 1.0)
                .param("threshold", -30.0),
        ),
        (
            "comp",
            BlockSpec::signal("comp", "comp")
                .param("threshold", -24.0)
                .param("ratio", 4.0),
        ),
        (
            "eq channel (low cut)",
            BlockSpec::signal("eq", "eq")
                .algorithm("channel")
                .param("low_cut", 80.0),
        ),
        (
            "eq parametric (flat)",
            BlockSpec::signal("eq", "eq").algorithm("parametric"),
        ),
        (
            "eq parametric (band pass)",
            BlockSpec::signal("eq", "eq")
                .algorithm("parametric")
                .param("band.1.used", 1.0)
                .param("band.1.shape", 6.0)
                .param("band.1.freq", 60.0)
                .param("band.1.q", 4.0)
                .param("band.1.slope", 2.0),
        ),
        (
            "drive 6 dB",
            BlockSpec::signal("sat", "drive").param("drive", 6.0),
        ),
        (
            "reverb room",
            BlockSpec::signal("verb", "reverb").algorithm("room"),
        ),
        (
            "reverb plate",
            BlockSpec::signal("verb", "reverb").algorithm("plate"),
        ),
        (
            "reverb hall",
            BlockSpec::signal("verb", "reverb").algorithm("hall"),
        ),
        (
            "metered phase, gain only",
            BlockSpec::signal("up", "gain")
                .param("gain", 0.0)
                .phase(Phase::Polish),
        ),
    ];
    for (name, spec) in blocks {
        let (_, _, took) = render(&one_block(spec), &input);
        println!("  {name:<28} {:>8.2}", per_block(took, frames));
    }
}
