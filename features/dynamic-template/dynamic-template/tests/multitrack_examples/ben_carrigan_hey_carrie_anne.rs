use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn ben_carrigan_hey_carrie_anne_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::folder("Drums")
        .track("Snare")
        .item("01_Snare.wav")
        .track("Cymbals")
        .item("05_Cymbal.wav")
        .end();

    // --- Percussion ---
    // Shaker, tambourine, timpani, and glockenspiel (classified as orchestral percussion)
    let percussion = TrackGroup::folder("Percussion")
        .track("Shaker")
        .item("02_Shaker.wav")
        .track("Tambourine")
        .item("03_Tambo.wav")
        .track("Orch 1")
        .item("04_Timp.wav")
        .track("Orch 2")
        .item("07_Glockenspiel.wav")
        .end();

    // --- Guitars ---
    // Two electric guitar tracks (same name, different takes)
    let guitars = TrackGroup::folder("Guitars")
        .track("Electric 1")
        .item("18_ElecGtr.wav")
        .track("Electric 2")
        .item("19_ElecGtr.wav")
        .end();

    // --- Keys ---
    let keys = TrackGroup::single_track("Keys", "06_Piano.wav");

    // --- Harmonica ---
    let harmonica = TrackGroup::single_track("Harmonica", "20_Harmonica.wav");

    // --- Vocals ---
    let bgvs = TrackGroup::folder("BGVs")
        .track("BGVs 1")
        .item("22_BackingVox1.wav")
        .track("BGVs 2")
        .item("23_BackingVox2.wav")
        .track("BGVs 3")
        .item("24_BackingVox3.wav")
        .end();

    let vocals = TrackGroup::folder("Vocals")
        .track("Lead")
        .item("21_LeadVox.wav")
        .group(bgvs)
        .end();

    // --- Orchestra ---
    // Full string section with live and sampled tracks, plus glockenspiel
    let violins = TrackGroup::folder("Violins")
        .track("First Violin")
        .item("08_Violin1.wav")
        .track("Second Violin")
        .item("09_Violin2.wav")
        .track("Violins")
        .item("13_ViolinSamples.wav")
        .end();

    let viola = TrackGroup::folder("Viola")
        .track("Viola 1")
        .item("10_Viola.wav")
        .track("Viola 2")
        .item("14_ViolaSamples.wav")
        .end();

    let cello = TrackGroup::folder("Cello")
        .track("Cello 1")
        .item("11_Cello.wav")
        .track("Cello 2")
        .item("15_CelloSamples.wav")
        .end();

    let contrabass = TrackGroup::folder("Contrabass")
        .track("Contrabass 1")
        .item("16_DoubleBassSamples1.wav")
        .track("Contrabass 2")
        .item("17_DoubleBassSamples2.wav")
        .end();

    let strings = TrackGroup::folder("Strings")
        .group(violins)
        .group(viola)
        .group(cello)
        .group(contrabass)
        .track("Strings")
        .item("12_StringsMix.wav")
        .end();

    let orch_perc = TrackGroup::folder("Percussion")
        .track("Percussion 1")
        .item("04_Timp.wav")
        .track("Percussion 2")
        .item("07_Glockenspiel.wav")
        .end();

    let orchestra = TrackGroup::folder("Orchestra")
        .group(strings)
        .group(orch_perc)
        .end();

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(percussion)
        .group(guitars)
        .group(keys)
        .group(harmonica)
        .group(vocals)
        .group(orchestra)
        .build();
    expected
}

#[test]
fn ben_carrigan_hey_carrie_anne() {
    // -- Setup & Fixtures
    // Ben Carrigan - Hey Carrie Anne: Full string section (violin, viola, cello, double bass)
    // with both live and sampled versions, plus glockenspiel, piano, electric guitar,
    // harmonica, percussion, and vocals. Cambridge-MT multitrack library.
    let items = vec![
        "01_Snare.wav",
        "02_Shaker.wav",
        "03_Tambo.wav",
        "04_Timp.wav",
        "05_Cymbal.wav",
        "06_Piano.wav",
        "07_Glockenspiel.wav",
        "08_Violin1.wav",
        "09_Violin2.wav",
        "10_Viola.wav",
        "11_Cello.wav",
        "12_StringsMix.wav",
        "13_ViolinSamples.wav",
        "14_ViolaSamples.wav",
        "15_CelloSamples.wav",
        "16_DoubleBassSamples1.wav",
        "17_DoubleBassSamples2.wav",
        "18_ElecGtr.wav",
        "19_ElecGtr.wav",
        "20_Harmonica.wav",
        "21_LeadVox.wav",
        "22_BackingVox1.wav",
        "23_BackingVox2.wav",
        "24_BackingVox3.wav",
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
    // Only snare and cymbal from the drum kit (no kick in this session)
    let expected = ben_carrigan_hey_carrie_anne_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
