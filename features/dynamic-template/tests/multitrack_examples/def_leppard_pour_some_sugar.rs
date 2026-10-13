use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn def_leppard_pour_some_sugar_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::folder("Drums")
        .track("Snare")
        .item("03_snareloop.wav")
        .track("Drum Kit 1")
        .item("01_Drums1.wav")
        .track("Drum Kit 2")
        .item("02_Drum2.wav")
        .end();

    // --- Bass ---
    // Synth bass
    let bass = TrackGroup::single_track("Bass", "04_SynthBass.wav");

    // --- Guitars ---
    // 13 guitar tracks with various arrangement roles
    let guitars = TrackGroup::folder("Guitars")
        .track("Electric 1")
        .item("05_Gtr1_hatloop.wav")
        .track("Electric 2")
        .item("06_Gtr2_kickloop.wav")
        .track("Electric 3")
        .item("07_Gtr_edit.wav")
        .track("Electric 4")
        .item("08_Gtr1_Riff1.wav")
        .track("Electric 5")
        .item("09_Gtr1_Riff2.wav")
        .track("Electric 6")
        .item("10_Gtr_Rhy1.wav")
        .track("Electric 7")
        .item("11_Gtr_Rhy2.wav")
        .track("Electric 8")
        .item("12_Gtr2_Rhy1.wav")
        .track("Electric 9")
        .item("13_Gtr2_Rhy2.wav")
        .track("Electric 10")
        .item("14_Gtr3_Rhy1.wav")
        .track("Electric 11")
        .item("15_Gtr3_Rhy2.wav")
        .track("Electric 12")
        .item("16_Gtr_Vox1.wav")
        .track("Electric 13")
        .item("17_Gtr_Vox2.wav")
        .end();

    // --- Vocals ---
    // Chorus vocals grouped under Lead, plus standalone Vox and Lead Vox tracks
    let chorus = TrackGroup::folder("Chorus")
        .track("Lead 1")
        .item("18_Vox_Chorus1.wav")
        .track("Lead 2")
        .item("19_Vox_Chorus2.wav")
        .end();

    let vocals = TrackGroup::folder("Vocals")
        .group(chorus)
        .track("Vocals")
        .item("22_Lead_Vox.wav")
        .track("Vocals 1")
        .item("20_Vox_1.wav")
        .track("Vocals 2")
        .item("21_Vox_2.wav")
        .end();

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(bass)
        .group(guitars)
        .group(vocals)
        .build();
    expected
}

#[test]
fn def_leppard_pour_some_sugar() {
    // -- Setup & Fixtures
    // Def Leppard - Pour Some Sugar On Me: 22-track 80s rock session with unusual naming
    // conventions. Multi-layered guitars with descriptive suffixes (hatloop, kickloop, Riff,
    // Rhy, Vox-guitar), synth bass, snare loop, and split vocal tracks (Lead, Chorus, Vox).
    let items = vec![
        "01_Drums1.wav",
        "02_Drum2.wav",
        "03_snareloop.wav",
        "04_SynthBass.wav",
        "05_Gtr1_hatloop.wav",
        "06_Gtr2_kickloop.wav",
        "07_Gtr_edit.wav",
        "08_Gtr1_Riff1.wav",
        "09_Gtr1_Riff2.wav",
        "10_Gtr_Rhy1.wav",
        "11_Gtr_Rhy2.wav",
        "12_Gtr2_Rhy1.wav",
        "13_Gtr2_Rhy2.wav",
        "14_Gtr3_Rhy1.wav",
        "15_Gtr3_Rhy2.wav",
        "16_Gtr_Vox1.wav",
        "17_Gtr_Vox2.wav",
        "18_Vox_Chorus1.wav",
        "19_Vox_Chorus2.wav",
        "20_Vox_1.wav",
        "21_Vox_2.wav",
        "22_Lead_Vox.wav",
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

    // --- Drums ---
    // Two drum bus prints plus snare loop (classified via "snareloop" pattern)
    let expected = def_leppard_pour_some_sugar_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
