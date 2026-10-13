use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn adam_buckley_drag_me_down_expected_leaves() -> (TrackGroup, TrackGroup, TrackGroup) {
    let kick = TrackGroup::folder("Kick")
        .track("Kick 1")
        .item("01_Kick1.wav")
        .track("Kick 2")
        .item("02_Kick2.wav")
        .track("Kick 3")
        .item("03_KickSample.wav")
        .end();

    let snare_trig = TrackGroup::folder("Trig")
        .track("Trig 1")
        .item("06_SnareSample1.wav")
        .track("Trig 2")
        .item("07_SnareSample2.wav")
        .end();

    let snare = TrackGroup::folder("Snare")
        .group(snare_trig)
        .track("Snare")
        .item("04_SnareUp.wav")
        .track("Down")
        .item("05_SnareDown.wav")
        .end();

    let tom1 = TrackGroup::folder("T1")
        .track("Tom 1")
        .item("10_Tom1.wav")
        .track("Tom 2")
        .item("11_Tom1Sample.wav")
        .end();

    let tom2 = TrackGroup::folder("T2")
        .track("Tom 1")
        .item("12_Tom2.wav")
        .track("Tom 2")
        .item("13_Tom2Sample.wav")
        .end();

    let tom3 = TrackGroup::folder("T3")
        .track("Tom 1")
        .item("14_Tom3.wav")
        .track("Tom 2")
        .item("15_Tom3Sample.wav")
        .end();

    let toms_folder = TrackGroup::folder("Toms")
        .group(tom1)
        .group(tom2)
        .group(tom3)
        .end();

    let cymbals = TrackGroup::folder("Cymbals")
        .track("OH")
        .item("16_Overheads.wav")
        .track("Hi Hat 1")
        .item("08_HiHat.wav")
        .track("Hi Hat 2")
        .item("09_HiHatSample.wav")
        .end();

    let drums = TrackGroup::folder("Drums")
        .group(kick)
        .group(snare)
        .group(toms_folder)
        .group(cymbals)
        .track("Rooms")
        .item("17_DrumsRoom.wav")
        .end();

    // --- Bass ---
    // DI + 3 effect variants (overdrive, fuzz, synth)
    let bass = TrackGroup::folder("Bass")
        .track("Bass 1")
        .item("18_BassDI.wav")
        .track("Overdrive")
        .item("19_BassOverdrive.wav")
        .track("Fuzz")
        .item("20_BassFuzz.wav")
        .track("Bass 2")
        .item("21_BassSynth.wav")
        .end();

    // --- Guitars ---
    // 5 electric guitars with DI/DT variants + 3 acoustic
    let electric = TrackGroup::folder("Electric")
        .track("Electric 1")
        .item("25_ElecGtr1.wav")
        .track("Electric 2")
        .item("26_ElecGtr2.wav")
        .track("Electric 3")
        .item("27_ElecGtr2DI.wav")
        .track("Electric 4")
        .item("28_ElecGtr2DT.wav")
        .track("Electric 5")
        .item("29_ElecGtr2DTDI.wav")
        .track("Electric 6")
        .item("30_ElecGtr3.wav")
        .track("Electric 7")
        .item("31_ElecGtr3DI.wav")
        .track("Electric 8")
        .item("32_ElecGtr4.wav")
        .track("Electric 9")
        .item("33_ElecGtr4DI.wav")
        .track("Electric 10")
        .item("34_ElecGtr5.wav")
        .track("Electric 11")
        .item("35_ElecGtr5DI.wav")
        .end();

    (drums, bass, electric)
}

fn adam_buckley_drag_me_down_expected() -> daw_proto::TrackHierarchy {
    let (drums, bass, electric) = adam_buckley_drag_me_down_expected_leaves();
    let acoustic = TrackGroup::folder("Acoustic")
        .track("Acoustic 1")
        .item("22_AcousticGtr1.wav")
        .track("Acoustic 2")
        .item("23_AcousticGtr2.wav")
        .track("Acoustic 3")
        .item("24_AcousticGtr3.wav")
        .end();

    let guitars = TrackGroup::folder("Guitars")
        .group(electric)
        .group(acoustic)
        .end();

    // --- Keys ---
    let keys = TrackGroup::single_track("Keys", "40_Piano.wav");

    // --- Synths ---
    let synths = TrackGroup::folder("Synths")
        .track("Synth1")
        .item("36_Synth1.wav")
        .track("Synth2")
        .item("37_Synth2.wav")
        .track("Synth3")
        .item("38_Synth3.wav")
        .track("Synth4")
        .item("39_Synth4.wav")
        .end();

    // --- Vocals ---
    let lead = TrackGroup::folder("Lead")
        .track("Lead 1")
        .item("43_LeadVox1.wav")
        .track("Lead 2")
        .item("44_LeadVox1Harm.wav")
        .track("Lead 3")
        .item("45_LeadVox2.wav")
        .end();

    let bgvs = TrackGroup::folder("BGVs")
        .track("BGVs 1")
        .item("46_BackingVox1.wav")
        .track("BGVs 2")
        .item("47_BackingVox1DT.wav")
        .track("BGVs 3")
        .item("48_BackingVox2.wav")
        .track("BGVs 4")
        .item("49_BackingVox2DT.wav")
        .track("BGVs 5")
        .item("50_BackingVox3.wav")
        .end();

    let vocals = TrackGroup::folder("Vocals").group(lead).group(bgvs).end();

    // --- SFX ---
    let sfx = TrackGroup::folder("SFX")
        .track("SFX 1")
        .item("41_SFX1.wav")
        .track("SFX 2")
        .item("42_SFX2.wav")
        .end();

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(bass)
        .group(guitars)
        .group(keys)
        .group(synths)
        .group(vocals)
        .group(sfx)
        .build();
    expected
}

#[test]
fn adam_buckley_drag_me_down() {
    // -- Setup & Fixtures
    // Adam Buckley - Drag Me Down: 50-track alternative rock session with augmented drums
    // (live multi-mic kit + sample triggers for kick/snare/hihats/toms), extensive bass
    // variants (DI, overdrive, fuzz, synth bass), 3 acoustic guitars, 5 electric guitars
    // (with DI and double-tracked variants), 4 synths, piano, SFX, multiple lead vocal
    // takes with harmony, and 3 backing vocal pairs (with doubles).
    // Cambridge-MT multitrack library.
    let items = vec![
        "01_Kick1.wav",
        "02_Kick2.wav",
        "03_KickSample.wav",
        "04_SnareUp.wav",
        "05_SnareDown.wav",
        "06_SnareSample1.wav",
        "07_SnareSample2.wav",
        "08_HiHat.wav",
        "09_HiHatSample.wav",
        "10_Tom1.wav",
        "11_Tom1Sample.wav",
        "12_Tom2.wav",
        "13_Tom2Sample.wav",
        "14_Tom3.wav",
        "15_Tom3Sample.wav",
        "16_Overheads.wav",
        "17_DrumsRoom.wav",
        "18_BassDI.wav",
        "19_BassOverdrive.wav",
        "20_BassFuzz.wav",
        "21_BassSynth.wav",
        "22_AcousticGtr1.wav",
        "23_AcousticGtr2.wav",
        "24_AcousticGtr3.wav",
        "25_ElecGtr1.wav",
        "26_ElecGtr2.wav",
        "27_ElecGtr2DI.wav",
        "28_ElecGtr2DT.wav",
        "29_ElecGtr2DTDI.wav",
        "30_ElecGtr3.wav",
        "31_ElecGtr3DI.wav",
        "32_ElecGtr4.wav",
        "33_ElecGtr4DI.wav",
        "34_ElecGtr5.wav",
        "35_ElecGtr5DI.wav",
        "36_Synth1.wav",
        "37_Synth2.wav",
        "38_Synth3.wav",
        "39_Synth4.wav",
        "40_Piano.wav",
        "41_SFX1.wav",
        "42_SFX2.wav",
        "43_LeadVox1.wav",
        "44_LeadVox1Harm.wav",
        "45_LeadVox2.wav",
        "46_BackingVox1.wav",
        "47_BackingVox1DT.wav",
        "48_BackingVox2.wav",
        "49_BackingVox2DT.wav",
        "50_BackingVox3.wav",
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
    // Live multi-mic kit augmented with sample triggers
    let expected = adam_buckley_drag_me_down_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
