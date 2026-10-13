use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn beatles_within_you_without_you_expected() -> daw_proto::TrackHierarchy {
    let percussion = TrackGroup::single_track("Percussion", "03_Tabla.wav");

    let vocals = TrackGroup::single_track("Vocals", "07_LeadVox.wav");

    let orchestra = TrackGroup::folder("Orchestra")
        .track("Cello")
        .item("06_Cello.wav")
        .track("Strings")
        .item("05_Strings.wav")
        .end();

    // Sitar, dilruba, tambura are non-Western instruments → Unsorted
    let unsorted = TrackGroup::folder("Unsorted")
        .track("Sitar")
        .item("01_Sitar.wav")
        .track("Dilruba")
        .item("02_Dilruba.wav")
        .track("Tambura")
        .item("04_Tambura.wav")
        .end();

    let expected = TrackStructureBuilder::new()
        .group(percussion)
        .group(vocals)
        .group(orchestra)
        .group(unsorted)
        .build();
    expected
}

#[test]
fn beatles_within_you_without_you() {
    // -- Setup & Fixtures
    // Beatles - Within You Without You: 7-stem Indian classical/psychedelic session
    // from MedleyDB. Features sitar, dilruba (Indian bowed string), tabla (Indian
    // percussion), tambura (Indian drone instrument), string section, cello, and vocals.
    // Tests exotic/non-Western instrument classification.
    let items = vec![
        "01_Sitar.wav",
        "02_Dilruba.wav",
        "03_Tabla.wav",
        "04_Tambura.wav",
        "05_Strings.wav",
        "06_Cello.wav",
        "07_LeadVox.wav",
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

    // Tabla now correctly classified as Percussion
    let expected = beatles_within_you_without_you_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
