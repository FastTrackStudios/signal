//! Self-contained HTML analysis reports — rendered audio + the full event
//! log as marker lanes over the waveform, and per-sample waveform views with
//! loop points. The debugging view for "why does this sound weird": every
//! `RenderTrace` event (voice spawns/ends, transitions, note-offs, sample
//! misses), `LegatoFireEvent`, and document `RenderMarker` lands on the
//! timeline, with computed loop-wrap ticks per looped voice.
//!
//! One template (`report_template.html`, REAPER-render-stats-inspired canvas
//! viewer, zero external resources) serves both modes; the injected JSON's
//! `mode` field selects the layout. Emitted by `fts signal pack
//! render-report` / `inspect-samples` and the trace_dump example.

use std::path::Path;

use serde_json::{Value, json};

use crate::SamplerError;
use crate::engine::trace::{RenderTrace, TraceKind};
use crate::engine::{EmittedMarker, LegatoFireEvent};
use crate::spec::ZoneSpec;

/// Min/max peak pairs over `block`-frame windows of an interleaved buffer,
/// mixed to mono (same shape as daw-proto's `TakePeakData`; reused by the
/// future analysis RPC).
pub fn compute_peaks(audio: &[f32], channels: usize, block: usize) -> (Vec<f32>, Vec<f32>) {
    let channels = channels.max(1);
    let frames = audio.len() / channels;
    let block = block.max(1);
    let n = frames.div_ceil(block);
    let mut mins = Vec::with_capacity(n);
    let mut maxs = Vec::with_capacity(n);
    for b in 0..n {
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        let start = b * block;
        let end = ((b + 1) * block).min(frames);
        for f in start..end {
            let mut s = 0.0f32;
            for c in 0..channels {
                s += audio[f * channels + c];
            }
            s /= channels as f32;
            lo = lo.min(s);
            hi = hi.max(s);
        }
        mins.push(if lo == f32::MAX { 0.0 } else { lo });
        maxs.push(if hi == f32::MIN { 0.0 } else { hi });
    }
    (mins, maxs)
}

/// Everything a render report draws besides the audio itself.
#[derive(Default)]
pub struct ReportSources {
    pub trace: RenderTrace,
    pub fires: Vec<LegatoFireEvent>,
    /// Document render markers as (frame, kind, note, line).
    pub markers: Vec<(u64, String, u8, u8)>,
    pub emitted: Vec<EmittedMarker>,
    /// Relative href of the sibling audio file for playback (e.g. "x.wav").
    pub audio_href: Option<String>,
    /// Solo stems: (note, label, href) — per-note isolated renders the viewer
    /// can switch the player between. Empty = full mix only.
    pub stems: Vec<(u8, String, String)>,
    /// Musical grid for the beat ruler: `(bpm, beats_per_bar)`, anchored so
    /// beat 1 of bar 1 is at frame 0. `None` = no ruler (free-time render).
    pub tempo: Option<(f64, u32)>,
    /// Relative href of a metronome click WAV (same length/rate as the mix),
    /// toggled on as a second synced audio layer. `None` = no click.
    pub click_href: Option<String>,
    /// Scheduling mode this render used, e.g. "DOCUMENT (prefire)" — shown as
    /// a header badge so a report is never mistaken for the LIVE/reactive path.
    pub mode_label: String,
    /// Reactive legato fires during the render. In DOCUMENT mode this MUST be
    /// 0 — anything else is the live/reactive path leaking into a document
    /// render (a missed schedule edge). Surfaced as a red banner when > 0.
    pub reactive_fallbacks: u64,
}

/// Max computed loop-wrap ticks recorded per voice (a 30 s held note with a
/// 2 s loop is ~15; the cap only guards degenerate tiny loops).
const MAX_WRAPS: usize = 512;

fn voice_json(
    spawn_frame: u64,
    line: u8,
    v: &crate::engine::TraceVoiceSpawn,
    end_frame: Option<u64>,
    total_frames: u64,
) -> Value {
    // Computed loop wraps (engine frames): the voice reaches loop_end after
    // (loop_end - start_frame) / rate frames, then wraps every
    // (loop_end - loop_start) / rate. Deterministic — voice.rs does no
    // re-timing — so the viewer can draw wrap ticks without trace events.
    let mut wraps: Vec<u64> = Vec::new();
    if v.loop_end > v.loop_start && v.rate > 0.0 {
        let horizon = end_frame.unwrap_or(total_frames);
        let first = spawn_frame as f64
            + (v.loop_end.saturating_sub(v.start_frame)) as f64 / v.rate;
        let period = (v.loop_end - v.loop_start) as f64 / v.rate;
        let mut t = first;
        while t < horizon as f64 && wraps.len() < MAX_WRAPS {
            wraps.push(t as u64);
            if period <= 0.0 {
                break;
            }
            t += period;
        }
    }
    json!({
        "id": v.voice_id,
        "kind": v.voice_kind,
        "file": v.file,
        "note": v.note,
        "root_key": v.root_key,
        "rate": v.rate,
        "gain": v.gain,
        "dynamic": v.dynamic,
        "articulation": v.articulation,
        "mic": v.mic,
        "direction": v.direction,
        "interval": v.interval,
        "rr": v.rr,
        "start_frame": v.start_frame,
        "loop_start": v.loop_start,
        "loop_end": v.loop_end,
        "loop_xfade": v.loop_xfade,
        "line": line,
        "spawn": spawn_frame,
        "end": end_frame,
        "wraps": wraps,
    })
}

/// Build the render-report JSON model.
pub fn render_report_json(
    name: &str,
    audio: &[f32],
    channels: usize,
    sample_rate: u32,
    sources: &ReportSources,
) -> Value {
    let frames = (audio.len() / channels.max(1)) as u64;
    // ~4096 columns regardless of length — plenty for a full-width canvas.
    let block = ((frames as usize / 4096).max(32)).next_power_of_two();
    let (mins, maxs) = compute_peaks(audio, channels, block);

    // Pair spawns with their ends.
    let mut ends: std::collections::BTreeMap<u64, u64> = Default::default();
    for e in &sources.trace.events {
        if let TraceKind::VoiceEnd { voice_id } = &e.kind {
            ends.entry(*voice_id).or_insert(e.frame);
        }
    }
    let mut voices = Vec::new();
    let mut events = Vec::new();
    for e in &sources.trace.events {
        match &e.kind {
            TraceKind::VoiceSpawn(v) => {
                // Hide inaudible ONE-SHOT layers (gain≈0 spawns that never
                // re-level). SustainLayer voices are kept even at gain 0 —
                // a CC1/CC2 sweep re-levels them later, and hiding them
                // masked the S14 missing-layer bug.
                if v.gain.abs() < 0.004 && !v.voice_kind.contains("Sustain") {
                    continue;
                }
                voices.push(voice_json(e.frame, e.line, v, ends.get(&v.voice_id).copied(), frames));
            }
            TraceKind::NoteOff { note } => {
                events.push(json!({"frame": e.frame, "line": e.line, "kind": "noteoff", "note": note}));
            }
            TraceKind::Transition { from, to, portamento } => {
                events.push(json!({"frame": e.frame, "line": e.line, "kind": "transition",
                    "from": from, "to": to, "portamento": portamento}));
            }
            TraceKind::SampleMiss { note, articulation, dynamic, rr, reason } => {
                events.push(json!({"frame": e.frame, "line": e.line, "kind": "miss",
                    "note": note, "articulation": articulation, "dynamic": dynamic,
                    "rr": rr, "reason": format!("{reason:?}")}));
            }
            TraceKind::VoiceEnd { .. } => {} // folded into voice spans
        }
    }
    for f in &sources.fires {
        events.push(json!({"frame": f.frame, "line": f.line, "kind": "fire",
            "from": f.from_note, "to": f.to_note, "velocity": f.velocity,
            "portamento": f.portamento, "arrival": f.arrival}));
    }
    for (frame, kind, note, line) in &sources.markers {
        events.push(json!({"frame": frame, "line": line, "kind": "marker", "mk": kind, "note": note}));
    }
    for m in &sources.emitted {
        events.push(json!({"frame": m.frame, "line": m.line, "kind": "emitted", "note": m.note}));
    }

    let stems: Vec<Value> = sources
        .stems
        .iter()
        .map(|(note, label, href)| json!({ "note": note, "label": label, "href": href }))
        .collect();
    json!({
        "mode": "render",
        "name": name,
        "sample_rate": sample_rate,
        "channels": channels,
        "frames": frames,
        "audio_href": sources.audio_href,
        "stems": stems,
        "tempo": sources.tempo.map(|(bpm, bpb)| json!({ "bpm": bpm, "beats_per_bar": bpb })),
        "click_href": sources.click_href,
        "mode_label": sources.mode_label,
        "reactive_fallbacks": sources.reactive_fallbacks,
        "peaks": { "block": block, "min": mins, "max": maxs },
        "voices": voices,
        "events": events,
    })
}

/// One sample in an inspector report: decoded audio + its zone metadata.
pub struct SampleView {
    pub title: String,
    pub audio: Vec<f32>,
    pub channels: usize,
    pub sample_rate: u32,
    pub zone: ZoneSpec,
}

/// Build the sample-inspector JSON model.
pub fn sample_report_json(name: &str, entries: &[SampleView]) -> Value {
    let samples: Vec<Value> = entries
        .iter()
        .map(|s| {
            let frames = (s.audio.len() / s.channels.max(1)) as u64;
            let block = ((frames as usize / 2048).max(16)).next_power_of_two();
            let (mins, maxs) = compute_peaks(&s.audio, s.channels, block);
            let z = &s.zone;
            json!({
                "title": s.title,
                "sample_rate": s.sample_rate,
                "frames": frames,
                "peaks": { "block": block, "min": mins, "max": maxs },
                "zone": {
                    "file": z.file,
                    "articulation": z.articulation,
                    "dynamic": z.dynamic,
                    "mic": z.mic,
                    "section": z.section,
                    "key_min": z.key_min, "key_max": z.key_max, "root_key": z.root_key,
                    "vel_min": z.vel_min, "vel_max": z.vel_max,
                    "rr_index": z.rr_index,
                    "direction": z.direction,
                    "interval": z.interval,
                    "loop_start": z.loop_start, "loop_end": z.loop_end,
                    "loop_xfade": z.loop_xfade,
                    "sample_start": z.sample_start, "sample_end": z.sample_end,
                    "lead_in_ms": z.lead_in_ms,
                    "arrival_ms": z.arrival_ms,
                    "gain_db": z.gain_db,
                    "tune_cents": z.tune_cents,
                }
            })
        })
        .collect();
    json!({ "mode": "samples", "name": name, "samples": samples })
}

/// Default real click sample (a short percussive click) stamped at each beat
/// when present; falls back to the synth blip if it can't be decoded.
pub const DEFAULT_CLICK_SAMPLE: &str =
    "/run/media/AudioHaven/SlateDigial/Trigger2Library/CLICKS/New Click -  Woodblock-eighth.wav";

/// Decode a click WAV to a mono f32 grain at `sample_rate` (linear-resampled),
/// trimmed to ~120 ms so beats never overlap. `None` if it can't be read.
fn load_click_grain(path: &Path, sample_rate: u32) -> Option<Vec<f32>> {
    let data = crate::engine::cache::load_sample(path).ok()?;
    let ch = data.channels.max(1) as usize;
    let src_frames = data.num_frames;
    let mono: Vec<f32> = (0..src_frames)
        .map(|f| {
            (0..ch).map(|c| data.frames[f * ch + c]).sum::<f32>() / ch as f32
        })
        .collect();
    let ratio = data.sample_rate as f64 / sample_rate as f64;
    let out_len = ((src_frames as f64 / ratio) as usize).min(sample_rate as usize / 8);
    let mut grain = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let sp = i as f64 * ratio;
        let i0 = sp as usize;
        let frac = (sp - i0 as f64) as f32;
        let a = mono.get(i0).copied().unwrap_or(0.0);
        let b = mono.get(i0 + 1).copied().unwrap_or(a);
        grain.push(a + (b - a) * frac);
    }
    Some(grain)
}

/// Generate an interleaved-stereo metronome click over `total_frames`,
/// anchored to beat 1 of bar 1 at frame 0. Uses the real click sample at
/// `click_sample` (louder on the downbeat) when decodable, else a synth blip.
/// Same rate/length as the mix so the viewer overlays it as a synced layer.
pub fn click_track(
    total_frames: usize,
    sample_rate: u32,
    bpm: f64,
    beats_per_bar: u32,
    click_sample: Option<&Path>,
) -> Vec<f32> {
    let mut out = vec![0.0f32; total_frames * 2];
    if bpm <= 0.0 {
        return out;
    }
    let fpb = 60.0 / bpm * sample_rate as f64;
    let bpbar = beats_per_bar.max(1);
    let sr = sample_rate as f32;
    let grain = click_sample.and_then(|p| load_click_grain(p, sample_rate));

    let mut beat = 0usize;
    loop {
        let start = (beat as f64 * fpb).round() as usize;
        if start >= total_frames {
            break;
        }
        let downbeat = (beat as u32) % bpbar == 0;
        let amp = if downbeat { 1.0 } else { 0.55 };
        match &grain {
            Some(g) => {
                for (i, &s) in g.iter().enumerate() {
                    let f = start + i;
                    if f >= total_frames {
                        break;
                    }
                    let v = s * amp;
                    out[f * 2] += v;
                    out[f * 2 + 1] += v;
                }
            }
            None => {
                // Synth fallback: fast-decaying tone (higher on the downbeat).
                let (freq, a) = if downbeat { (1500.0f32, 0.6) } else { (1000.0f32, 0.4) };
                let click_len = (sample_rate as f64 * 0.035) as usize;
                for i in 0..click_len {
                    let f = start + i;
                    if f >= total_frames {
                        break;
                    }
                    let t = i as f32 / sr;
                    let s = (t * freq * std::f32::consts::TAU).sin() * (-t * 90.0).exp() * a;
                    out[f * 2] += s;
                    out[f * 2 + 1] += s;
                }
            }
        }
        beat += 1;
    }
    out
}

/// Write a report HTML file: the shared template + injected JSON.
pub fn write_report_html(path: &Path, data: &Value) -> Result<(), SamplerError> {
    const TEMPLATE: &str = include_str!("report_template.html");
    let json = serde_json::to_string(data)
        .map_err(|e| SamplerError::SpecParse(format!("report json: {e}")))?;
    // Guard against `</script>` inside file-name strings.
    let json = json.replace("</", "<\\/");
    let html = TEMPLATE.replace("/*__REPORT_DATA__*/null", &json);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, html)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_of_known_ramp() {
        // Mono ramp 0..1 over 100 frames, block 25 → 4 pairs with rising maxima.
        let audio: Vec<f32> = (0..100).map(|i| i as f32 / 100.0).collect();
        let (mins, maxs) = compute_peaks(&audio, 1, 25);
        assert_eq!(mins.len(), 4);
        assert_eq!(mins[0], 0.0);
        assert!((maxs[3] - 0.99).abs() < 1e-6);
        assert!(maxs.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn wraps_computed_from_spawn_metadata() {
        let v = crate::engine::TraceVoiceSpawn {
            voice_id: 7,
            voice_kind: "SustainLayer",
            file: "s.wav".into(),
            note: 60,
            root_key: 60,
            rate: 1.0,
            gain: 1.0,
            dynamic: String::new(),
            articulation: "sus".into(),
            mic: String::new(),
            direction: String::new(),
            interval: 0,
            rr: 0,
            start_frame: 0,
            loop_start: 1000,
            loop_end: 2000,
            loop_xfade: 0,
        };
        // Spawn at 100; first wrap at 100+2000=2100, then every 1000 frames.
        let j = voice_json(100, 0, &v, None, 6000);
        let wraps: Vec<u64> = j["wraps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_u64().unwrap())
            .collect();
        assert_eq!(wraps, vec![2100, 3100, 4100, 5100]);
    }
}
