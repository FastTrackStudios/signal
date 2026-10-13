//! Walk every `.logicx` fixture and print a structured summary.
//! Run with `cargo test -p dawfile-logic --test fixture_summary -- --nocapture`.

use dawfile_logic::read_session;
use std::path::PathBuf;

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

fn fixtures() -> Vec<PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(FIXTURE_DIR)
        .unwrap()
        .filter_map(|e| {
            let p = e.ok()?.path();
            // .logicx is a directory bundle
            (p.extension()?.to_str()? == "logicx").then_some(p)
        })
        .collect();
    v.sort();
    v
}

#[test]
fn dump_all_logic_fixtures() {
    let files = fixtures();
    assert!(!files.is_empty());

    eprintln!("\n=== {} Logic fixtures ===", files.len());
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy();
        let s = read_session(path).unwrap_or_else(|e| panic!("parse {name}: {e}"));

        let total_clips: usize = s.tracks.iter().map(|t| t.clips.len()).sum();

        eprintln!(
            "── {name}\n   creator={} variant={:?} sr={} bpm={:.2} {}/{} key={}-{}\n   \
             tracks={} clips={} audio_files={} markers={} tempo_events={} summing_groups={} chunks={}",
            s.creator_version,
            s.variant_name,
            s.sample_rate,
            s.bpm,
            s.time_sig_numerator,
            s.time_sig_denominator,
            s.key,
            s.key_gender,
            s.tracks.len(),
            total_clips,
            s.audio_files.len(),
            s.markers.len(),
            s.tempo_events.len(),
            s.summing_groups.len(),
            s.chunks.len(),
        );

        for (i, t) in s.tracks.iter().enumerate().take(8) {
            eprintln!(
                "   track[{i}]: ch={} {:?} kind={:?} db={:?} mute={} solo={} clips={}",
                t.channel,
                t.name,
                t.kind,
                t.fader_db,
                t.muted,
                t.soloed,
                t.clips.len()
            );
            for c in t.clips.iter().take(3) {
                eprintln!(
                    "      clip: {:?} @ beat {:.2} len={:.2} kind={}",
                    c.name,
                    c.position_beats,
                    c.length_beats,
                    match &c.kind {
                        dawfile_logic::ClipKind::Audio { file_path } =>
                            format!("Audio({:?})", file_path),
                        dawfile_logic::ClipKind::Midi { notes } =>
                            format!("Midi(notes={})", notes.len()),
                        dawfile_logic::ClipKind::TakeFolder(tf) => format!(
                            "TakeFolder(takes={}, comp_ranges={})",
                            tf.takes.len(),
                            tf.comp_ranges.len()
                        ),
                        dawfile_logic::ClipKind::Other => "Other".into(),
                    }
                );
            }
        }
        for (i, f) in s.audio_files.iter().enumerate().take(8) {
            eprintln!(
                "   audio_file[{i}]: {:?} (vol={:?} usable={})",
                f.filename, f.vol_name, f.usable
            );
        }
    }
}
