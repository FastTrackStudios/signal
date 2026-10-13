use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn eagles_hotel_california_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::single_track("Drums", "01_Drums.wav");

    // --- Bass ---
    let bass = TrackGroup::single_track("Bass", "02_Bass.wav");

    // --- Guitars ---
    let electric = TrackGroup::folder("Electric")
        .track("Lead")
        .item("09_ElecGtrLeadSolo.wav")
        .track("Wah")
        .item("07_ElecGtrWahWah.wav")
        .track("Distortion")
        .item("08_ElecGtrDistortion.wav")
        .track("Electric")
        .item("06_ElecGtrMuted.wav")
        .end();

    let acoustic = TrackGroup::folder("Acoustic")
        .track("Acoustic 1")
        .item("03_AcousticGtr1.wav")
        .track("Acoustic 2")
        .item("04_AcousticGtr2.wav")
        .track("Acoustic 3")
        .item("05_AcousticGtr3Flanger.wav")
        .end();

    let guitars = TrackGroup::folder("Guitars")
        .group(electric)
        .group(acoustic)
        .end();

    // --- Keys ---
    let keys = TrackGroup::single_track("Keys", "10_Piano.wav");

    // --- Vocals ---
    let bgvs = TrackGroup::folder("BGVs")
        .track("BGVs 1")
        .item("13_BackingVox1.wav")
        .track("BGVs 2")
        .item("14_BackingVox2.wav")
        .track("BGVs 3")
        .item("15_BackingVox3.wav")
        .end();

    let vocals = TrackGroup::folder("Vocals")
        .track("Lead")
        .item("12_LeadVox.wav")
        .group(bgvs)
        .end();

    // --- SFX ---
    let sfx = TrackGroup::single_track("SFX", "11_SFX.wav");

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(bass)
        .group(guitars)
        .group(keys)
        .group(vocals)
        .group(sfx)
        .build();
    expected
}

#[test]
fn eagles_hotel_california() {
    // -- Setup & Fixtures
    // Eagles - Hotel California: 15-channel classic rock session. 3 acoustic guitars
    // (one with flanger effect), 4 electric guitar styles (muted, wah-wah, distortion,
    // lead solo), piano, bass, full drums, SFX, and dual vocal tracks.
    // Tests varied guitar technique descriptors and effect-based naming.
    let items = vec![
        "01_Drums.wav",
        "02_Bass.wav",
        "03_AcousticGtr1.wav",
        "04_AcousticGtr2.wav",
        "05_AcousticGtr3Flanger.wav",
        "06_ElecGtrMuted.wav",
        "07_ElecGtrWahWah.wav",
        "08_ElecGtrDistortion.wav",
        "09_ElecGtrLeadSolo.wav",
        "10_Piano.wav",
        "11_SFX.wav",
        "12_LeadVox.wav",
        "13_BackingVox1.wav",
        "14_BackingVox2.wav",
        "15_BackingVox3.wav",
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
    let expected = eagles_hotel_california_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
