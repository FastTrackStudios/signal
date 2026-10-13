use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn marc_martel_dont_stop_me_now_expected_leaves(
) -> (TrackGroup, TrackGroup, TrackGroup, TrackGroup, TrackGroup) {
    let kick = TrackGroup::folder("Kick")
        .folder("SUM")
        .track("In")
        .item("Kick In")
        .track("Out")
        .item("Kick Out")
        .end()
        .track("Kick")
        .item("Kick Sample")
        .end();

    // Snare has Top/Bottom as direct children, "Sample" items aliased to Trig subfolder
    let snare = TrackGroup::folder("Snare")
        .track("Top")
        .item("Snare Top")
        .track("Bottom")
        .item("Snare Bottom")
        .folder("Trig")
        .track("Trig 1")
        .item("Snare Sample")
        .track("Trig 2")
        .item("Snare Sample Two")
        .end()
        .end();

    let toms = TrackGroup::folder("Toms")
        .track("T1")
        .item("Tom1")
        .track("T2")
        .item("Tom2")
        .end();

    // Cymbals: OH and Hi Hat as direct children (no subfolder collapse needed)
    let cymbals = TrackGroup::folder("Cymbals")
        .track("OH")
        .item("OH")
        .track("Hi Hat")
        .item("HighHat")
        .end();

    // Drum Kit collapsed away (single child of Drums), so Drums contains kit subgroups directly
    let drums = TrackGroup::folder("Drums")
        .group(kick)
        .group(snare)
        .group(toms)
        .group(cymbals)
        .track("Rooms")
        .item("Rooms")
        .end();

    // --- Percussion ---
    let percussion = TrackGroup::single_track("Percussion", "Percussion");

    // --- Bass ---
    let bass = TrackGroup::single_track("Bass", "Bass DI");

    // --- Guitars ---
    // Clean and Lead subfolders with Left/Right stereo pairs
    let clean = TrackGroup::folder("Clean")
        .track("Left")
        .item("Lead Guitar Clean DI Left")
        .track("Right")
        .item("Lead Guitar Clean DI Right")
        .end();

    let lead_guitar = TrackGroup::folder("Lead")
        .track("Left")
        .item("Lead Guitar Amplitube Left")
        .track("Right")
        .item("Lead Guitar Amplitube Right")
        .end();

    (drums, percussion, bass, clean, lead_guitar)
}

fn marc_martel_dont_stop_me_now_expected() -> daw_proto::TrackHierarchy {
    let (drums, percussion, bass, clean, lead_guitar) =
        marc_martel_dont_stop_me_now_expected_leaves();
    let guitars = TrackGroup::folder("Guitars")
        .group(clean)
        .group(lead_guitar)
        .end();

    // --- Keys ---
    let keys = TrackGroup::single_track("Keys", "Piano");

    // --- Vocals ---
    // Lead folder with main vocal and vocal effects
    let lead_vocal = TrackGroup::folder("Lead")
        .track("Lead 1")
        .item("Vocal")
        .track("Plate")
        .item("Vocal.Eko.Plate")
        .track("Lead 2")
        .item("Vocal.Magic")
        .end();

    let bgvs = TrackGroup::folder("BGVs")
        .track("BGVs 1")
        .item("BGV1")
        .track("BGVs 2")
        .item("BGV2")
        .track("BGVs 3")
        .item("BGV3")
        .track("BGVs 4")
        .item("BGV4")
        .end();

    let vocals = TrackGroup::folder("Vocals")
        .group(lead_vocal)
        .group(bgvs)
        .end();

    // --- SFX ---
    // H3000 effects (harmonizer/effects processor)
    let sfx = TrackGroup::folder("SFX")
        .track("H3000.One")
        .item("H3000.One")
        .track("H3000.Two")
        .item("H3000.Two")
        .track("H3000.Three")
        .item("H3000.Three")
        .end();

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(percussion)
        .group(bass)
        .group(guitars)
        .group(keys)
        .group(vocals)
        .group(sfx)
        .build();
    expected
}

#[test]
fn marc_martel_dont_stop_me_now() {
    // -- Setup & Fixtures
    // Marc Martel - Don't Stop Me Now: Queen cover with layered drums and H3000 effects
    let items = vec![
        "Kick In",
        "Kick Out",
        "Kick Sample",
        "Snare Top",
        "Snare Bottom",
        "Snare Sample",
        "Snare Sample Two",
        "Tom1",
        "Tom2",
        "HighHat",
        "OH",
        "Rooms",
        "Percussion",
        "Bass DI",
        "Piano",
        "Lead Guitar Amplitube Left",
        "Lead Guitar Amplitube Right",
        "Lead Guitar Clean DI Left",
        "Lead Guitar Clean DI Right",
        "Vocal",
        "H3000.One",
        "H3000.Two",
        "H3000.Three",
        "Vocal.Eko.Plate",
        "Vocal.Magic",
        "BGV1",
        "BGV2",
        "BGV3",
        "BGV4",
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
    // Kick has SUM subfolder for In/Out mics, "Kick Sample" stays as sibling (no SUM pattern match)
    let expected = marc_martel_dont_stop_me_now_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
