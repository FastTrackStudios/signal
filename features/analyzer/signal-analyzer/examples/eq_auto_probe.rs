//! Does Pro-Q's auto threshold depend on what else is in the signal?
//!
//! A single-band probe cannot answer this. Measured alone, one dynamic band
//! matches the plugin to a few tenths of a decibel on both a tone and noise.
//! Measured inside a preset, the same band behaves differently: on
//! "Room 1 - Mud Removal" a -9 dB bell with a +9 range on Auto sits at its
//! base in the plugin and expands fully in ours, a 7 dB difference at 159 Hz —
//! while the identical band in isolation agrees with the plugin to 0.22 dB.
//!
//! Two explanations fit that, and they are separable:
//!
//! 1. **Programme content.** The threshold is derived from the whole signal,
//!    so energy far from the band changes whether the band triggers. Varying
//!    the *stimulus* while leaving the band alone tests this.
//! 2. **Band interaction.** The threshold, or the detector's feed, is affected
//!    by the other bands in the instance. Varying the *bands* while leaving
//!    the stimulus alone tests this.
//!
//! This runs both, one at a time, and reports the gain the band under test
//! applies at its own frequency in each case.

use signal_fx::NativeEq;
use signal_plugin_host::{HostedPlugin, PluginEvents, PluginInstance};

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;
const SECONDS: f64 = 2.0;
const STRIDE: usize = 23;

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter()
        .position(|x| x == name)
        .and_then(|i| a.get(i + 1).cloned())
}
fn num(name: &str, d: f64) -> f64 {
    arg(name).and_then(|v| v.parse().ok()).unwrap_or(d)
}

fn proq_q(q: f64) -> f32 {
    (q / 0.025).log(40.0f64 / 0.025) as f32
}

/// Power at `hz`, via Goertzel.
fn power_at(buf: &[f32], hz: f64) -> f64 {
    let w = std::f64::consts::TAU * hz / SR;
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for &x in buf {
        let s0 = f64::from(x) + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    (coeff * s1).mul_add(-s2, s1.mul_add(s1, s2 * s2)) / (buf.len() as f64).powi(2)
}

/// Noise at `rms`, plus any context partials.
fn stimulus(rms: f64, context: &[(f64, f64)], frames: usize) -> Vec<f32> {
    let mut s = 0x51DE_0042u64;
    let mut phase = vec![0.0f64; context.len()];
    (0..frames)
        .map(|_| {
            s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let u = ((s >> 33) as f64 / (1u64 << 31) as f64) - 1.0;
            let mut v = rms * u * 3.0f64.sqrt();
            for (k, (_, amp)) in context.iter().enumerate() {
                v += amp * phase[k].sin();
                phase[k] += std::f64::consts::TAU * context[k].0 / SR;
            }
            v as f32
        })
        .collect()
}

/// One band's slots in a Pro-Q state.
#[derive(Clone, Copy)]
struct Band {
    freq: f64,
    q: f64,
    gain: f64,
    range: f64,
    auto: bool,
    shape: f64,
}

fn write_band(p: &mut [f32], idx: usize, b: Band) {
    let o = idx * STRIDE;
    p[o] = 1.0; // Used
    p[o + 1] = 1.0; // Enabled
    p[o + 2] = b.freq.log2() as f32;
    p[o + 3] = b.gain as f32;
    p[o + 4] = proq_q(b.q);
    p[o + 5] = b.shape as f32;
    p[o + 6] = 2.0; // 12 dB/oct
    p[o + 7] = 2.0; // Stereo
    p[o + 8] = 1.0; // All speakers except LFE
    p[o + 9] = b.range as f32;
    p[o + 10] = 1.0; // Dynamics enabled
    p[o + 11] = if b.auto { 1.0 } else { 0.0 };
    p[o + 12] = 1.0; // Auto threshold position
    p[o + 13] = 50.0;
    p[o + 14] = 50.0;
}

fn set_native(eq: &mut NativeEq, idx: usize, b: Band) {
    let n = idx + 1;
    // The engine's shape order swaps Pro-Q's 2 and 3.
    let shape = match b.shape as i32 {
        2 => 3.0,
        3 => 2.0,
        other => f64::from(other),
    };
    for (name, v) in [
        ("used", 1.0),
        ("on", 1.0),
        ("freq", b.freq),
        ("gain", b.gain),
        ("q", b.q),
        ("shape", shape),
        ("slope", 2.0),
        ("dyn_range", b.range),
        ("dyn_atk", 50.0),
        ("dyn_rel", 50.0),
        ("dyn_auto", if b.auto { 1.0 } else { 0.0 }),
    ] {
        eq.set_named(&format!("b{n}_{name}"), v);
    }
}

fn render_plugin(p: &mut HostedPlugin, input: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(input.len());
    let mut pos = 0;
    while pos < input.len() {
        let n = BLOCK.min(input.len() - pos);
        let mut buf = vec![0.0f32; n * 2];
        for i in 0..n {
            buf[2 * i] = input[pos + i];
            buf[2 * i + 1] = input[pos + i];
        }
        if p.process_interleaved(&mut buf, &[], &[]).is_err() {
            return out;
        }
        out.extend((0..n).map(|i| buf[2 * i]));
        pos += n;
    }
    out
}

fn render_native(eq: &mut NativeEq, input: &[f32]) -> Vec<f32> {
    let ev = PluginEvents::default();
    let mut out = Vec::with_capacity(input.len());
    let mut pos = 0;
    while pos < input.len() {
        let n = BLOCK.min(input.len() - pos);
        let l = &input[pos..pos + n];
        let (mut ol, mut or) = (vec![0.0f32; n], vec![0.0f32; n]);
        eq.process_block(l, l, &mut ol, &mut or, &ev)
            .expect("process");
        out.extend_from_slice(&ol);
        pos += n;
    }
    out
}

/// Load `bands` into the plugin and into ours, then report the gain applied at
/// `probe_hz`.
fn measure(
    plugin: &mut HostedPlugin,
    bands: &[Band],
    context: &[(f64, f64)],
    rms: f64,
    probe_hz: f64,
) -> (f64, f64) {
    let mut blob = plugin.save_state().expect("save_state");
    let count = u32::from_le_bytes(blob[16..20].try_into().unwrap()) as usize;
    let mut p = vec![0.0f32; count];
    for (i, b) in bands.iter().enumerate() {
        write_band(&mut p, i, *b);
    }
    if count > 555 {
        p[555] = 1.0; // Gain Scale
    }
    for (i, v) in p.iter().enumerate() {
        let at = 20 + i * 4;
        blob[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    plugin.load_state(&blob).expect("load_state");

    let mut ours = NativeEq::new(SR);
    for (i, b) in bands.iter().enumerate() {
        set_native(&mut ours, i, *b);
    }
    ours.prepare(SR, BLOCK as u32).expect("prepare");

    let frames = (SR * SECONDS) as usize;
    let input = stimulus(rms, context, frames);
    let cut = input.len() / 2;
    let a = render_plugin(plugin, &input);
    let b = render_native(&mut ours, &input);
    if a.len() < input.len() || b.len() < input.len() {
        return (f64::NAN, f64::NAN);
    }
    let dry = power_at(&input[cut..], probe_hz);
    (
        10.0 * (power_at(&a[cut..], probe_hz) / dry).log10(),
        10.0 * (power_at(&b[cut..], probe_hz) / dry).log10(),
    )
}

/// Render `seconds` of the same stimulus and throw it away.
///
/// If the plugin's Auto adapts to the programme, every measurement is a
/// measurement of its history as much as of its settings — so the history has
/// to be made explicit before anything is read off it.
// Not called by the probe as it currently runs, but it documents how the
// analyser's history has to be primed before a reading means anything.
#[expect(dead_code)]
fn settle(plugin: &mut HostedPlugin, ours: &mut NativeEq, input: &[f32], seconds: f64) {
    let want = (SR * seconds) as usize;
    let mut done = 0;
    while done < want {
        let take = input.len().min(want - done);
        let _ = render_plugin(plugin, &input[..take]);
        let _ = render_native(ours, &input[..take]);
        done += take;
    }
}

fn main() {
    let Some(path) = arg("--plugin") else {
        eprintln!("usage: eq_auto_probe --plugin <path>");
        std::process::exit(2);
    };
    let rms = 10.0f64.powf(num("--level", -18.8) / 20.0);

    let mut plugin = if let Ok(Some(p)) = HostedPlugin::load(&path) { p } else {
        eprintln!("{path}: could not load");
        std::process::exit(1);
    };
    plugin.prepare(SR, BLOCK as u32).expect("prepare");

    // The band under test, taken from "Room 1 - Mud Removal": a cut that
    // expands back to flat when its region is loud.
    let probe = Band {
        freq: 130.0,
        q: 1.26,
        gain: -9.0,
        range: 9.0,
        auto: true,
        shape: 0.0,
    };

    println!(
        "band under test: bell 130 Hz Q 1.26, {:+} dB, range {:+}, auto",
        probe.gain, probe.range
    );
    println!("noise at {:.1} dBFS RMS\n", 20.0 * rms.log10());

    // ── 0. Is Auto time-varying? ────────────────────────────────────────
    //
    // Two runs of the identical configuration disagreed by 7 dB depending on
    // what had been rendered before them, which is not something a static
    // threshold can do. This measures the same band repeatedly on unchanging
    // material: a settled threshold gives the same answer every time, an
    // adapting one walks.
    {
        println!("── 0. does Auto settle, or keep moving? ──");
        println!("  one band, unchanging noise, read once a second from a cold start.");
        println!("  Swept over three ranges and three levels, because a rate fitted to a");
        println!("  single trajectory has been wrong every previous time — see the plan's");
        println!("  \"fit across the range, never to one point\".\n");

        // The whole trajectory, so a fit can be done offline rather than by
        // eye. Emitted as JSON when `--json` is given.
        let mut runs: Vec<serde_json::Value> = Vec::new();

        for range in [4.5f64, 9.0, 18.0] {
            for level_db in [-30.0f64, -18.8, -9.0] {
                // Base sits at -range so full engagement is exactly 0 dB and
                // the trajectory reads as "fraction of travel" directly.
                let b = Band {
                    gain: -range,
                    range,
                    ..probe
                };
                let level = 10.0f64.powf(level_db / 20.0);

                let mut blob = plugin.save_state().expect("save_state");
                let count = u32::from_le_bytes(blob[16..20].try_into().unwrap()) as usize;
                let mut p = vec![0.0f32; count];
                write_band(&mut p, 0, b);
                if count > 555 {
                    p[555] = 1.0;
                }
                for (i, v) in p.iter().enumerate() {
                    let at = 20 + i * 4;
                    blob[at..at + 4].copy_from_slice(&v.to_le_bytes());
                }
                plugin.load_state(&blob).expect("load_state");

                // A fresh instance each run: the point is the cold-start
                // trajectory, and a reused one carries the last run's history.
                let mut ours = NativeEq::new(SR);
                set_native(&mut ours, 0, b);
                ours.prepare(SR, BLOCK as u32).expect("prepare");

                println!(
                    "  range {range:+.1} dB, noise at {level_db:.1} dBFS RMS  (0 dB = fully engaged)"
                );
                println!(
                    "  {:<10} {:>9} {:>9} {:>9}",
                    "elapsed", "Pro-Q", "ours", "diff"
                );

                let chunk = stimulus(level, &[], (SR * 1.0) as usize);
                let cut = chunk.len() * 3 / 4;
                let dry = power_at(&chunk[cut..], b.freq);
                let (mut proq, mut mine) = (Vec::new(), Vec::new());
                // Twelve seconds is enough at -18.8 dBFS and above. At -30 it
                // is not — the plugin was still walking when the window ran
                // out, so the whole point of the probe (a settled reading) was
                // lost. `--seconds` buys the low-level runs the time they need.
                let seconds = num("--seconds", 12.0) as usize;
                for second in 1..=seconds {
                    let a = render_plugin(&mut plugin, &chunk);
                    let o = render_native(&mut ours, &chunk);
                    if a.len() < chunk.len() || o.len() < chunk.len() {
                        break;
                    }
                    // Read the last quarter of each second.
                    let ra = 10.0 * (power_at(&a[cut..], b.freq) / dry).log10();
                    let ro = 10.0 * (power_at(&o[cut..], b.freq) / dry).log10();
                    println!(
                        "  {:<10} {ra:>9.2} {ro:>9.2} {:>9.2}",
                        format!("{second} s"),
                        ro - ra
                    );
                    proq.push(ra);
                    mine.push(ro);
                }
                let worst = proq
                    .iter()
                    .zip(mine.iter())
                    .map(|(a, o)| (o - a).abs())
                    .fold(0.0f64, f64::max);
                println!("  worst over the trajectory: {worst:.2} dB\n");
                runs.push(serde_json::json!({
                    "range_db": range,
                    "level_dbfs": level_db,
                    "freq_hz": b.freq,
                    "q": b.q,
                    "proq_db": proq,
                    "ours_db": mine,
                    "worst_db": worst,
                }));
            }
        }

        if let Some(out) = arg("--json") {
            let _ = std::fs::write(
                &out,
                serde_json::to_string_pretty(&serde_json::json!({ "trajectories": runs })).unwrap(),
            );
            println!("  wrote {out}\n");
        }
        if std::env::args().any(|a| a == "--traj") {
            return;
        }
    }

    println!("── 1. does other PROGRAMME content change it? ──");
    println!(
        "  {:<34} {:>9} {:>9} {:>9}",
        "context", "Pro-Q", "ours", "diff"
    );
    for (label, ctx) in [
        ("noise only", &[][..]),
        ("+ 5 kHz tone at -6 dBFS", &[(5000.0, 0.5)][..]),
        ("+ 50 Hz tone at -6 dBFS", &[(50.0, 0.5)][..]),
        (
            "+ tones at 50/400/5k",
            &[(50.0, 0.3), (400.0, 0.3), (5000.0, 0.3)][..],
        ),
    ] {
        let (a, b) = measure(&mut plugin, &[probe], ctx, rms, probe.freq);
        println!("  {label:<34} {a:>9.2} {b:>9.2} {:>9.2}", b - a);
    }

    println!("\n── 1b. how does that dependence vary with the content? ──");
    println!(
        "  {:<34} {:>9} {:>9} {:>9}",
        "context tone", "Pro-Q", "ours", "diff"
    );
    for hz in [50.0f64, 130.0, 300.0, 1000.0, 5000.0, 15000.0] {
        let (a, b) = measure(&mut plugin, &[probe], &[(hz, 0.5)], rms, probe.freq);
        println!(
            "  {:<34} {a:>9.2} {b:>9.2} {:>9.2}",
            format!("{hz:.0} Hz at -6 dBFS"),
            b - a
        );
    }
    for amp_db in [-30.0f64, -24.0, -18.0, -12.0, -6.0] {
        let amp = 10.0f64.powf(amp_db / 20.0);
        let (a, b) = measure(&mut plugin, &[probe], &[(5000.0, amp)], rms, probe.freq);
        println!(
            "  {:<34} {a:>9.2} {b:>9.2} {:>9.2}",
            format!("5 kHz at {amp_db:.0} dBFS"),
            b - a
        );
    }

    println!("\n── 1c. is the detector frequency-weighted? ──");
    println!("  Same noise, same Q, same range — only the band's frequency moves.");
    println!("  In-band noise energy rises with frequency at constant Q, so an");
    println!("  unweighted detector should expand MORE the higher it sits.");
    println!(
        "  {:<34} {:>9} {:>9} {:>9}",
        "band frequency", "Pro-Q", "ours", "diff"
    );
    for hz in [60.0f64, 130.0, 300.0, 1000.0, 3000.0, 8000.0] {
        let b = Band { freq: hz, ..probe };
        let (a, o) = measure(&mut plugin, &[b], &[], rms, hz);
        println!(
            "  {:<34} {a:>9.2} {o:>9.2} {:>9.2}",
            format!("{hz:.0} Hz"),
            o - a
        );
    }

    println!("\n── 2. do other BANDS change it? ──");
    println!(
        "  {:<34} {:>9} {:>9} {:>9}",
        "instance", "Pro-Q", "ours", "diff"
    );
    let wide_cut = Band {
        freq: 400.0,
        q: 0.5,
        gain: -4.0,
        range: 4.0,
        auto: true,
        shape: 0.0,
    };
    let hi_shelf = Band {
        freq: 6000.0,
        q: 0.3,
        gain: 5.0,
        range: -9.0,
        auto: true,
        shape: 1.0,
    };
    let static_boost = Band {
        freq: 2000.0,
        q: 1.0,
        gain: 6.0,
        range: 0.0,
        auto: false,
        shape: 0.0,
    };
    for (label, bands) in [
        ("the band alone", vec![probe]),
        ("+ a static +6 dB bell at 2 kHz", vec![probe, static_boost]),
        ("+ a second dynamic bell", vec![probe, wide_cut]),
        ("+ the rest of the preset", vec![probe, wide_cut, hi_shelf]),
    ] {
        let (a, b) = measure(&mut plugin, &bands, &[], rms, probe.freq);
        println!("  {label:<34} {a:>9.2} {b:>9.2} {:>9.2}", b - a);
    }
}
