use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn metallica_blackened_expected() -> daw_proto::TrackHierarchy {
    let oh = TrackGroup::folder("OH")
        .track("L")
        .item("03_OH-Left.wav")
        .track("R")
        .item("04_OH-Right.wav")
        .end();

    let drums = TrackGroup::folder("Drums")
        .track("Kick")
        .item("01_Kick.wav")
        .track("Snare")
        .item("02_Snare.wav")
        .group(oh)
        .end();

    // --- Bass ---
    let bass = TrackGroup::single_track("Bass", "05_Bass.wav");

    // --- Guitars ---
    // Stereo pair: Guitar1 Left and Guitar2 Right
    let guitars = TrackGroup::folder("Guitars")
        .track("Left")
        .item("06_Guitar1-Left.wav")
        .track("Right")
        .item("07_Guitar2-Right.wav")
        .end();

    // --- Vocals ---
    let vocals = TrackGroup::folder("Vocals")
        .track("Vocals 1")
        .item("08_Vocals1.wav")
        .track("Vocals 2")
        .item("09_Vocals2.wav")
        .end();

    // --- Unsorted ---
    // "Overdubs" not recognized as a standard instrument
    let unsorted = TrackGroup::folder("Unsorted")
        .track("Overdubs1")
        .item("10_Overdubs1.wav")
        .track("Overdubs2")
        .item("11_Overdubs2.wav")
        .end();

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(bass)
        .group(guitars)
        .group(vocals)
        .group(unsorted)
        .build();
    expected
}

#[test]
fn metallica_blackened() {
    // -- Setup & Fixtures
    // Metallica - Blackened: 11-track thrash metal session with stereo overhead pair (L/R),
    // stereo guitar pair (Left/Right), dual vocal tracks, and overdub pair. Tests stereo
    // panning designation in track names and minimal metal arrangement.
    let items = vec![
        "01_Kick.wav",
        "02_Snare.wav",
        "03_OH-Left.wav",
        "04_OH-Right.wav",
        "05_Bass.wav",
        "06_Guitar1-Left.wav",
        "07_Guitar2-Right.wav",
        "08_Vocals1.wav",
        "09_Vocals2.wav",
        "10_Overdubs1.wav",
        "11_Overdubs2.wav",
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
    // Kick, snare, and OH stereo pair (L/R)
    let expected = metallica_blackened_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
