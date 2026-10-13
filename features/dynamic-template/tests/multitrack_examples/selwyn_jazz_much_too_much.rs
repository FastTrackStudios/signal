use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn selwyn_jazz_much_too_much_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::folder("Drums")
        .track("Kick")
        .item("01_Kick.wav")
        .track("Snare")
        .item("02_Snare.wav")
        .track("OH")
        .item("03_Overheads.wav")
        .track("Rooms")
        .item("13_Room.wav")
        .end();

    // --- Bass ---
    let bass = TrackGroup::single_track("Bass", "04_Bass.wav");

    // --- Guitars ---
    let guitars = TrackGroup::single_track("Guitars", "05_Guitar.wav");

    // --- Keys ---
    let keys = TrackGroup::single_track("Keys", "06_Piano.wav");

    // --- Horns ---
    // Three saxophones grouped under Horns
    let horns = TrackGroup::folder("Horns")
        .track("Saxophone 1")
        .item("07_Saxophone1.wav")
        .track("Saxophone 2")
        .item("08_Saxophone2.wav")
        .track("Saxophone 3")
        .item("09_Saxophone3.wav")
        .end();

    // --- Vocals ---
    // Lead scratch and overdub takes
    let vocals = TrackGroup::folder("Vocals")
        .track("Lead 1")
        .item("14_LeadVoxScratch.wav")
        .track("Lead 2")
        .item("15_LeadVoxOverdub.wav")
        .end();

    // --- Orchestra ---
    // Trumpets and trombone classified into orchestral section
    let trumpets = TrackGroup::folder("Trumpets")
        .track("Trumpets 1")
        .item("10_Trumpet1.wav")
        .track("Trumpets 2")
        .item("11_Trumpet2.wav")
        .end();

    let orchestra = TrackGroup::folder("Orchestra")
        .group(trumpets)
        .track("Trombone")
        .item("12_Trombone.wav")
        .end();

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(bass)
        .group(guitars)
        .group(keys)
        .group(horns)
        .group(vocals)
        .group(orchestra)
        .build();
    expected
}

#[test]
fn selwyn_jazz_much_too_much() {
    // -- Setup & Fixtures
    // Selwyn Jazz - Much Too Much: Full jazz big-band session with saxophone section,
    // trumpet pair, trombone, piano, guitar, bass, drums, room mic, and two vocal takes
    // (scratch + overdub). Cambridge-MT multitrack library.
    let items = vec![
        "01_Kick.wav",
        "02_Snare.wav",
        "03_Overheads.wav",
        "04_Bass.wav",
        "05_Guitar.wav",
        "06_Piano.wav",
        "07_Saxophone1.wav",
        "08_Saxophone2.wav",
        "09_Saxophone3.wav",
        "10_Trumpet1.wav",
        "11_Trumpet2.wav",
        "12_Trombone.wav",
        "13_Room.wav",
        "14_LeadVoxScratch.wav",
        "15_LeadVoxOverdub.wav",
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
    // Standard jazz kit: kick, snare, overheads, room mic
    let expected = selwyn_jazz_much_too_much_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
