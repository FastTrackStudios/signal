//! Match FTS-Reverb against the Strymon BigSky plugin, algorithm by
//! algorithm.
//!
//! Both reverbs are driven through the same `daw::standalone::offline_fx::render`
//! — same stimulus, same pre-roll, same block size — and measured with
//! [`signal_analyzer::reverb_character`]. BigSky is set by its own knob
//! names and values; ours is set through [`Algo::map`], the translation
//! from BigSky's knobs to our parameters. That translation is half of what
//! this tool exists to get right (the other half is the DSP).
//!
//! ```text
//! EX="cargo +1.94.0 run --release -p signal-analyzer --example bigsky_match --"
//!
//! $EX list                                   # BigSky's parameters, selectors spelled out
//! $EX run                                    # Cloud at BigSky's defaults, all stimuli
//! $EX run --knobs Decay=8000,MOD=127 --stimuli impulse,sine1k
//! $EX run --ours size=0.8,diffusion=0.9      # override our side after the mapping
//! $EX run --input guitar.wav                 # any WAV as the stimulus
//! $EX sweep PreDelay 0,32,64,127             # how a knob moves both reverbs
//! $EX sweep Tone 0,32,64,96,127 --ref-only   # reverse-engineer one knob's law
//! $EX sweep Tone 0,64,127 --ref-only --bands  # + absolute level per octave (static filters)
//! ```
//!
//! Output goes to `--out` (default `target/bigsky-match/<algo>/`): the
//! stimulus, both renders as WAVs to listen to, and `report.md`.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use daw::standalone::offline_fx::{self, RenderOptions};
use fx_blocks::NativeReverb;
use signal_analyzer::reverb_character::{self, Character};
use signal_analyzer::reverb_stimuli::Stimulus;

const SR: f64 = 48_000.0;
const BIGSKY: &str = "/Library/Audio/Plug-Ins/VST3/Strymon/BigSky.vst3";

/// BigSky knob values by name, as the plugin displays them.
type Knobs = BTreeMap<String, String>;

/// One BigSky algorithm and how its knobs translate to ours.
struct Algo {
    name: &'static str,
    /// `EFFECT TYPE` position.
    bigsky_type: &'static str,
    /// Prefix of this algorithm's knobs (`CLOUD_Decay`, …).
    prefix: &'static str,
    /// BigSky's own defaults for the algorithm — the sound it ships with.
    defaults: &'static [(&'static str, &'static str)],
    /// BigSky knobs → our `(param name, value)`, in the order to apply.
    map: fn(&Knobs) -> Vec<(String, f64)>,
}

fn knob(k: &Knobs, name: &str) -> f64 {
    k.iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .and_then(|(_, v)| v.trim().parse().ok())
        .unwrap_or(0.0)
}

/// Piecewise-linear lookup in `(x, y)` points (sorted by x), clamped at
/// the ends; `log` interpolates in log-log (for time laws).
fn lookup(points: &[(f64, f64)], x: f64, log: bool) -> f64 {
    let (Some(first), Some(last)) = (points.first(), points.last()) else { return 0.0 };
    if x <= first.0 {
        return first.1;
    }
    if x >= last.0 {
        return last.1;
    }
    for w in points.windows(2) {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        if x <= x1 {
            if log && x0 > 0.0 && y0 > 0.0 {
                let f = (x / x0).ln() / (x1 / x0).ln();
                return y0 * (y1 / y0).powf(f);
            }
            return y0 + (y1 - y0) * (x - x0) / (x1 - x0);
        }
    }
    last.1
}

/// BigSky Cloud `Decay` (ms, as displayed) → measured T20 (s), impulse,
/// other knobs at defaults. Not a time: everything up to ~3000 rings for
/// ~4.5 s, and above that T20 ≈ 11.4·(D/8000)^1.5. 50000 is infinite.
const CLOUD_DECAY_T20: &[(f64, f64)] = &[
    (1000.0, 4.50),
    (2000.0, 4.53),
    (3000.0, 4.77),
    (4000.0, 5.60),
    (5000.0, 6.78),
    (6000.0, 8.15),
    (8000.0, 11.39),
    (12000.0, 19.83),
    (16000.0, 30.60),
    (24000.0, 57.16),
    (32000.0, 75.19),
];

/// BigSky `PreDelay` (0–127) → measured onset (ms). ≈0.023·v² up to 64,
/// then accelerating to 1.5 s.
const CLOUD_PREDELAY_MS: &[(f64, f64)] = &[
    (0.0, 0.0),
    (8.0, 2.6),
    (16.0, 7.1),
    (24.0, 14.5),
    (32.0, 24.3),
    (48.0, 52.5),
    (64.0, 93.5),
    (80.0, 162.2),
    (96.0, 323.7),
    (112.0, 729.2),
    (120.0, 1086.4),
    (127.0, 1499.8),
];

/// BigSky `Tone` (0–127) → cutoff of a static 2-pole (Q≈0.5) low-pass on
/// the wet, fitted to the octave levels of a noise burst (≤0.7 dB RMS).
/// Per-band decay does not move with Tone: it is not in-loop damping.
const CLOUD_TONE_HZ: &[(f64, f64)] = &[
    (0.0, 926.0),
    (16.0, 1649.0),
    (32.0, 2455.0),
    (48.0, 3276.0),
    (64.0, 4037.0),
    (80.0, 4877.0),
    (96.0, 5776.0),
    (112.0, 6841.0),
    (127.0, 8022.0),
];

/// Our wet level against BigSky's at MIX 127, dB.
const CLOUD_WET_GAIN_DB: f64 = 4.0;

/// Cloud: BigSky `Decay` 1000–50000, `PreDelay`/`Tone`/`MOD` 0–127,
/// `LowEnd`/`Diffusion` −10…+10.
///
/// Decay, PreDelay and Tone are measured laws (the tables above); LowEnd
/// maps straight across (our Cloud implements BigSky's law for it). MOD
/// is still a first guess.
fn map_cloud(k: &Knobs) -> Vec<(String, f64)> {
    let p = |n: &str, v: f64| (n.to_string(), v);
    vec![
        p("algorithm", 4.0),
        p("mix", 1.0),
        p("dry", 0.0),
        p("decay_time", lookup(CLOUD_DECAY_T20, knob(k, "Decay"), true)),
        // Our pre-delay line holds 500 ms; BigSky reaches 1.5 s past ~110.
        p("predelay", lookup(CLOUD_PREDELAY_MS, knob(k, "PreDelay"), false).min(500.0)),
        p("high_cut", lookup(CLOUD_TONE_HZ, knob(k, "Tone"), true)),
        p("damping", 0.0),
        p("modulation", knob(k, "MOD") / 127.0),
        p("low_end", (knob(k, "LowEnd") + 10.0) / 20.0),
        p("diffusion", (knob(k, "Diffusion") + 10.0) / 20.0),
        // BigSky's MIX is a wet level (+10 dB from 64 to 127; this tool
        // renders at 127). Matched on the impulse's energy.
        p("wet_gain", CLOUD_WET_GAIN_DB),
    ]
}

const ALGOS: &[Algo] = &[Algo {
    name: "cloud",
    bigsky_type: "CLOUD",
    prefix: "CLOUD_",
    defaults: &[
        ("Decay", "2340"),
        ("PreDelay", "32"),
        ("Tone", "110"),
        ("MOD", "81"),
        ("LowEnd", "-2"),
        ("Diffusion", "4"),
        ("Hold", "OFF"),
    ],
    map: map_cloud,
}];

struct Args(Vec<String>);

impl Args {
    fn opt(&self, k: &str) -> Option<&str> {
        self.0.iter().position(|a| a == k).and_then(|i| self.0.get(i + 1)).map(String::as_str)
    }
    fn flag(&self, k: &str) -> bool {
        self.0.iter().any(|a| a == k)
    }
    fn positional(&self, i: usize) -> Option<&str> {
        let mut skip = false;
        self.0
            .iter()
            .filter(|a| {
                let keep = !skip && !a.starts_with("--");
                skip = a.starts_with("--") && !matches!(a.as_str(), "--ref-only" | "--bands");
                keep
            })
            .nth(i)
            .map(String::as_str)
    }
}

/// `a=1,b=2` → pairs.
fn pairs(s: Option<&str>) -> Vec<(String, String)> {
    s.unwrap_or("")
        .split(',')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("bigsky_match: {msg}");
    std::process::exit(1)
}

/// Render `input` through BigSky with `knobs`. A fresh instance per render,
/// so no tail from the previous one leaks into the pre-roll.
fn render_bigsky(algo: &Algo, knobs: &Knobs, input: &[f32], tail_s: f64) -> (Vec<f32>, Vec<f32>) {
    let mut plugin = offline_fx::load_vst3(Path::new(BIGSKY), 0).unwrap_or_else(|e| fail(format!("{e:?}")));
    let mut specs = vec![format!("EFFECT TYPE={}", algo.bigsky_type), "MIX=127".into()];
    specs.extend(knobs.iter().map(|(k, v)| format!("{}{k}={v}", algo.prefix)));
    let refs: Vec<&str> = specs.iter().map(String::as_str).collect();
    let params = offline_fx::resolve_all(&mut *plugin, &refs).unwrap_or_else(|e| fail(e));
    let opts = RenderOptions { sample_rate: SR, tail_secs: tail_s, params, pump_run_loop: true, ..RenderOptions::default() };
    let out = offline_fx::render(&mut *plugin, input, input, &opts).unwrap_or_else(|e| fail(format!("{e:?}")));
    plugin.deactivate();
    out
}

fn render_ours(settings: &[(String, f64)], input: &[f32], tail_s: f64) -> (Vec<f32>, Vec<f32>) {
    let mut rev = NativeReverb::new(SR);
    for (name, v) in settings {
        rev.set_named(name, *v);
    }
    let opts = RenderOptions { sample_rate: SR, tail_secs: tail_s, ..RenderOptions::default() };
    offline_fx::render(&mut rev, input, input, &opts).unwrap_or_else(|e| fail(format!("{e:?}")))
}

/// Our settings: the mapping, then any `--ours` overrides on top.
fn our_settings(algo: &Algo, knobs: &Knobs, overrides: &[(String, String)]) -> Vec<(String, f64)> {
    let mut s = (algo.map)(knobs);
    for (k, v) in overrides {
        let v: f64 = v.parse().unwrap_or_else(|_| fail(format!("--ours {k}={v}: not a number")));
        match s.iter_mut().find(|(n, _)| n == k) {
            Some(slot) => slot.1 = v,
            None => s.push((k.clone(), v)),
        }
    }
    s
}

fn tail_for(knobs: &Knobs) -> f64 {
    (knob(knobs, "Decay") / 1000.0 * 1.5 + 2.0).clamp(4.0, 40.0)
}

fn write_wav(path: &Path, l: &[f32], r: &[f32]) {
    let spec = hound::WavSpec { channels: 2, sample_rate: SR as u32, bits_per_sample: 32, sample_format: hound::SampleFormat::Float };
    let res = hound::WavWriter::create(path, spec).and_then(|mut w| {
        for (a, b) in l.iter().zip(r) {
            w.write_sample(*a)?;
            w.write_sample(*b)?;
        }
        w.finalize()
    });
    if let Err(e) = res {
        fail(format!("{}: {e}", path.display()));
    }
}

/// A WAV as a mono stimulus at 48 kHz (channels averaged; no resampling).
fn read_input(path: &Path) -> Vec<f32> {
    let mut r = hound::WavReader::open(path).unwrap_or_else(|e| fail(format!("{}: {e}", path.display())));
    let spec = r.spec();
    if spec.sample_rate != SR as u32 {
        eprintln!("warning: {} is {} Hz, used as 48 kHz", path.display(), spec.sample_rate);
    }
    let ch = usize::from(spec.channels.max(1));
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().map_while(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let scale = 2f32.powi(i32::from(spec.bits_per_sample) - 1);
            r.samples::<i32>().map_while(Result::ok).map(|v| v as f32 / scale).collect()
        }
    };
    samples.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect()
}

/// A stimulus to run: its name, samples, excitation end and tone.
struct Case {
    name: String,
    input: Vec<f32>,
    end_s: f64,
    tone: Option<f64>,
}

impl Case {
    fn of(s: Stimulus) -> Self {
        Self { name: s.name().into(), input: s.generate(SR), end_s: s.excitation_end_s(), tone: s.tone_hz() }
    }
}

fn main() {
    let args = Args(std::env::args().skip(1).collect());
    let algo_name = args.opt("--algo").unwrap_or("cloud");
    let algo = ALGOS.iter().find(|a| a.name == algo_name).unwrap_or_else(|| fail(format!("unknown --algo {algo_name}")));
    let mut knobs: Knobs = algo.defaults.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
    for (k, v) in pairs(args.opt("--knobs")) {
        let key = knobs.keys().find(|n| n.eq_ignore_ascii_case(&k)).cloned().unwrap_or(k);
        knobs.insert(key, v);
    }
    let overrides = pairs(args.opt("--ours"));
    let out = args.opt("--out").map_or_else(|| PathBuf::from("target/bigsky-match").join(algo.name), PathBuf::from);

    match args.positional(0).unwrap_or("run") {
        "list" => list(),
        "run" => run(&args, algo, &knobs, &overrides, &out),
        "sweep" => sweep(&args, algo, &knobs, &overrides),
        other => fail(format!("unknown command {other:?} (list | run | sweep)")),
    }
}

fn list() {
    let mut plugin = offline_fx::load_vst3(Path::new(BIGSKY), 0).unwrap_or_else(|e| fail(format!("{e:?}")));
    for info in plugin.params() {
        let current = plugin.param_value(info.id).unwrap_or(info.default);
        let text = plugin.value_to_text(info.id, current).unwrap_or_default();
        let (lo, hi) = (plugin.value_to_text(info.id, info.min).unwrap_or_default(), plugin.value_to_text(info.id, info.max).unwrap_or_default());
        println!("{:>4}  {:<24} {:>10}   [{} .. {}]", info.id, info.name, text.trim(), lo.trim(), hi.trim());
        if let Some(pos) = offline_fx::param_positions(&mut *plugin, &info) {
            println!("{:>4}  {:<24} {}", "", "", pos.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>().join(" | "));
        }
    }
}

fn run(args: &Args, algo: &Algo, knobs: &Knobs, overrides: &[(String, String)], out: &Path) {
    let cases: Vec<Case> = if let Some(path) = args.opt("--input") {
        let input = read_input(Path::new(path));
        let name = Path::new(path).file_stem().map_or_else(|| "input".into(), |s| s.to_string_lossy().into_owned());
        let end_s = input.len() as f64 / SR;
        vec![Case { name, input, end_s, tone: None }]
    } else {
        let names = args.opt("--stimuli").unwrap_or("impulse,burst,snare,sine220,sine1k,pluck,pad");
        names
            .split(',')
            .map(|n| Case::of(Stimulus::from_name(n.trim()).unwrap_or_else(|| fail(format!("unknown stimulus {n:?}")))))
            .collect()
    };
    if let Err(e) = std::fs::create_dir_all(out) {
        fail(format!("{}: {e}", out.display()));
    }
    let ours = our_settings(algo, knobs, overrides);
    let tail = tail_for(knobs);

    let mut doc = String::new();
    let _ = writeln!(doc, "# BigSky {} vs FTS-Reverb\n", algo.bigsky_type);
    let _ = writeln!(doc, "BigSky: {}\n", knobs.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", "));
    let _ = writeln!(doc, "Ours: {}\n", ours.iter().map(|(k, v)| format!("{k}={v:.3}")).collect::<Vec<_>>().join(", "));
    let mut summary = String::from("| stimulus | level Δ dB | decay Δ | envelope dB | echo dens. | spec early dB | spec late dB | corr | mod spread Δ |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|\n");
    let mut sections = String::new();
    for case in &cases {
        eprintln!("{}: rendering BigSky…", case.name);
        let (bl, br) = render_bigsky(algo, knobs, &case.input, tail);
        eprintln!("{}: rendering ours…", case.name);
        let (ol, or) = render_ours(&ours, &case.input, tail);
        write_wav(&out.join(format!("{}.input.wav", case.name)), &case.input, &case.input);
        write_wav(&out.join(format!("{}.bigsky.wav", case.name)), &bl, &br);
        write_wav(&out.join(format!("{}.ours.wav", case.name)), &ol, &or);
        let a = reverb_character::measure(&bl, &br, SR, case.end_s, case.tone);
        let b = reverb_character::measure(&ol, &or, SR, case.end_s, case.tone);
        let d = reverb_character::distance(&a, &b);
        let _ = writeln!(
            summary,
            "| {} | {:+.1} | {} | {:.1} | {:.2} | {} | {} | {:.2} | {} |",
            case.name,
            d.level_db,
            pct(d.decay_log2),
            d.envelope_db,
            d.echo_density,
            finite(d.spectrum_early_db),
            finite(d.spectrum_late_db),
            d.correlation,
            pct(d.modulation_spread_log2),
        );
        sections.push_str(&reverb_character::report(&case.name, &a, &b, "BigSky", "ours"));
    }
    let _ = writeln!(doc, "## Distance\n\n{summary}\n## Detail\n\n{sections}");
    let path = out.join("report.md");
    if let Err(e) = std::fs::write(&path, &doc) {
        fail(format!("{}: {e}", path.display()));
    }
    println!("{doc}");
    eprintln!("wrote {} (and WAVs beside it)", path.display());
}

/// A mean |log2 ratio| as "how far off", in percent.
fn pct(v: Option<f64>) -> String {
    v.map_or_else(|| "—".into(), |v| format!("{:.0}%", (2f64.powf(v) - 1.0) * 100.0))
}

/// One decimal, or "—" for a measurement that does not apply (NaN).
fn finite(v: f64) -> String {
    if v.is_finite() { format!("{v:.1}") } else { "—".into() }
}

/// One row of a sweep: the headline numbers for a single render.
fn sweep_row(c: &Character) -> String {
    let t = |v: Option<f64>| v.map_or_else(|| "—".into(), |x| format!("{x:.2}"));
    let band = |hz: f64| c.bands.iter().find(|b| (b.centre_hz - hz).abs() < 1.0).and_then(|b| b.decay.t20);
    let late_centroid = {
        let v: Vec<f64> = c.centroid_hz.iter().skip(4).take(10).copied().collect();
        v.iter().sum::<f64>() / v.len().max(1) as f64
    };
    let tilt = c.spectrum_late.get(18).zip(c.spectrum_late.get(9)).map(|(hi, lo)| hi - lo);
    let m = c.modulation.map_or_else(|| "—".into(), |m| format!("{:.1}/{:.1}Hz", m.sustain_spread_hz, m.tail_spread_hz));
    format!(
        "{:>7.1} {:>7.1} {:>6.0} {:>6} {:>6} {:>6} {:>6} {:>6} {:>7} {:>7.0} {:>7} {:>6.2} {:>11}",
        c.energy_db,
        c.onset_ms,
        c.peak_ms,
        t(c.decay.edt),
        t(c.decay.t20),
        t(band(500.0)),
        t(band(4000.0)),
        c.echo_density.get(10).map_or_else(|| "—".into(), |v| format!("{v:.2}")),
        t(c.mixing_ms.map(|v| v / 1000.0)),
        late_centroid,
        tilt.map_or_else(|| "—".into(), |v| format!("{v:+.1}")),
        c.correlation[2],
        m
    )
}

/// Absolute energy per octave (dB): what a static filter does shows up
/// here as a fixed shape, independent of the decay.
fn bands_row(c: &Character) -> String {
    c.bands
        .iter()
        .map(|b| format!("{:.0}:{:+.1}", b.centre_hz, c.energy_db + b.level_db))
        .collect::<Vec<_>>()
        .join(" ")
}

fn sweep(args: &Args, algo: &Algo, knobs: &Knobs, overrides: &[(String, String)]) {
    let (Some(name), Some(values)) = (args.positional(1), args.positional(2)) else {
        fail("usage: sweep <Knob> <v1,v2,...> [--stimulus impulse] [--ref-only]")
    };
    let stim = args.opt("--stimulus").unwrap_or("impulse");
    let case = Case::of(Stimulus::from_name(stim).unwrap_or_else(|| fail(format!("unknown stimulus {stim:?}"))));
    let key = knobs.keys().find(|n| n.eq_ignore_ascii_case(name)).cloned().unwrap_or_else(|| name.to_string());
    println!(
        "{algo} {key} sweep on {stim}  (tilt = late 4k − 500 Hz ⅓-oct, NED@50ms, corr 0.2–1 s, mod = held/tail spread around the tone)",
        algo = algo.name
    );
    println!(
        "{:>8} {:>7}  {:>7} {:>7} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>7} {:>7} {:>7} {:>6} {:>11}",
        key, "who", "energy", "onset", "peak", "EDT", "T20", "T@500", "T@4k", "NED50", "mix(s)", "centr", "tilt", "corr", "mod"
    );
    for v in values.split(',') {
        let mut k = knobs.clone();
        k.insert(key.clone(), v.trim().to_string());
        let tail = tail_for(&k);
        let (bl, br) = render_bigsky(algo, &k, &case.input, tail);
        let a = reverb_character::measure(&bl, &br, SR, case.end_s, case.tone);
        println!("{:>8} {:>7}  {}", v.trim(), "BigSky", sweep_row(&a));
        if args.flag("--bands") {
            println!("{:>8} {:>7}  {}", "", "octaves", bands_row(&a));
        }
        if !args.flag("--ref-only") {
            let (ol, or) = render_ours(&our_settings(algo, &k, overrides), &case.input, tail);
            let b = reverb_character::measure(&ol, &or, SR, case.end_s, case.tone);
            println!("{:>8} {:>7}  {}", "", "ours", sweep_row(&b));
            if args.flag("--bands") {
                println!("{:>8} {:>7}  {}", "", "octaves", bands_row(&b));
            }
        }
    }
}
