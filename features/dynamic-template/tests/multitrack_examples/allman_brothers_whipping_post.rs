use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn allman_brothers_whipping_post_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::folder("Drums")
        .track("Drum Kit 1")
        .item("01_Drums1.wav")
        .track("Drum Kit 2")
        .item("02_Drums2.wav")
        .end();

    // Congas now correctly classified as Percussion alongside Tambourine
    let percussion = TrackGroup::folder("Percussion")
        .track("Tambourine")
        .item("12_Tambourine.wav")
        .track("Conga")
        .item("07_Congas.wav")
        .end();

    let bass = TrackGroup::single_track("Bass", "03_Bass.wav");

    let guitars = TrackGroup::folder("Guitars")
        .track("Electric 1")
        .item("04_ElecGtr1.wav")
        .track("Electric 2")
        .item("05_ElecGtr2.wav")
        .end();

    let keys = TrackGroup::single_track("Keys", "06_Organ.wav");

    let bgvs = TrackGroup::folder("BGVs")
        .track("BGVs 1")
        .item("09_BackingVox1.wav")
        .track("BGVs 2")
        .item("10_BackingVox2.wav")
        .track("BGVs 3")
        .item("11_BackingVox3.wav")
        .end();

    let vocals = TrackGroup::folder("Vocals")
        .track("Lead")
        .item("08_LeadVox.wav")
        .group(bgvs)
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
fn allman_brothers_whipping_post() {
    // -- Setup & Fixtures
    // Allman Brothers Band - Whipping Post: 12-stem Southern rock session from MedleyDB.
    // Dual drummer setup (classic ABB), bass, dual lead guitars (Duane + Dickey),
    // Hammond organ, congas, lead vocals and backing vocals. Tests dual drum kit
    // and dual lead guitar arrangement.
    let items = vec![
        "01_Drums1.wav",
        "02_Drums2.wav",
        "03_Bass.wav",
        "04_ElecGtr1.wav",
        "05_ElecGtr2.wav",
        "06_Organ.wav",
        "07_Congas.wav",
        "08_LeadVox.wav",
        "09_BackingVox1.wav",
        "10_BackingVox2.wav",
        "11_BackingVox3.wav",
        "12_Tambourine.wav",
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

    let expected = allman_brothers_whipping_post_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
