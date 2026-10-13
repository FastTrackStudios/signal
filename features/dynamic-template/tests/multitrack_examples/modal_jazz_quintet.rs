use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn modal_jazz_quintet_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::single_track("Drums", "01_Drums.wav");
    let keys = TrackGroup::single_track("Keys", "03_Piano.wav");
    let horns = TrackGroup::single_track("Horns", "04_TenorSax.wav");

    // Double bass → Orchestra/Contrabass, Trumpet → Orchestra/Trumpets
    let orchestra = TrackGroup::folder("Orchestra")
        .track("Contrabass")
        .item("02_DoubleBass.wav")
        .track("Trumpets")
        .item("05_Trumpet.wav")
        .end();

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(keys)
        .group(horns)
        .group(orchestra)
        .build();
    expected
}

#[test]
fn modal_jazz_quintet() {
    // -- Setup & Fixtures
    // MusicDelta Modal Jazz: 5-stem classic jazz quintet from MedleyDB. Double bass,
    // drum set, piano, tenor saxophone, and trumpet. Classic Miles Davis-style modal
    // jazz instrumentation. Tests pure jazz combo classification.
    let items = vec![
        "01_Drums.wav",
        "02_DoubleBass.wav",
        "03_Piano.wav",
        "04_TenorSax.wav",
        "05_Trumpet.wav",
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

    let expected = modal_jazz_quintet_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
