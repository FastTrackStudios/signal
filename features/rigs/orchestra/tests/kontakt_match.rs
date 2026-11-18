//! Engine behaviour pinned against Kontakt null tests (`tests/null_test.py`,
//! probe renders from `tests/gen_probe.py`). Each test states what the
//! CSS-in-Kontakt render did, measured, and checks the engine does the same.
//!
//! Gated on the Cinematic Studio Strings library being on this machine (the
//! same pattern as the A/B examples): without it they pass vacuously.

use sampler::SamplerRig;
use sampler::engine::trace::TraceKind;
use signal_orchestra::{CSS_CONFIG, CSS_ROOT, load_strings};

const SR: u32 = 48_000;
const ID: &str = "v1";

fn rig() -> Option<SamplerRig> {
    if !std::path::Path::new(CSS_ROOT).exists() {
        return None;
    }
    let rig = SamplerRig::new_offline(SR);
    load_strings(&rig, ID, "1st Violins", "Mix", CSS_ROOT, CSS_CONFIG).ok()?;
    rig.set_trace_enabled(ID, true);
    Some(rig)
}

fn render(rig: &SamplerRig, secs: f64) {
    let mut buf = vec![0.0f32; 128 * 2];
    for _ in 0..((secs * f64::from(SR)) as usize).div_ceil(128) {
        rig.render_offline(&mut buf).expect("render");
    }
}

/// Probe #0: CC1=127, CC2=0, velocity 40 → Kontakt's zone-1 first-note path
/// plays the Nonvib ff body ALONE (null −68.7 dB against that one file) — no
/// re-bow, no attack ornament.
/// Kontakt's velocity zones (persistent `$eluxs=64`, `$0uhls=100`).
#[test]
fn velocity_zones_match_the_script() {
    let text = std::fs::read_to_string(CSS_CONFIG).expect("config");
    let spec = sampler::spec::LibrarySpec::from_styx(&text).expect("parse");
    let cfg = spec.legato_cfg();
    assert_eq!(cfg.velocity_splits, vec![64, 100]);
    assert_eq!(cfg.velocity_range(40), 1);
    assert_eq!(cfg.velocity_range(64), 1);
    assert_eq!(cfg.velocity_range(65), 2);
    assert_eq!(cfg.velocity_range(101), 3);
}

#[test]
fn soft_first_note_plays_only_the_body() {
    let Some(rig) = rig() else { return };
    rig.cc(ID, 59, 0);
    rig.cc(ID, 11, 127);
    rig.cc(ID, 64, 0);
    rig.cc(ID, 58, 2);
    rig.cc(ID, 1, 127);
    rig.cc(ID, 2, 0);
    render(&rig, 1.5);
    let _ = rig.warm_note(ID, 67);
    rig.note_on(ID, 67, 40);
    render(&rig, 0.5);
    let trace = rig.render_trace(ID);
    let extra: Vec<String> = trace
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            TraceKind::VoiceSpawn(v) if v.voice_kind != "SustainLayer" => {
                Some(format!("{} {}", v.voice_kind, v.file))
            }
            _ => None,
        })
        .collect();
    assert!(extra.is_empty(), "first soft note spawned {extra:?}");
}

/// Probe #2: key 72 plays the B3 zone (root 71) RESAMPLED by +100 c on top of
/// the +9 c group tune (Kontakt null −52.7 dB at +109.003 c), not through a
/// time-preserving shifter.
#[test]
fn key_between_samples_is_resampled() {
    let Some(rig) = rig() else { return };
    rig.cc(ID, 1, 127);
    rig.cc(ID, 2, 0);
    render(&rig, 1.5);
    let _ = rig.warm_note(ID, 72);
    rig.note_on(ID, 72, 40);
    render(&rig, 0.2);
    let trace = rig.render_trace(ID);
    let ff = trace
        .spawns_of_file("Nonvib/ff_B3")
        .first()
        .map(|(_, v)| v.rate)
        .expect("Nonvib ff B3 spawned");
    let want = 2f64.powf(109.0 / 1200.0);
    assert!((ff - want).abs() < 1e-4, "rate {ff} want {want}");
}

fn rms(x: &[f32]) -> f64 {
    (x.iter().map(|&v| f64::from(v) * f64::from(v)).sum::<f64>() / x.len().max(1) as f64).sqrt()
}

/// Probe #0 level: with every group curve at 1.0 Kontakt plays the zone at
/// ×1.3414 (+2.55 dB) over the raw file (null −68.7 dB at that gain).
#[test]
fn full_level_body_is_plus_2_55_db_over_the_file() {
    let Some(rig) = rig() else { return };
    rig.cc(ID, 11, 127);
    rig.cc(ID, 58, 2);
    rig.cc(ID, 1, 127);
    rig.cc(ID, 2, 0);
    render(&rig, 1.5);
    let _ = rig.warm_note(ID, 67);
    rig.note_on(ID, 67, 40);
    // 1.0 .. 2.0 s after note-on, stereo interleaved
    let mut out = vec![0.0f32; 2 * SR as usize * 2];
    rig.render_offline(&mut out).expect("render");
    let ours = &out[2 * SR as usize..];
    let file = format!("{CSS_ROOT}/Mix/1st Violins/Sustain/Nonvib/ff_G3.wav");
    let mut r = hound::WavReader::open(&file).expect("raw file");
    let raw: Vec<f32> = r
        .samples::<i32>()
        .map(|s| s.expect("sample") as f32 / (1i64 << 23) as f32)
        .collect();
    // The body plays ~0.7 % fast (+12.2 c); over 1 s that is a 7 ms shift —
    // irrelevant to RMS of a steady sustain.
    let file_win = &raw[2 * SR as usize..2 * 2 * SR as usize];
    let ratio = rms(ours) / rms(file_win);
    assert!(
        (ratio - 1.3414).abs() < 0.01,
        "level ratio {ratio:.4}, Kontakt 1.3414"
    );
}

/// Probe #10: a velocity-120 first note plays the velocity-3 body variant
/// (Kontakt zone sample start 168000) and LOOPS 48010..end like Kontakt —
/// it must still sound after the variant reaches the end of the file
/// (2.6 s at +12.2 c), where it used to fall silent.
#[test]
fn body_variant_keeps_sounding_past_the_file_end() {
    let Some(rig) = rig() else { return };
    rig.cc(ID, 59, 0);
    rig.cc(ID, 58, 2);
    rig.cc(ID, 1, 127);
    rig.cc(ID, 2, 0);
    render(&rig, 1.5);
    let _ = rig.warm_note(ID, 67);
    rig.note_on(ID, 67, 100); // variant 2 (counter 0 → 1)
    render(&rig, 0.5);
    rig.note_off(ID, 67);
    render(&rig, 2.0);
    rig.note_on(ID, 67, 120); // variant 3, start 168000
    render(&rig, 3.0);
    let mut out = vec![0.0f32; 2 * SR as usize / 2];
    rig.render_offline(&mut out).expect("render");
    assert!(
        rms(&out) > 0.01,
        "body silent 3.0..3.5 s after a variant start"
    );
}

/// CSB holds CC2 at 126 (`dynamics.fixed_ccs`); a CC2 0 the player sends is
/// ignored on every mono line. Kontakt plays both releases of every legato
/// phrase (null test: the old note's in-phrase release and the phrase end's
/// release). Before the fix only the first phrase did: the second, allocated
/// to another line, heard the CC2 0 and its release curves read 0.
#[test]
fn every_brass_phrase_plays_its_releases() {
    let Some(brass) = signal_orchestra::library("brass") else {
        return;
    };
    if !std::path::Path::new(brass.root).exists() {
        return;
    }
    let rig = SamplerRig::new_offline(SR);
    if signal_orchestra::load_instrument(&rig, ID, brass, "Solo Trumpet", "Mix").is_err() {
        return;
    }
    rig.set_trace_enabled(ID, true);
    rig.cc(ID, 58, 2);
    rig.cc(ID, 1, 127);
    rig.cc(ID, 2, 0);
    render(&rig, 0.5);
    for _ in 0..2 {
        let _ = rig.warm_note(ID, 67);
        let _ = rig.warm_note(ID, 69);
        rig.note_on(ID, 67, 40);
        render(&rig, 0.2);
        rig.note_on(ID, 69, 40);
        render(&rig, 0.05);
        rig.note_off(ID, 67);
        render(&rig, 1.95);
        rig.note_off(ID, 69);
        render(&rig, 2.0);
    }
    let releases = rig
        .render_trace(ID)
        .events
        .iter()
        .filter(|e| matches!(&e.kind, TraceKind::VoiceSpawn(v) if v.voice_kind == "Release"))
        .count();
    assert_eq!(
        releases, 4,
        "two phrases × (in-phrase + phrase-end) releases"
    );
}
