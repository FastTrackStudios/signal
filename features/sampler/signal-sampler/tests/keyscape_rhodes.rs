//! Integration coverage for the LA Custom Rhodes trigger path — the
//! multi-sample Keyscape patch that surfaced the pedal-body swap, the
//! round-robin short-sample bug, and the 44.1→48 kHz detuning. Drives the real
//! pack through the structured render trace and asserts that every note
//! triggers what it should. SKIPs when the pack isn't present (CI / other
//! machines), like the CSS pitch tests.
//!
//!   cargo test -p signal-sampler --test keyscape_rhodes

use std::path::Path;

use signal_sampler::{SamplerRig, TraceKind};

const PACK: &str = "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/\
Packs/Rhodes - LA Custom.signalpack";
const ID: &str = "rhodes";
const SR: u32 = 48_000;
const BLK: usize = 512;
/// 44.1 kHz pack on a 48 kHz engine → a same-pitch (note == sampled) body must
/// play at this rate, not 1.0. This is the detuning guard.
const SR_RATE: f64 = 44_100.0 / 48_000.0;

/// Load + preload the pack, or `None` to SKIP.
fn rhodes() -> Option<SamplerRig> {
    if !Path::new(PACK).exists() {
        eprintln!("SKIP keyscape_rhodes: pack not present at {PACK}");
        return None;
    }
    let rig = SamplerRig::new_offline(SR);
    rig.load_pack(ID, Path::new(PACK)).ok()?;
    rig.set_midi_channel(ID, 0);
    rig.set_default_instrument(ID);
    let _ = rig.preload_instrument(ID);
    let mut buf = vec![0.0f32; BLK * 2];
    for _ in 0..40 {
        buf.iter_mut().for_each(|s| *s = 0.0);
        let _ = rig.render_offline(&mut buf);
    }
    Some(rig)
}

fn render(rig: &SamplerRig, buf: &mut [f32], blocks: usize) {
    for _ in 0..blocks {
        buf.iter_mut().for_each(|s| *s = 0.0);
        let _ = rig.render_offline(buf);
    }
}

fn is_body(kind: &str) -> bool {
    kind.starts_with("Sustain")
}

/// Every note across the keyboard must sound a body voice — not a miss, not a
/// release in isolation. This is the "I only hear the attack / the release /
/// nothing" guard.
#[test]
fn every_note_sounds_a_body() {
    let Some(rig) = rhodes() else { return };
    rig.set_trace_enabled(ID, true);
    let mut buf = vec![0.0f32; BLK * 2];
    for note in 21u8..=108 {
        rig.midi_message(0, 0x90, note, 96);
        render(&rig, &mut buf, 3);
        rig.midi_message(0, 0x80, note, 64);
        render(&rig, &mut buf, 2);
    }
    let trace = rig.render_trace(ID);

    let mut dead = Vec::new();
    for note in 21u8..=108 {
        let spawns = trace.spawns_of_note(note);
        if !spawns.iter().any(|v| is_body(v.voice_kind)) {
            dead.push(note);
        }
    }
    assert!(dead.is_empty(), "notes with no body voice: {dead:?}");

    // A miss anywhere means a requested voice found no/unloaded sample.
    let misses = trace.misses();
    assert!(
        misses.is_empty(),
        "unexpected sample misses: {:?}",
        misses.iter().map(|(_, n, r)| (*n, *r)).collect::<Vec<_>>()
    );
}

/// The body plays in tune: a same-pitch note advances at the 44.1→48 kHz rate,
/// never 1.0 (which was +147 cents sharp). Guards the sample-rate fix.
#[test]
fn body_is_in_tune() {
    let Some(rig) = rhodes() else { return };
    rig.set_trace_enabled(ID, true);
    let mut buf = vec![0.0f32; BLK * 2];
    for note in [48u8, 60, 72, 84] {
        rig.midi_message(0, 0x90, note, 96);
        render(&rig, &mut buf, 3);
        rig.midi_message(0, 0x80, note, 64);
        render(&rig, &mut buf, 2);
    }
    let trace = rig.render_trace(ID);
    for note in [48u8, 60, 72, 84] {
        let body = trace
            .spawns_of_note(note)
            .into_iter()
            .find(|v| is_body(v.voice_kind))
            .unwrap_or_else(|| panic!("note {note} had no body"));
        // These notes are individually sampled (sampled_note == note), so the
        // only rate term is the SR ratio.
        assert!(
            (body.rate - SR_RATE).abs() < 0.01,
            "note {note} body rate {} — expected {SR_RATE:.4} (sample-rate compensated), \
             1.0 would be +147 cents sharp",
            body.rate
        );
    }
}

/// A held note at mf resolves to the long sustain sample, never a short
/// medium-velocity round-robin. Guards the RR fix (the "dies after the attack"
/// bug): the dyn-102 sustains exist only at rr0, so every strike must still
/// land on one.
#[test]
fn held_note_resolves_to_the_long_sustain() {
    let Some(rig) = rhodes() else { return };
    rig.set_trace_enabled(ID, true);
    let mut buf = vec![0.0f32; BLK * 2];
    // Strike note 60 several times so the round-robin counter cycles 0-3.
    for _ in 0..6 {
        rig.midi_message(0, 0x90, 60, 100);
        render(&rig, &mut buf, 2);
        rig.midi_message(0, 0x80, 60, 64);
        render(&rig, &mut buf, 2);
    }
    let trace = rig.render_trace(ID);
    let bodies: Vec<&str> = trace
        .spawns_of_note(60)
        .into_iter()
        .filter(|v| is_body(v.voice_kind))
        .map(|v| v.file.as_str())
        .collect();
    assert_eq!(bodies.len(), 6, "expected 6 body strikes, got {bodies:?}");
    for file in &bodies {
        assert!(
            file.contains(" 102"),
            "strike resolved to {file}, not the long dyn-102 sustain — RR fell back to a short sample"
        );
    }
}

/// Fast repeats each retrigger a fresh body (no dropped strikes).
#[test]
fn fast_repeats_each_retrigger() {
    let Some(rig) = rhodes() else { return };
    rig.set_trace_enabled(ID, true);
    let mut buf = vec![0.0f32; BLK * 2];
    for _ in 0..8 {
        rig.midi_message(0, 0x90, 64, 100);
        render(&rig, &mut buf, 2); // ~20 ms
        rig.midi_message(0, 0x80, 64, 64);
        render(&rig, &mut buf, 2);
    }
    let trace = rig.render_trace(ID);
    let bodies = trace
        .spawns_of_note(64)
        .into_iter()
        .filter(|v| is_body(v.voice_kind))
        .count();
    assert_eq!(bodies, 8, "8 fast strikes should spawn 8 bodies, got {bodies}");
}

/// A note-off with release info fires a release tail — after the body sounded.
#[test]
fn note_off_fires_a_release_tail() {
    let Some(rig) = rhodes() else { return };
    rig.set_trace_enabled(ID, true);
    let mut buf = vec![0.0f32; BLK * 2];
    rig.midi_message(0, 0x90, 60, 100);
    render(&rig, &mut buf, 4);
    rig.midi_message(0, 0x80, 60, 64); // release velocity 64
    render(&rig, &mut buf, 4);
    let trace = rig.render_trace(ID);
    let has_release = trace
        .spawns_of_note(60)
        .into_iter()
        .any(|v| v.voice_kind == "Release");
    assert!(has_release, "note-off did not fire a release tail");
}

/// The release tail's loudness follows the NOTE-ON strike velocity, not the
/// note-off velocity. A soft strike must get a much quieter release than a firm
/// strike even when both send the same note-off velocity — otherwise a
/// pianissimo note's quiet body is buried under a full-volume key-up click
/// ("plays a mech noise, the note doesn't ring").
#[test]
fn release_tail_tracks_strike_velocity_not_note_off() {
    let Some(rig) = rhodes() else { return };
    let mut buf = vec![0.0f32; BLK * 2];

    let mut release_gain = |vel: u8| -> f32 {
        rig.set_trace_enabled(ID, true);
        rig.midi_message(0, 0x90, 72, vel);
        render(&rig, &mut buf, 6);
        rig.midi_message(0, 0x80, 72, 64); // identical "no-info" note-off both times
        render(&rig, &mut buf, 6);
        rig.render_trace(ID)
            .spawns_of_note(72)
            .into_iter()
            .find(|v| v.voice_kind == "Release")
            .map(|v| v.gain)
            .unwrap_or(0.0)
    };

    let soft = release_gain(20);
    let firm = release_gain(110);
    assert!(soft > 0.0 && firm > 0.0, "both strikes should fire a release ({soft}, {firm})");
    assert!(
        firm > soft * 4.0,
        "release must track strike velocity: soft(v20)={soft:.4} vs firm(v110)={firm:.4} \
         — same note-off velocity, so a fixed release would make these equal"
    );
}

/// Under the sustain pedal the body stays the playable instrument (`lacrm`),
/// never the pedal-NOISE articulation (`lacrped`), and the pedal noise fires as
/// its own layer. Guards the pedal-body-swap fix.
#[test]
fn sustain_pedal_keeps_the_body_not_the_pedal_noise() {
    let Some(rig) = rhodes() else { return };
    rig.set_trace_enabled(ID, true);
    let mut buf = vec![0.0f32; BLK * 2];
    rig.midi_message(0, 0xB0, 64, 127); // sustain pedal down
    render(&rig, &mut buf, 4);
    rig.midi_message(0, 0x90, 62, 100); // strike under the pedal
    render(&rig, &mut buf, 4);
    let trace = rig.render_trace(ID);

    // The struck note's body must be a full-keyboard body, not pedal noise.
    let body = trace
        .spawns_of_note(62)
        .into_iter()
        .find(|v| is_body(v.voice_kind))
        .expect("note under pedal spawned no body");
    assert!(
        !body.articulation.to_lowercase().contains("ped"),
        "under the pedal the body swapped to pedal noise: {}",
        body.articulation
    );

    // The pedal noise itself should have fired (its own ambience layer).
    let pedal_noise = trace.events.iter().any(|e| match &e.kind {
        TraceKind::VoiceSpawn(v) => v.articulation.to_lowercase().contains("ped"),
        _ => false,
    });
    assert!(pedal_noise, "pedal-down produced no pedal-noise voice");
}
