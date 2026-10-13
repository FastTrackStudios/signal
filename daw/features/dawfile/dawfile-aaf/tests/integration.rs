//! Integration tests against real-world AAF fixture files.
//!
//! Fixtures are downloaded from the LibAAF project and cover files generated
//! by DaVinci Resolve (DR_*), Avid Media Composer (MC_*), Adobe Premiere
//! (PR_*), and Pro Tools (PT_*).

use dawfile_aaf::parse::parse_session;
use dawfile_aaf::types::{ClipKind, TrackKind};
use std::path::Path;

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

// ─── Helper assertions ────────────────────────────────────────────────────────

fn audio_tracks(session: &dawfile_aaf::types::AafSession) -> usize {
    session
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Audio)
        .count()
}

#[allow(dead_code)]
fn video_tracks(session: &dawfile_aaf::types::AafSession) -> usize {
    session
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Video)
        .count()
}

fn source_clip_count(session: &dawfile_aaf::types::AafSession) -> usize {
    session
        .tracks
        .iter()
        .flat_map(|t| &t.clips)
        .filter(|c| matches!(c.kind, ClipKind::SourceClip { .. }))
        .count()
}

fn clips_with_positive_length(session: &dawfile_aaf::types::AafSession) -> usize {
    session
        .tracks
        .iter()
        .flat_map(|t| &t.clips)
        .filter(|c| c.length > 0)
        .count()
}

// ─── All files parse without error ───────────────────────────────────────────

#[test]
fn all_fixtures_parse_ok() {
    let fixtures_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut count = 0;
    for entry in std::fs::read_dir(&fixtures_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("aaf") {
            continue;
        }
        parse_session(&path)
            .unwrap_or_else(|e| panic!("parse failed for {}: {}", path.display(), e));
        count += 1;
    }
    assert!(
        count >= 21,
        "expected at least 21 fixture files, found {count}"
    );
}

// ─── DaVinci Resolve ─────────────────────────────────────────────────────────

#[test]
fn dr_empty_parses() {
    let s = parse_session(&fixture("DR_Empty.aaf")).unwrap();
    assert_eq!(s.session_sample_rate, 48000);
    assert!(audio_tracks(&s) >= 1, "expected at least 1 audio track");
}

#[test]
fn dr_markers() {
    let s = parse_session(&fixture("DR_Markers.aaf")).unwrap();
    assert!(audio_tracks(&s) >= 1);
    assert_eq!(s.markers.len(), 2, "DR_Markers should have 2 markers");
}

#[test]
fn dr_audio_levels() {
    let s = parse_session(&fixture("DR_Audio_Levels.aaf")).unwrap();
    assert!(audio_tracks(&s) >= 1, "expected at least 1 audio track");
    // DR exports audio-only AAF; timecode is extracted but no separate video track.
    assert!(s.timecode_start.is_some(), "expected timecode");
}

#[test]
fn dr_multichannel() {
    let s = parse_session(&fixture("DR_Multichannel_5.1_single_source.aaf")).unwrap();
    assert!(audio_tracks(&s) >= 1);
}

// ─── Avid Media Composer ─────────────────────────────────────────────────────

#[test]
fn mc_empty_parses() {
    let s = parse_session(&fixture("MC_Empty.aaf")).unwrap();
    assert_eq!(s.session_sample_rate, 48000);
}

#[test]
fn mc_fades() {
    let s = parse_session(&fixture("MC_Fades.aaf")).unwrap();
    assert!(audio_tracks(&s) >= 1);
    let clips = source_clip_count(&s);
    assert!(
        clips >= 5,
        "MC_Fades should have at least 5 source clips, got {clips}"
    );
    let with_len = clips_with_positive_length(&s);
    assert!(
        with_len >= 5,
        "MC_Fades clips should have positive lengths, got {with_len}"
    );
}

#[test]
fn mc_markers() {
    let s = parse_session(&fixture("MC_Markers.aaf")).unwrap();
    assert!(audio_tracks(&s) >= 1);
    assert_eq!(s.markers.len(), 2, "MC_Markers should have 2 markers");
    assert!(source_clip_count(&s) >= 1);
}

#[test]
fn mc_audio_levels() {
    let s = parse_session(&fixture("MC_Audio_Levels.aaf")).unwrap();
    assert!(audio_tracks(&s) >= 1, "expected audio tracks");
    // Timecode track is present but no video content in an audio-levels test file.
    assert!(s.timecode_start.is_some(), "expected timecode");
}

#[test]
fn mc_tc_25() {
    let s = parse_session(&fixture("MC_TC_25.aaf")).unwrap();
    assert!(
        s.timecode_start.is_some(),
        "MC_TC_25 should have a timecode start"
    );
    if let Some(tc) = s.timecode_start {
        assert_eq!(tc.fps, 25);
        assert!(!tc.drop_frame);
    }
}

#[test]
fn mc_tc_29_97_df() {
    let s = parse_session(&fixture("MC_TC_29.97_DF.aaf")).unwrap();
    assert!(s.timecode_start.is_some());
    if let Some(tc) = s.timecode_start {
        assert_eq!(tc.fps, 30);
        assert!(tc.drop_frame);
    }
}

#[test]
fn mc_tc_23_976() {
    let s = parse_session(&fixture("MC_TC_23.976.aaf")).unwrap();
    assert!(s.timecode_start.is_some());
}

// ─── Adobe Premiere ──────────────────────────────────────────────────────────

#[test]
fn pr_fades() {
    let s = parse_session(&fixture("PR_Fades.aaf")).unwrap();
    assert!(audio_tracks(&s) >= 1);
}

#[test]
fn pr_clip_length_beyond_eof() {
    // Should parse without error even when clip length exceeds source file length.
    let s = parse_session(&fixture("PR_Clip_length_beyond_EOF.aaf")).unwrap();
    let _ = s;
}

// ─── Pro Tools ───────────────────────────────────────────────────────────────

#[test]
fn pt_fades() {
    let s = parse_session(&fixture("PT_Fades.aaf")).unwrap();
    assert_eq!(s.session_sample_rate, 48000);
    assert!(
        audio_tracks(&s) >= 2,
        "PT_Fades should have at least 2 audio tracks"
    );
    let clips = source_clip_count(&s);
    assert_eq!(clips, 9, "PT_Fades should have 9 source clips");
    let with_len = clips_with_positive_length(&s);
    assert_eq!(
        with_len, 9,
        "all 9 PT_Fades clips should have positive lengths"
    );
}

#[test]
fn pt_wav_external_same_directory() {
    let s = parse_session(&fixture("PT_WAV_External_same_directory.aaf")).unwrap();
    assert_eq!(s.session_sample_rate, 48000);
    assert!(audio_tracks(&s) >= 1);
    let clips = source_clip_count(&s);
    assert_eq!(
        clips, 1,
        "PT_WAV_External_same_directory should have 1 source clip"
    );
    // Clip should have a resolved file URL.
    for t in &s.tracks {
        for c in &t.clips {
            if let ClipKind::SourceClip { source_file, .. } = &c.kind {
                assert!(
                    source_file.is_some(),
                    "source clip should resolve to a file URL"
                );
            }
        }
    }
}

#[test]
fn pt_wav_external() {
    let s = parse_session(&fixture("PT_WAV_External.aaf")).unwrap();
    assert!(audio_tracks(&s) >= 1);
}

#[test]
fn pt_audio_levels() {
    let s = parse_session(&fixture(
        "PT_Audio_Levels-noEMCC-noExpTracksAsMultiChannel.aaf",
    ))
    .unwrap();
    assert!(audio_tracks(&s) >= 1);
}

#[test]
fn pt_multichannel() {
    let s = parse_session(&fixture("PT_Multichannel_5.1_multi_source.aaf")).unwrap();
    assert!(audio_tracks(&s) >= 1);
}
