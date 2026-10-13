use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn moosmusic_funk_expected_leaves() -> (TrackGroup, TrackGroup) {
    let kick_sum = TrackGroup::folder("SUM")
        .track("In")
        .item("04_KickIn.wav")
        .track("Out")
        .item("05_KickOut.wav")
        .end();

    let kick = TrackGroup::folder("Kick")
        .group(kick_sum)
        .track("Kick")
        .item("01_KickSample.wav")
        .end();

    let snare = TrackGroup::folder("Snare")
        .track("Trig")
        .item("02_SnareSample.wav")
        .track("Down")
        .item("06_SnareDown.wav")
        .track("Snare")
        .item("07_SnareUp.wav")
        .end();

    let hihat = TrackGroup::folder("Hi Hat")
        .track("Hi Hat 1")
        .item("03_HiHatSample.wav")
        .track("Hi Hat 2")
        .item("08_HiHat.wav")
        .end();

    let cymbals = TrackGroup::folder("Cymbals")
        .group(hihat)
        .track("OH")
        .item("09_Overheads.wav")
        .track("Cymbals")
        .item("13_Cymbal.wav")
        .track("Rev")
        .item("19_RevCymbal.wav")
        .end();

    let rooms = TrackGroup::folder("Rooms")
        .track("Rooms 1")
        .item("10_DrumsRoom1.wav")
        .track("Rooms 2")
        .item("11_DrumsRoom2.wav")
        .end();

    let drums = TrackGroup::folder("Drums")
        .group(kick)
        .group(snare)
        .track("Toms")
        .item("12_Tom.wav")
        .group(cymbals)
        .group(rooms)
        .end();

    // --- Percussion ---
    // Full percussion section: shaker, tambourine, cowbell, woodblock, claps
    let percussion = TrackGroup::folder("Percussion")
        .track("Shaker")
        .item("16_Shaker.wav")
        .track("Tambourine")
        .item("17_Tambourine.wav")
        .track("Cowbell")
        .item("15_Cowbell.wav")
        .track("Woodblock")
        .item("14_Woodblock.wav")
        .track("Clap")
        .item("18_Claps.wav")
        .end();

    // --- Bass ---
    (drums, percussion)
}

fn moosmusic_funk_expected() -> daw_proto::TrackHierarchy {
    let (drums, percussion) = moosmusic_funk_expected_leaves();
    let bass = TrackGroup::single_track("Bass", "20_BassDI.wav");

    // --- Guitars ---
    // 7 electric guitars
    let guitars = TrackGroup::folder("Guitars")
        .track("Electric 1")
        .item("22_ElecGtr1.wav")
        .track("Electric 2")
        .item("23_ElecGtr2.wav")
        .track("Electric 3")
        .item("24_ElecGtr3.wav")
        .track("Electric 4")
        .item("25_ElecGtr4.wav")
        .track("Electric 5")
        .item("26_ElecGtr5.wav")
        .track("Electric 6")
        .item("27_ElecGtr6.wav")
        .track("Electric 7")
        .item("28_ElecGtr7.wav")
        .end();

    // --- Keys ---
    let keys = TrackGroup::single_track("Keys", "21_Rhodes.wav");

    // --- Vocals ---
    let bgvs = TrackGroup::folder("BGVs")
        .track("BGVs 1")
        .item("30_BackingVox1.wav")
        .track("BGVs 2")
        .item("31_BackingVox2.wav")
        .track("BGVs 3")
        .item("32_BackingVox3.wav")
        .track("BGVs 4")
        .item("33_BackingVox4.wav")
        .track("BGVs 5")
        .item("34_BackingVox5.wav")
        .end();

    let vocals = TrackGroup::folder("Vocals")
        .track("Lead")
        .item("29_LeadVox.wav")
        .group(bgvs)
        .end();

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(percussion)
        .group(bass)
        .group(guitars)
        .group(keys)
        .group(vocals)
        .build();
    expected
}

#[test]
fn moosmusic_funk() {
    // -- Setup & Fixtures
    // MR0903 Moosmusic: 34-track funk/soul session with extensive drum setup (samples +
    // live multi-mic kit + dual room mics), full percussion section (woodblock, cowbell,
    // shaker, tambourine, claps), bass DI, Rhodes electric piano, 7 electric guitars,
    // and a vocal stack with 5 backing vocals. Cambridge-MT multitrack library.
    let items = vec![
        "01_KickSample.wav",
        "02_SnareSample.wav",
        "03_HiHatSample.wav",
        "04_KickIn.wav",
        "05_KickOut.wav",
        "06_SnareDown.wav",
        "07_SnareUp.wav",
        "08_HiHat.wav",
        "09_Overheads.wav",
        "10_DrumsRoom1.wav",
        "11_DrumsRoom2.wav",
        "12_Tom.wav",
        "13_Cymbal.wav",
        "14_Woodblock.wav",
        "15_Cowbell.wav",
        "16_Shaker.wav",
        "17_Tambourine.wav",
        "18_Claps.wav",
        "19_RevCymbal.wav",
        "20_BassDI.wav",
        "21_Rhodes.wav",
        "22_ElecGtr1.wav",
        "23_ElecGtr2.wav",
        "24_ElecGtr3.wav",
        "25_ElecGtr4.wav",
        "26_ElecGtr5.wav",
        "27_ElecGtr6.wav",
        "28_ElecGtr7.wav",
        "29_LeadVox.wav",
        "30_BackingVox1.wav",
        "31_BackingVox2.wav",
        "32_BackingVox3.wav",
        "33_BackingVox4.wav",
        "34_BackingVox5.wav",
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
    // Multi-mic kit with samples, overheads, dual room mics
    let expected = moosmusic_funk_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
