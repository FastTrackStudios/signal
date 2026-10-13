use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn hezekiah_jones_borrowed_heart_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::folder("Drums")
        .track("Cymbals")
        .item("07_Cymbal.wav")
        .track("Drum Kit")
        .item("09_Drums.wav")
        .end();

    // Vibraphone dual-classified
    let percussion = TrackGroup::folder("Percussion")
        .track("Orch")
        .item("05_Vibraphone.wav")
        .track("Aux")
        .item("08_AuxPerc.wav")
        .end();

    // ElecBass now correctly classified as Bass
    let bass = TrackGroup::single_track("Bass", "04_ElecBass.wav");

    let guitars = TrackGroup::folder("Guitars")
        .track("Acoustic")
        .item("01_AcousticGtr.wav")
        .track("Steel")
        .item("12_LapSteel.wav")
        .track("Banjo")
        .item("03_Banjo.wav")
        .end();

    let bgvs = TrackGroup::folder("BGVs")
        .track("BGVs 1")
        .item("02_BackingVox1.wav")
        .track("BGVs 2")
        .item("06_BackingVox2.wav")
        .end();

    let vocals = TrackGroup::folder("Vocals")
        .track("Lead")
        .item("10_LeadVox.wav")
        .group(bgvs)
        .end();

    let orchestra = TrackGroup::folder("Orchestra")
        .track("Strings")
        .item("11_Violins.wav")
        .track("Percussion")
        .item("05_Vibraphone.wav")
        .end();

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(percussion)
        .group(bass)
        .group(guitars)
        .group(vocals)
        .group(orchestra)
        .build();
    expected
}

#[test]
fn hezekiah_jones_borrowed_heart() {
    // -- Setup & Fixtures
    // Hezekiah Jones - Borrowed Heart: 12-stem folk/Americana session from MedleyDB.
    // Acoustic guitar, backing + lead vocals, banjo, electric bass, vibraphone,
    // cymbal, auxiliary percussion, drums, violin section, and lap steel guitar.
    // Tests folk instrumentation with banjo, lap steel, and vibraphone.
    let items = vec![
        "01_AcousticGtr.wav",
        "02_BackingVox1.wav",
        "03_Banjo.wav",
        "04_ElecBass.wav",
        "05_Vibraphone.wav",
        "06_BackingVox2.wav",
        "07_Cymbal.wav",
        "08_AuxPerc.wav",
        "09_Drums.wav",
        "10_LeadVox.wav",
        "11_Violins.wav",
        "12_LapSteel.wav",
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

    let expected = hezekiah_jones_borrowed_heart_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
