use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn queen_bicycle_race_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::folder("Drums")
        .track("Kick")
        .item("01_Kick.wav")
        .track("Snare")
        .item("02_Snare.wav")
        .track("Toms")
        .item("03_Toms.wav")
        .track("OH")
        .item("04_Overheads.wav")
        .end();

    let bass = TrackGroup::single_track("Bass", "05_Bass.wav");

    let guitars = TrackGroup::folder("Guitars")
        .track("Electric 1")
        .item("06_Guitar1.wav")
        .track("Electric 2")
        .item("07_Guitar2.wav")
        .track("Electric 3")
        .item("08_Guitar3.wav")
        .end();

    let keys = TrackGroup::single_track("Keys", "09_Piano.wav");

    // Bells classified as synths (confirmed correct)
    let synths = TrackGroup::single_track("Synths", "10_Bells.wav");

    let vocals = TrackGroup::folder("Vocals")
        .track("Lead")
        .item("11_LeadVox.wav")
        .track("BGVs")
        .item("12_BackingVox.wav")
        .end();

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(bass)
        .group(guitars)
        .group(keys)
        .group(synths)
        .group(vocals)
        .build();
    expected
}

#[test]
fn queen_bicycle_race() {
    // -- Setup & Fixtures
    // Queen - Bicycle Race: 12-stem rock session. Layered Queen production with
    // separated drum kit (kick, snare, toms, overheads), bass, Brian May's multi-tracked
    // guitar layers, piano, bells, lead vocals and backing vocal choir.
    let items = vec![
        "01_Kick.wav",
        "02_Snare.wav",
        "03_Toms.wav",
        "04_Overheads.wav",
        "05_Bass.wav",
        "06_Guitar1.wav",
        "07_Guitar2.wav",
        "08_Guitar3.wav",
        "09_Piano.wav",
        "10_Bells.wav",
        "11_LeadVox.wav",
        "12_BackingVox.wav",
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

    // Toms now correctly classified in Drums
    let expected = queen_bicycle_race_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
