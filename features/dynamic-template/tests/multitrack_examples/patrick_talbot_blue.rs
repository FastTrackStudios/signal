use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn patrick_talbot_blue_expected() -> daw_proto::TrackHierarchy {
    let kick_sum = TrackGroup::folder("SUM")
        .track("In")
        .item("01_KickIn.wav")
        .track("Out")
        .item("02_KickOut.wav")
        .end();

    let kick = TrackGroup::folder("Kick")
        .group(kick_sum)
        .track("Kick")
        .item("03_KickSub.wav")
        .end();

    let snare = TrackGroup::folder("Snare")
        .track("Snare 1")
        .item("04_SnareUp1.wav")
        .track("Snare 2")
        .item("05_SnareUp2.wav")
        .track("Down")
        .item("06_SnareDown.wav")
        .end();

    let toms = TrackGroup::folder("Toms")
        .track("T1")
        .item("07_Tom1.wav")
        .track("T2")
        .item("08_Tom2.wav")
        .track("T3")
        .item("09_Tom3.wav")
        .track("T4")
        .item("10_Tom4.wav")
        .end();

    let cymbals = TrackGroup::folder("Cymbals")
        .track("OH")
        .item("12_Overheads.wav")
        .track("Hi Hat")
        .item("11_HiHat.wav")
        .end();

    let rooms = TrackGroup::folder("Rooms")
        .track("Rooms 1")
        .item("13_DrumsRoom1.wav")
        .track("Rooms 2")
        .item("14_DrumsRoom2.wav")
        .end();

    let drums = TrackGroup::folder("Drums")
        .group(kick)
        .group(snare)
        .group(toms)
        .group(cymbals)
        .group(rooms)
        .end();

    // --- Percussion ---
    // Vibraphone classified as percussion
    let percussion = TrackGroup::single_track("Percussion", "18_Vibes.wav");

    // --- Bass ---
    let bass = TrackGroup::single_track("Bass", "15_Bass.wav");

    // --- Guitars ---
    let guitars = TrackGroup::folder("Guitars")
        .track("Electric 1")
        .item("16_ElecGtr1.wav")
        .track("Electric 2")
        .item("17_ElecGtr2.wav")
        .end();

    // --- Keys ---
    // Rhodes and toy piano
    let keys = TrackGroup::folder("Keys")
        .track("Piano")
        .item("20_ToyPiano.wav")
        .track("Rhodes")
        .item("19_Rhodes.wav")
        .end();

    // --- Vocals ---
    let bgvs = TrackGroup::folder("BGVs")
        .track("BGVs 1")
        .item("22_BackingVox1.wav")
        .track("BGVs 2")
        .item("23_BackingVox2.wav")
        .end();

    let vocals = TrackGroup::folder("Vocals")
        .track("Lead")
        .item("21_LeadVox.wav")
        .group(bgvs)
        .end();

    // --- Orchestra ---
    // Vibes also classified under orchestra
    let orchestra = TrackGroup::single_track("Orchestra", "18_Vibes.wav");

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
        .group(orchestra)
        .build();
    expected
}

#[test]
fn patrick_talbot_blue() {
    // -- Setup & Fixtures
    // Patrick Talbot - Blue: 23-track jazz-rock session with extensively mic'd drums
    // (3 kick mics: in/out/sub, 3 snare mics: up1/up2/down, 4 toms, dual room mics),
    // bass, 2 electric guitars, vibraphone, Rhodes electric piano, toy piano,
    // and vocals with backing. Cambridge-MT multitrack library.
    let items = vec![
        "01_KickIn.wav",
        "02_KickOut.wav",
        "03_KickSub.wav",
        "04_SnareUp1.wav",
        "05_SnareUp2.wav",
        "06_SnareDown.wav",
        "07_Tom1.wav",
        "08_Tom2.wav",
        "09_Tom3.wav",
        "10_Tom4.wav",
        "11_HiHat.wav",
        "12_Overheads.wav",
        "13_DrumsRoom1.wav",
        "14_DrumsRoom2.wav",
        "15_Bass.wav",
        "16_ElecGtr1.wav",
        "17_ElecGtr2.wav",
        "18_Vibes.wav",
        "19_Rhodes.wav",
        "20_ToyPiano.wav",
        "21_LeadVox.wav",
        "22_BackingVox1.wav",
        "23_BackingVox2.wav",
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
    // Extensive multi-mic kit
    let expected = patrick_talbot_blue_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
