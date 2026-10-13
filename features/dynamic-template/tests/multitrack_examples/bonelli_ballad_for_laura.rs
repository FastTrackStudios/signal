use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn bonelli_ballad_for_laura_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::single_track("Drums", "06_Drums.wav");
    let keys = TrackGroup::single_track("Keys", "07_Piano.wav");
    let horns = TrackGroup::single_track("Horns", "03_TenorSax.wav");

    let brass = TrackGroup::folder("Brass")
        .track("Trumpets")
        .item("01_Trumpet.wav")
        .track("Trombone")
        .item("02_Trombone.wav")
        .end();

    let orchestra = TrackGroup::folder("Orchestra")
        .track("Contrabass")
        .item("04_DoubleBass.wav")
        .group(brass)
        .track("Clarinets")
        .item("05_BassClarinet.wav")
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
fn bonelli_ballad_for_laura() {
    // -- Setup & Fixtures
    // Rodrigo Bonelli - Ballad for Laura: 7-stem jazz ensemble session from MedleyDB.
    // Trumpet, trombone, tenor saxophone, double bass, bass clarinet, drums, and piano.
    // Tests jazz ensemble with diverse brass/woodwind classification.
    let items = vec![
        "01_Trumpet.wav",
        "02_Trombone.wav",
        "03_TenorSax.wav",
        "04_DoubleBass.wav",
        "05_BassClarinet.wav",
        "06_Drums.wav",
        "07_Piano.wav",
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

    let expected = bonelli_ballad_for_laura_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
