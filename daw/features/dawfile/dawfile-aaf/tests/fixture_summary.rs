//! Walk every `.aaf` fixture and print a structured summary.
//! Run with `cargo test -p dawfile-aaf --test fixture_summary -- --nocapture`.

use dawfile_aaf::parse::parse_session;
use dawfile_aaf::types::{ClipKind, TrackKind};
use std::path::PathBuf;

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

fn fixtures() -> Vec<PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(FIXTURE_DIR)
        .unwrap()
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension()?.to_str()? == "aaf").then_some(p)
        })
        .collect();
    v.sort();
    v
}

#[test]
fn dump_all_aaf_fixtures() {
    let files = fixtures();
    assert!(!files.is_empty());

    eprintln!("\n=== {} AAF fixtures ===", files.len());
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy();
        let s = parse_session(path).unwrap_or_else(|e| panic!("parse {name}: {e}"));

        let audio_tracks = s
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Audio)
            .count();
        let video_tracks = s
            .tracks
            .iter()
            .filter(|t| t.kind == TrackKind::Video)
            .count();
        let total_clips: usize = s.tracks.iter().map(|t| t.clips.len()).sum();
        let source_clips: usize = s
            .tracks
            .iter()
            .flat_map(|t| &t.clips)
            .filter(|c| matches!(c.kind, ClipKind::SourceClip { .. }))
            .count();
        let resolved_files: usize = s
            .tracks
            .iter()
            .flat_map(|t| &t.clips)
            .filter(|c| {
                matches!(
                    &c.kind,
                    ClipKind::SourceClip {
                        source_file: Some(_),
                        ..
                    }
                )
            })
            .count();

        eprintln!(
            "── {name}\n   sample_rate={} omfi={} tracks={} (audio={} video={}) clips={} \
             source_clips={} resolved_files={} markers={} compositions={} timecode={}",
            s.session_sample_rate,
            s.object_model_version,
            s.tracks.len(),
            audio_tracks,
            video_tracks,
            total_clips,
            source_clips,
            resolved_files,
            s.markers.len(),
            s.compositions.len(),
            s.timecode_start
                .as_ref()
                .map(|tc| format!("{}fps df={}", tc.fps, tc.drop_frame))
                .unwrap_or_else(|| "none".into()),
        );
    }
}
