use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn helado_negro_mitad_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::single_track("Drums", "04_DrumMachine.wav");
    // Vibraphone dual-classified as percussion and orchestral
    let percussion = TrackGroup::single_track("Percussion", "07_Vibraphone.wav");
    let keys = TrackGroup::single_track("Keys", "10_TackPiano.wav");

    let synths = TrackGroup::folder("Synths")
        .track("Synth1")
        .item("02_Synth1.wav")
        .track("Synth2")
        .item("05_Synth2.wav")
        .track("Synth3")
        .item("06_Synth3.wav")
        .track("Synth4")
        .item("09_Synth4.wav")
        .end();

    let vocals = TrackGroup::folder("Vocals")
        .track("Lead 1")
        .item("03_LeadVox1.wav")
        .track("Lead 2")
        .item("08_LeadVox2.wav")
        .end();

    let orchestra = TrackGroup::single_track("Orchestra", "07_Vibraphone.wav");
    let sfx = TrackGroup::single_track("SFX", "01_FX.wav");

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(percussion)
        .group(keys)
        .group(synths)
        .group(vocals)
        .group(orchestra)
        .group(sfx)
        .build();
    expected
}

#[test]
fn helado_negro_mitad() {
    // -- Setup & Fixtures
    // Helado Negro - Mitad del Mundo: 10-stem electronic/experimental session from MedleyDB.
    // FX/processed sounds, 4 synthesizers, dual male vocal takes, drum machine,
    // vibraphone, and tack piano. Tests heavily electronic arrangement with
    // unusual keyboard instruments.
    let items = vec![
        "01_FX.wav",
        "02_Synth1.wav",
        "03_LeadVox1.wav",
        "04_DrumMachine.wav",
        "05_Synth2.wav",
        "06_Synth3.wav",
        "07_Vibraphone.wav",
        "08_LeadVox2.wav",
        "09_Synth4.wav",
        "10_TackPiano.wav",
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

    let expected = helado_negro_mitad_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
