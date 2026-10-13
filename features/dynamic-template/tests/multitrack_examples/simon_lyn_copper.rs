use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn simon_lyn_copper_expected() -> daw_proto::TrackHierarchy {
    let sfx = TrackGroup::folder("SFX")
        .track("SFX 1")
        .item("12_SFX1.wav")
        .track("SFX 2")
        .item("13_SFX2.wav")
        .end();

    // --- Unsorted ---
    // All the orchestral string parts end up unsorted since "Pizz", "Arco",
    // "Theme", "Lyrical" are not recognized instrument keywords
    let unsorted = TrackGroup::folder("Unsorted")
        .track("Pizz1")
        .item("01_Pizz1.wav")
        .track("Pizz2")
        .item("02_Pizz2.wav")
        .track("Pizz3")
        .item("03_Pizz3.wav")
        .track("Pizz4")
        .item("04_Pizz4.wav")
        .track("Pizz5")
        .item("05_Pizz5.wav")
        .track("ArcoRhythm1")
        .item("06_ArcoRhythm1.wav")
        .track("ArcoRhythm2")
        .item("07_ArcoRhythm2.wav")
        .track("ArcoRhythm3")
        .item("08_ArcoRhythm3.wav")
        .track("ArcoRhythm4")
        .item("09_ArcoRhythm4.wav")
        .track("ArcoRhythm5")
        .item("10_ArcoRhythm5.wav")
        .track("ArcoRhythm6")
        .item("11_ArcoRhythm6.wav")
        .track("Theme")
        .item("14_Theme.wav")
        .track("ThemeDT")
        .item("15_ThemeDT.wav")
        .track("ThemeCounter")
        .item("16_ThemeCounter.wav")
        .track("Lyrical1")
        .item("17_Lyrical1.wav")
        .track("Lyrical2")
        .item("18_Lyrical2.wav")
        .track("Lyrical3")
        .item("19_Lyrical3.wav")
        .track("Lyrical3DT")
        .item("20_Lyrical3DT.wav")
        .track("Lyrical4")
        .item("21_Lyrical4.wav")
        .track("Lyrical4Octave")
        .item("22_Lyrical4Octave.wav")
        .track("Lyrical5")
        .item("23_Lyrical5.wav")
        .track("Lyrical5Octave")
        .item("24_Lyrical5Octave.wav")
        .track("Lyrical6")
        .item("25_Lyrical6.wav")
        .track("Lyrical6Octave")
        .item("26_Lyrical6Octave.wav")
        .end();

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(sfx)
        .group(unsorted)
        .build();
    expected
}

#[test]
fn simon_lyn_copper() {
    // -- Setup & Fixtures
    // Simon Lyn - Copper: 26-track purely orchestral string session. No drums, no vocals.
    // Pizzicato strings (5 tracks), arco rhythm section (6 tracks), theme/counter-theme
    // parts, lyrical melodic lines with octave doubles, SFX transitions, and doubled
    // theme tracks. Tests pure orchestral classification without typical rock instruments.
    // Most tracks end up unsorted since "Pizz", "Arco", "Theme", "Lyrical" etc. are
    // not standard instrument keywords. Cambridge-MT multitrack library.
    let items = vec![
        "01_Pizz1.wav",
        "02_Pizz2.wav",
        "03_Pizz3.wav",
        "04_Pizz4.wav",
        "05_Pizz5.wav",
        "06_ArcoRhythm1.wav",
        "07_ArcoRhythm2.wav",
        "08_ArcoRhythm3.wav",
        "09_ArcoRhythm4.wav",
        "10_ArcoRhythm5.wav",
        "11_ArcoRhythm6.wav",
        "12_SFX1.wav",
        "13_SFX2.wav",
        "14_Theme.wav",
        "15_ThemeDT.wav",
        "16_ThemeCounter.wav",
        "17_Lyrical1.wav",
        "18_Lyrical2.wav",
        "19_Lyrical3.wav",
        "20_Lyrical3DT.wav",
        "21_Lyrical4.wav",
        "22_Lyrical4Octave.wav",
        "23_Lyrical5.wav",
        "24_Lyrical5Octave.wav",
        "25_Lyrical6.wav",
        "26_Lyrical6Octave.wav",
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

    // --- SFX ---
    let expected = simon_lyn_copper_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
