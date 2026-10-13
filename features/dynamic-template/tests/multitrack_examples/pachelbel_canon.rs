use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn pachelbel_canon_expected() -> daw_proto::TrackHierarchy {
    let violins = TrackGroup::folder("Violins")
        .track("First Violin")
        .item("01_Violin1.wav")
        .track("Second Violin")
        .item("04_Violin2.wav")
        .end();

    let orchestra = TrackGroup::folder("Orchestra")
        .group(violins)
        .track("Viola")
        .item("03_Viola.wav")
        .track("Cello")
        .item("02_Cello.wav")
        .end();

    let expected = TrackStructureBuilder::new().group(orchestra).build();
    expected
}

#[test]
fn pachelbel_canon() {
    // -- Setup & Fixtures
    // MusicDelta Pachelbel Canon: 4-stem baroque string quartet from MedleyDB.
    // Two violins, viola, and cello. Pure classical string arrangement with no
    // drums, vocals, or modern instruments. Tests minimal orchestral classification.
    let items = vec![
        "01_Violin1.wav",
        "02_Cello.wav",
        "03_Viola.wav",
        "04_Violin2.wav",
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

    let expected = pachelbel_canon_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
