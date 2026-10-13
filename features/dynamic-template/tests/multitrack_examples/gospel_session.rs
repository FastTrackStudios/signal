use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn gospel_session_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::single_track("Drums", "01_Drums.wav");

    // Claps now correctly classified as Percussion
    let percussion = TrackGroup::single_track("Percussion", "05_Claps.wav");

    // ElecBass now correctly classified as Bass
    let bass = TrackGroup::single_track("Bass", "02_ElecBass.wav");

    let guitars = TrackGroup::folder("Guitars")
        .track("Electric 1")
        .item("03_ElecGtr1.wav")
        .track("Electric 2")
        .item("04_ElecGtr2.wav")
        .end();

    let vocals = TrackGroup::single_track("Vocals", "06_LeadVox.wav");

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(percussion)
        .group(bass)
        .group(guitars)
        .group(vocals)
        .build();
    expected
}

#[test]
fn gospel_session() {
    // -- Setup & Fixtures
    // MusicDelta Gospel: 6-stem gospel session from MedleyDB. Drum set, electric bass,
    // 2 clean electric guitars, hand claps, and female lead vocal. Tests gospel
    // arrangement with claps classification.
    let items = vec![
        "01_Drums.wav",
        "02_ElecBass.wav",
        "03_ElecGtr1.wav",
        "04_ElecGtr2.wav",
        "05_Claps.wav",
        "06_LeadVox.wav",
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

    let expected = gospel_session_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
