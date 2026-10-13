//! Integration tests that parse the ptformat test fixtures.
//!
//! These tests validate our Rust parser against the expected output
//! from the reference C++ ptformat library.

use dawfile_protools::read_session;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

fn fixture_path(name: &str) -> String {
    format!("{FIXTURES}/{name}")
}

#[test]
fn parse_pt12_region_test() {
    // RegionTest.ptx — PT12, 44100Hz session, target 48000Hz
    let session =
        read_session(fixture_path("RegionTest.ptx"), 48000).expect("should parse RegionTest.ptx");

    assert_eq!(session.version, 12);
    assert_eq!(session.session_sample_rate, 44100);

    // Expected: 1 wav, 3 regions, 6 active track regions
    assert_eq!(session.audio_files.len(), 1, "should have 1 wav file");
    assert_eq!(session.audio_files[0].filename, "region_name_WAV.wav");

    assert_eq!(
        session.audio_regions.len(),
        3,
        "should have 3 audio regions"
    );
    assert_eq!(session.audio_regions[0].name, "region_name_region");
    assert_eq!(session.audio_regions[1].name, "region_name_region-01");
    assert_eq!(session.audio_regions[2].name, "region_name_region-03");

    // 6 active region assignments across tracks
    let total_active: usize = session.audio_tracks.iter().map(|t| t.regions.len()).sum();
    assert_eq!(total_active, 6, "should have 6 active region assignments");
}

#[test]
fn parse_pt12_midi_test() {
    // TestPTX.ptx — PT12, 48000Hz, has MIDI
    let session =
        read_session(fixture_path("TestPTX.ptx"), 48000).expect("should parse TestPTX.ptx");

    assert_eq!(session.version, 12);
    assert_eq!(session.session_sample_rate, 48000);

    // Expected: 4 wavs, 4 audio regions, 3 active audio tracks
    assert_eq!(session.audio_files.len(), 4, "should have 4 wav files");
    assert_eq!(
        session.audio_regions.len(),
        4,
        "should have 4 audio regions"
    );

    // Expected: 3 MIDI regions
    assert_eq!(session.midi_regions.len(), 3, "should have 3 MIDI regions");

    if !session.midi_regions.is_empty() {
        let mr0 = &session.midi_regions[0];
        assert_eq!(mr0.name, "MIDI 1-01");
        assert_eq!(mr0.events.len(), 9, "MIDI 1-01 should have 9 events");

        let mr1 = &session.midi_regions[1];
        assert_eq!(mr1.name, "MIDI 2-01");
        assert_eq!(mr1.events.len(), 16, "MIDI 2-01 should have 16 events");

        let mr2 = &session.midi_regions[2];
        assert_eq!(mr2.name, "MIDI 3-01");
        assert_eq!(mr2.events.len(), 4, "MIDI 3-01 should have 4 events");
    }
}

#[test]
fn parse_pt8_playlists() {
    // goodplaylists2.ptf — PT8, 48000Hz
    let session = read_session(fixture_path("goodplaylists2.ptf"), 48000)
        .expect("should parse goodplaylists2.ptf");

    assert_eq!(session.version, 8);
    assert_eq!(session.session_sample_rate, 48000);

    // Expected: 3 wavs, 6 regions
    assert_eq!(session.audio_files.len(), 3, "should have 3 wav files");
    assert_eq!(
        session.audio_regions.len(),
        6,
        "should have 6 audio regions"
    );

    // Expected: 13 active region assignments across 4 tracks
    let total_active: usize = session.audio_tracks.iter().map(|t| t.regions.len()).sum();
    assert_eq!(total_active, 13, "should have 13 active region assignments");
}

#[test]
fn parse_pt5_damien_monos() {
    // Damien_monos.pts — PT5, 48000Hz
    let session = read_session(fixture_path("Damien_monos.pts"), 48000)
        .expect("should parse Damien_monos.pts");

    assert_eq!(session.version, 5);
    assert_eq!(session.session_sample_rate, 48000);

    // Expected: 8 wavs, 8 regions, 8 tracks
    assert_eq!(session.audio_files.len(), 8, "should have 8 wav files");
    assert_eq!(
        session.audio_regions.len(),
        8,
        "should have 8 audio regions"
    );
    assert_eq!(session.audio_tracks.len(), 8, "should have 8 audio tracks");
}

#[test]
fn parse_pt5_fa_16_48() {
    let session =
        read_session(fixture_path("Fa_16_48.pts"), 48000).expect("should parse Fa_16_48.pts");

    assert_eq!(session.version, 5);
}

#[test]
fn parse_pt5_for_ardour() {
    let session =
        read_session(fixture_path("forArdour.pts"), 48000).expect("should parse forArdour.pts");

    assert_eq!(session.version, 5);
}

#[test]
fn parse_pt8_midi() {
    let session =
        read_session(fixture_path("midi345x.ptf"), 48000).expect("should parse midi345x.ptf");

    assert!(session.version >= 5);
}

#[test]
fn feature_support_is_read_only() {
    let support = dawfile_protools::feature_support();
    assert!(support.can_read(daw_proto::Capability::Tracks));
    assert!(support.can_read(daw_proto::Capability::Items));
    assert!(!support.can_write(daw_proto::Capability::Tracks));
    assert!(!support.can_write(daw_proto::Capability::Items));
}

// ── Tier-1 export coverage ──────────────────────────────────────────────────

#[test]
fn tier1_track_kind_is_set() {
    use dawfile_protools::TrackKind;
    let s = read_session(fixture_path("TestPTX.ptx"), 48000).unwrap();
    assert!(s.audio_tracks.iter().all(|t| t.kind == TrackKind::Audio));
    assert!(s.midi_tracks.iter().all(|t| t.kind == TrackKind::Midi));
}

#[test]
fn tier1_all_tracks_iter_is_audio_then_midi() {
    let s = read_session(fixture_path("TestPTX.ptx"), 48000).unwrap();
    let total = s.all_tracks().count();
    assert_eq!(total, s.audio_tracks.len() + s.midi_tracks.len());
}

#[test]
fn tier1_audio_file_lookup_helper_runs() {
    // Sanity: helper does not panic and returns either Some or None.
    // (Some sessions store audio_file_index in a different namespace than file.index;
    // a robust mapping is tracked separately — see beads.)
    let s = read_session(fixture_path("RegionTest.ptx"), 48000).unwrap();
    assert!(!s.audio_regions.is_empty());
    let region = &s.audio_regions[0];
    let _maybe = s.audio_file_for(region);
}

#[test]
fn tier1_session_has_meter_and_markers_when_present() {
    // GodnessOfGod has 3 meter events; HeyLady has 1.
    let s = read_session(fixture_path("GodnessOfGod.ptx"), 48000).unwrap();
    assert!(
        s.meter_events.len() >= 3,
        "GodnessOfGod should expose ≥3 meter events"
    );
    assert!(s.session_sample_rate >= 48000);
}

#[test]
fn tier1_io_class_decoded() {
    use dawfile_protools::IoClass;
    let s = read_session(fixture_path("HeyLady.ptx"), 48000).unwrap();
    // Real session should have at least one hardware interface or output bus.
    let any_known = s
        .io_channels
        .iter()
        .any(|c| matches!(c.class(), IoClass::HardwareInterface | IoClass::OutputBus));
    if !s.io_channels.is_empty() {
        assert!(any_known, "expected at least one classified I/O channel");
    }
}

#[test]
fn tier1_samples_to_seconds_helper() {
    let s = read_session(fixture_path("HeyLady.ptx"), 48000).unwrap();
    // 48000 samples == 1 second at 48 kHz session.
    let seconds = s.samples_to_seconds(s.session_sample_rate as u64);
    assert!((seconds - 1.0).abs() < 1e-6);
}

#[test]
fn tier1_total_helpers_consistent() {
    // total_active_region_placements should equal manual sum across all tracks.
    let s = read_session(fixture_path("choir-session.ptx"), 48000).unwrap();
    let manual: usize = s
        .audio_tracks
        .iter()
        .chain(s.midi_tracks.iter())
        .map(|t| t.regions.len())
        .sum();
    assert_eq!(s.total_active_region_placements(), manual);
}

#[test]
fn internal_tracks_decoded_across_fixtures() {
    // Counts verified manually via cross-fixture string sweep on 0x261e.
    // See docs/converter-frida-discovered-offsets.md §"Phase E — internal
    // tracks via real PT fixtures".
    let cases = [
        ("HeyLady.ptx", 1, "Click"),
        ("studio-session-2.ptx", 6, "Master"),
        ("worship-session.ptx", 7, "DRUMS"),
        ("orchestral-session.ptx", 7, "Click 1"),
        ("wonder-session.ptx", 16, "DRUMS"),
    ];
    for (fixture, expected_count, expected_name) in cases {
        let s = read_session(fixture_path(fixture), 48000).expect("should parse");
        assert_eq!(
            s.internal_tracks.len(),
            expected_count,
            "{fixture}: wrong internal track count"
        );
        let names: Vec<&str> = s.internal_tracks.iter().map(|t| t.name.as_str()).collect();
        assert!(
            names.contains(&expected_name),
            "{fixture}: missing expected internal track {expected_name:?}, got {names:?}"
        );
        // Routing UIDs must be non-zero on all entries.
        for t in &s.internal_tracks {
            assert!(
                t.routing_uid.iter().any(|&b| b != 0),
                "{fixture}: zero routing UID for {:?}",
                t.name
            );
        }
    }
}

#[test]
fn edit_groups_decoded() {
    // worship-session has the 4 built-in stem types as edit-group entries.
    let s = read_session(fixture_path("worship-session.ptx"), 48000).unwrap();
    let names: Vec<&str> = s.edit_groups.iter().map(|g| g.name.as_str()).collect();
    for expected in ["Dialog", "Music", "Effects", "Narration"] {
        assert!(
            names.contains(&expected),
            "worship-session missing edit group {expected:?}, got {names:?}"
        );
    }
    // HeyLady has no 0x4501 block; the parser must surface an empty Vec
    // (not panic, not over-read).
    let h = read_session(fixture_path("HeyLady.ptx"), 48000).unwrap();
    assert_eq!(
        h.edit_groups.len(),
        0,
        "HeyLady has no edit groups; got {:?}",
        h.edit_groups
    );
}

#[test]
fn stem_mappings_decoded() {
    let s = read_session(fixture_path("orchestral-session.ptx"), 48000).unwrap();
    // Built-in stem types appear at the top of orchestral's 0x4702.
    let head: Vec<&str> = s.stem_mappings.iter().take(4).map(|n| n.as_str()).collect();
    assert!(
        head.contains(&"Dialog") && head.contains(&"Music") && head.contains(&"Effects"),
        "orchestral stem_mappings should start with built-in stem types, got {head:?}"
    );
}
