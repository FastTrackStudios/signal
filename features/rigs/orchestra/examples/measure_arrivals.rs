//! Offline per-zone ARRIVAL measurement — re-derive every zone's heard-arrival
//! marker from the actual sample audio, and write it into the pack inventory
//! (`arrival_ms` per zone in the zones styx).
//!
//! Why: pack metadata can lie. The CSS 1st Violins pack's `lead_in_ms` values
//! came from library metadata that the audio contradicts (legato joins landed
//! −82..+91 ms off-grid acoustically while the engine's scheduling was
//! sample-exact); its transition zones carry no loop points to cross-check
//! against. The only ground truth is the recording itself, so this tool
//! measures it — deterministically (same audio → same styx, byte-identical)
//! and idempotently (re-running rewrites the same values).
//!
//! What "arrival" means per zone class (matching the acoustic cross-check in
//! `tests/legato_arrival.rs`, so the verification loop closes):
//!
//! * **legato transition** (`interval > 0`: Leg / NVLeg / Port) — the
//!   destination-pitch settle: the first sustained crossing of the
//!   destination's harmonic-energy share past 50% ([`pitch_share_curve`]).
//!   The source and destination pitches are known from the zone identity
//!   (`root_key` is the LOWER pitch; `direction` says which end is the
//!   source).
//! * **short / one-shot** (Staccato / Spiccato / Pizzicato / …) — the
//!   rhythmic peak: the strongest spectral-flux peak (parabolic-refined).
//!   Per round-robin, this replaces the single global
//!   `short_note_timing.pre_delay_ms`.
//! * **re-trigger** (Legzero — `Legato` kind, `interval == 0`) and **fresh
//!   sustain / trill** — the perceptual onset: the spectral-flux leading edge
//!   (first crossing of 25% of the window peak), where the note starts
//!   speaking. A bowed swell's flux PEAK sits deep in the bloom; the edge is
//!   what the ear locks to against a click.
//! * **release zones** — skipped (they fire at note-off; grid arrival is
//!   meaningless).
//!
//! Robustness: every measurement is graded. Low-confidence measurements
//! (no pitch crossing, unsettled destination, flat/ambiguous flux) are NOT
//! written to the styx — they are listed in the report with the reason, and
//! the engine keeps its fallback (`lead_in_ms` / `pre_delay_ms` /
//! trigger-time) for those zones.
//!
//! Usage (any library that ships a config + zones styx pair):
//!
//! ```text
//! cargo run --release -p signal-orchestra --example measure_arrivals -- \
//!     [--config <engine-config.styx>] [--zones <library.styx>] \
//!     [--root <samples-root>] [--write] [--report <path>] [--threads N]
//! ```
//!
//! Defaults target CSS 1st Violins (`CSS_ROOT` / `CSS_CONFIG`). Without
//! `--write` it is a dry run: measure + report only.

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use signal_orchestra::timing::{pitch_share_curve, spectral_flux};
use signal_orchestra::{CSS_CONFIG, CSS_ROOT};
use signal_sampler::spec::{ArticulationKind, LibrarySpec, ZoneSpec};
use signal_sampler::PlayerPatch;

/// Longest plausible arrival (ms) — anything later is a measurement error
/// (mis-tracked pitch, room-noise flux), not a marker.
const MAX_ARRIVAL_MS: f64 = 1200.0;
/// Analysis window (sec) from sample start. CSS transition swells top out
/// well under a second; shorts/attacks speak in the first ~200 ms.
const SCAN_SEC: f64 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Transition,
    Retrigger,
    Short,
    SustainEdge,
    Skip,
}

#[derive(Debug, Clone)]
struct Measurement {
    file: String,
    class: Class,
    arrival_ms: Option<f64>,
    /// `None` = confident; `Some(reason)` = flagged, not written.
    flag: Option<String>,
}

fn classify(spec: &LibrarySpec, z: &ZoneSpec) -> Class {
    // Non-attack triggers (release/pedal/cc/aftertouch) have no grid arrival.
    if !(z.trigger_mode.is_empty() || z.trigger_mode.eq_ignore_ascii_case("attack")) {
        return Class::Skip;
    }
    let kind = spec
        .articulation(&z.articulation)
        .map(|a| a.kind.clone())
        .unwrap_or(ArticulationKind::Sustain);
    match kind {
        ArticulationKind::Release => Class::Skip,
        ArticulationKind::Legato if z.interval > 0 => Class::Transition,
        ArticulationKind::Legato => Class::Retrigger,
        ArticulationKind::Short | ArticulationKind::OneShot => Class::Short,
        _ => Class::SustainEdge,
    }
}

/// Decode a WAV into interleaved stereo f32 (mono duplicated), plus its rate.
fn load_wav(path: &Path) -> Result<(Vec<f32>, u32), String> {
    let mut r = hound::WavReader::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let spec = r.spec();
    let ch = spec.channels.max(1) as usize;
    let norm = |v: i32, bits: u16| v as f32 / (1i64 << (bits - 1)) as f32;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
        hound::SampleFormat::Int => r
            .samples::<i32>()
            .map(|s| norm(s.unwrap_or(0), spec.bits_per_sample))
            .collect(),
    };
    let frames = samples.len() / ch;
    let mut out = Vec::with_capacity(frames * 2);
    for f in 0..frames {
        let l = samples[f * ch];
        let rr = if ch >= 2 { samples[f * ch + 1] } else { l };
        out.push(l);
        out.push(rr);
    }
    Ok((out, spec.sample_rate))
}

/// Destination-pitch settle of a transition sample: first sustained crossing
/// of the destination share past 50%, graded on the shape of the curve.
fn measure_settle(audio: &[f32], sr: u32, from: u8, to: u8) -> (Option<f64>, Option<String>) {
    let dur = audio.len() as f64 / 2.0 / f64::from(sr);
    let t1 = SCAN_SEC.min(dur - 0.05);
    if t1 <= 0.1 {
        return (None, Some("sample too short".into()));
    }
    let curve = pitch_share_curve(audio, sr, from, to, 0.02, t1);
    if curve.v.len() < 8 {
        return (None, Some("curve too short".into()));
    }
    // Smooth the share (~40 ms box): vibrato and — on octaves/fifths, where
    // harmonic collisions leave the detector its fundamentals only — source
    // leak-through both wobble the raw curve hard across 0.5. A wobble
    // excursion is NOT the arrival; the SETTLE is.
    let win = ((0.06 / curve.hop_sec) as usize).max(1);
    let sm: Vec<f64> = (0..curve.v.len())
        .map(|i| {
            let lo = i.saturating_sub(win / 2);
            let hi = (i + win / 2 + 1).min(curve.v.len());
            curve.v[lo..hi].iter().map(|&v| f64::from(v)).sum::<f64>() / (hi - lo) as f64
        })
        .collect();
    // Validated first crossing: the arrival is the first RAW sustained
    // (3-hop) crossing of 0.5 whose following 300 ms holds — raw mean
    // ≥ 0.55 and no more than 110 ms cumulative below 0.5 (smoothed).
    // The raw crossing keeps clean zones EXACT (any
    // smoothing of the crossing itself biases a convex rise late — over-
    // measuring the marker and over-skipping the sample at spawn); the
    // smoothed floor rejects the failure modes:
    //  * octave/fifth leak-through EXCURSIONS (source harmonics in the
    //    destination bins) briefly cross 0.5 then collapse to ~0.3 — the
    //    floor fails, and the search advances to the next crossing (the
    //    real settle). Unguarded, these measured octaves absurdly early,
    //    got flagged, and the engine fell back to garbage `lead_in`
    //    metadata (the CSS octave zones claim up to 900 ms);
    //  * VIBRATO on an arrived destination dips the share below 0.5 every
    //    cycle — dips to 0.40 are tolerated, so the genuine first crossing
    //    is kept (a strict stay-above-0.5 plateau pushed those markers
    //    ~100 ms late).
    let hold_n = ((0.30 / curve.hop_sec) as usize).max(1);
    let mut settle: Option<usize> = None;
    for i in 1..curve.v.len().saturating_sub(2) {
        // v3 (owner ear-calibrated on ff_up_F3_2 375 ms / ff_up_C#3_2
        // 425 ms): perception locks onto the FIRST STRONG crossing — the
        // vibrato wobble AFTER it is "the note, with vibrato", not
        // "not arrived yet". So a single hop ≥ 0.60 is a candidate even
        // when the next hop dips (the old 3-hop gate pushed these
        // markers 60–85 ms past perception; with full-glide prefire that
        // error plays destination content BEFORE the beat).
        let raw_cross = curve.v[i - 1] < 0.5
            && (curve.v[i] >= 0.6
                || (curve.v[i] >= 0.5 && curve.v[i + 1] >= 0.5 && curve.v[i + 2] >= 0.5));
        // A slightly weaker single-hop crossing (0.55) also qualifies when
        // the immediate 120 ms mean stays destination-side — vibrato can
        // shave the first crossing's peak under 0.6 at hop resolution.
        let weak_n = ((0.12 / curve.hop_sec) as usize).max(1);
        let weak_hi = (i + weak_n).min(curve.v.len());
        let weak_mean = curve.v[i..weak_hi]
            .iter()
            .map(|&v| f64::from(v))
            .sum::<f64>()
            / (weak_hi - i) as f64;
        let raw_cross =
            raw_cross || (curve.v[i - 1] < 0.5 && curve.v[i] >= 0.55 && weak_mean >= 0.5);
        if !raw_cross {
            continue;
        }
        let hi = (i + hold_n).min(sm.len());
        let mean = curve.v[i..hi].iter().map(|&v| f64::from(v)).sum::<f64>() / (hi - i) as f64;
        // Reject only SUSTAINED relapses: cumulative time below 0.5 in the
        // hold window. Two failure modes had to be separated:
        //  * FIFTHS null narrowly — dest h2/h4 collide with source h3/h6,
        //    so the detector keeps only dest h1/h3, and a vibrato sweep can
        //    null those bins for a few tens of ms right after a REAL
        //    arrival (ff_up_G3_7: share hits 0.68, dips to 0.06 for ~50 ms,
        //    then settles; the ear hears the D throughout — marking the
        //    post-dip settle played the note ~90 ms early). A single narrow
        //    null accumulates well under the budget → accepted.
        //  * OCTAVE leak-through EXCURSIONS relapse repeatedly — the source
        //    keeps feeding the destination bins, and the share spends
        //    ~200 ms+ under 0.5 across the window → rejected, search
        //    advances to the true settle.
        let below_sec = sm[i..hi].iter().filter(|&&v| v < 0.5).count() as f64 * curve.hop_sec;
        // OCTAVES get a stricter grade: their leak-through is structural
        // (every destination harmonic coincides with a source harmonic, so
        // the pruned detector runs on mutually-leaking fundamentals) and a
        // leak excursion can look as good as a fifth's genuine
        // narrow-nulled arrival (G3_12's excursion: mean 0.64 / 88 ms below
        // vs G3_7's real arrival: 0.74 / 69 ms). A true octave settle is
        // near-total (share ≈ 1.0) — demand it.
        let (mean_min, below_max) = if from.abs_diff(to) >= 12 {
            (0.70, 0.05)
        } else {
            // v3: vibrato-heavy arrivals hold a hold-window mean just above
            // 0.5 (C#3_2 first crossing: 0.54) — the old 0.55 gate rejected
            // the crossing perception locks onto.
            (0.50, 0.15)
        };
        if std::env::var_os("ARRIVAL_DEBUG").is_some() {
            eprintln!(
                "  candidate {:.0} ms: mean {mean:.2}, below-0.5 {:.0} ms -> {}",
                (curve.t0 + i as f64 * curve.hop_sec) * 1000.0,
                below_sec * 1000.0,
                if mean < mean_min || below_sec > below_max {
                    "reject"
                } else {
                    "ACCEPT"
                }
            );
        }
        if mean < mean_min || below_sec > below_max {
            continue;
        }
        settle = Some(i);
        break;
    }
    let Some(idx) = settle else {
        let head = sm.iter().take(8).sum::<f64>() / 8.0;
        return if head >= 0.5 {
            (None, Some("destination already dominant at start".into()))
        } else {
            (None, Some("no destination-pitch settle".into()))
        };
    };
    // Interpolated 0.5 crossing on the raw curve.
    let frac = f64::from(
        ((0.5 - curve.v[idx - 1]) / (curve.v[idx] - curve.v[idx - 1]).max(1e-9)).clamp(0.0, 1.0),
    );
    let t = curve.t0 + ((idx - 1) as f64 + frac) * curve.hop_sec;
    let ms = t * 1000.0;
    if ms > MAX_ARRIVAL_MS {
        return (
            Some(ms),
            Some(format!("settle {ms:.0} ms implausibly late")),
        );
    }
    // Grade: the source must genuinely own the head of the sample (a curve
    // that starts near the plateau is an ambiguous take).
    let pre_n = idx.min(((0.06 / curve.hop_sec) as usize).max(1));
    let pre_mean = sm[..idx].iter().rev().take(pre_n).sum::<f64>() / pre_n as f64;
    if pre_mean > 0.65 {
        return (
            Some(ms),
            Some(format!(
                "source not dominant before settle (pre share {pre_mean:.2})"
            )),
        );
    }
    (Some(ms), None)
}

/// Flux-derived onset of an attack sample. `peak == true` → rhythmic peak
/// (shorts); else leading edge at 25% of the window peak (sustains/re-bows).
fn measure_onset(
    audio: &[f32],
    sr: u32,
    peak: bool,
    window_sec: f64,
) -> (Option<f64>, Option<String>) {
    let dur = audio.len() as f64 / 2.0 / f64::from(sr);
    // The analysis window bounds where the arrival can live — a longer
    // window lets a LATER musical event (a second swell, a bow-off) own the
    // flux peak / drag the 25% edge threshold.
    let t1 = window_sec.min(dur);
    let mut flux = spectral_flux(audio, sr);
    if flux.v.len() < 2 {
        return (None, Some("sample too short for flux".into()));
    }
    // Flux frame 0 diffs against an all-zero previous spectrum, so a sample
    // whose audio starts at frame 0 gets a huge ARTIFICIAL peak there —
    // zero it so thresholds and peaks come from real spectral change. (A
    // hot-started sample's true onset is the start; the following frames
    // still carry the settling energy and register.)
    flux.v[0] = 0.0;
    let hi = (((t1 - flux.t0) / flux.hop_sec).max(1.0) as usize).min(flux.v.len());
    let window = &flux.v[..hi];
    let peak_v = window.iter().cloned().fold(f32::MIN, f32::max);
    if peak_v <= 0.0 {
        return (None, Some("flat flux (silence?)".into()));
    }
    let t = if peak {
        flux.onset_near(t1 / 2.0, t1 / 2.0)
    } else {
        // Two-pass leading edge. Pass 1 scans the whole window for a rough
        // onset; pass 2 re-derives the edge inside the SAME local window
        // the acoustic cross-check uses around a note's expected arrival
        // ([−0.10 s, +0.25 s] — `FluxCurve::leading_edge` in
        // `tests/legato_arrival.rs`), so the 25%-of-peak threshold is
        // computed over the same neighbourhood in both places. A whole-file
        // window would let a later, bigger swell raise the threshold and
        // push the marker late — which then reads as the note speaking
        // BEFORE the click in the render.
        flux.leading_edge(t1 / 2.0, t1 / 2.0, t1 / 2.0)
            .and_then(|rough| flux.leading_edge(rough, 0.10, 0.25))
    };
    let Some(t) = t else {
        return (None, Some("no flux onset found".into()));
    };
    let ms = (t * 1000.0).max(0.0);
    if ms > MAX_ARRIVAL_MS {
        return (Some(ms), Some(format!("onset {ms:.0} ms implausibly late")));
    }
    if peak {
        // A short's rhythmic peak must stand clear of the window's median
        // activity — a verby room mic can smear the attack into the tail.
        let mut sorted: Vec<f32> = window.to_vec();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let median = sorted[sorted.len() / 2];
        if median > 0.0 && peak_v / median < 2.0 {
            return (
                Some(ms),
                Some(format!(
                    "flux peak not prominent (peak/median {:.1})",
                    peak_v / median
                )),
            );
        }
    }
    (Some(ms), None)
}

fn measure_zone(spec: &LibrarySpec, z: &ZoneSpec, path: &Path) -> Measurement {
    let class = classify(spec, z);
    if class == Class::Skip {
        return Measurement {
            file: z.file.clone(),
            class,
            arrival_ms: None,
            flag: None,
        };
    }
    let (audio, sr) = match load_wav(path) {
        Ok(x) => x,
        Err(e) => {
            return Measurement {
                file: z.file.clone(),
                class,
                arrival_ms: None,
                flag: Some(format!("decode failed: {e}")),
            };
        }
    };
    let (arrival, flag) = match class {
        Class::SustainEdge => {
            // Measure the onset THROUGH the library's authored attack ramp
            // (`performance.attack_ms` — CSS's "arco attack"): the engine
            // applies this envelope to every fresh sustain voice, so the
            // heard onset in a render is the sample's own bloom shaped by
            // the ramp. Measuring the raw file instead leaves a constant
            // late bias of tens of ms. Same ramp law as
            // `Voice::next_frame`: `gain += (1 − gain) / frames_left`.
            let attack_ms = spec.performance.attack_ms.unwrap_or(0);
            let mut shaped = audio.clone();
            if attack_ms > 0 {
                let ramp = (f64::from(attack_ms) / 1000.0 * f64::from(sr)) as usize;
                let mut gain = 0.0f32;
                for f in 0..ramp.min(shaped.len() / 2) {
                    gain += (1.0 - gain) / (ramp - f) as f32;
                    shaped[f * 2] *= gain;
                    shaped[f * 2 + 1] *= gain;
                }
            }
            measure_onset(&shaped, sr, false, 0.9)
        }
        Class::Transition => {
            // Zone identity → recorded pitches: `root_key` is the LOWER
            // pitch; `up` = root → root+interval, `down` = the reverse.
            let iv = z.interval.min(12) as u8;
            let (from, to) = if z.direction.eq_ignore_ascii_case("up") {
                (z.root_key, z.root_key.saturating_add(iv))
            } else {
                (z.root_key.saturating_add(iv), z.root_key)
            };
            measure_settle(&audio, sr, from, to)
        }
        Class::Short => measure_onset(&audio, sr, true, SCAN_SEC),
        // Re-trigger (Legzero) samples RECORD the previous note's tail
        // before the bow change — a leading edge finds the tail's hot start
        // (~10 ms), not the re-attack, and every re-bow then lands its
        // accent ~100..180 ms late. The re-bow IS a short-like articulation
        // event inside continuous sound: its arrival is the flux PEAK,
        // searched only over the recorded tail + bow change (the first
        // ~0.6 s — beyond that the peak is the note's own later swell).
        Class::Retrigger => measure_onset(&audio, sr, true, 0.6),
        Class::Skip => unreachable!(),
    };
    Measurement {
        file: z.file.clone(),
        class,
        arrival_ms: arrival,
        flag,
    }
}

/// Rewrite the zones styx: set/replace `arrival_ms` on measured zones and
/// REMOVE it from zones whose measurement is now flagged (idempotent —
/// re-running produces byte-identical output). Zone blocks are matched by
/// their `file` value; formatting of everything else is preserved verbatim.
fn rewrite_zones_styx(text: &str, arrivals: &BTreeMap<String, f64>) -> String {
    let mut out: Vec<String> = Vec::with_capacity(text.lines().count() + arrivals.len());
    let mut block: Vec<String> = Vec::new();
    let mut in_block = false;
    for line in text.lines() {
        if !in_block {
            if line.trim_end() == "    {" {
                in_block = true;
                block.clear();
                block.push(line.to_string());
            } else {
                out.push(line.to_string());
            }
            continue;
        }
        if line.trim_end() == "    }" {
            block.push(line.to_string());
            // Extract this block's file value.
            let file = block.iter().find_map(|l| {
                let t = l.trim_start();
                t.strip_prefix("file")
                    .map(|rest| rest.trim().trim_matches('"').to_string())
            });
            // Drop any existing arrival line, then insert the new one (if
            // measured) right after `lead_in_ms` when present, else before
            // the closing brace.
            block.retain(|l| !l.trim_start().starts_with("arrival_ms"));
            if let Some(ms) = file.as_deref().and_then(|f| arrivals.get(f)) {
                let arrival_line = format!("        arrival_ms   {ms:.1}");
                let after_lead = block
                    .iter()
                    .position(|l| l.trim_start().starts_with("lead_in_ms"));
                match after_lead {
                    Some(i) => block.insert(i + 1, arrival_line),
                    None => {
                        let close = block.len() - 1;
                        block.insert(close, arrival_line);
                    }
                }
            }
            out.append(&mut block);
            in_block = false;
            continue;
        }
        block.push(line.to_string());
    }
    // Trailing (unterminated) block content, if any — pass through.
    out.append(&mut block);
    let mut s = out.join("\n");
    if text.ends_with('\n') {
        s.push('\n');
    }
    s
}

fn main() -> Result<(), String> {
    let mut config = PathBuf::from(CSS_CONFIG);
    let mut zones: Option<PathBuf> = None;
    let mut root = PathBuf::from(CSS_ROOT);
    let mut write = false;
    let mut report_path: Option<PathBuf> = None;
    let mut threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8)
        .min(16);
    let mut inspect: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--config" => config = PathBuf::from(val()?),
            "--zones" => zones = Some(PathBuf::from(val()?)),
            "--root" => root = PathBuf::from(val()?),
            "--write" => write = true,
            "--report" => report_path = Some(PathBuf::from(val()?)),
            "--threads" => threads = val()?.parse().map_err(|e| format!("--threads: {e}"))?,
            "--inspect" => inspect = Some(val()?),
            other => return Err(format!("unknown arg {other}")),
        }
    }
    let zones_path = zones.unwrap_or_else(|| root.join("_patches/1st Violins/library.styx"));

    let patch =
        PlayerPatch::load_merged(&config, &zones_path, &root).map_err(|e| format!("load: {e}"))?;
    let spec = &patch.spec;
    eprintln!(
        "measuring {} zones from {}",
        spec.zones.len(),
        zones_path.display()
    );

    // ── Inspect mode: dump one zone's analysis curves and exit ───────────
    if let Some(pat) = inspect {
        for (zi, z) in spec.zones.iter().enumerate() {
            if !z.file.contains(&pat) {
                continue;
            }
            let class = classify(spec, z);
            let (audio, sr) = load_wav(&patch.zone_paths[zi])?;
            eprintln!(
                "── {} [{class:?}] root={} interval={} dir={} lead_in={} arrival={}",
                z.file, z.root_key, z.interval, z.direction, z.lead_in_ms, z.arrival_ms
            );
            if class == Class::Transition {
                let iv = z.interval.min(12) as u8;
                let (from, to) = if z.direction.eq_ignore_ascii_case("up") {
                    (z.root_key, z.root_key.saturating_add(iv))
                } else {
                    (z.root_key.saturating_add(iv), z.root_key)
                };
                let dur = audio.len() as f64 / 2.0 / f64::from(sr);
                let curve = pitch_share_curve(&audio, sr, from, to, 0.02, SCAN_SEC.min(dur - 0.05));
                for (i, v) in curve.v.iter().enumerate().step_by(8) {
                    let t = curve.t0 + i as f64 * curve.hop_sec;
                    let bar = "#".repeat((v * 50.0) as usize);
                    eprintln!("  {:6.0} ms  {v:.2} {bar}", t * 1000.0);
                }
                let (ms, flag) = measure_settle(&audio, sr, from, to);
                eprintln!("  measured: {ms:?} flag {flag:?}");
            } else {
                let mut flux = spectral_flux(&audio, sr);
                if !flux.v.is_empty() {
                    flux.v[0] = 0.0;
                }
                let n = flux.v.len().min((1.0 / flux.hop_sec) as usize);
                let peak = flux.v[..n].iter().cloned().fold(1e-9f32, f32::max);
                for (i, v) in flux.v[..n].iter().enumerate().step_by(8) {
                    let t = flux.t0 + i as f64 * flux.hop_sec;
                    let bar = "#".repeat((v / peak * 50.0) as usize);
                    eprintln!("  {:6.0} ms  {bar}", t * 1000.0);
                }
                let (pk, f1) = measure_onset(
                    &audio,
                    sr,
                    true,
                    if class == Class::Retrigger {
                        0.6
                    } else {
                        SCAN_SEC
                    },
                );
                let (edge, f2) = measure_onset(&audio, sr, false, 0.9);
                eprintln!("  flux-peak {pk:?} ({f1:?}) / flux-edge {edge:?} ({f2:?})");
            }
        }
        return Ok(());
    }

    // Measure every zone (unique files measured once; multi-zone files share).
    let jobs: Vec<usize> = (0..spec.zones.len()).collect();
    let results: Mutex<Vec<Measurement>> = Mutex::new(Vec::with_capacity(jobs.len()));
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..threads.max(1) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if i >= jobs.len() {
                    break;
                }
                let zi = jobs[i];
                let m = measure_zone(spec, &spec.zones[zi], &patch.zone_paths[zi]);
                let mut r = results.lock().unwrap();
                r.push(m);
                if r.len().is_multiple_of(2000) {
                    eprintln!("  {}/{}", r.len(), jobs.len());
                }
            });
        }
    });
    let results = results.into_inner().unwrap();

    // Aggregate per file (deterministic: BTreeMap order).
    let mut arrivals: BTreeMap<String, f64> = BTreeMap::new();
    let mut flagged: BTreeMap<String, (Class, Option<f64>, String)> = BTreeMap::new();
    let mut skipped = 0usize;
    let mut per_class: BTreeMap<&'static str, (usize, Vec<f64>)> = BTreeMap::new();
    for m in &results {
        if m.class == Class::Skip {
            skipped += 1;
            continue;
        }
        let cname = match m.class {
            Class::Transition => "transition (pitch settle)",
            Class::Retrigger => "retrigger (flux edge)",
            Class::Short => "short (flux peak)",
            Class::SustainEdge => "sustain (flux edge)",
            Class::Skip => unreachable!(),
        };
        match (&m.flag, m.arrival_ms) {
            (None, Some(ms)) => {
                // Round to 0.1 ms for a stable, deterministic styx.
                arrivals.insert(m.file.clone(), (ms * 10.0).round() / 10.0);
                let e = per_class.entry(cname).or_default();
                e.0 += 1;
                e.1.push(ms);
            }
            _ => {
                flagged.insert(
                    m.file.clone(),
                    (
                        m.class,
                        m.arrival_ms,
                        m.flag.clone().unwrap_or_else(|| "unmeasured".into()),
                    ),
                );
            }
        }
    }

    // Report.
    let mut report = String::new();
    report.push_str(&format!(
        "arrival measurement: {} zones — {} measured, {} low-confidence (not written), {} skipped (release/non-attack)\n",
        results.len(),
        arrivals.len(),
        flagged.len(),
        skipped
    ));
    for (cname, (n, vals)) in &per_class {
        let mut v = vals.clone();
        v.sort_by(|a, b| a.total_cmp(b));
        let med = v[v.len() / 2];
        report.push_str(&format!(
            "  {cname}: {n} measured, arrival min {:.0} / median {med:.0} / p95 {:.0} / max {:.0} ms\n",
            v[0],
            v[((v.len() as f64 * 0.95) as usize).min(v.len() - 1)],
            v[v.len() - 1]
        ));
    }
    report.push_str("\nlow-confidence zones (fallback markers kept):\n");
    for (file, (class, ms, reason)) in &flagged {
        report.push_str(&format!(
            "  {file} [{class:?}] {} — {reason}\n",
            ms.map(|m| format!("{m:.0} ms"))
                .unwrap_or_else(|| "-".into())
        ));
    }
    print!("{report}");
    if let Some(p) = &report_path {
        std::fs::File::create(p)
            .and_then(|mut f| f.write_all(report.as_bytes()))
            .map_err(|e| format!("write report: {e}"))?;
        eprintln!("report → {}", p.display());
    }

    if write {
        let text = std::fs::read_to_string(&zones_path).map_err(|e| e.to_string())?;
        let new_text = rewrite_zones_styx(&text, &arrivals);
        let tmp = zones_path.with_extension("styx.tmp");
        std::fs::write(&tmp, &new_text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &zones_path).map_err(|e| e.to_string())?;
        eprintln!(
            "wrote {} arrival markers → {}",
            arrivals.len(),
            zones_path.display()
        );
    } else {
        eprintln!("dry run (pass --write to update the zones styx)");
    }
    Ok(())
}
