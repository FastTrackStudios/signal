use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn queen_dont_stop_me_now_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::folder("Drums")
        .track("Kick")
        .item("01_Kick.wav")
        .track("Snare")
        .item("02_Snare.wav")
        .track("Drum Kit")
        .item("03_Kit.wav")
        .end();

    let percussion = TrackGroup::single_track("Percussion", "07_Percussion.wav");
    let bass = TrackGroup::single_track("Bass", "04_Bass.wav");
    let guitars = TrackGroup::single_track("Guitars", "05_Guitar.wav");
    let keys = TrackGroup::single_track("Keys", "06_Piano.wav");

    let vocals = TrackGroup::folder("Vocals")
        .track("Lead")
        .item("08_LeadVox.wav")
        .track("BGVs")
        .item("09_BackingVox.wav")
        .end();

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(percussion)
        .group(bass)
        .group(guitars)
        .group(keys)
        .group(vocals)
        .build();
    expected
}

#[test]
fn queen_dont_stop_me_now() {
    // -- Setup & Fixtures
    // Queen - Don't Stop Me Now: 9-stem rock session. Classic Queen arrangement with
    // separated drum components (kick + snare + full kit), bass, guitar, piano,
    // percussion, lead vocals, and backing vocals. Tests drum component + full kit overlap.
    let items = vec![
        "01_Kick.wav",
        "02_Snare.wav",
        "03_Kit.wav",
        "04_Bass.wav",
        "05_Guitar.wav",
        "06_Piano.wav",
        "07_Percussion.wav",
        "08_LeadVox.wav",
        "09_BackingVox.wav",
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

    let expected = queen_dont_stop_me_now_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
