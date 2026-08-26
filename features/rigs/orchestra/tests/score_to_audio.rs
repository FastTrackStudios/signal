//! End-to-end bridge tests: MusicXML score → keyflow-orchestra engine →
//! TrackDocument → annotated Schedule → (with CSS on disk) offline audio.
//!
//! The schedule-level tests run unconditionally; the full-audio render is
//! gated on the Cinematic Studio Strings library + config being present on
//! this machine (same pattern as the A/B examples).

use signal_orchestra::{load_strings, part_to_document, render_part_offline, CSS_CONFIG, CSS_ROOT};
use signal_sampler::document::{annotate, stage1_annotations};
use signal_sampler::spec::LibrarySpec;
use signal_sampler::SamplerRig;

const SR: u32 = 48_000;
const SEED: u64 = 0x60D0_1234_D0C5_EED0;

fn fixture() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../../crates/keyflow/examples/mxl/GOING HOME - Dvorak New World Symphony 1.6 mxml.mxl",
    )
}

/// A CSS-shaped spec (legato engine + shorts + CC58 map) for spec-dependent
/// annotation without the real library on disk.
fn test_spec() -> LibrarySpec {
    LibrarySpec::from_styx(
        r#"
name BridgeTest
articulations (
    { id Sus,  label Sustain,  kind @Sustain, rr 1 }
    { id Leg,  label Legato,   kind @Legato,  rr 4, directional true }
    { id Stac, label Staccato, kind @Short,   rr 4 }
)
legato_engine {
    expressive {
        zones (
            {vel_range (0 64),    label slow, delay_ms 333}
            {vel_range (65 127),  label fast, delay_ms 100}
        )
    }
    portamento { trigger_vel_max 10, volume_controller CC5 }
}
short_note_timing { pre_delay_ms 60 }
keyswitch {
    cc58_map {
        0-5   "Sustain: Low Latency Legato"
        21-25 Staccato
    }
}
"#,
    )
    .expect("parse test spec")
}

fn violin_part(score: &keyflow_orchestra::Score) -> &keyflow_orchestra::score::Part {
    score
        .parts
        .iter()
        .find(|p| p.name.to_lowercase().contains("violin"))
        .expect("violin part in fixture")
}

/// Score → annotated schedule, unconditionally: the keyflow-orchestra engine
/// output converts to a document whose schedule is deterministic, covers
/// every note, prefires legato, and whose stage-1 annotations are EQUIVALENT
/// to keyflow-orchestra's mirror-pass inference over the same notes (the two
/// consumers of the shared keyflow-annotate model agree through the real
/// conversion path).
#[test]
fn score_to_schedule_equivalence() {
    let score = keyflow_orchestra::score::load(fixture()).expect("parse fixture");
    let part = violin_part(&score);

    let cfg = keyflow_orchestra::Config {
        profile: keyflow_orchestra::ProfileKind::Strings,
        timing_comp: false,
        tempo_map: Some(score.meta.tempos.clone()),
        ..keyflow_orchestra::Config::default()
    };
    let po = keyflow_orchestra::process_part(part, &cfg);
    assert!(!po.empty && !po.notes.is_empty(), "engine produced notes");

    let doc = part_to_document(&po, &score.meta.tempos, SEED);
    assert_eq!(doc.notes.len(), po.notes.len());

    // ── Schedule invariants ──────────────────────────────────────────────
    let spec = test_spec();
    let sched = annotate(&doc, &spec, SR);
    assert_eq!(sched.note_count, doc.notes.len());
    assert!(!sched.events.is_empty());
    assert!(sched.end_frame > 0);
    assert!(
        sched.legato_count > 0,
        "a legato strings part must prefire transitions"
    );
    // Deterministic: same document + spec ⇒ identical schedule.
    let again = annotate(&doc, &spec, SR);
    assert_eq!(sched.events, again.events);

    // ── Annotation equivalence with the mirror pass ──────────────────────
    let src: Vec<keyflow_orchestra::MidiNote> = po
        .notes
        .iter()
        .map(keyflow_orchestra::MidiNote::from)
        .collect();
    let mirror = keyflow_orchestra::mirror::stage1_annotations(&src, &po.ccs, None, &cfg);
    let doc_ann = stage1_annotations(&doc, spec.legato_engine.is_some());
    assert_eq!(mirror.len(), doc_ann.len());
    for (i, (m, d)) in mirror.iter().zip(doc_ann.iter()).enumerate() {
        assert_eq!(
            (m.ks_val, m.legato_from, m.re_bow_to),
            (d.ks_val, d.legato_from, d.re_bow_to),
            "note {i}: mirror vs document annotation through the bridge"
        );
    }
}

/// Full audio proof, gated on the CSS library being installed: score →
/// document → `render_offline_document` with the real 1st Violins patch must
/// produce a non-silent buffer with ZERO reactive legato fallbacks (every
/// transition was prefired by the schedule).
#[test]
fn score_to_nonsilent_audio_with_css() {
    let zones = std::path::Path::new(CSS_ROOT).join("_patches/1st Violins/library.styx");
    if !zones.exists() || !std::path::Path::new(CSS_CONFIG).exists() {
        eprintln!("skipping: CSS library/config not on this machine");
        return;
    }

    let score = keyflow_orchestra::score::load(fixture()).expect("parse fixture");
    let part = violin_part(&score);

    let rig = SamplerRig::new_offline_with_cache_budget(SR, Some(8 * 1024 * 1024 * 1024));
    load_strings(
        &rig,
        "strings_1v",
        "1st Violins",
        "Mix",
        CSS_ROOT,
        CSS_CONFIG,
    )
    .expect("load CSS 1st Violins");

    let res = render_part_offline(
        &rig,
        "strings_1v",
        part,
        &score.meta.tempos,
        keyflow_orchestra::ProfileKind::Strings,
        SEED,
    )
    .expect("render");

    assert_eq!(res.note_count, {
        let cfg = keyflow_orchestra::Config {
            profile: keyflow_orchestra::ProfileKind::Strings,
            timing_comp: false,
            tempo_map: Some(score.meta.tempos.clone()),
            ..keyflow_orchestra::Config::default()
        };
        keyflow_orchestra::process_part(part, &cfg).notes.len()
    });
    assert_eq!(
        res.reactive_fallbacks, 0,
        "document playback must never hit the reactive legato path"
    );
    let peak = res.audio.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(
        peak > 1e-3,
        "rendered buffer is silent (peak {peak}) — end-to-end path broken"
    );
    assert!(!res.transitions.is_empty(), "legato transitions fired");
}
