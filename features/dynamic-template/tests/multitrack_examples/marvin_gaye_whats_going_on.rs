use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn marvin_gaye_whats_going_on_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::single_track("Drums", "01_Drums.wav");

    // --- Percussion ---
    // Bongo + vibraphone (dual-classified as orchestral percussion)
    let percussion = TrackGroup::folder("Percussion")
        .track("Bongo")
        .item("09_Bongo.wav")
        .track("Orch")
        .item("06_Vibraphone.wav")
        .end();

    // --- Bass ---
    let bass = TrackGroup::single_track("Bass", "02_Bass.wav");

    // --- Guitars ---
    let guitars = TrackGroup::folder("Guitars")
        .track("Electric")
        .item("04_ElecGtr.wav")
        .track("Acoustic")
        .item("03_AcousticGtr.wav")
        .end();

    // --- Keys ---
    let keys = TrackGroup::single_track("Keys", "05_Piano.wav");

    // --- Horns ---
    let horns_group = TrackGroup::single_track("Horns", "07_AltoSax.wav");

    // --- Vocals ---
    let bgvs = TrackGroup::folder("BGVs")
        .track("BGVs 1")
        .item("11_BackingVox1.wav")
        .track("BGVs 2")
        .item("12_BackingVox2.wav")
        .end();

    let vocals = TrackGroup::folder("Vocals")
        .track("Lead")
        .item("10_LeadVox.wav")
        .group(bgvs)
        .end();

    // --- Orchestra ---
    let orchestra = TrackGroup::folder("Orchestra")
        .track("Strings")
        .item("08_Strings.wav")
        .track("Percussion")
        .item("06_Vibraphone.wav")
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
        .group(horns_group)
        .group(vocals)
        .group(orchestra)
        .build();
    expected
}

#[test]
fn marvin_gaye_whats_going_on() {
    // -- Setup & Fixtures
    // Marvin Gaye - What's Going On: 12-stem soul/R&B session from MedleyDB.
    // Alto sax, piano, vibraphone, string section, full drum kit, bass, acoustic guitar,
    // clean electric guitar, bongo, lead vocals, and backing vocals. Tests jazz/soul
    // instrumentation with orchestral and percussion crossover elements.
    let items = vec![
        "01_Drums.wav",
        "02_Bass.wav",
        "03_AcousticGtr.wav",
        "04_ElecGtr.wav",
        "05_Piano.wav",
        "06_Vibraphone.wav",
        "07_AltoSax.wav",
        "08_Strings.wav",
        "09_Bongo.wav",
        "10_LeadVox.wav",
        "11_BackingVox1.wav",
        "12_BackingVox2.wav",
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
    let expected = marvin_gaye_whats_going_on_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
