use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn beatles_dont_let_me_down_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::folder("Drums")
        .track("Kick")
        .item("01.Kick_01.wav")
        .track("Snare")
        .item("02.Snare_01.wav")
        .track("OH")
        .item("03.OH_01.wav")
        .end();

    // --- Bass ---
    // Items expanded to separate tracks
    let bass = TrackGroup::folder("Bass")
        .track("Bass")
        .item("04.Bass Di_01.wav")
        .track("Amp")
        .item("05.Bass Amp_01.wav")
        .end();

    // --- Guitars ---
    // "Gtr" now routes into ElectricGuitar group, giving proper DI/Amp MultiMic names.
    // Amp descriptor comes first in field_value_descriptors order.
    let guitars = TrackGroup::folder("Guitars")
        .track("Amp")
        .item("07.Gtr Amp_01.wav")
        .track("DI")
        .item("06.Gtr DI_01.wav")
        .end();

    // --- Keys ---
    let keys = TrackGroup::single_track("Keys", "08.Keys_01.wav");

    // --- Lead Vocals ---
    // Lead collapses into Vocals folder when it's the only vocal type
    // Child tracks keep "Lead 1", "Lead 2" names since they're in the Lead grouping
    let lead = TrackGroup::folder("Vocals")
        .track("Lead 1")
        .item("09.Vi.Vocal_01.wav")
        .track("Lead 2")
        .item("10.Caitlin Vocal_01.wav")
        .end();

    // --- Reference ---
    let reference =
        TrackGroup::single_track("Reference", "11.Don't Let Me Down Joe Carrell Mix_01.wav");

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(bass)
        .group(guitars)
        .group(keys)
        .group(lead)
        .group(reference)
        .build();
    expected
}

#[test]
fn beatles_dont_let_me_down() {
    // -- Setup & Fixtures
    let items = vec![
        "01.Kick_01.wav",
        "02.Snare_01.wav",
        "03.OH_01.wav",
        "04.Bass Di_01.wav",
        "05.Bass Amp_01.wav",
        "06.Gtr DI_01.wav",
        "07.Gtr Amp_01.wav",
        "08.Keys_01.wav",
        "09.Vi.Vocal_01.wav",
        "10.Caitlin Vocal_01.wav",
        "11.Don't Let Me Down Joe Carrell Mix_01.wav",
    ];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    // ============================================================================
    // Expected structure (WITH EXPANSION)
    // ============================================================================
    // With default expansion enabled:
    // - Cymbals folder collapses to OH (single child)
    // - Bass items expand to separate tracks (Bass DI, Bass Amp)
    // - Guitar items expand to separate tracks (Guitar DI, Guitar Amp)
    // - Lead vocals expand to Lead 1, Lead 2

    // --- Drums ---
    // Cymbals folder collapsed since only one cymbal (OH)
    let expected = beatles_dont_let_me_down_expected();

    // Full structure assertion
    assert_tracks_equal(&tracks, &expected).unwrap();
}
