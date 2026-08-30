//! Measure a translated Pro-Q 4 preset against the plugin it came from.
//!
//! The reverb library was measured; the EQ library was not. It was generated
//! on the assumption that an EQ translation is a parameter mapping rather than
//! a fit — every field has a counterpart, so nothing is being approximated and
//! there is no error to report. That assumption is worth exactly as much as
//! the evidence behind it, and it had none: the first two things this harness
//! was built to check both turned out to be wrong (Q was crossing as Pro-Q's
//! normalized storage instead of as a Q, and the Auto threshold sentinel was
//! crossing as 0 dB).
//!
//! # How
//!
//! Both sides get the same white noise, and the comparison is the transfer
//! function — the ratio of output to input spectrum, averaged over many
//! frames and read in dB across log-spaced bands. That measures what an EQ
//! actually is, which a sample-by-sample null cannot: the two use different
//! filter topologies and will never be sample-identical even when they sound
//! the same, while a magnitude response either matches or does not.
//!
//! Noise rather than a sweep because the dynamic bands need a steady level to
//! settle against; a sweep would put a different level in every band.
//!
//! # Stereo
//!
//! The stimulus is **decorrelated stereo**: independent noise in the mid and
//! the side, so both halves of the image carry full-level programme. Fed mono
//! — which is what this did until it was noticed — Mid is the signal and Side
//! is silence, so a Side band is inert and a Mid band is indistinguishable
//! from a Stereo one. Both then "verify", and neither is tested. Twelve of the
//! 171 factory presets came back bit-identical to their input under mono
//! noise, which is not a pass; it is a measurement of nothing.
//!
//! The transfer function is taken separately for the mid and the side sum, and
//! the reported error spans both. `--mono` restores the old single-channel
//! stimulus for comparison against readings taken before this.
//!
//! ```sh
//! cargo run --release -p signal-analyzer --example eq_match -- \
//!     --plugin ~/.vst3/yabridge/"FabFilter Pro-Q 4.vst3" \
//!     --preset "/path/to/Vocals/Bright Vocal.ffp"
//! ```

use realfft::RealFftPlanner;
use signal_fx::NativeEq;
use signal_plugin_host::{HostedPlugin, PluginEvents, PluginInstance};

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;
/// FFT size for the transfer-function estimate.
const FFT: usize = 8192;
/// Frames averaged. The dynamic bands settle over the first few, so the
/// estimate is taken from well past the start.
const FRAMES: usize = 24;
/// Frames rendered and discarded before anything is measured.
///
/// Pro-Q's auto-threshold bands **adapt over about seven seconds** — measured
/// by feeding one band unchanging noise and reading it every second, the
/// applied gain walked from -1.54 dB to -0.41 and only then held. Eight frames
/// is 0.7 s, so every reading taken with that warmup was of a plugin still
/// moving, and the same configuration measured twice in one run disagreed by
/// 7 dB depending on what had been rendered before it.
///
/// 95 hops is a little over eight seconds. It makes the sweep several times
/// slower and is the difference between measuring the plugin and measuring its
/// transient.
const WARMUP_FRAMES: usize = 95;

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

/// Deterministic white noise — a fixed spectrum across runs keeps the
/// comparison stable, and both sides must hear the identical signal.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        ((self.0 >> 33) as f64 / (1u64 << 31) as f64) - 1.0
    }
}

/// Left and right of the stimulus.
///
/// `mid` and `side` are independent, so the sum and the difference of the two
/// channels are both full-level noise and a band on either one has something
/// to work on.
fn stimulus(frames: usize, amplitude: f64) -> (Vec<f32>, Vec<f32>) {
    let mono = std::env::args().any(|a| a == "--mono");
    let mut rng = Lcg(0xC0FF_EE01);
    let mut side_rng = Lcg(0x5EED_1D0F);
    // `--tonal` adds resonances to the noise bed.
    //
    // Flat noise is a degenerate input for anything that reacts to spectrum:
    // a resonance suppressor has nothing to suppress, and what little it does
    // is driven by the random bin-to-bin fluctuation of the noise itself,
    // which two engines will never agree on in detail. Programme material has
    // peaks; a measurement meant to predict how a preset behaves on it should
    // too.
    let tonal = std::env::args().any(|a| a == "--tonal");
    let partials: [f64; 6] = [110.0, 275.0, 700.0, 1650.0, 3900.0, 9200.0];
    // The side carries the SAME resonances, a third of a turn out of phase.
    //
    // Put them in the mid alone and the side's spectrum at those frequencies
    // is noise 36 dB down inside the band being read, so the side transfer
    // function there measures whatever asymmetry leaks out of either engine
    // rather than anything either engine is doing: on "Kick - IN 01" every
    // band read 10 to 14 dB out at 100 Hz, including a bell at 12 kHz. Give
    // them different frequencies instead and each component is starved where
    // the other one is fed, which only moves the problem.
    let mut phase = [0.0f64; 6];
    let mut side_phase = [2.1f64; 6];
    // Normalise so `--tonal` sits at the SAME level as flat noise. Six
    // partials at 1.6x the noise amplitude raise the RMS by 13.8 dB, which put
    // the tonal sweep at -5 dBFS with peaks near full scale — so it was
    // measuring how each engine behaves when driven hard as much as it was
    // measuring the spectrum, and Character's saturation with it. One variable
    // per probe: the tonal run differs from the flat one in spectrum only.
    let scale = if tonal {
        // variance = a^2/3 (noise) + 6 * (1.6 a)^2 / 2 (partials)
        (1.0f64 / 3.0) / (1.0 / 3.0 + 6.0 * 1.6 * 1.6 / 2.0)
    } else {
        1.0
    }
    .sqrt();
    let amplitude = amplitude * scale;
    let (mut left, mut right) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
    for _ in 0..frames {
        // Both halves carry resonances, on different frequencies.
        let mut mid = amplitude * rng.next();
        if tonal {
            for (k, f) in partials.iter().enumerate() {
                mid += amplitude * 1.6 * phase[k].sin();
                phase[k] += std::f64::consts::TAU * f / SR;
            }
        }
        let mut side = if mono { 0.0 } else { amplitude * side_rng.next() };
        if tonal && !mono {
            for (k, f) in partials.iter().enumerate() {
                side += amplitude * 1.6 * side_phase[k].sin();
                side_phase[k] += std::f64::consts::TAU * f / SR;
            }
        }
        left.push((mid + side) as f32);
        right.push((mid - side) as f32);
    }
    (left, right)
}

/// Mid and side of a channel pair.
fn to_ms(left: &[f32], right: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let mid = left.iter().zip(right).map(|(l, r)| 0.5 * (l + r)).collect();
    let side = left.iter().zip(right).map(|(l, r)| 0.5 * (l - r)).collect();
    (mid, side)
}

/// Average magnitude spectrum of `buf`, in linear units, skipping the warmup.
fn spectrum(buf: &[f32]) -> Vec<f64> {
    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(FFT);
    let mut mag = vec![0.0f64; FFT / 2 + 1];
    let mut used = 0usize;

    // Hann, so leakage does not smear a narrow notch across neighbours.
    let window: Vec<f64> = (0..FFT)
        .map(|i| {
            let x = std::f64::consts::TAU * i as f64 / FFT as f64;
            0.5 - 0.5 * x.cos()
        })
        .collect();

    let start = WARMUP_FRAMES * FFT / 2;
    let mut pos = start;
    while pos + FFT <= buf.len() && used < FRAMES {
        let mut frame: Vec<f64> = (0..FFT).map(|i| buf[pos + i] as f64 * window[i]).collect();
        let mut out = fft.make_output_vec();
        fft.process(&mut frame, &mut out).expect("fft");
        for (m, c) in mag.iter_mut().zip(out.iter()) {
            *m += c.norm();
        }
        used += 1;
        pos += FFT / 2; // 50% overlap
    }
    let n = used.max(1) as f64;
    mag.iter_mut().for_each(|m| *m /= n);
    mag
}

/// Render through the hosted plugin.
fn render_reference(plugin: &mut HostedPlugin, left: &[f32], right: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let (mut ol, mut or) = (Vec::with_capacity(left.len()), Vec::with_capacity(left.len()));
    let mut pos = 0;
    while pos < left.len() {
        let n = BLOCK.min(left.len() - pos);
        let mut buf = vec![0.0f32; n * 2];
        for i in 0..n {
            buf[2 * i] = left[pos + i];
            buf[2 * i + 1] = right[pos + i];
        }
        if plugin.process_interleaved(&mut buf, &[], &[]).is_err() {
            eprintln!("process failed");
            std::process::exit(1);
        }
        ol.extend((0..n).map(|i| buf[2 * i]));
        or.extend((0..n).map(|i| buf[2 * i + 1]));
        pos += n;
    }
    (ol, or)
}

/// Render through our own EQ with the translated parameters.
fn render_native(params: &[(String, f64)], left: &[f32], right: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let mut eq = NativeEq::new(SR);
    for (name, value) in params {
        eq.set_named(name, *value);
    }
    eq.prepare(SR, BLOCK as u32).expect("prepare");
    let show_ag = std::env::args().any(|a| a == "--auto-gain");
    if show_ag {
        print!("  static curve:");
        let mut hz = 25.0f64;
        while hz < 22_000.0 {
            print!(" {:.0}:{:+.1}", hz, eq.static_magnitude_db(hz));
            hz *= 2.0;
        }
        println!();
    }
    let events = PluginEvents::default();
    let (mut outl, mut outr) = (Vec::with_capacity(left.len()), Vec::with_capacity(left.len()));
    let mut pos = 0;
    while pos < left.len() {
        let n = BLOCK.min(left.len() - pos);
        let (mut ol, mut or) = (vec![0.0f32; n], vec![0.0f32; n]);
        eq.process_block(&left[pos..pos + n], &right[pos..pos + n], &mut ol, &mut or, &events)
            .expect("process");
        outl.extend_from_slice(&ol);
        outr.extend_from_slice(&or);
        pos += n;
    }
    if show_ag {
        println!("auto gain compensation: {:+.2} dB", eq.auto_gain_db());
    }
    (outl, outr)
}

/// Log-spaced comparison bands: third-octave from 25 Hz to 16 kHz.
fn band_centres() -> Vec<f64> {
    let mut f = 25.0f64;
    let mut out = Vec::new();
    while f <= 16_000.0 {
        out.push(f);
        f *= 2.0f64.powf(1.0 / 3.0);
    }
    out
}

/// Transfer function in dB at each band centre, averaged over the band.
fn response_db(input: &[f64], output: &[f64], centres: &[f64]) -> Vec<f64> {
    let bin_hz = SR / FFT as f64;
    centres
        .iter()
        .map(|&c| {
            let lo = c / 2.0f64.powf(1.0 / 6.0);
            let hi = c * 2.0f64.powf(1.0 / 6.0);
            let (mut num, mut den) = (0.0f64, 0.0f64);
            for i in 0..input.len() {
                let f = i as f64 * bin_hz;
                if f >= lo && f <= hi {
                    num += output[i] * output[i];
                    den += input[i] * input[i];
                }
            }
            if den <= 1e-30 {
                return 0.0;
            }
            10.0 * (num / den).log10()
        })
        .collect()
}

fn main() {
    let (Some(plugin_path), Some(preset_path)) = (arg("--plugin"), arg("--preset")) else {
        eprintln!("usage: eq_match --plugin <path> --preset <file.ffp> [--json <out>]");
        std::process::exit(2);
    };

    let mut plugin = match HostedPlugin::load(&plugin_path) {
        Ok(Some(p)) => p,
        _ => {
            eprintln!("{plugin_path}: could not load");
            std::process::exit(1);
        }
    };
    plugin.prepare(SR, BLOCK as u32).expect("prepare");

    // ── Translate ──────────────────────────────────────────────────────
    let bytes = std::fs::read(&preset_path).expect("read preset");
    let older = bytes.len() > 4 && matches!(&bytes[..4], b"FQ2p" | b"FQ3p");
    let floats: Vec<f32> = if older {
        // A Pro-Q 2 or 3 preset has a different, shorter band record (334
        // floats against 576), and converting it here would mean guessing at
        // FabFilter's own mapping — any difference would then read as a DSP
        // error rather than an import one. The idea was to let the plugin do
        // the conversion and read the modernised state back.
        //
        // **It does not work.** Pro-Q 4 reads .ffp files through its own
        // loader, not through the VST3 state interface, and `load_state`
        // refuses an FQ3p blob outright. The 87 Pro-Q 3 and 27 Pro-Q 2 presets
        // in the library are therefore out of reach until someone writes the
        // conversion, and what that would measure is the conversion. Left here
        // so the attempt is on the record and fails loudly.
        if plugin.load_state(&bytes).is_err() {
            eprintln!("{preset_path}: the plugin would not load this preset");
            std::process::exit(1);
        }
        let blob = plugin.save_state().expect("save_state");
        let count = u32::from_le_bytes(blob[16..20].try_into().unwrap()) as usize;
        (0..count)
            .map(|i| f32::from_le_bytes(blob[20 + i * 4..24 + i * 4].try_into().unwrap()))
            .collect()
    } else if signal_import::fabfilter::parser::is_text_format(&bytes) {
        let text = String::from_utf8_lossy(&bytes);
        signal_import::fabfilter::parser::parse_ffp_text(&text)
            .expect("parse")
            .parameters
            .iter()
            .map(|(_, v)| *v as f32)
            .collect()
    } else {
        signal_import::fabfilter::ffbs::parse(&bytes).expect("parse").params
    };
    let state = signal_import::fabfilter::ffbs::FfbsState {
        version: 1,
        params: floats.clone(),
        metadata: Default::default(),
    };
    let eq = signal_import::fabfilter::proq4::decode(&state).expect("decode");
    let mut params = signal_import::fabfilter::proq4::to_native_eq_params(&eq);

    // `--only-band N` keeps the Nth *active* band and silences the rest, on
    // both sides. A preset's error is almost never spread evenly across its
    // bands, and the fastest way to find which one carries it is to listen to
    // them one at a time.
    // A comma-separated list, so a suspected interaction between two bands
    // can be reproduced with just those two.
    let only: Option<Vec<usize>> = arg("--only-band").map(|v| {
        v.split(',').filter_map(|t| t.trim().parse::<usize>().ok()).collect()
    });
    let mut floats = floats;
    if let Some(ns) = only.filter(|v| !v.is_empty()) {
        let active: Vec<usize> = eq
            .bands
            .iter()
            .enumerate()
            .filter(|(_, b)| b.is_active())
            .map(|(i, _)| i)
            .collect();
        let keep: Vec<usize> = ns
            .iter()
            .filter_map(|n| active.get(n.saturating_sub(1)).copied())
            .collect();
        if keep.is_empty() {
            eprintln!("--only-band: the preset has {} active bands", active.len());
            std::process::exit(2);
        }
        const STRIDE: usize = 23;
        for &i in &active {
            if !keep.contains(&i) {
                // Both flags: `Used` alone is the UI's idea of an occupied
                // slot and does not stop the plugin processing the band.
                for field in [0usize, 1] {
                    if let Some(slot) = floats.get_mut(i * STRIDE + field) {
                        *slot = 0.0; // Used, Enabled
                    }
                }
            }
        }
        for (name, value) in params.iter_mut() {
            if let Some(rest) = name.strip_prefix('b') {
                if let Some((idx, field)) = rest.split_once('_') {
                    if field == "used"
                        && !idx.parse::<usize>().is_ok_and(|i| ns.contains(&i))
                    {
                        *value = 0.0;
                    }
                }
            }
        }
        println!("only bands {ns:?} of {} active", active.len());
        if std::env::args().any(|a| a == "--dump-params") {
            for (name, value) in &params {
                if *value != 0.0 || name.ends_with("_used") {
                    println!("    {name} = {value}");
                }
            }
        }
    }

    // ── Reference ──────────────────────────────────────────────────────
    // Splice the preset into the plugin's own state container — Pro-Q refuses
    // host writes to several band parameters, so this is the only way to put a
    // preset in front of it.
    let mut blob = plugin.save_state().expect("save_state");
    // A straight copy is correct across both preset vintages: the 24 bands and
    // the 24 globals sit at identical offsets in each, and newer builds only
    // append a per-band `Spectral Tilt` block past them. A shorter preset
    // therefore leaves that tail at the plugin's own defaults, which is what
    // it should be.
    let count = u32::from_le_bytes(blob[16..20].try_into().unwrap()) as usize;
    // `--default-globals` leaves the plugin's own instance-wide settings in
    // place and splices only the band records. If a preset's error is a flat
    // offset, this says in one run whether it comes from a band or from a
    // global.
    let band_floats = if std::env::args().any(|a| a == "--default-globals") {
        signal_import::fabfilter::proq4::GLOBALS_OFFSET
    } else {
        floats.len()
    };
    for (i, v) in floats.iter().take(count.min(band_floats)).enumerate() {
        let at = 20 + i * 4;
        blob[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    // `--global i=v` writes one float outright, for measuring what a global
    // the translation does not carry actually does.
    {
        let argv: Vec<String> = std::env::args().collect();
        for (i, a) in argv.iter().enumerate() {
            if a != "--global" {
                continue;
            }
            let Some(spec) = argv.get(i + 1) else { continue };
            let Some((idx, v)) = spec.split_once('=') else { continue };
            if let (Ok(idx), Ok(v)) = (idx.trim().parse::<usize>(), v.trim().parse::<f32>()) {
                if idx < count {
                    let at = 20 + idx * 4;
                    blob[at..at + 4].copy_from_slice(&v.to_le_bytes());
                }
            }
        }
    }
    // `--only-global i` adds back exactly one instance-wide float on top of
    // `--default-globals`, which is how the one that matters gets found.
    if let Some(i) = arg("--only-global").and_then(|v| v.parse::<usize>().ok()) {
        if let Some(v) = floats.get(i) {
            if i < count {
                let at = 20 + i * 4;
                blob[at..at + 4].copy_from_slice(&v.to_le_bytes());
            }
        }
    }
    plugin.load_state(&blob).expect("load_state");

    // ── Measure ────────────────────────────────────────────────────────
    let frames = (WARMUP_FRAMES + FRAMES + 2) * FFT / 2 + FFT;
    // `--level` in dBFS RMS of each of mid and side. The default is what the
    // library has always been measured at; a second level is the cheapest way
    // to find a preset whose agreement is an accident of one loudness.
    let level_db = arg("--level").and_then(|v| v.parse::<f64>().ok()).unwrap_or(-18.8);
    let (in_l, in_r) = stimulus(frames, 10.0f64.powf(level_db / 20.0) * 3.0f64.sqrt());
    let (ref_l, ref_r) = render_reference(&mut plugin, &in_l, &in_r);
    let (our_l, our_r) = render_native(&params, &in_l, &in_r);

    let rms = |b: &[f32]| {
        (b.iter().map(|s| (*s as f64) * (*s as f64)).sum::<f64>() / b.len() as f64).sqrt()
    };
    println!(
        "levels: input {:.6}  reference {:.6}  ours {:.6}",
        rms(&in_l),
        rms(&ref_l),
        rms(&our_l)
    );
    let identical = ref_l.iter().zip(in_l.iter()).all(|(a, b)| (a - b).abs() < 1e-9)
        && ref_r.iter().zip(in_r.iter()).all(|(a, b)| (a - b).abs() < 1e-9);
    if identical {
        println!(
            "WARNING: the reference is bit-identical to the input — the plugin is not processing"
        );
    }

    let centres = band_centres();
    let stem = std::path::Path::new(&preset_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    // Mid and side are measured separately. Summed to mono first, a Side band
    // cancels out of the measurement entirely — which is how 22 M/S presets
    // came to be "verified" without anything having looked at them.
    let (mid_in, side_in) = to_ms(&in_l, &in_r);
    let (mid_ref, side_ref) = to_ms(&ref_l, &ref_r);
    let (mid_our, side_our) = to_ms(&our_l, &our_r);

    let mut rows: Vec<(&str, f64, f64, f64)> = Vec::new(); // component, hz, ref, ours
    let mut worst = 0.0f64;
    let mut worst_hz = 0.0f64;
    let mut worst_where = "mid";
    let mut sum = 0.0f64;
    let mut n = 0usize;

    println!("{stem}\n  where       Hz      Pro-Q       ours       diff");
    for (label, input, reference, ours) in [
        ("mid", &mid_in, &mid_ref, &mid_our),
        ("side", &side_in, &side_ref, &side_our),
    ] {
        // A silent component carries no information — under `--mono` the side
        // is exactly that, and averaging its zeros in would halve every
        // number for no reason.
        if rms(input) < 1.0e-6 {
            continue;
        }
        let spec_in = spectrum(input);
        let r_db = response_db(&spec_in, &spectrum(reference), &centres);
        let o_db = response_db(&spec_in, &spectrum(ours), &centres);
        for ((c, r), o) in centres.iter().zip(r_db.iter()).zip(o_db.iter()) {
            let d = (o - r).abs();
            sum += d;
            n += 1;
            if d > worst {
                worst = d;
                worst_hz = *c;
                worst_where = label;
            }
            println!("  {label:<5} {c:>7.0}  {r:>8.2}  {o:>9.2}  {:>9.2}", o - r);
            rows.push((label, *c, *r, *o));
        }
    }
    let mean = sum / n.max(1) as f64;
    println!("\nworst {worst:.2} dB at {worst_hz:.0} Hz ({worst_where})   mean {mean:.2} dB");

    if let Some(out) = arg("--json") {
        let doc = serde_json::json!({
            "preset": stem,
            "bands": rows.iter()
                .map(|(w, c, r, o)| serde_json::json!({
                    "component": w, "hz": c, "reference_db": r, "ours_db": o,
                    "difference_db": o - r,
                }))
                .collect::<Vec<_>>(),
            "worst_difference_db": worst,
            "worst_hz": worst_hz,
            "worst_component": worst_where,
            "mean_difference_db": mean,
        });
        let _ = std::fs::write(out, serde_json::to_string_pretty(&doc).unwrap());
    }
}
