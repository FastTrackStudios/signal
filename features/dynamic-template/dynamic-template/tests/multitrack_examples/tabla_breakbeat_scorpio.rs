use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn tabla_breakbeat_scorpio_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::single_track("Drums", "01_DrumMachine.wav");
    let percussion = TrackGroup::single_track("Percussion", "02_Tabla.wav");
    let keys = TrackGroup::single_track("Keys", "04_Accordion.wav");
    let synths = TrackGroup::single_track("Synths", "03_Synthesizer.wav");

    let sfx = TrackGroup::folder("SFX")
        .track("FX1")
        .item("05_FX1.wav")
        .track("FX2")
        .item("06_FX2.wav")
        .track("FX3")
        .item("07_FX3.wav")
        .end();

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(percussion)
        .group(keys)
        .group(synths)
        .group(sfx)
        .build();
    expected
}

#[test]
fn tabla_breakbeat_scorpio() {
    // -- Setup & Fixtures
    // Tabla Breakbeat Science - Scorpio: 7-stem electronic/world fusion session from
    // MedleyDB. Combines Indian tabla with electronic drum machine, synthesizer,
    // accordion, and processed FX sounds. Tests electronic + world hybrid classification.
    let items = vec![
        "01_DrumMachine.wav",
        "02_Tabla.wav",
        "03_Synthesizer.wav",
        "04_Accordion.wav",
        "05_FX1.wav",
        "06_FX2.wav",
        "07_FX3.wav",
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

    let expected = tabla_breakbeat_scorpio_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
