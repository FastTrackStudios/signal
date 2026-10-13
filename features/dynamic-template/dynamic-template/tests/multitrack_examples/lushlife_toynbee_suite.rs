use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn lushlife_toynbee_suite_expected() -> daw_proto::TrackHierarchy {
    let ekit = TrackGroup::folder("Electronic Kit")
        .track("Electronic Kit 1")
        .item("02_DrumMachine1.wav")
        .track("Electronic Kit 2")
        .item("06_DrumMachine2.wav")
        .track("Electronic Kit 3")
        .item("22_DrumMachine3.wav")
        .end();

    let drums = TrackGroup::folder("Drums")
        .track("Drum Kit")
        .item("07_Drums.wav")
        .group(ekit)
        .end();

    // Glockenspiel dual-classified as percussion, plus claps now classified
    let percussion = TrackGroup::folder("Percussion")
        .track("Clap")
        .item("03_Claps.wav")
        .track("Orch")
        .item("09_Glockenspiel.wav")
        .end();

    // ElecBass now correctly classified as Bass
    let bass = TrackGroup::single_track("Bass", "08_ElecBass.wav");

    let guitars = TrackGroup::folder("Guitars")
        .track("Electric 1")
        .item("10_ElecGtr1.wav")
        .track("Electric 2")
        .item("11_ElecGtr2.wav")
        .track("Electric 3")
        .item("12_ElecGtr3.wav")
        .end();

    let keys = TrackGroup::single_track("Keys", "23_Piano.wav");

    let synths = TrackGroup::folder("Synths")
        .track("Synth1")
        .item("05_Synth1.wav")
        .track("Synth2")
        .item("16_Synth2.wav")
        .track("Synth3")
        .item("19_Synth3.wav")
        .track("Synth4")
        .item("21_Synth4.wav")
        .end();

    let horns = TrackGroup::single_track("Horns", "13_HornSection.wav");

    let vocals = TrackGroup::folder("Vocals")
        .track("Lead 1")
        .item("17_LeadVox1.wav")
        .track("Lead 2")
        .item("18_LeadVox2.wav")
        .track("Lead 3")
        .item("26_LeadVox3.wav")
        .end();

    let brass_folder = TrackGroup::folder("Brass")
        .track("Trumpets")
        .item("14_TrumpetSection.wav")
        .track("Horns")
        .item("13_HornSection.wav")
        .track("Brass")
        .item("01_BrassSection.wav")
        .end();

    let orchestra = TrackGroup::folder("Orchestra")
        .track("Strings")
        .item("24_Strings.wav")
        .group(brass_folder)
        .track("Clarinets")
        .item("04_Clarinet.wav")
        .track("Percussion")
        .item("09_Glockenspiel.wav")
        .end();

    // Scratches now classified as SFX
    let sfx = TrackGroup::folder("SFX")
        .track("FX1")
        .item("15_FX1.wav")
        .track("FX2")
        .item("20_FX2.wav")
        .track("Scratches")
        .item("25_Scratches.wav")
        .end();

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(percussion)
        .group(bass)
        .group(guitars)
        .group(keys)
        .group(synths)
        .group(horns)
        .group(vocals)
        .group(orchestra)
        .group(sfx)
        .build();
    expected
}

#[test]
fn lushlife_toynbee_suite() {
    // -- Setup & Fixtures
    // Lushlife - Toynbee Suite: 26-stem hip-hop/jazz fusion session from MedleyDB.
    // Brass section, 3 drum machines, live drums, claps, clarinet, glockenspiel,
    // 3 clean electric guitars, horn section, trumpet section, FX sounds, 4 synths,
    // 3 male rapper takes, piano, string section, and DJ scratches.
    // Tests massive cross-genre session with orchestral + electronic + hip-hop.
    let items = vec![
        "01_BrassSection.wav",
        "02_DrumMachine1.wav",
        "03_Claps.wav",
        "04_Clarinet.wav",
        "05_Synth1.wav",
        "06_DrumMachine2.wav",
        "07_Drums.wav",
        "08_ElecBass.wav",
        "09_Glockenspiel.wav",
        "10_ElecGtr1.wav",
        "11_ElecGtr2.wav",
        "12_ElecGtr3.wav",
        "13_HornSection.wav",
        "14_TrumpetSection.wav",
        "15_FX1.wav",
        "16_Synth2.wav",
        "17_LeadVox1.wav",
        "18_LeadVox2.wav",
        "19_Synth3.wav",
        "20_FX2.wav",
        "21_Synth4.wav",
        "22_DrumMachine3.wav",
        "23_Piano.wav",
        "24_Strings.wav",
        "25_Scratches.wav",
        "26_LeadVox3.wav",
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

    let expected = lushlife_toynbee_suite_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
