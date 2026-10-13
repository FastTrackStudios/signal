use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn boston_more_than_a_feeling_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::folder("Drums")
        .track("Kick")
        .item("01_Kick.wav")
        .track("Snare")
        .item("02_Snare.wav")
        .track("OH")
        .item("03_Overheads.wav")
        .end();

    let bass = TrackGroup::single_track("Bass", "04_Bass.wav");

    let electric = TrackGroup::folder("Electric")
        .track("Lead")
        .item("07_LeadGtr.wav")
        .track("Rhythm")
        .item("06_RhythmGtr.wav")
        .end();

    let guitars = TrackGroup::folder("Guitars")
        .group(electric)
        .track("Acoustic")
        .item("05_AcousticGtr.wav")
        .end();

    let keys = TrackGroup::single_track("Keys", "08_Organ.wav");

    let vocals = TrackGroup::folder("Vocals")
        .track("Lead")
        .item("09_LeadVox.wav")
        .track("BGVs")
        .item("10_BackingVox.wav")
        .end();

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(bass)
        .group(guitars)
        .group(keys)
        .group(vocals)
        .build();
    expected
}

#[test]
fn boston_more_than_a_feeling() {
    // -- Setup & Fixtures
    // Boston - More Than A Feeling: 10-stem classic rock session. Multi-layered
    // guitars (rhythm, lead, acoustic), bass, drums with overheads, organ,
    // lead vocals and backing vocals. Tests Tom Scholz's layered guitar style.
    let items = vec![
        "01_Kick.wav",
        "02_Snare.wav",
        "03_Overheads.wav",
        "04_Bass.wav",
        "05_AcousticGtr.wav",
        "06_RhythmGtr.wav",
        "07_LeadGtr.wav",
        "08_Organ.wav",
        "09_LeadVox.wav",
        "10_BackingVox.wav",
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

    let expected = boston_more_than_a_feeling_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
