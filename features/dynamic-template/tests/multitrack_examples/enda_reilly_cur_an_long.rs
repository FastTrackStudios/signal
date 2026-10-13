use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn enda_reilly_cur_an_long_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::folder("Drums")
        .track("Drum Kit 1")
        .item("01_DrumMic1.wav")
        .track("Drum Kit 2")
        .item("02_DrumMic2.wav")
        .track("Drum Kit 3")
        .item("03_Brushes.wav")
        .end();

    // --- Bass ---
    // Dual-mic bass (likely close + room or two positions)
    let bass = TrackGroup::folder("Bass")
        .track("Bass 1")
        .item("04_BassMic1.wav")
        .track("Bass 2")
        .item("05_BassMic2.wav")
        .end();

    // --- Guitars ---
    // Mandolin dual-mic nested under acoustic guitar family
    let mando = TrackGroup::folder("Mando")
        .track("Mando 1")
        .item("07_MandolinMic1.wav")
        .track("Mando 2")
        .item("08_MandolinMic2.wav")
        .end();

    let guitars = TrackGroup::folder("Guitars")
        .group(mando)
        .track("Acoustic")
        .item("06_AcousticGtr.wav")
        .end();

    // --- Fiddle ---
    // Dual-mic fiddle
    let fiddle = TrackGroup::folder("Fiddle")
        .track("Fiddle 1")
        .item("09_Fiddle1.wav")
        .track("Fiddle 2")
        .item("10_Fiddle2.wav")
        .end();

    // --- Keys ---
    // Accordion now correctly classified as Keys
    let keys = TrackGroup::single_track("Keys", "11_Accordion.wav");

    // --- Fiddle ---
    // (already defined above)

    // --- Vocals ---
    let vocals = TrackGroup::single_track("Vocals", "12_LeadVox.wav");

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(bass)
        .group(guitars)
        .group(keys)
        .group(fiddle)
        .group(vocals)
        .build();
    expected
}

#[test]
fn enda_reilly_cur_an_long() {
    // -- Setup & Fixtures
    // Enda Reilly - Cur An Long Ag Seol: Irish folk session with drum brushes, dual-mic
    // bass, acoustic guitar, dual-mic mandolin, dual-mic fiddle, accordion, and lead
    // vocals. Tests folk/traditional instruments and dual-mic grouping.
    // Cambridge-MT multitrack library.
    let items = vec![
        "01_DrumMic1.wav",
        "02_DrumMic2.wav",
        "03_Brushes.wav",
        "04_BassMic1.wav",
        "05_BassMic2.wav",
        "06_AcousticGtr.wav",
        "07_MandolinMic1.wav",
        "08_MandolinMic2.wav",
        "09_Fiddle1.wav",
        "10_Fiddle2.wav",
        "11_Accordion.wav",
        "12_LeadVox.wav",
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
    // Dual-mic drum setup plus brushes (classified as Drum Kit via "brush"/"brushes" pattern)
    let expected = enda_reilly_cur_an_long_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
