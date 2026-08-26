//! EQUIVALENCE GATE: keyflow-orchestra's mirror pass and signal-sampler's
//! document mode carry the same stage-1 annotation model (articulation state
//! at note-on, legato edges, same-pitch re-bows). Historically the sampler
//! side was a hand-maintained parity COPY of `keyflow-orchestra/src/mirror.rs`
//! kept in sync by comment convention; this test replaces the convention with
//! an assertion — both paths must produce identical annotations over the whole
//! MusicXML corpus (the same fixtures keyflow's `mirror_parity.rs` runs, where
//! the mirror side is itself verified against the real CSS engine).
//!
//! Scope: the DRIFT-PRONE shared model only — hints are keyflow-only
//! (documents are MIDI-domain), so both run hintless here; the re-bow
//! capability gate is computed keyflow-style (profile) and passed to the
//! document side, which normally derives it from the library spec.

use keyflow_orchestra::mirror::stage1_annotations as mirror_annotations;
use keyflow_orchestra::{detect_profile, process_part, Config, MidiNote};
use signal_sampler::document::{
    stage1_annotations as doc_annotations, DocCc, DocNote, TrackDocument,
};

fn corpus_files() -> Vec<std::path::PathBuf> {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../crates/keyflow/examples/mxl");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("keyflow mxl corpus missing")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "mxl"))
        .collect();
    files.sort();
    assert!(files.len() >= 18, "corpus shrank: {}", files.len());
    files
}

/// keyflow engine channels are 1-based; documents are 0-based.
fn to_document(notes: &[MidiNote], ccs: &[keyflow_orchestra::engine::CcEvent]) -> TrackDocument {
    TrackDocument {
        version: 1,
        seed: 0,
        auto_divisi: false, // keyflow already assigned divisi channels
        notes: notes
            .iter()
            .map(|n| DocNote {
                start_qn: n.start_qn,
                end_qn: n.end_qn,
                chan: n.chan.saturating_sub(1),
                pitch: n.pitch.clamp(0, 127) as u8,
                vel: n.vel,
            })
            .collect(),
        ccs: ccs
            .iter()
            .map(|e| DocCc {
                qn: e.qn,
                chan: e.chan.saturating_sub(1),
                cc: e.cc,
                val: e.val,
            })
            .collect(),
        tempo: Vec::new(),
    }
}

/// Both annotation paths over every part of every corpus score must agree on
/// (ks_val, legato_from, re_bow_to) for every note.
#[test]
fn document_annotation_matches_mirror_across_corpus() {
    for f in corpus_files() {
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let score = keyflow_orchestra::score::load(&f).expect("parse");
        for part in &score.parts {
            let profile = detect_profile(&part.name);
            let cfg = Config {
                profile,
                timing_comp: false,
                tempo_map: Some(score.meta.tempos.clone()),
                ..Config::default()
            };
            let stage1 = process_part(part, &cfg);
            if stage1.empty {
                continue;
            }
            let src: Vec<MidiNote> = stage1.notes.iter().map(MidiNote::from).collect();

            // Mirror path (hintless — documents have no notation events).
            let mcfg = Config {
                profile,
                ..Config::default()
            };
            let m = mirror_annotations(&src, &stage1.ccs, None, &mcfg);

            // Document path. The re-bow capability gate mirrors keyflow's:
            // re_bow enabled + profile has legato + not polyphonic.
            let prof = profile.profile();
            let legato_capable = mcfg.re_bow && prof.legato.is_some() && !prof.polyphonic;
            let doc = to_document(&src, &stage1.ccs);
            let d = doc_annotations(&doc, legato_capable);

            assert_eq!(m.len(), d.len(), "{name}/{}: length", part.name);
            for (i, (ma, da)) in m.iter().zip(d.iter()).enumerate() {
                assert_eq!(
                    (ma.ks_val, ma.legato_from, ma.re_bow_to),
                    (da.ks_val, da.legato_from, da.re_bow_to),
                    "{name}/{} note {i} ({:?}): mirror vs document annotation",
                    part.name,
                    src[i],
                );
            }
        }
    }
}
