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

/// Cloud: BigSky `Decay` 1000–50000 ms, `PreDelay`/`Tone`/`MOD` 0–127,
/// `LowEnd`/`Diffusion` −10…+10.
///
/// First-guess translation — every line here is a hypothesis for
/// `sweep` to confirm or correct.
fn map_cloud(k: &Knobs) -> Vec<(String, f64)> {
    let p = |n: &str, v: f64| (n.to_string(), v);
    vec![
        p("algorithm", 4.0),
        p("mix", 1.0),
        p("dry", 0.0),
        p("decay_time", knob(k, "Decay") / 1000.0),
        p("predelay", knob(k, "PreDelay")),
        p("modulation", knob(k, "MOD") / 127.0),
        p("damping", 1.0 - knob(k, "Tone") / 127.0),
        p("low_end", (knob(k, "LowEnd") + 10.0) / 20.0),
        p("diffusion", (knob(k, "Diffusion") + 10.0) / 20.0),
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
                skip = a.starts_with("--") && !matches!(a.as_str(), "--ref-only");
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
    let mut summary = String::from("| stimulus | level Δ dB | decay Δ | envelope dB | echo dens. | spec early dB | spec late dB | corr | mod dB |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|\n");
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
            "| {} | {:+.1} | {} | {:.1} | {:.2} | {:.1} | {:.1} | {:.2} | {} |",
            case.name,
            d.level_db,
            d.decay_log2.map_or_else(|| "—".into(), |v| format!("{:.0}%", (2f64.powf(v) - 1.0) * 100.0)),
            d.envelope_db,
            d.echo_density,
            d.spectrum_early_db,
            d.spectrum_late_db,
            d.correlation,
            d.modulation_db.map_or_else(|| "—".into(), |v| format!("{v:.1}")),
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

/// One row of a sweep: the headline numbers for a single render.
fn sweep_row(c: &Character) -> String {
    let t = |v: Option<f64>| v.map_or_else(|| "—".into(), |x| format!("{x:.2}"));
    let band = |hz: f64| c.bands.iter().find(|b| (b.centre_hz - hz).abs() < 1.0).and_then(|b| b.decay.t20);
    let late_centroid = {
        let v: Vec<f64> = c.centroid_hz.iter().skip(4).take(10).copied().collect();
        v.iter().sum::<f64>() / v.len().max(1) as f64
    };
    let tilt = c.spectrum_late.get(18).zip(c.spectrum_late.get(9)).map(|(hi, lo)| hi - lo);
    let m = c.modulation.map_or_else(|| "—".into(), |m| format!("{:.1}/{:.1}", m.sustain_side_db, m.tail_side_db));
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

fn sweep(args: &Args, algo: &Algo, knobs: &Knobs, overrides: &[(String, String)]) {
    let (Some(name), Some(values)) = (args.positional(1), args.positional(2)) else {
        fail("usage: sweep <Knob> <v1,v2,...> [--stimulus impulse] [--ref-only]")
    };
    let stim = args.opt("--stimulus").unwrap_or("impulse");
    let case = Case::of(Stimulus::from_name(stim).unwrap_or_else(|| fail(format!("unknown stimulus {stim:?}"))));
    let key = knobs.keys().find(|n| n.eq_ignore_ascii_case(name)).cloned().unwrap_or_else(|| name.to_string());
    println!(
        "{algo} {key} sweep on {stim}  (tilt = late 4k − 500 Hz ⅓-oct, NED@50ms, corr 0.2–1 s, mod = held/tail sidebands dB)",
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
        if !args.flag("--ref-only") {
            let (ol, or) = render_ours(&our_settings(algo, &k, overrides), &case.input, tail);
            let b = reverb_character::measure(&ol, &or, SR, case.end_s, case.tone);
            println!("{:>8} {:>7}  {}", "", "ours", sweep_row(&b));
        }
    }
}
