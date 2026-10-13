use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn family_band_again_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::single_track("Drums", "03_Drums.wav");
    let percussion = TrackGroup::single_track("Percussion", "11_AuxPerc.wav");

    // ElecBass now correctly classified as Bass
    let bass = TrackGroup::single_track("Bass", "01_ElecBass.wav");

    let electric = TrackGroup::folder("Electric")
        .track("Electric 1")
        .item("05_ElecGtr.wav")
        .track("Electric 2")
        .item("06_DistortedGtr.wav")
        .end();

    let steel = TrackGroup::folder("Steel")
        .track("Steel 1")
        .item("07_LapSteel1.wav")
        .track("Steel 2")
        .item("08_LapSteel2.wav")
        .end();

    let guitars = TrackGroup::folder("Guitars")
        .group(electric)
        .track("Acoustic")
        .item("04_AcousticGtr.wav")
        .group(steel)
        .end();

    // Melodica now correctly classified as Keys
    let keys = TrackGroup::single_track("Keys", "10_Melodica.wav");

    let vocals = TrackGroup::folder("Vocals")
        .track("Lead")
        .item("09_LeadVox.wav")
        .track("BGVs")
        .item("02_BackingVox.wav")
        .end();

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(percussion)
        .group(bass)
        .group(guitars)
        .group(keys)
        .group(vocals)
        .build();
    expected
}

#[test]
fn family_band_again() {
    // -- Setup & Fixtures
    // Family Band - Again: 11-stem country/Americana session from MedleyDB. Electric bass,
    // backing + lead vocals, drums, acoustic guitar, clean + distorted electric guitars,
    // 2 lap steel guitars, melodica, and auxiliary percussion. Tests lap steel guitar
    // and melodica classification.
    let items = vec![
        "01_ElecBass.wav",
        "02_BackingVox.wav",
        "03_Drums.wav",
        "04_AcousticGtr.wav",
        "05_ElecGtr.wav",
        "06_DistortedGtr.wav",
        "07_LapSteel1.wav",
        "08_LapSteel2.wav",
        "09_LeadVox.wav",
        "10_Melodica.wav",
        "11_AuxPerc.wav",
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

    let expected = family_band_again_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
