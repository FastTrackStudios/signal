use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn van_halen_aint_talkin_bout_love_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::folder("Drums")
        .track("Kick")
        .item("01_Kick.wav")
        .track("Snare")
        .item("02_Snare.wav")
        .track("Toms")
        .item("03_Tom.wav")
        .track("Cymbals")
        .item("04_Cymbals.wav")
        .end();

    let bass = TrackGroup::single_track("Bass", "05_Bass.wav");

    let guitars = TrackGroup::folder("Guitars")
        .track("Electric 1")
        .item("06_Guitar1.wav")
        .track("Electric 2")
        .item("07_Guitar2.wav")
        .end();

    let vocals = TrackGroup::single_track("Vocals", "08_Vox.wav");

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(bass)
        .group(guitars)
        .group(vocals)
        .build();
    expected
}

#[test]
fn van_halen_aint_talkin_bout_love() {
    // -- Setup & Fixtures
    // Van Halen - Ain't Talkin' 'Bout Love: 8-stem hard rock session.
    // Classic hard rock instrumentation: bass, cymbals, 2 guitars (rhythm + lead),
    // kick, snare, tom, and vocals. Tests basic rock kit separation.
    let items = vec![
        "01_Kick.wav",
        "02_Snare.wav",
        "03_Tom.wav",
        "04_Cymbals.wav",
        "05_Bass.wav",
        "06_Guitar1.wav",
        "07_Guitar2.wav",
        "08_Vox.wav",
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

    let expected = van_halen_aint_talkin_bout_love_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
