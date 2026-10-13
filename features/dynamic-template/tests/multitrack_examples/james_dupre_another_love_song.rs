use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn james_dupre_another_love_song_expected_leaves() -> (TrackGroup, TrackGroup, TrackGroup) {
    let kick = TrackGroup::folder("Kick")
        .track("SUM")
        .item("02.Kick Out_01.wav")
        .track("Kick 1")
        .item("01.Kick_01.wav")
        .track("Kick 2")
        .item("03.Kick Sample_01.wav")
        .end();

    // Snare: Bottom mic + "Sample Stereo" items aliased to Trig subfolder + bare snare
    let snare = TrackGroup::folder("Snare")
        .track("Bottom")
        .item("07.Snare Bottom_01.wav")
        .folder("Trig")
        .track("Trig 1")
        .item("05.Snare Sample Stereo_01.wav")
        .track("Trig 2")
        .item("06.Snare Sample Stereo 2_01.wav")
        .end()
        .track("Snare")
        .item("04.Snare _01.wav")
        .end();

    let toms = TrackGroup::folder("Toms")
        .track("T1")
        .item("09.Tom 1_01.wav")
        .track("T2")
        .item("10.Tom 2_01.wav")
        .end();

    let cymbals = TrackGroup::folder("Cymbals")
        .track("OH")
        .item("11.OH_01.wav")
        .track("Hi Hat")
        .item("08.Hat_01.wav")
        .end();

    let rooms = TrackGroup::folder("Rooms")
        .track("Mono")
        .item("13.Room Mono_01.wav")
        .track("Stereo")
        .item("12.Room Stereo_01.wav")
        .end();

    // Drum Kit collapsed (single child of Drums), so subgroups are direct children of Drums
    let drums = TrackGroup::folder("Drums")
        .group(kick)
        .group(snare)
        .group(toms)
        .group(cymbals)
        .group(rooms)
        .track("Drum Kit")
        .item("14.Kit Mono_01.wav")
        .end();

    // --- Bass ---
    let bass = TrackGroup::single_track("Bass", "15.Bass_01.wav");

    // --- Guitars ---
    let electric = TrackGroup::folder("Electric")
        .track("Electric 1")
        .item("18.EG2 (57)_01.wav")
        .track("Electric 2")
        .item("19.EG2 (FH)_01.wav")
        .track("Electric 3")
        .item("20.EG1 (57)_01.wav")
        .track("Electric 4")
        .item("21.EG1 (FH)_01.wav")
        .track("Electric 5")
        .item("22.EG3 (57)_01.wav")
        .track("Electric 6")
        .item("23.EG3 (FH)_01.wav")
        .end();

    (drums, bass, electric)
}

fn james_dupre_another_love_song_expected() -> daw_proto::TrackHierarchy {
    let (drums, bass, electric) = james_dupre_another_love_song_expected_leaves();
    let guitars = TrackGroup::folder("Guitars")
        .group(electric)
        .track("Acoustic")
        .item("16.AG1_01.wav")
        .track("Steel")
        .item("24.Steel_01.wav")
        .track("Banjo")
        .item("17.Banjo  _01.wav")
        .end();

    // --- Keys ---
    let organ = TrackGroup::folder("Organ")
        .track("Organ 1")
        .item("26.B3_01.wav")
        .track("Organ 2")
        .item("27.B3 Low_01.wav")
        .end();

    let keys = TrackGroup::folder("Keys")
        .group(organ)
        .track("Keys")
        .item("25.Keys_01.wav")
        .end();

    // --- Vocals ---
    let bgvs = TrackGroup::folder("BGVs")
        .track("BGVs 1")
        .item("29.BGV 1_01.wav")
        .track("BGVs 2")
        .item("30.BGV 2_01.wav")
        .end();

    let vocals = TrackGroup::folder("Vocals")
        .track("Lead")
        .item("28.Lead Vocal_01.wav")
        .group(bgvs)
        .end();

    // --- Guide ---
    let guide = TrackGroup::single_track("Guide", "Click 128.wav");

    // --- Reference ---
    let reference = TrackGroup::single_track("Reference", "31.AnotherLoveSong Joe MIX_01.wav");

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(bass)
        .group(guitars)
        .group(keys)
        .group(vocals)
        .group(guide)
        .group(reference)
        .build();
    expected
}

#[test]
fn james_dupre_another_love_song() {
    // -- Setup & Fixtures
    // James Dupre - Another Love Song: Country/americana with B3 organ and various guitars
    let items = vec![
        "01.Kick_01.wav",
        "02.Kick Out_01.wav",
        "03.Kick Sample_01.wav",
        "04.Snare _01.wav",
        "05.Snare Sample Stereo_01.wav",
        "06.Snare Sample Stereo 2_01.wav",
        "07.Snare Bottom_01.wav",
        "08.Hat_01.wav",
        "09.Tom 1_01.wav",
        "10.Tom 2_01.wav",
        "11.OH_01.wav",
        "12.Room Stereo_01.wav",
        "13.Room Mono_01.wav",
        "14.Kit Mono_01.wav",
        "15.Bass_01.wav",
        "16.AG1_01.wav",
        "17.Banjo  _01.wav",
        "18.EG2 (57)_01.wav",
        "19.EG2 (FH)_01.wav",
        "20.EG1 (57)_01.wav",
        "21.EG1 (FH)_01.wav",
        "22.EG3 (57)_01.wav",
        "23.EG3 (FH)_01.wav",
        "24.Steel_01.wav",
        "25.Keys_01.wav",
        "26.B3_01.wav",
        "27.B3 Low_01.wav",
        "28.Lead Vocal_01.wav",
        "29.BGV 1_01.wav",
        "30.BGV 2_01.wav",
        "31.AnotherLoveSong Joe MIX_01.wav",
        "Click 128.wav",
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
    // Kick: "Out" matches SUM pattern, others are siblings
    let expected = james_dupre_another_love_song_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
