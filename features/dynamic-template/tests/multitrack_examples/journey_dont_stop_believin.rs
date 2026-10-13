use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn journey_dont_stop_believin_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::folder("Drums")
        .track("Kick")
        .item("01_Kick.wav")
        .track("Snare")
        .item("02_Snare.wav")
        .track("OH")
        .item("03_Overheads.wav")
        .end();

    let bass = TrackGroup::single_track("Bass", "04_Bass.wav");

    let guitars = TrackGroup::folder("Guitars")
        .track("Lead")
        .item("06_LeadGtr.wav")
        .track("Rhythm")
        .item("05_RhythmGtr.wav")
        .end();

    let keys = TrackGroup::single_track("Keys", "07_Piano.wav");
    let synths = TrackGroup::single_track("Synths", "08_Synth.wav");

    let vocals = TrackGroup::folder("Vocals")
        .track("Lead")
        .item("09_LeadVox.wav")
        .track("BGVs")
        .item("10_BackingVox.wav")
        .end();

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(bass)
        .group(guitars)
        .group(keys)
        .group(synths)
        .group(vocals)
        .build();
    expected
}

#[test]
fn journey_dont_stop_believin() {
    // -- Setup & Fixtures
    // Journey - Don't Stop Believin': 10-stem arena rock session. Classic AOR
    // instrumentation with full drums, bass, rhythm and lead guitars, iconic piano
    // and synth parts, lead vocals and backing vocals.
    let items = vec![
        "01_Kick.wav",
        "02_Snare.wav",
        "03_Overheads.wav",
        "04_Bass.wav",
        "05_RhythmGtr.wav",
        "06_LeadGtr.wav",
        "07_Piano.wav",
        "08_Synth.wav",
        "09_LeadVox.wav",
        "10_BackingVox.wav",
    ];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    // ============================================================================
    // Expected structure
    // ============================================================================

    let expected = journey_dont_stop_believin_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
