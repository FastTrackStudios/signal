use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn cherry_poppin_daddies_zoot_suit_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::single_track("Drums", "01_Drums.wav");
    let percussion = TrackGroup::single_track("Percussion", "09_Tambourine.wav");
    let guitars = TrackGroup::single_track("Guitars", "03_ElecGtr.wav");
    let keys = TrackGroup::single_track("Keys", "04_Piano.wav");
    let horns_group = TrackGroup::single_track("Horns", "08_HornSection.wav");

    let vocals = TrackGroup::folder("Vocals")
        .track("Lead")
        .item("10_LeadVox.wav")
        .track("BGVs")
        .item("11_BackingVox.wav")
        .end();

    // Orchestra: double bass + brass section
    let trumpets = TrackGroup::folder("Trumpets")
        .track("Trumpets 1")
        .item("05_Trumpet1.wav")
        .track("Trumpets 2")
        .item("06_Trumpet2.wav")
        .end();

    let brass = TrackGroup::folder("Brass")
        .group(trumpets)
        .track("Horns")
        .item("08_HornSection.wav")
        .track("Trombone")
        .item("07_Trombone.wav")
        .end();

    let orchestra = TrackGroup::folder("Orchestra")
        .track("Contrabass")
        .item("02_DoubleBass.wav")
        .group(brass)
        .end();

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(percussion)
        .group(guitars)
        .group(keys)
        .group(horns_group)
        .group(vocals)
        .group(orchestra)
        .build();
    expected
}

#[test]
fn cherry_poppin_daddies_zoot_suit() {
    // -- Setup & Fixtures
    // Cherry Poppin' Daddies - Zoot Suit Riot: 11-stem swing/ska session from MedleyDB.
    // Horn section with trumpets and trombone, double bass, clean electric guitar, piano,
    // full drums, tambourine, lead vocals and backing vocals. Tests big band / swing
    // instrumentation with horn section classification.
    let items = vec![
        "01_Drums.wav",
        "02_DoubleBass.wav",
        "03_ElecGtr.wav",
        "04_Piano.wav",
        "05_Trumpet1.wav",
        "06_Trumpet2.wav",
        "07_Trombone.wav",
        "08_HornSection.wav",
        "09_Tambourine.wav",
        "10_LeadVox.wav",
        "11_BackingVox.wav",
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

    let expected = cherry_poppin_daddies_zoot_suit_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
