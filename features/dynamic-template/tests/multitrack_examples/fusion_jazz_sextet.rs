use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn fusion_jazz_sextet_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::single_track("Drums", "01_Drums.wav");

    // ElecBass now correctly classified as Bass
    let bass = TrackGroup::single_track("Bass", "02_ElecBass.wav");

    // ElecPiano now correctly classified as Keys
    let keys = TrackGroup::single_track("Keys", "03_ElecPiano.wav");

    let synths = TrackGroup::single_track("Synths", "04_Synthesizer.wav");
    let horns = TrackGroup::single_track("Horns", "05_SopranoSax.wav");
    let orchestra = TrackGroup::single_track("Orchestra", "06_Trumpet.wav");

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(bass)
        .group(keys)
        .group(synths)
        .group(horns)
        .group(orchestra)
        .build();
    expected
}

#[test]
fn fusion_jazz_sextet() {
    // -- Setup & Fixtures
    // MusicDelta Fusion Jazz: 6-stem jazz fusion session from MedleyDB. Drum set,
    // electric bass, electric piano (Rhodes-style), synthesizer, soprano saxophone,
    // and trumpet. Tests jazz fusion with electric instruments and soprano sax.
    let items = vec![
        "01_Drums.wav",
        "02_ElecBass.wav",
        "03_ElecPiano.wav",
        "04_Synthesizer.wav",
        "05_SopranoSax.wav",
        "06_Trumpet.wav",
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

    let expected = fusion_jazz_sextet_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
