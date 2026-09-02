//! The render bridge: measure FTS-Reverb against a real reference reverb.
//!
//! Renders the same impulse through a hosted reference plugin and through
//! `signal_fx::NativeReverb` driven by the translated parameters, then reports
//! how far apart they are under the analyzer's metrics.
//!
//! ```text
//! # Compare a Valhalla factory preset against our reverb
//! cargo run -p signal-analyzer --example reverb_match -- \
//!     --plugin "$HOME/.vst3/yabridge/ValhallaVintageVerb.vst3" \
//!     --preset "$HOME/.wine-audiohaven/drive_c/ProgramData/Valhalla DSP, LLC/\
//! ValhallaVintageVerb/Presets/Factory/Plates/A Plate.vpreset"
//!
//! # Tune our reverb to the reference's measured decay, then compare
//! cargo run -p signal-analyzer --example reverb_match -- \
//!     --plugin ... --preset ... --tune
//!
//! # Enumerate a parameter's menu, by sweeping it and reading the display text
//! cargo run -p signal-analyzer --example reverb_match -- \
//!     --plugin "$HOME/.vst3/yabridge/ValhallaVintageVerb.vst3" \
//!     --enumerate ReverbMode
//! ```
//!
//! Reference state is injected **by parameter name**, not by loading a state
//! chunk: Valhalla's exposed parameter names are exactly its preset XML
//! attribute names, and both sides are normalized 0–1, so a name match is
//! lossless and avoids synthesizing VST3 chunk framing.

use std::collections::BTreeMap;
use std::path::Path;

use signal_analyzer::{compare, decay, generators, DecayFit, Thresholds};
use signal_fx::NativeReverb;
use signal_import::valhalla;
use signal_plugin_host::{HostedPlugin, PluginInstance};

const SAMPLE_RATE: f64 = 48_000.0;
const BLOCK: usize = 512;

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

/// Where a preset's cached reference render lives, if caching is on.
fn cache_path_for(preset_path: &str) -> Option<std::path::PathBuf> {
    let dir = arg("--reference-cache")?;
    let stem = Path::new(preset_path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "preset".into());
    Some(Path::new(&dir).join(format!("{stem}.reference.wav")))
}

fn main() {
    // A cached reference makes the plugin unnecessary — don't even try to load
    // it. That is the difference between "the cache saves time" and "the cache
    // lets the work continue when the plugin is unavailable".
    if let Some(preset_path) = arg("--preset") {
        if let Some(cached) = cache_path_for(&preset_path).and_then(|p| read_wav(&p)) {
            println!(
                "reference: from cache ({} samples, no plugin needed)",
                cached.len()
            );
            run_comparison(None, &preset_path, Some(cached));
            return;
        }
    }

    let Some(plugin_path) = arg("--plugin") else {
        eprintln!("usage: reverb_match --plugin <path> [--preset <file> | --enumerate <param>]");
        std::process::exit(2);
    };

    let mut reference = match HostedPlugin::load(&plugin_path) {
        Ok(Some(p)) => p,
        Ok(None) => {
            eprintln!("{plugin_path}: resolved to the synthetic backend");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("{plugin_path}: load failed: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = reference.prepare(SAMPLE_RATE, BLOCK as u32) {
        eprintln!("prepare failed: {e}");
        std::process::exit(1);
    }
    println!("reference: {}", reference.descriptor().name);

    if let Some(param) = arg("--enumerate") {
        enumerate(&mut reference, &param);
        return;
    }

    let Some(preset_path) = arg("--preset") else {
        eprintln!("need --preset <file> or --enumerate <param>");
        std::process::exit(2);
    };
    run_comparison(Some(&mut reference), &preset_path, None);
}

/// Sweep a parameter across its range and print each distinct display text.
///
/// This is how a plugin's menu is recovered without guessing: Valhalla stores
/// mode selectors as `index / (count - 1)` fractions, and the plugin renders
/// each to its real name.
fn enumerate(plugin: &mut HostedPlugin, param_name: &str) {
    let Some(info) = plugin
        .params()
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case(param_name))
    else {
        eprintln!("no parameter named {param_name:?}");
        std::process::exit(1);
    };

    // `value_to_text(id, v)` is not usable here: through the VST3 bridge it
    // reports the parameter's *current* text and ignores `v`. So actually set
    // the value, run a block to flush the write, and read back what the
    // plugin now says it is.
    //
    // With `--slots N`, query exactly the N+1 fractions `k/N` — Valhalla
    // stores menu selectors as such fractions, and asking at the boundaries
    // says what each slot holds, which a fine sweep cannot (it merges a slot
    // into its predecessor's label range).
    let slots: Option<usize> = arg("--slots").and_then(|s| s.parse().ok());
    let mut scratch = vec![0.0f32; BLOCK * 2];
    let mut probe = |plugin: &mut HostedPlugin, v: f64| -> Option<String> {
        plugin.set_param(info.id, v);
        scratch.iter_mut().for_each(|s| *s = 0.0);
        plugin.process_interleaved(&mut scratch, &[], &[]).ok()?;
        let current = plugin.param_value(info.id).unwrap_or(v);
        plugin.value_to_text(info.id, current)
    };

    if let Some(n) = slots {
        println!("{} — {n} slots", info.name);
        for k in 0..=n {
            let v = info.min + (info.max - info.min) * (k as f64 / n as f64);
            match probe(plugin, v) {
                Some(text) => println!("  slot {k:>3}  value={v:.6}  {text}"),
                None => println!("  slot {k:>3}  value={v:.6}  <no text>"),
            }
        }
        return;
    }

    // Oversample the range so no menu entry is stepped over, then collapse to
    // the first fraction that produced each distinct label.
    const STEPS: usize = 400;
    let mut seen: BTreeMap<String, f64> = BTreeMap::new();
    for i in 0..=STEPS {
        let v = info.min + (info.max - info.min) * (i as f64 / STEPS as f64);
        match probe(plugin, v) {
            Some(text) => {
                seen.entry(text).or_insert(v);
            }
            None => break,
        }
    }

    println!("{} — {} distinct values", info.name, seen.len());
    let mut by_value: Vec<(f64, String)> = seen.into_iter().map(|(t, v)| (v, t)).collect();
    by_value.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let count = by_value.len();
    for (idx, (v, text)) in by_value.iter().enumerate() {
        // Report the index the analyzer's `mode_index` would recover.
        let recovered = (v * (count.saturating_sub(1)) as f64).round() as usize;
        println!("  [{idx:>3}] value={v:.6}  index~{recovered:>3}  {text}");
    }
}

/// Write a mono f32 render so it can be re-analyzed without the plugin.
fn write_wav(path: &Path, samples: &[f32]) -> std::io::Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE as u32,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut w =
        hound::WavWriter::create(path, spec).map_err(|e| std::io::Error::other(e.to_string()))?;
    for &x in samples {
        w.write_sample(x)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
    }
    w.finalize()
        .map_err(|e| std::io::Error::other(e.to_string()))
}

/// Read a render written by [`write_wav`].
fn read_wav(path: &Path) -> Option<Vec<f32>> {
    let mut r = hound::WavReader::open(path).ok()?;
    Some(r.samples::<f32>().filter_map(|s| s.ok()).collect())
}

/// Blocks of silence to run before the impulse.
///
/// Parameter writes are drained at the top of a block, and a reverb that
/// changes algorithm rebuilds (and clears) its delay lines when it sees them.
/// Firing the impulse in that same block gets it swallowed — which reads as a
/// silent reference and looks exactly like a translation failure. Settling
/// first is what makes the measurement mean anything.
const WARMUP_BLOCKS: usize = 16;

/// Render `frames` of an impulse through a hosted plugin, 100% wet.
fn render_reference(plugin: &mut HostedPlugin, frames: usize) -> Vec<f32> {
    let stimulus = generators::impulse(frames);
    let mut out = Vec::with_capacity(frames);

    let mut warm = vec![0.0f32; BLOCK * 2];
    for _ in 0..WARMUP_BLOCKS {
        warm.iter_mut().for_each(|s| *s = 0.0);
        if plugin.process_interleaved(&mut warm, &[], &[]).is_err() {
            break;
        }
    }

    let mut pos = 0;
    while pos < frames {
        let n = BLOCK.min(frames - pos);
        // Interleaved stereo, processed in place.
        let mut buf = vec![0.0f32; n * 2];
        for i in 0..n {
            buf[2 * i] = stimulus[pos + i];
            buf[2 * i + 1] = stimulus[pos + i];
        }
        if let Err(e) = plugin.process_interleaved(&mut buf, &[], &[]) {
            eprintln!("process failed: {e}");
            std::process::exit(1);
        }
        // Take the left channel — the metrics are mono.
        out.extend((0..n).map(|i| buf[2 * i]));
        pos += n;
    }
    out
}

/// Render the same impulse through our own reverb.
fn render_native(params: &[(String, f64)], frames: usize) -> Vec<f32> {
    let mut rev = NativeReverb::new(SAMPLE_RATE);
    for (name, value) in params {
        rev.set_named(name, *value);
    }
    // Fully wet, to compare tails rather than the dry impulse.
    rev.set_named("mix", 1.0);
    if let Err(e) = rev.prepare(SAMPLE_RATE, BLOCK as u32) {
        eprintln!("native prepare failed: {e}");
        std::process::exit(1);
    }

    let stimulus = generators::impulse(frames);
    let mut out = Vec::with_capacity(frames);
    let events = signal_plugin_host::PluginEvents::default();

    // Settle on the same terms as the reference, so neither side is measured
    // mid-rebuild.
    let (mut wl, mut wr) = (vec![0.0f32; BLOCK], vec![0.0f32; BLOCK]);
    let quiet = vec![0.0f32; BLOCK];
    for _ in 0..WARMUP_BLOCKS {
        let _ = rev.process_block(&quiet, &quiet, &mut wl, &mut wr, &events);
    }

    let mut pos = 0;
    while pos < frames {
        let n = BLOCK.min(frames - pos);
        let in_l = &stimulus[pos..pos + n];
        let (mut out_l, mut out_r) = (vec![0.0f32; n], vec![0.0f32; n]);
        if let Err(e) = rev.process_block(in_l, in_l, &mut out_l, &mut out_r, &events) {
            eprintln!("native process failed: {e}");
            std::process::exit(1);
        }
        out.extend_from_slice(&out_l);
        pos += n;
    }
    out
}

fn run_comparison(
    mut reference: Option<&mut HostedPlugin>,
    preset_path: &str,
    cached_reference: Option<Vec<f32>>,
) {
    let bytes = match std::fs::read(preset_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("{preset_path}: {e}");
            std::process::exit(1);
        }
    };
    let Some(xml) = valhalla::extract_xml(&bytes) else {
        eprintln!("{preset_path}: no Valhalla element found");
        std::process::exit(1);
    };
    let Some(patch) = valhalla::parse_xml(&xml) else {
        eprintln!("{preset_path}: could not parse the Valhalla element");
        std::process::exit(1);
    };
    println!(
        "preset   : {} ({:?})",
        patch.preset_name.as_deref().unwrap_or("<unnamed>"),
        patch.plugin
    );

    // Drive the reference by name — Valhalla's parameter names are its XML
    // attribute names, and both are normalized 0-1.
    let by_name: BTreeMap<String, u32> = match reference.as_deref_mut() {
        Some(plugin) => plugin
            .params()
            .into_iter()
            .map(|p| (p.name.to_ascii_lowercase(), p.id))
            .collect(),
        // Working from a cached render: there is no plugin to address, and
        // nothing to apply the preset to.
        None => BTreeMap::new(),
    };

    let skip_params = std::env::args().any(|a| a == "--no-preset-params");
    let mut applied = 0usize;
    let mut skipped = Vec::new();
    for (key, raw) in patch.attributes.iter().filter(|_| !skip_params) {
        let Ok(value) = raw.parse::<f64>() else {
            continue; // presetName / pluginVersion
        };
        match by_name.get(&key.to_ascii_lowercase()) {
            Some(&id) => {
                if let Some(plugin) = reference.as_deref_mut() {
                    plugin.set_param(id, value);
                }
                applied += 1;
            }
            None => skipped.push(key.clone()),
        }
    }
    // Compare wet tails.
    if let (Some(&id), Some(plugin)) = (by_name.get("mix"), reference.as_deref_mut()) {
        plugin.set_param(id, 1.0);
    }
    if reference.is_some() {
        println!("applied  : {applied} parameters; unmatched: {skipped:?}");
    }

    // Size the render to the reference, rather than assuming six seconds.
    //
    // A decay fit needs the energy curve to fall 25 dB inside the window. A
    // reference that rings for 7.6 s cannot do that in 6 s, so its longest
    // bands come back "not measurable" — and a band that cannot be measured
    // cannot be matched, cannot be corrected, and (worse) leaves its Decay
    // Rate EQ band sitting at whatever value it drifted to, bleeding into the
    // neighbours that *are* being fitted. Four of DH-Beastly Verb's eight
    // octaves were invisible for exactly this reason.
    //
    // So: probe once, find the longest band the reference actually has, and
    // re-render both sides with room for it.
    const PROBE_SECONDS: f64 = 8.0;
    const TAIL_HEADROOM: f64 = 3.0;
    const MAX_SECONDS: f64 = 40.0;

    // A cached reference render makes the plugin optional. Rendering a
    // reference is the only part of this that needs the hosted plugin at all,
    // and it never changes for a given preset — so once captured, tuning can
    // continue without it (and identically, which a live render cannot
    // promise: the reference is re-rendered every run).
    let cache_path = arg("--reference-cache").map(|dir| {
        let stem = Path::new(preset_path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "preset".into());
        Path::new(&dir).join(format!("{stem}.reference.wav"))
    });
    if let Some(p) = &cache_path {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
    }
    let cached = cached_reference.or_else(|| cache_path.as_deref().and_then(read_wav));

    let probe_frames = (PROBE_SECONDS * SAMPLE_RATE) as usize;
    let probe_ir = match (&cached, reference.as_deref_mut()) {
        (Some(ir), _) => ir.clone(),
        (None, Some(plugin)) => render_reference(plugin, probe_frames),
        (None, None) => {
            eprintln!("no cached reference and no plugin to render one");
            std::process::exit(1);
        }
    };
    let longest = decay::reverb_time_per_band(&probe_ir, SAMPLE_RATE, DecayFit::T20)
        .iter()
        .filter_map(|(_, rt)| *rt)
        .fold(None, |acc: Option<f64>, v| {
            Some(acc.map_or(v, |a: f64| a.max(v)))
        })
        .or_else(|| decay::reverb_time_best_effort(&probe_ir, SAMPLE_RATE, DecayFit::T20))
        .unwrap_or(PROBE_SECONDS / TAIL_HEADROOM);
    let seconds = (longest * TAIL_HEADROOM).clamp(PROBE_SECONDS, MAX_SECONDS);
    let frames = (seconds * SAMPLE_RATE) as usize;

    if std::env::args().any(|a| a == "--debug-state") {
        if let Some(reference) = reference.as_deref_mut() {
            for want in ["mix", "bypass", "decay", "size"] {
                if let Some(info) = reference
                    .params()
                    .into_iter()
                    .find(|p| p.name.eq_ignore_ascii_case(want))
                {
                    let v = reference.param_value(info.id).unwrap_or(f64::NAN);
                    let t = reference.value_to_text(info.id, v).unwrap_or_default();
                    println!(
                        "debug    : {:<8} id={} value={v:.4} text={t}",
                        info.name, info.id
                    );
                }
            }
            println!("debug    : latency {} frames", reference.latency());
        }
    }

    let reference_ir = match (&cached, reference) {
        // A cached render is already whatever length it was captured at.
        (Some(_), _) => probe_ir,
        (None, Some(plugin)) if frames > probe_frames => render_reference(plugin, frames),
        _ => probe_ir,
    };
    if cached.is_none() {
        if let Some(p) = &cache_path {
            match write_wav(p, &reference_ir) {
                Ok(()) => println!("cached   : {}", p.display()),
                Err(e) => eprintln!("could not cache the reference: {e}"),
            }
        }
    }
    let frames = reference_ir.len();
    println!("window   : {seconds:.1} s (longest reference band {longest:.2} s)");

    let mut params = valhalla::to_native_reverb_params(&patch);
    let mut native_ir = render_native(&params, frames);

    // `--tune`: close the loop. The translated `decay_time` is only an
    // estimate — it comes from the reference control's displayed time, which
    // ignores how `Size` scales the space. Measuring what the reference
    // actually did and asking for exactly that is strictly better, and is the
    // whole point of having a render bridge rather than a lookup table.
    if std::env::args().any(|a| a == "--tune") {
        // Target the GEOMETRIC MEAN of the reference's per-band decay, not its
        // broadband RT60.
        //
        // The bands and the length are not independent: whatever the length
        // loop settles on, the Decay Rate EQ has to lift or cut every band the
        // rest of the way. Aiming at the broadband figure leaves that work
        // lopsided — on a bass-heavy chamber (5.2 s at 62 Hz, sub-measurable
        // at 1 kHz) it drove the low bands to the 4.0x ceiling and the high
        // ones to the 0.1x floor at the same time, clamping at both ends and
        // adding so much gain the render clipped. Centring the length on the
        // bands leaves the multipliers spread around 1.0, which is the range
        // they are good at.
        // Measured once and reused for every comparison below — the
        // reference does not change, and band-splitting it is half the cost of
        // a comparison.
        let reference_band_table =
            decay::reverb_time_per_band(&reference_ir, SAMPLE_RATE, DecayFit::T20);
        let reference_bands: Vec<f64> = reference_band_table
            .iter()
            .filter_map(|(_, rt)| *rt)
            .collect();
        let band_target = (!reference_bands.is_empty()).then(|| {
            let log_sum: f64 = reference_bands.iter().map(|v| v.ln()).sum();
            (log_sum / reference_bands.len() as f64).exp()
        });
        // The reference's LONGEST band. Setting the length here makes every
        // band correction a cut, and cuts are the cheap direction: a boost has
        // to be scaled back to keep the feedback loop stable, so a curve that
        // needs large boosts clamps and stops improving. A bass-heavy chamber
        // (5.2 s at 62 Hz against a 3.4 s mean) pinned band 1 at the 4.0x
        // ceiling and band 8 at the 0.1x floor simultaneously, and added
        // enough gain on the way to clip the render.
        let longest_band = reference_bands
            .iter()
            .copied()
            .fold(None, |acc: Option<f64>, v| {
                Some(acc.map_or(v, |a| a.max(v)))
            });
        let overall = decay::reverb_time_best_effort(&reference_ir, SAMPLE_RATE, DecayFit::T20);
        // Try both length targets and keep whichever fits better. Neither wins
        // everywhere: the band mean rescued a bass-heavy chamber and three
        // short presets, and lost ground on a bright one whose bands the mean
        // pulled away from. Running both costs a second render pass and
        // removes the guess.
        // `(target, measured the same way)` — the length loop has to compare
        // like with like, or it chases a number it is not steering.
        let candidates: Vec<(f64, bool)> =
            [(band_target, true), (overall, false), (longest_band, true)]
                .into_iter()
                .filter_map(|(t, by_band)| t.map(|t| (t, by_band)))
                .fold(Vec::new(), |mut acc: Vec<(f64, bool)>, c| {
                    if !acc.iter().any(|(x, _)| (x - c.0).abs() < 1e-6) {
                        acc.push(c);
                    }
                    acc
                });
        match candidates.first().copied() {
            Some(_) => {
                // Two nested loops. The inner one sets the overall length:
                // asking the engine for N seconds gets close but not exact,
                // because the diffusers, output damping and shelf all sit
                // outside the feedback loop whose T60 we set. The outer one
                // sets the tail's *colour* — how the decay varies with
                // frequency — by reading the reference's own per-band ratios
                // and bending our Decay Rate EQ to match.
                //
                // Fitting from measurement rather than from the source
                // plugin's tone controls is deliberate: those controls
                // interact with the reverb's own damping, and their displayed
                // multipliers do not survive translation (see
                // `signal_import::valhalla`).
                const ROUNDS: usize = 24;
                // Stop as soon as the fit is good enough, and stop early if it
                // stalls. Without these the tuner ran every candidate to the
                // round limit whatever the result — hundreds of full-length
                // renders for a match it had already found in three.
                //
                // The target is a knob because it is a straight trade: a live
                // sweep hosting a plugin per preset wants to stop early, while
                // a re-tune from cached references is pure arithmetic and can
                // afford to converge properly. Reported alongside the result so
                // a preset's error is read against the bar it was tuned to.
                let good_enough: f64 = arg("--target-error")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0.10);
                const STALL_ROUNDS: usize = 3;
                const IMPROVEMENT: f64 = 0.02;
                const LEN_PASSES: usize = 4;
                const LEN_TOLERANCE: f64 = 0.02;

                // One Decay Rate EQ band per measured octave, so every band
                // can be corrected independently. A shelf at each end (they
                // must catch everything beyond the outermost centre) and
                // bells between.
                //
                // Fewer bands than octaves does not converge: with a single
                // shelf covering 62.5 and 125 Hz, an error in one could only
                // be fixed by also moving the other, and the fit stalled
                // around 0.25 with the shelves pinned at their limit.
                // `(octave this band answers for, filter corner, shape)`.
                //
                // The corner is NOT the octave for the two shelves. A shelf
                // reaches only half its gain at its own corner and the full
                // amount an octave or so beyond, so a high shelf sitting at
                // 8 kHz delivers half the cut it is asked for at 8 kHz — which
                // left a chamber's top octave ringing twice as long as the
                // reference with the shelf already most of the way to its
                // limit. Placing the corners inside the range hands the
                // outermost octaves the full shelf; the bells either side
                // absorb the overlap as the fit iterates.
                const BAND_PLAN: [(f64, f64, f64); 8] = [
                    (62.5, 125.0, 1.0), // low shelf, cornered an octave up
                    (125.0, 125.0, 0.0),
                    (250.0, 250.0, 0.0),
                    (500.0, 500.0, 0.0),
                    (1000.0, 1000.0, 0.0),
                    (2000.0, 2000.0, 0.0),
                    (4000.0, 4000.0, 0.0),
                    (8000.0, 4000.0, 2.0), // high shelf, cornered an octave down
                ];
                // Roughly one octave wide, so neighbouring bells overlap
                // little and the fit stays well-conditioned.
                const BAND_Q: f64 = 1.4;
                // Take a partial correction each round. A full step
                // overshoots — neighbouring bands overlap, so correcting each
                // one from its own ratio double-counts the shared region and
                // the fit oscillates instead of settling (a hall bounced
                // between 0.26 and 0.67 for twenty rounds without improving).
                //
                // An earlier attempt at damping made things worse, but that
                // was before the curve was normalized to unit geometric mean:
                // with the length still coupled to the shape, a smaller step
                // simply took longer to chase a moving target. Decoupled,
                // damping does what it should.
                const RELAXATION: f64 = 0.5;

                let mut best: Option<(f64, Vec<(String, f64)>, Vec<f32>)> = None;

                'candidates: for (target, by_band) in candidates.iter().copied() {
                    let mut rates = [1.0f64; BAND_PLAN.len()];
                    let mut request = target;
                    let mut stalled = 0usize;
                    let mut last_error = f64::INFINITY;
                    params.retain(|(n, _)| !n.starts_with("dband") && n != "decay_time");
                    for round in 1..=ROUNDS {
                        // --- length ---
                        for _ in 0..LEN_PASSES {
                            params.retain(|(n, _)| n != "decay_time");
                            params.push(("decay_time".to_string(), request));
                            native_ir = render_native(&params, frames);
                            let got = if by_band {
                                let ours = decay::reverb_time_per_band(
                                    &native_ir,
                                    SAMPLE_RATE,
                                    DecayFit::T20,
                                );
                                let measured: Vec<f64> =
                                    ours.iter().filter_map(|(_, rt)| *rt).collect();
                                if measured.is_empty() {
                                    decay::reverb_time_best_effort(
                                        &native_ir,
                                        SAMPLE_RATE,
                                        DecayFit::T20,
                                    )
                                } else {
                                    let log_sum: f64 = measured.iter().map(|v| v.ln()).sum();
                                    Some((log_sum / measured.len() as f64).exp())
                                }
                            } else {
                                decay::reverb_time_best_effort(
                                    &native_ir,
                                    SAMPLE_RATE,
                                    DecayFit::T20,
                                )
                            };
                            let Some(got) = got else {
                                break;
                            };
                            if (got - target).abs() / target <= LEN_TOLERANCE {
                                break;
                            }
                            // Bounded around the target. On a very short
                            // reference — a 0.17 s room, where only one octave is
                            // measurable at all — the ratio step is noisy enough
                            // to walk the request into the tens of seconds, which
                            // is not a correction, it is a runaway. Eight-fold either way
                            // leaves room for a genuine large correction.
                            request = (request * target / got)
                                .clamp(target * 0.125, target * 8.0)
                                .clamp(0.05, 60.0);
                        }

                        // --- colour ---
                        let cmp = decay::compare_decay_against(
                            &reference_band_table,
                            &native_ir,
                            SAMPLE_RATE,
                            DecayFit::T20,
                        );
                        let worst = cmp
                            .worst_ratio_error
                            .map(|e| format!("{e:.3}"))
                            .unwrap_or_else(|| "n/a".into());
                        println!(
                        "tuned    : target {target:.3} s  round {round}  length {request:.3} s  worst band error {worst}"
                    );
                        if let Some(e) = cmp.worst_ratio_error {
                            if best.as_ref().is_none_or(|(b, _, _)| e < *b) {
                                best = Some((e, params.clone(), native_ir.clone()));
                                stalled = 0;
                            } else if e > last_error - IMPROVEMENT {
                                stalled += 1;
                            }
                            last_error = e;
                            if e <= good_enough {
                                break 'candidates;
                            }
                            if stalled >= STALL_ROUNDS {
                                break;
                            }
                        }

                        // Our band rings long (ratio > 1) -> ask for less there.
                        // Averaged over the octaves each band is responsible for.
                        for (slot, (centre, _corner, _shape)) in BAND_PLAN.iter().enumerate() {
                            let responsible: Vec<f64> = cmp
                                .bands
                                .iter()
                                .filter(|b| (b.centre_hz - centre).abs() < 1.0)
                                .filter_map(|b| b.ratio)
                                .collect();
                            if responsible.is_empty() {
                                // Nothing to compare this octave against, so no
                                // correction is justified — and leaving whatever
                                // value it drifted to earlier is worse than
                                // neutral. A shelf stuck at 0.1x on an octave the
                                // reference cannot measure still bleeds into its
                                // neighbours, which is how a passing 4 kHz band
                                // got dragged under by an uncorrectable 8 kHz one.
                                rates[slot] = 1.0;
                                continue;
                            }
                            let mean = responsible.iter().sum::<f64>() / responsible.len() as f64;
                            if mean > 0.0 {
                                rates[slot] = (rates[slot] / mean.powf(RELAXATION)).clamp(0.1, 4.0);
                            }
                        }

                        // Normalize the curve to unit geometric mean, so the
                        // bands carry SHAPE only and `decay_time` carries the
                        // length. Without this the two fight: every band cut also
                        // shortens the whole tail, the length loop compensates by
                        // asking for more, and the pair walks off together — a
                        // 2.7 s reference ended up requesting 21.6 s with every
                        // band pinned near its floor, which is the same reverb
                        // described twice over.
                        let log_mean =
                            rates.iter().map(|r| r.ln()).sum::<f64>() / rates.len() as f64;
                        let norm = log_mean.exp();
                        if norm.is_finite() && norm > 0.0 {
                            for r in rates.iter_mut() {
                                *r = (*r / norm).clamp(0.1, 4.0);
                            }
                        }

                        params.retain(|(n, _)| !n.starts_with("dband"));
                        for (slot, (_centre, corner, shape)) in BAND_PLAN.iter().enumerate() {
                            if (rates[slot] - 1.0).abs() <= 0.01 {
                                continue; // a 1.0x band would spend a slot doing nothing
                            }
                            let n = slot + 1;
                            params.push((format!("dband{n}_shape"), *shape));
                            params.push((format!("dband{n}_freq"), *corner));
                            params.push((format!("dband{n}_rate"), rates[slot]));
                            params.push((format!("dband{n}_q"), BAND_Q));
                        }
                    }
                }

                match best {
                    Some((err, best_params, best_ir)) => {
                        params = best_params;
                        native_ir = best_ir;
                        let show = |k: &str| {
                            params
                                .iter()
                                .find(|(n, _)| n == k)
                                .map(|(_, v)| *v)
                                .unwrap_or(1.0)
                        };
                        let curve: Vec<String> = (1..=BAND_PLAN.len())
                            .map(|n| format!("{:.2}", show(&format!("dband{n}_rate"))))
                            .collect();
                        println!(
                            "tuned    : best — decay_time {:.3} s, decay curve [{}]x (band error {err:.3})",
                            show("decay_time"),
                            curve.join(" "),
                        );
                    }
                    None => println!(
                        "tuned    : no round produced a measurable band comparison; leaving the translation untouched"
                    ),
                }
            }
            None => println!("tuned    : skipped — the reference decay was not measurable"),
        }
    }

    // Report the raw decay of each side before judging the difference — a
    // reference that did not actually produce a tail (an unauthorized or
    // silent plugin) would otherwise look like a translation failure.
    for (label, ir) in [("reference", &reference_ir), ("ours", &native_ir)] {
        let rt = decay::reverb_time_best_effort(ir, SAMPLE_RATE, DecayFit::T20);
        let peak = signal_analyzer::null::peak_db(ir);
        println!("{label:9}: peak {peak:7.2} dBFS   RT60 {rt:?}");
    }

    // ── Level ──────────────────────────────────────────────────────────
    //
    // Decay and level are independent: the tuner above shapes the tail, and
    // whatever level that lands at is then a single number away from the
    // reference. Unlike decay this needs no search — a wet trim moves the
    // whole measurement by exactly its own value, so measuring the difference
    // once gives the answer directly.
    //
    // This is what `loudness_passed` was failing on across the whole library.
    // It was not a tuning failure: `NativeReverb` had no wet trim to tune, so
    // there was no parameter that could have made it pass.
    {
        let probe = compare(
            &reference_ir,
            &native_ir,
            &reference_ir,
            &native_ir,
            SAMPLE_RATE,
            Thresholds::reverb_match(),
        );
        let diff = probe.loudness.loudness_difference_lu;
        if diff.is_finite() {
            // Positive means we are louder, so the trim is its negative. The
            // existing trim is part of what was measured, hence += .
            let current = params
                .iter()
                .find(|(n, _)| n == "wet_gain")
                .map(|(_, v)| *v)
                .unwrap_or(0.0);
            let trim = (current - diff).clamp(-36.0, 36.0);
            if trim.abs() > 1e-6 {
                match params.iter_mut().find(|(n, _)| n == "wet_gain") {
                    Some((_, v)) => *v = trim,
                    None => params.push(("wet_gain".to_string(), trim)),
                }
                native_ir = render_native(&params, reference_ir.len());
                println!("levelled : wet_gain {trim:+.2} dB (was {diff:+.2} LU off)");
            }
        }
    }

    let result = compare(
        &reference_ir,
        &native_ir,
        &reference_ir,
        &native_ir,
        SAMPLE_RATE,
        Thresholds::reverb_match(),
    );

    println!("\nper-band decay (RT60 s):");
    for b in &result.decay.bands {
        match (b.reference_s, b.candidate_s, b.ratio) {
            (Some(r), Some(c), Some(ratio)) => println!(
                "  {:>7.0} Hz   ref {r:6.3}   ours {c:6.3}   ratio {ratio:5.2}",
                b.centre_hz
            ),
            _ => println!("  {:>7.0} Hz   not measurable", b.centre_hz),
        }
    }

    // Save the outcome. The tuned parameter set is not a by-product of this
    // run — it IS the translated preset, and re-deriving it costs a full
    // plugin-hosted tuning pass. Written alongside the measurements that
    // justify it, so a preset can be trusted or re-examined later.
    if let Some(dir) = arg("--save-dir") {
        let stem = Path::new(preset_path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "preset".into());
        let _ = std::fs::create_dir_all(&dir);
        let out = Path::new(&dir).join(format!("{stem}.json"));

        let bands: Vec<serde_json::Value> = result
            .decay
            .bands
            .iter()
            .map(|b| {
                serde_json::json!({
                    "centre_hz": b.centre_hz,
                    "reference_s": b.reference_s,
                    "ours_s": b.candidate_s,
                    "ratio": b.ratio,
                })
            })
            .collect();
        let tuned: Vec<serde_json::Value> = params
            .iter()
            .map(|(n, v)| serde_json::json!({ "name": n, "value": v }))
            .collect();

        let doc = serde_json::json!({
            "source": {
                "preset": stem,
                "file": preset_path,
                "plugin": format!("{:?}", patch.plugin),
                "plugin_version": patch.plugin_version,
                "mode": patch.mode_name(),
                "mode_value": patch.mode_value(),
            },
            "target": {
                "engine": "signal_fx::NativeReverb",
                "parameters": tuned,
            },
            "measurement": {
                "sample_rate": SAMPLE_RATE,
                "window_seconds": reference_ir.len() as f64 / SAMPLE_RATE,
                "decay_bands": bands,
                "worst_band_ratio_error": result.decay.worst_ratio_error,
                "loudness_difference_lu": result.loudness.loudness_difference_lu,
                "worst_band_level_difference_db": result.loudness.worst_band_difference_db,
                // Reported per criterion, not just overall. A preset can match
                // the reference's decay exactly and still fail the combined
                // verdict on level — our wet output currently runs hot — and a
                // bare `passed: false` next to a 0.08 decay error reads as a
                // failure the decay numbers plainly contradict.
                "decay_passed": result
                    .results
                    .iter()
                    .find(|r| r.criterion == signal_analyzer::Criterion::Decay)
                    .map(|r| r.passed),
                "loudness_passed": result
                    .results
                    .iter()
                    .find(|r| r.criterion == signal_analyzer::Criterion::Loudness)
                    .map(|r| r.passed),
                "all_criteria_passed": result.passed(),
            },
        });
        match serde_json::to_string_pretty(&doc)
            .map_err(|e| std::io::Error::other(e.to_string()))
            .and_then(|text| std::fs::write(&out, text))
        {
            Ok(()) => println!("saved    : {}", out.display()),
            Err(e) => eprintln!("could not save the result: {e}"),
        }
    }

    println!(
        "\nverdict: {}",
        if result.passed() { "MATCH" } else { "MISMATCH" }
    );
    for r in &result.results {
        println!(
            "  {:?}: {} (measured {:?}, threshold {})",
            r.criterion,
            if r.passed { "pass" } else { "FAIL" },
            r.measured.map(|m| (m * 1000.0).round() / 1000.0),
            r.threshold
        );
    }
}
