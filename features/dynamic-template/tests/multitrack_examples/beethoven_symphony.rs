use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn beethoven_symphony_expected() -> daw_proto::TrackHierarchy {
    let percussion = TrackGroup::single_track("Percussion", "18_Timpani.wav");

    // --- Horns ---
    // French horns classified under top-level Horns
    let horns = TrackGroup::folder("Horns")
        .track("Orch 1")
        .item("16_FrenchHorn1.wav")
        .track("Orch 2")
        .item("17_FrenchHorn2.wav")
        .end();

    // --- Orchestra ---
    let violins = TrackGroup::folder("Violins")
        .track("First Violin")
        .item("01_Violin1.wav")
        .track("Second Violin")
        .item("02_Violin2.wav")
        .end();

    let strings = TrackGroup::folder("Strings")
        .group(violins)
        .track("Viola")
        .item("03_Viola.wav")
        .track("Cello")
        .item("04_Cello.wav")
        .track("Contrabass")
        .item("05_DoubleBass.wav")
        .end();

    let trumpets = TrackGroup::folder("Trumpets")
        .track("Trumpets 1")
        .item("14_Trumpet1.wav")
        .track("Trumpets 2")
        .item("15_Trumpet2.wav")
        .end();

    let brass_horns = TrackGroup::folder("Horns")
        .track("Horns 1")
        .item("16_FrenchHorn1.wav")
        .track("Horns 2")
        .item("17_FrenchHorn2.wav")
        .end();

    let brass = TrackGroup::folder("Brass")
        .group(trumpets)
        .group(brass_horns)
        .end();

    let flutes = TrackGroup::folder("Flutes")
        .track("Flutes 1")
        .item("06_Flute1.wav")
        .track("Flutes 2")
        .item("07_Flute2.wav")
        .end();

    let oboes = TrackGroup::folder("Oboes")
        .track("Oboes 1")
        .item("10_Oboe1.wav")
        .track("Oboes 2")
        .item("11_Oboe2.wav")
        .end();

    let clarinets = TrackGroup::folder("Clarinets")
        .track("Clarinets 1")
        .item("08_Clarinet1.wav")
        .track("Clarinets 2")
        .item("09_Clarinet2.wav")
        .end();

    let bassoons = TrackGroup::folder("Bassoons")
        .track("Bassoons 1")
        .item("12_Bassoon1.wav")
        .track("Bassoons 2")
        .item("13_Bassoon2.wav")
        .end();

    let woodwinds = TrackGroup::folder("Woodwinds")
        .group(flutes)
        .group(oboes)
        .group(clarinets)
        .group(bassoons)
        .end();

    let orchestra = TrackGroup::folder("Orchestra")
        .group(strings)
        .group(brass)
        .group(woodwinds)
        .track("Percussion")
        .item("18_Timpani.wav")
        .end();

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(percussion)
        .group(horns)
        .group(orchestra)
        .build();
    expected
}

#[test]
fn beethoven_symphony() {
    // -- Setup & Fixtures
    // MusicDelta Beethoven: 18-stem full orchestral arrangement from the MedleyDB dataset.
    // Complete classical orchestra: 2 violin sections, viola, cello, double bass, 2 flutes,
    // 2 clarinets, 2 oboes, 2 bassoons, 2 trumpets, 2 french horns, and timpani.
    // Tests pure orchestral classification with no modern instruments.
    let items = vec![
        "01_Violin1.wav",
        "02_Violin2.wav",
        "03_Viola.wav",
        "04_Cello.wav",
        "05_DoubleBass.wav",
        "06_Flute1.wav",
        "07_Flute2.wav",
        "08_Clarinet1.wav",
        "09_Clarinet2.wav",
        "10_Oboe1.wav",
        "11_Oboe2.wav",
        "12_Bassoon1.wav",
        "13_Bassoon2.wav",
        "14_Trumpet1.wav",
        "15_Trumpet2.wav",
        "16_FrenchHorn1.wav",
        "17_FrenchHorn2.wav",
        "18_Timpani.wav",
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

    // --- Percussion ---
    // Timpani dual-classified as percussion
    let expected = beethoven_symphony_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
