use dawfile_ableton::*;

fn farmhouse() -> AbletonLiveSet {
    read_live_set("tests/fixtures/Farmhouse.als").expect("failed to parse Farmhouse.als")
}

fn lucid_dreaming() -> AbletonLiveSet {
    read_live_set("tests/fixtures/LucidDreaming.als").expect("failed to parse LucidDreaming.als")
}

// ─── Version detection ──────────────────────────────────────────────────────

#[test]
fn farmhouse_version() {
    let set = farmhouse();
    assert_eq!(set.version.major, 12);
    assert_eq!(set.version.minor, 0);
    assert_eq!(set.version.creator, "Ableton Live 12.3.6");
}

#[test]
fn lucid_dreaming_version() {
    let set = lucid_dreaming();
    assert_eq!(set.version.major, 12);
    assert_eq!(set.version.creator, "Ableton Live 12.3");
}

// ─── Tempo and time signature ───────────────────────────────────────────────

#[test]
fn farmhouse_tempo() {
    let set = farmhouse();
    assert_eq!(set.tempo, 120.0);
    assert_eq!(set.time_signature.numerator, 4);
    assert_eq!(set.time_signature.denominator, 4);
}

#[test]
fn farmhouse_tempo_automation() {
    let set = farmhouse();
    assert_eq!(set.tempo_automation.len(), 30);
    // First point is 120 BPM at beat 0
    assert_eq!(set.tempo_automation[0].time, 0.0);
    assert_eq!(set.tempo_automation[0].value, 120.0);
    // Second point changes to 108 BPM
    assert_eq!(set.tempo_automation[1].value, 108.0);
}

#[test]
fn lucid_dreaming_tempo_automation() {
    let set = lucid_dreaming();
    assert_eq!(set.tempo_automation.len(), 4);
    // Ramps to 170.020004 BPM (exact value from the .als file)
    assert_eq!(set.tempo_automation[1].value, 170.020004);
}

// ─── Track counts ───────────────────────────────────────────────────────────

#[test]
fn farmhouse_track_counts() {
    let set = farmhouse();
    assert_eq!(set.audio_tracks.len(), 29);
    assert_eq!(set.midi_tracks.len(), 13);
    assert_eq!(set.group_tracks.len(), 8);
    assert_eq!(set.return_tracks.len(), 0);
}

#[test]
fn lucid_dreaming_track_counts() {
    let set = lucid_dreaming();
    assert_eq!(set.audio_tracks.len(), 15);
    assert_eq!(set.midi_tracks.len(), 1);
    assert_eq!(set.group_tracks.len(), 2);
}

// ─── Track properties ───────────────────────────────────────────────────────

#[test]
fn farmhouse_track_names() {
    let set = farmhouse();
    let names: Vec<&str> = set
        .audio_tracks
        .iter()
        .map(|t| t.common.effective_name.as_str())
        .collect();
    assert!(names.contains(&"GTR FX"));
    assert!(names.contains(&"Bass"));
    assert!(names.contains(&"SONG REFERENCE"));
}

#[test]
fn farmhouse_track_mixer() {
    let set = farmhouse();
    let song_ref = set
        .audio_tracks
        .iter()
        .find(|t| t.common.effective_name == "SONG REFERENCE")
        .expect("SONG REFERENCE track not found");
    // Volume is attenuated (0.42)
    assert!(song_ref.common.mixer.volume < 0.5);
    assert!(song_ref.common.mixer.volume > 0.3);
}

#[test]
fn farmhouse_group_tracks() {
    let set = farmhouse();
    let group_names: Vec<&str> = set
        .group_tracks
        .iter()
        .map(|t| t.common.effective_name.as_str())
        .collect();
    assert!(group_names.contains(&"INSTRUMENT"));
    assert!(group_names.contains(&"GTR Rig"));
    assert!(group_names.contains(&"Bass Rig"));
    assert!(group_names.contains(&"VOCAL FX"));
}

// ─── Audio clips ────────────────────────────────────────────────────────────

#[test]
fn farmhouse_audio_clips() {
    let set = farmhouse();
    let gtr = set
        .audio_tracks
        .iter()
        .find(|t| t.common.effective_name == "GTR" && t.common.id == 122)
        .expect("GTR track not found");
    assert_eq!(gtr.arrangement_clips.len(), 11);

    // First clip has a sample reference
    let clip = &gtr.arrangement_clips[0];
    assert!(clip.sample_ref.is_some());
    let sr = clip.sample_ref.as_ref().unwrap();
    assert!(
        sr.path
            .as_ref()
            .unwrap()
            .to_string_lossy()
            .contains("Mountains At Midnight")
    );
}

#[test]
fn farmhouse_clip_positions() {
    let set = farmhouse();
    let other = set
        .audio_tracks
        .iter()
        .find(|t| t.common.effective_name == "Other" && t.common.id == 120)
        .expect("Other track not found");
    assert_eq!(other.arrangement_clips.len(), 1);
    let clip = &other.arrangement_clips[0];
    assert!(clip.common.time > 4600.0); // positioned deep in the arrangement
    assert!(clip.common.current_end - clip.common.current_start > 400.0); // long clip
}

// ─── MIDI clips and notes ───────────────────────────────────────────────────

#[test]
fn farmhouse_midi_tracks() {
    let set = farmhouse();
    let midi_names: Vec<&str> = set
        .midi_tracks
        .iter()
        .map(|t| t.common.effective_name.as_str())
        .collect();
    assert!(midi_names.contains(&"CLICK"));
    assert!(midi_names.contains(&"STOP"));
    assert!(midi_names.contains(&"GUIDE"));
}

#[test]
fn farmhouse_midi_clips_have_notes() {
    let set = farmhouse();
    // Find tracks with MIDI clips that have notes
    let total_notes: usize = set
        .midi_tracks
        .iter()
        .flat_map(|t| &t.arrangement_clips)
        .flat_map(|c| &c.key_tracks)
        .map(|kt| kt.notes.len())
        .sum();
    assert!(total_notes > 0, "expected MIDI notes in arrangement clips");
}

// ─── Devices and FX ─────────────────────────────────────────────────────────

#[test]
fn farmhouse_devices() {
    let set = farmhouse();
    let gtr_fx = set
        .audio_tracks
        .iter()
        .find(|t| t.common.effective_name == "GTR FX")
        .expect("GTR FX track not found");
    // 6 top-level devices + plugins inside rack branches
    assert!(gtr_fx.common.devices.len() >= 6);

    // Check that we have typed builtin params
    let tuner = gtr_fx
        .common
        .devices
        .iter()
        .find(|d| d.name == "Tuner")
        .expect("Tuner device not found on GTR FX");
    assert_eq!(tuner.format, DeviceFormat::Builtin);
    assert!(tuner.builtin_params.is_some());
}

#[test]
fn farmhouse_master_track() {
    let set = farmhouse();
    let master = set.master_track.as_ref().expect("no master track");
    assert_eq!(master.devices.len(), 1);
    assert_eq!(master.devices[0].name, "Eq8");
    assert!(master.devices[0].builtin_params.is_some());
}

#[test]
fn lucid_dreaming_master_glue_compressor() {
    let set = lucid_dreaming();
    let master = set.master_track.as_ref().expect("no master track");
    assert_eq!(master.devices.len(), 1);
    assert_eq!(master.devices[0].name, "GlueCompressor");
    assert!(master.devices[0].builtin_params.is_some());
}

// ─── Track automation ───────────────────────────────────────────────────────

#[test]
fn farmhouse_track_automation() {
    let set = farmhouse();
    let total_envelopes: usize = set
        .audio_tracks
        .iter()
        .map(|t| t.common.automation_envelopes.len())
        .sum::<usize>()
        + set
            .midi_tracks
            .iter()
            .map(|t| t.common.automation_envelopes.len())
            .sum::<usize>();
    assert!(total_envelopes > 0, "expected track automation envelopes");

    // GTR FX has 2 automation envelopes
    let gtr_fx = set
        .audio_tracks
        .iter()
        .find(|t| t.common.effective_name == "GTR FX")
        .expect("GTR FX not found");
    assert_eq!(gtr_fx.common.automation_envelopes.len(), 2);
}

#[test]
fn farmhouse_clip_envelopes() {
    let set = farmhouse();
    let total_clip_envelopes: usize = set
        .audio_tracks
        .iter()
        .flat_map(|t| &t.arrangement_clips)
        .map(|c| c.common.envelopes.len())
        .sum::<usize>()
        + set
            .midi_tracks
            .iter()
            .flat_map(|t| &t.arrangement_clips)
            .map(|c| c.common.envelopes.len())
            .sum::<usize>();
    assert!(
        total_clip_envelopes > 0,
        "expected clip automation envelopes"
    );
}

// ─── Routing ────────────────────────────────────────────────────────────────

#[test]
fn farmhouse_routing() {
    let set = farmhouse();
    let other = set
        .audio_tracks
        .iter()
        .find(|t| t.common.effective_name == "Other" && t.common.id == 120)
        .expect("Other track not found");
    // Routed to a group track
    assert!(other.audio_output.target.contains("GroupTrack"));
}

#[test]
fn farmhouse_master_output() {
    let set = farmhouse();
    let master = set.master_track.as_ref().unwrap();
    // Master routes to stereo output pair 11 (S10 = 0-indexed pair 10)
    assert!(master.audio_output.target.contains("External"));
}

// ─── Locators / markers ─────────────────────────────────────────────────────

#[test]
fn farmhouse_locators() {
    let set = farmhouse();
    assert_eq!(set.locators.len(), 7);
}

#[test]
fn lucid_dreaming_locators() {
    let set = lucid_dreaming();
    assert_eq!(set.locators.len(), 3);
}

// ─── Scenes ─────────────────────────────────────────────────────────────────

#[test]
fn farmhouse_scenes() {
    let set = farmhouse();
    assert_eq!(set.scenes.len(), 49);
}

#[test]
fn lucid_dreaming_scenes() {
    let set = lucid_dreaming();
    assert_eq!(set.scenes.len(), 8);
}

// ─── Groove pool ────────────────────────────────────────────────────────────

#[test]
fn lucid_dreaming_grooves() {
    let set = lucid_dreaming();
    assert_eq!(set.groove_pool.len(), 1);
}

// ─── Round-trip: read → write → read ────────────────────────────────────────

#[test]
fn round_trip_lucid_dreaming() {
    let original = lucid_dreaming();

    // Serialize to .als bytes
    let bytes = write_live_set_bytes(&original).expect("failed to write");

    // Parse back
    let reparsed = parse_live_set_bytes(&bytes).expect("failed to re-parse");

    // Core properties should match
    assert_eq!(reparsed.version.major, original.version.major);
    assert_eq!(reparsed.version.minor, original.version.minor);
    assert_eq!(reparsed.tempo, original.tempo);
    assert_eq!(
        reparsed.time_signature.numerator,
        original.time_signature.numerator
    );
    assert_eq!(
        reparsed.time_signature.denominator,
        original.time_signature.denominator
    );
    assert_eq!(reparsed.audio_tracks.len(), original.audio_tracks.len());
    assert_eq!(reparsed.midi_tracks.len(), original.midi_tracks.len());
    assert_eq!(reparsed.locators.len(), original.locators.len());
}

#[test]
fn round_trip_farmhouse() {
    let original = farmhouse();

    let bytes = write_live_set_bytes(&original).expect("failed to write");
    let reparsed = parse_live_set_bytes(&bytes).expect("failed to re-parse");

    assert_eq!(reparsed.tempo, original.tempo);
    assert_eq!(reparsed.audio_tracks.len(), original.audio_tracks.len());
    assert_eq!(reparsed.midi_tracks.len(), original.midi_tracks.len());
    assert_eq!(reparsed.group_tracks.len(), original.group_tracks.len());
    assert_eq!(reparsed.locators.len(), original.locators.len());
    assert_eq!(reparsed.scenes.len(), original.scenes.len());
    assert_eq!(
        reparsed.tempo_automation.len(),
        original.tempo_automation.len()
    );

    // Verify track names survived round-trip
    for (orig, rt) in original
        .audio_tracks
        .iter()
        .zip(reparsed.audio_tracks.iter())
    {
        assert_eq!(rt.common.effective_name, orig.common.effective_name);
    }
}

// ─── Error handling ─────────────────────────────────────────────────────────

#[test]
fn invalid_file_returns_error() {
    let result = read_live_set("tests/fixtures/nonexistent.als");
    assert!(result.is_err());
}

#[test]
fn non_gzip_file_returns_error() {
    // Create a temp file with non-gzip content
    let dir = std::env::temp_dir().join("dawfile-ableton-test");
    std::fs::create_dir_all(&dir).ok();
    let path = dir.join("not_gzip.als");
    std::fs::write(&path, b"this is not gzip").unwrap();
    let result = read_live_set(&path);
    assert!(result.is_err());
    match result.unwrap_err() {
        AbletonError::NotGzip => {} // expected
        other => panic!("expected NotGzip, got: {other}"),
    }
    std::fs::remove_file(path).ok();
}
