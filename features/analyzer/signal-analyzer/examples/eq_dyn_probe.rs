//! Measure one dynamic band's static curve against Pro-Q 4's.
//!
//! The library comparison drives broadband noise and reports one number per
//! preset, which is the right shape for "does this preset match" and the wrong
//! shape for "why doesn't it". A dynamic band is a *transfer characteristic* —
//! how much gain it applies at each input level — and that is what has to line
//! up. Measuring it directly says whether a mismatch is the threshold, the
//! range, the knee or the ratio, none of which a single broadband figure can
//! separate.
//!
//! Both sides get one band, configured identically, and a steady tone at the
//! band's own frequency swept in level. The output at that frequency against
//! the input at that frequency is the gain the band applied.
//!
//! ```sh
//! cargo run --release -p signal-analyzer --example eq_dyn_probe -- \
//!     --plugin ~/.vst3/yabridge/"FabFilter Pro-Q 4.vst3" \
//!     [--range -12] [--threshold -30] [--freq 1000] [--auto]
//! ```

use signal_fx::NativeEq;
use signal_plugin_host::{HostedPlugin, PluginEvents, PluginInstance};

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;
/// Long enough for the detector to settle at each level.
const SECONDS: f64 = 1.5;

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}
fn num(name: &str, default: f64) -> f64 {
    arg(name).and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Goertzel power at `freq`.
fn power_at(buf: &[f32], freq: f64) -> f64 {
    let w = std::f64::consts::TAU * freq / SR;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for &x in buf {
        let s0 = x as f64 + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (s1 * s1 + s2 * s2 - coeff * s1 * s2) / (buf.len() as f64).powi(2)
}

/// Phase of the Goertzel bin at `freq`, in degrees.
///
/// An All Pass is magnitude-flat by construction, so a magnitude sweep cannot
/// see it at all — it reports 0.00 everywhere whether the filter is right,
/// wrong, or absent. Phase is the only observable it has.
fn phase_at(buf: &[f32], freq: f64) -> f64 {
    let w = std::f64::consts::TAU * freq / SR;
    let (mut re, mut im) = (0.0f64, 0.0f64);
    for (i, &x) in buf.iter().enumerate() {
        let a = w * i as f64;
        re += x as f64 * a.cos();
        im -= x as f64 * a.sin();
    }
    im.atan2(re).to_degrees()
}

fn tone(freq: f64, amplitude: f64, frames: usize) -> Vec<f32> {
    let inc = std::f64::consts::TAU * freq / SR;
    (0..frames).map(|i| (amplitude * (inc * i as f64).sin()) as f32).collect()
}

/// Band-limited noise at a chosen RMS, for probing the detector's response to
/// crest factor rather than to a single frequency.
///
/// A tone and noise at the same RMS are the same level and a different signal.
/// Where the two stimuli disagree about a detector, what differs is its level
/// *estimator* — the peak/RMS blend and its window — not its curve.
fn noise(amplitude: f64, frames: usize) -> Vec<f32> {
    let mut s = 0xD1CE_0007u64;
    (0..frames)
        .map(|_| {
            s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let u = ((s >> 33) as f64 / (1u64 << 31) as f64) - 1.0;
            // Uniform noise has RMS = 1/sqrt(3); scale so `amplitude` is RMS.
            (amplitude * u * 3.0f64.sqrt()) as f32
        })
        .collect()
}

/// Pro-Q's normalized Q for a real Q.
fn proq_q(q: f64) -> f32 {
    ((q / 0.025).ln() / (40.0f64 / 0.025).ln()) as f32
}

/// Pro-Q's normalized threshold for a dB value — the inverse of the
/// three-segment curve measured from the plugin.
fn proq_threshold(db: f64) -> f32 {
    (if db <= -72.0 {
        (db + 90.0) / 180.0
    } else if db <= -48.0 {
        0.1 + (db + 72.0) / 240.0
    } else {
        db / 60.0 + 1.0
    }) as f32
}

/// One band of Pro-Q state, everything else unused.
#[allow(clippy::too_many_arguments)]
fn proq_state(
    freq: f64,
    q: f64,
    range_db: f64,
    threshold_db: f64,
    auto: bool,
    shape: f64,
    gain: f64,
    placement: f64,
    slope: f64,
) -> Vec<f32> {
    const STRIDE: usize = 23;
    let mut p = vec![0.0f32; 600];
    // Band 1.
    p[0] = 1.0; // Used
    p[1] = 1.0; // Enabled
    p[2] = freq.log2() as f32;
    p[3] = gain as f32;
    p[4] = proq_q(q);
    p[5] = shape as f32;
    p[6] = slope as f32; // slope index — 2 is 12 dB/oct, 10 is Brickwall
    p[7] = placement as f32;
    p[8] = 1.0; // All speakers except LFE
    p[9] = range_db as f32;
    p[10] = 1.0; // Dynamics enabled
    p[11] = if auto { 1.0 } else { 0.0 };
    p[12] = if auto { 1.0 } else { proq_threshold(threshold_db) };
    p[13] = 50.0; // Attack %
    p[14] = 50.0; // Release %
    // Instance-wide globals. Pro-Q 4 adds Character (0 Clean, 1 Subtle,
    // 2 Warm — "vintage non-linearities") and keeps Pro-Q 3's Processing Mode
    // (0 zero latency, 1 Natural Phase, 2 linear phase). Neither had ever been
    // measured here, and 26 and 29 of the 171 factory presets set them.
    p[552] = num("--mode", 0.0) as f32;
    p[554] = num("--character", 0.0) as f32;
    // Output Level is NOT stored in dB — see the mapping measured with this.
    p[556] = num("--outlevel", 0.0) as f32;
    p[555] = num("--gainscale", 1.0) as f32;
    // Every other band explicitly unused.
    for b in 1..24 {
        p[b * STRIDE] = 0.0;
    }
    // Globals: gain scale unity, nothing bypassed.
    p
}

fn render_plugin(plugin: &mut HostedPlugin, input: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(input.len());
    let mut pos = 0;
    while pos < input.len() {
        let n = BLOCK.min(input.len() - pos);
        let mut buf = vec![0.0f32; n * 2];
        for i in 0..n {
            buf[2 * i] = input[pos + i];
            buf[2 * i + 1] = input[pos + i];
        }
        if plugin.process_interleaved(&mut buf, &[], &[]).is_err() {
            return out;
        }
        out.extend((0..n).map(|i| buf[2 * i]));
        pos += n;
    }
    out
}

fn render_native(eq: &mut NativeEq, input: &[f32]) -> Vec<f32> {
    let events = PluginEvents::default();
    let mut out = Vec::with_capacity(input.len());
    let mut pos = 0;
    while pos < input.len() {
        let n = BLOCK.min(input.len() - pos);
        let l = &input[pos..pos + n];
        let (mut ol, mut or) = (vec![0.0f32; n], vec![0.0f32; n]);
        eq.process_block(l, l, &mut ol, &mut or, &events).expect("process");
        out.extend_from_slice(&ol);
        pos += n;
    }
    out
}

fn main() {
    let Some(plugin_path) = arg("--plugin") else {
        eprintln!("usage: eq_dyn_probe --plugin <path> [--range dB] [--threshold dB] [--freq Hz]");
        std::process::exit(2);
    };
    let freq = num("--freq", 1000.0);
    let q = num("--q", 1.0);
    let range = num("--range", -12.0);
    let threshold = num("--threshold", -30.0);
    let auto = std::env::args().any(|a| a == "--auto");
    let use_noise = std::env::args().any(|a| a == "--noise");

    let mut plugin = match HostedPlugin::load(&plugin_path) {
        Ok(Some(p)) => p,
        _ => {
            eprintln!("{plugin_path}: could not load");
            std::process::exit(1);
        }
    };
    plugin.prepare(SR, BLOCK as u32).expect("prepare");

    // Pro-Q's shape numbering: 0 Bell, 1 Low Shelf, 2 Low Cut, 3 High Shelf.
    let proq_shape = num("--shape", 0.0);
    // The engine's canonical order swaps 2 and 3.
    let native_shape = match proq_shape as i32 {
        2 => 3.0,
        3 => 2.0,
        other => other as f64,
    };
    let static_gain = num("--gain", 0.0);
    // Pro-Q placement: 0 Left, 1 Right, 2 Stereo, 3 Mid, 4 Side — verified
    // against the plugin. The engine's order is Stereo, Left, Right, Mid,
    // Side, so only the first three move.
    let proq_place = num("--placement", 2.0);
    let native_place = match proq_place as i32 {
        0 => 1.0,
        1 => 2.0,
        2 => 0.0,
        other => other as f64,
    };
    // Pro-Q's slope index: 0..9 are 0/6/12/18/24/30/36/48/72/96 dB per octave
    // and 10 is Brickwall — which is not "96 and a bit". Left at the default
    // every cut in this probe was measured at 12 dB/oct, which is why the four
    // shapes below had never been seen at their real steepness.
    let slope = num("--slope", 2.0);
    let floats = proq_state(
        freq, q, range, threshold, auto, proq_shape, static_gain, proq_place, slope,
    );
    let mut blob = plugin.save_state().expect("save_state");
    let count = u32::from_le_bytes(blob[16..20].try_into().unwrap()) as usize;
    for (i, v) in floats.iter().take(count).enumerate() {
        let at = 20 + i * 4;
        blob[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    plugin.load_state(&blob).expect("load_state");

    let mut ours = NativeEq::new(SR);
    for (n, v) in [
        ("b1_used", 1.0),
        ("b1_on", 1.0),
        ("b1_freq", freq),
        ("b1_gain", static_gain),
        ("b1_q", q),
        ("b1_shape", native_shape),
        ("b1_slope", slope),
        ("b1_placement", native_place),
        ("b1_dyn_range", range),
        ("b1_dyn_thr", threshold),
        ("b1_dyn_atk", 50.0),
        ("b1_dyn_rel", 50.0),
        ("b1_dyn_auto", if auto { 1.0 } else { 0.0 }),
    ] {
        ours.set_named(n, v);
    }
    ours.prepare(SR, BLOCK as u32).expect("prepare");

    println!(
        "one bell at {freq:.0} Hz, Q {q}, range {range:+} dB, threshold {}{}\n",
        if auto { "auto".to_string() } else { format!("{threshold:+} dB") },
        if auto { "" } else { "" }
    );
    println!(
        "  stimulus: {}",
        if use_noise { "band-limited noise" } else { "tone at the band" }
    );
    println!("  in dBFS     Pro-Q       ours       diff");

    // `--ballistics` steps the level and watches the gain move.
    //
    // A steady tone settles, so a level sweep says nothing about attack and
    // release — but programme material never settles, and the AVERAGE gain a
    // band applies to it is set by how fast it moves. That is why a band can
    // match a static curve exactly and still measure a decibel or two out on
    // noise.
    if std::env::args().any(|a| a == "--ballistics") {
        let quiet = 10.0f64.powf(-60.0 / 20.0);
        let loud = 10.0f64.powf(-6.0 / 20.0);
        // 0.3 s quiet, then 0.7 s loud, then 0.7 s quiet again.
        let seg = |amp: f64, secs: f64| tone(freq, amp, (SR * secs) as usize);
        let mut input = seg(quiet, 0.3);
        input.extend(seg(loud, 0.7));
        input.extend(seg(quiet, 0.7));

        let a = render_plugin(&mut plugin, &input);
        let b = render_native(&mut ours, &input);

        // Track the envelope of each in short windows.
        let win = (SR * 0.010) as usize;
        println!("  ms after step   Pro-Q       ours       diff   (attack)");
        let mut worst = 0.0f64;
        let mut report = |label: &str, base: f64, offsets: &[f64], worst: &mut f64| {
            println!("  --- {label} ---");
            for off in offsets {
                let start = ((base + off / 1000.0) * SR) as usize;
                if start + win >= a.len().min(b.len()) {
                    break;
                }
                let dry_amp = if base < 0.9 { loud } else { quiet };
                let dry = (dry_amp * dry_amp) * 0.25;
                let ra = 10.0 * (power_at(&a[start..start + win], freq) / dry).log10();
                let rb = 10.0 * (power_at(&b[start..start + win], freq) / dry).log10();
                let d = rb - ra;
                if d.abs() > *worst {
                    *worst = d.abs();
                }
                println!("  {off:>10.0}    {ra:>8.2}   {rb:>8.2}   {d:>8.2}");
            }
        };
        report("attack, from the step at 0.3 s", 0.3, &[1.0, 3.0, 10.0, 30.0, 100.0, 300.0, 600.0], &mut worst);
        report("release, from the step at 1.0 s", 1.0, &[1.0, 3.0, 10.0, 30.0, 100.0, 300.0, 600.0], &mut worst);
        println!("\nworst difference {worst:.2} dB");
        return;
    }

    // `--sweep` holds the level well past the threshold and sweeps FREQUENCY
    // instead. With the band pinned at full range its dynamics are settled and
    // what is left is the shape of the filter it applies — which is a
    // different question from where its curve sits, and the one that decides
    // whether a preset measures right on programme material that never lets it
    // off the cap.
    if std::env::args().any(|a| a == "--sweep") {
        let phase_mode = std::env::args().any(|a| a == "--phase");
        println!("  level held at -6 dBFS, band at full range\n");
        println!(
            "      Hz     Pro-Q       ours       diff   ({})",
            if phase_mode { "degrees" } else { "dB" }
        );
        let frames = (SR * SECONDS) as usize;
        let mut worst = 0.0f64;
        // A third of an octave steps straight over a brickwall transition —
        // the plugin goes from passband to -90 dB inside one step, so the
        // sweep reports two points and neither is on the slope. `--steps` is
        // steps per octave.
        let steps = num("--steps", 3.0).max(1.0);
        let lo = num("--sweep-lo", 62.5);
        let hi = num("--sweep-hi", 16_000.0);
        let mut probe = lo;
        while probe <= hi {
            let input = tone(probe, 10.0f64.powf(-6.0 / 20.0), frames);
            let cut = input.len() / 2;
            let a = render_plugin(&mut plugin, &input);
            let b = render_native(&mut ours, &input);
            if a.len() < input.len() || b.len() < input.len() {
                break;
            }
            let dry = power_at(&input[cut..], probe);
            let (r, o) = if phase_mode {
                let base = phase_at(&input[cut..], probe);
                let wrap = |d: f64| (d + 540.0).rem_euclid(360.0) - 180.0;
                (
                    wrap(phase_at(&a[cut..], probe) - base),
                    wrap(phase_at(&b[cut..], probe) - base),
                )
            } else {
                (
                    10.0 * (power_at(&a[cut..], probe) / dry).log10(),
                    10.0 * (power_at(&b[cut..], probe) / dry).log10(),
                )
            };
            let d = if phase_mode {
                (o - r + 540.0).rem_euclid(360.0) - 180.0
            } else {
                o - r
            };
            if d.abs() > worst {
                worst = d.abs();
            }
            println!("  {probe:>7.0}  {r:>9.2}  {o:>9.2}  {d:>9.2}");
            probe *= 2.0f64.powf(1.0 / steps);
        }
        println!(
            "\nworst difference {worst:.2} {}",
            if phase_mode { "degrees" } else { "dB" }
        );
        return;
    }

    let frames = (SR * SECONDS) as usize;
    let mut worst = 0.0f64;
    for level_db in [-60.0, -54.0, -48.0, -42.0, -36.0, -30.0, -24.0, -18.0, -12.0, -6.0, -3.0] {
        let amp = 10.0f64.powf(level_db / 20.0);
        let input = if use_noise {
            noise(amp, frames)
        } else {
            tone(freq, amp, frames)
        };
        // Measure the settled tail only.
        let cut = input.len() / 2;

        let a = render_plugin(&mut plugin, &input);
        let b = render_native(&mut ours, &input);
        if a.len() < input.len() || b.len() < input.len() {
            eprintln!("render fell short");
            break;
        }
        let dry = power_at(&input[cut..], freq);
        let ref_db = 10.0 * (power_at(&a[cut..], freq) / dry).log10();
        let our_db = 10.0 * (power_at(&b[cut..], freq) / dry).log10();
        let d = our_db - ref_db;
        if d.abs() > worst {
            worst = d.abs();
        }
        println!("  {level_db:>7.0}  {ref_db:>9.2}  {our_db:>9.2}  {d:>9.2}");
    }
    println!("\nworst difference {worst:.2} dB");
}
