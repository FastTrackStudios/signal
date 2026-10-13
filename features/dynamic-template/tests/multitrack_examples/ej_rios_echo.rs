use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn ej_rios_echo_expected_leaves() -> (
    TrackGroup,
    TrackGroup,
    TrackGroup,
    TrackGroup,
    TrackGroup,
    TrackGroup,
) {
    let kick = TrackGroup::folder("Kick")
        .track("Kick 1")
        .item("01_Kick1.wav")
        .track("Kick 2")
        .item("02_Kick2.wav")
        .track("Kick 3")
        .item("03_Kick3.wav")
        .end();

    let snare = TrackGroup::folder("Snare")
        .track("Snare 1")
        .item("04_Snare1.wav")
        .track("Snare 2")
        .item("05_Snare2.wav")
        .end();

    let toms = TrackGroup::folder("Toms")
        .track("T1")
        .item("17_DrumkitTom1.wav")
        .track("T2")
        .item("18_DrumkitTom2.wav")
        .track("Tom")
        .item("10_Tom.wav")
        .end();

    let hihat = TrackGroup::folder("Hi Hat")
        .track("Hi Hat 1")
        .item("07_HiHat1.wav")
        .track("Hi Hat 2")
        .item("08_HiHat2.wav")
        .track("Hi Hat 3")
        .item("09_HiHat3.wav")
        .end();

    let crash = TrackGroup::folder("Crash")
        .track("Crash 1")
        .item("11_Crash.wav")
        .track("Crash 2")
        .item("21_Crash.wav")
        .end();

    let cymbals = TrackGroup::folder("Cymbals")
        .group(hihat)
        .group(crash)
        .track("OH")
        .item("15_DrumkitOverheads.wav")
        .end();

    let drums = TrackGroup::folder("Drums")
        .group(kick)
        .group(snare)
        .group(toms)
        .group(cymbals)
        .track("Rooms")
        .item("16_DrumkitRoom.wav")
        .end();

    // --- Percussion ---
    // Clap and timpani (classified as orchestral percussion)
    let percussion = TrackGroup::folder("Percussion")
        .track("Clap")
        .item("06_Clap.wav")
        .track("Orch 1")
        .item("19_Timpani1.wav")
        .track("Orch 2")
        .item("20_Timpani2.wav")
        .end();

    // --- Bass ---
    let bass = TrackGroup::folder("Bass")
        .track("Bass 1")
        .item("23_Bass1.wav")
        .track("Bass 2")
        .item("24_Bass2.wav")
        .track("Bass 3")
        .item("25_Bass3.wav")
        .end();

    // --- Guitars ---
    // 4 electric guitars (2 with doubles)
    let guitars = TrackGroup::folder("Guitars")
        .track("Electric 1")
        .item("26_ElecGtr1.wav")
        .track("Electric 2")
        .item("27_ElecGtr1DT.wav")
        .track("Electric 3")
        .item("28_ElecGtr2.wav")
        .track("Electric 4")
        .item("29_ElecGtr2DT.wav")
        .track("Electric 5")
        .item("30_ElecGtr3.wav")
        .track("Electric 6")
        .item("31_ElecGtr4.wav")
        .end();

    // --- Keys ---
    let keys = TrackGroup::single_track("Keys", "38_Piano.wav");

    // --- Synths ---
    let synths = TrackGroup::folder("Synths")
        .track("Synth1")
        .item("32_Synth1.wav")
        .track("Synth2")
        .item("33_Synth2.wav")
        .track("Synth3")
        .item("34_Synth3.wav")
        .track("Synth4")
        .item("35_Synth4.wav")
        .track("Synth5")
        .item("36_Synth5.wav")
        .track("Synth6")
        .item("37_Synth6.wav")
        .end();

    // --- Vocals ---
    (drums, percussion, bass, guitars, keys, synths)
}

fn ej_rios_echo_expected() -> daw_proto::TrackHierarchy {
    let (drums, percussion, bass, guitars, keys, synths) = ej_rios_echo_expected_leaves();
    let lead = TrackGroup::folder("Lead")
        .track("Lead 1")
        .item("43_LeadVox1.wav")
        .track("Lead 2")
        .item("44_LeadVox1DT1.wav")
        .track("Lead 3")
        .item("45_LeadVox1DT2.wav")
        .track("Lead 4")
        .item("46_LeadVox1SFX.wav")
        .track("Lead 5")
        .item("47_LeadVox2.wav")
        .track("Lead 6")
        .item("48_LeadVox2DT1.wav")
        .track("Lead 7")
        .item("49_LeadVox2DT2.wav")
        .track("Lead 8")
        .item("50_LeadVox2AdLibs.wav")
        .end();

    let bgvs = TrackGroup::folder("BGVs")
        .track("BGVs 1")
        .item("51_BackingVox1.wav")
        .track("BGVs 2")
        .item("52_BackingVox1DT.wav")
        .track("BGVs 3")
        .item("53_BackingVox2.wav")
        .track("BGVs 4")
        .item("54_BackingVox2DT.wav")
        .track("BGVs 5")
        .item("55_BackingVox3.wav")
        .track("BGVs 6")
        .item("56_BackingVox3DT.wav")
        .track("BGVs 7")
        .item("57_BackingVox4.wav")
        .track("BGVs 8")
        .item("58_BackingVox4DT.wav")
        .track("BGVs 9")
        .item("59_BackingVox5.wav")
        .track("BGVs 10")
        .item("60_BackingVox6.wav")
        .track("BGVs 11")
        .item("61_BackingVox7.wav")
        .track("BGVs 12")
        .item("62_BackingVox8.wav")
        .track("BGVs 13")
        .item("63_BackingVox9.wav")
        .end();

    let vocals = TrackGroup::folder("Vocals").group(lead).group(bgvs).end();

    // --- Orchestra ---
    // Strings section: violins, cellos, pizzicato strings; plus timpani as orchestral perc
    let cello = TrackGroup::folder("Cello")
        .track("Cello 1")
        .item("40_Cellos1.wav")
        .track("Cello 2")
        .item("41_Cellos2.wav")
        .end();

    let strings = TrackGroup::folder("Strings")
        .track("Violins")
        .item("39_Violins.wav")
        .group(cello)
        .track("Strings")
        .item("42_StringsPizz.wav")
        .end();

    let orch_perc = TrackGroup::folder("Percussion")
        .track("Percussion 1")
        .item("19_Timpani1.wav")
        .track("Percussion 2")
        .item("20_Timpani2.wav")
        .end();

    let orchestra = TrackGroup::folder("Orchestra")
        .group(strings)
        .group(orch_perc)
        .end();

    // --- SFX ---
    let sfx = TrackGroup::folder("SFX")
        .track("Impact1")
        .item("12_Impact1.wav")
        .track("Impact2")
        .item("13_Impact2.wav")
        .track("SFX")
        .item("22_SFX.wav")
        .end();

    // --- Unsorted ---
    // DrumkitClose not classified
    let unsorted = TrackGroup::single_track("Unsorted", "14_DrumkitClose.wav");

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(percussion)
        .group(bass)
        .group(guitars)
        .group(keys)
        .group(synths)
        .group(vocals)
        .group(orchestra)
        .group(sfx)
        .group(unsorted)
        .build();
    expected
}

#[test]
fn ej_rios_echo() {
    // -- Setup & Fixtures
    // EJ Rios - Echo: Massive 63-track orchestral rock session. Multi-layered drums
    // (close kit + overheads + room + timpani), 3 bass tracks, 4 electric guitars (with
    // doubles), 6 synths, piano, full string section (violins, cellos, pizzicato),
    // multiple lead vocal takes with doubles and ad-libs, and 9+ backing vocals.
    // Cambridge-MT multitrack library.
    let items = vec![
        "01_Kick1.wav",
        "02_Kick2.wav",
        "03_Kick3.wav",
        "04_Snare1.wav",
        "05_Snare2.wav",
        "06_Clap.wav",
        "07_HiHat1.wav",
        "08_HiHat2.wav",
        "09_HiHat3.wav",
        "10_Tom.wav",
        "11_Crash.wav",
        "12_Impact1.wav",
        "13_Impact2.wav",
        "14_DrumkitClose.wav",
        "15_DrumkitOverheads.wav",
        "16_DrumkitRoom.wav",
        "17_DrumkitTom1.wav",
        "18_DrumkitTom2.wav",
        "19_Timpani1.wav",
        "20_Timpani2.wav",
        "21_Crash.wav",
        "22_SFX.wav",
        "23_Bass1.wav",
        "24_Bass2.wav",
        "25_Bass3.wav",
        "26_ElecGtr1.wav",
        "27_ElecGtr1DT.wav",
        "28_ElecGtr2.wav",
        "29_ElecGtr2DT.wav",
        "30_ElecGtr3.wav",
        "31_ElecGtr4.wav",
        "32_Synth1.wav",
        "33_Synth2.wav",
        "34_Synth3.wav",
        "35_Synth4.wav",
        "36_Synth5.wav",
        "37_Synth6.wav",
        "38_Piano.wav",
        "39_Violins.wav",
        "40_Cellos1.wav",
        "41_Cellos2.wav",
        "42_StringsPizz.wav",
        "43_LeadVox1.wav",
        "44_LeadVox1DT1.wav",
        "45_LeadVox1DT2.wav",
        "46_LeadVox1SFX.wav",
        "47_LeadVox2.wav",
        "48_LeadVox2DT1.wav",
        "49_LeadVox2DT2.wav",
        "50_LeadVox2AdLibs.wav",
        "51_BackingVox1.wav",
        "52_BackingVox1DT.wav",
        "53_BackingVox2.wav",
        "54_BackingVox2DT.wav",
        "55_BackingVox3.wav",
        "56_BackingVox3DT.wav",
        "57_BackingVox4.wav",
        "58_BackingVox4DT.wav",
        "59_BackingVox5.wav",
        "60_BackingVox6.wav",
        "61_BackingVox7.wav",
        "62_BackingVox8.wav",
        "63_BackingVox9.wav",
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
    let expected = ej_rios_echo_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
