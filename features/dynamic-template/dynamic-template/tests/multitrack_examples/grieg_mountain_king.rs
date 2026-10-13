use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn grieg_mountain_king_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::folder("Drums")
        .track("Toms")
        .item("16_Toms.wav")
        .track("Cymbals")
        .item("15_Cymbal.wav")
        .end();
    let percussion = TrackGroup::single_track("Percussion", "17_Timpani.wav");
    let horns = TrackGroup::single_track("Horns", "11_FrenchHornSection.wav");

    // Full orchestra
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

    let brass = TrackGroup::folder("Brass")
        .track("Trumpets")
        .item("13_TrumpetSection.wav")
        .track("Horns")
        .item("11_FrenchHornSection.wav")
        .track("Trombone")
        .item("12_Trombone.wav")
        .track("Tuba")
        .item("14_Tuba.wav")
        .end();

    let woodwinds = TrackGroup::folder("Woodwinds")
        .track("Piccolo")
        .item("06_Piccolo.wav")
        .track("Flutes")
        .item("07_Flute.wav")
        .track("Oboes")
        .item("10_Oboe.wav")
        .track("Clarinets")
        .item("09_Clarinet.wav")
        .track("Bassoons")
        .item("08_Bassoon.wav")
        .end();

    let orchestra = TrackGroup::folder("Orchestra")
        .group(strings)
        .group(brass)
        .group(woodwinds)
        .track("Percussion")
        .item("17_Timpani.wav")
        .end();

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(percussion)
        .group(horns)
        .group(orchestra)
        .build();
    expected
}

#[test]
fn grieg_mountain_king() {
    // -- Setup & Fixtures
    // MusicDelta - In The Hall of the Mountain King: 17-stem full orchestral arrangement
    // from MedleyDB. Complete classical orchestra: 2 violin sections, viola, cello,
    // double bass, piccolo, flute, bassoon, clarinet, oboe, french horn section,
    // trombone, trumpet section, tuba, cymbal, toms, and timpani. Tests comprehensive
    // orchestral classification with full woodwind, brass, and string sections.
    let items = vec![
        "01_Violin1.wav",
        "02_Violin2.wav",
        "03_Viola.wav",
        "04_Cello.wav",
        "05_DoubleBass.wav",
        "06_Piccolo.wav",
        "07_Flute.wav",
        "08_Bassoon.wav",
        "09_Clarinet.wav",
        "10_Oboe.wav",
        "11_FrenchHornSection.wav",
        "12_Trombone.wav",
        "13_TrumpetSection.wav",
        "14_Tuba.wav",
        "15_Cymbal.wav",
        "16_Toms.wav",
        "17_Timpani.wav",
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

    // Cymbal and Toms now both classified as Drums
    let expected = grieg_mountain_king_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
