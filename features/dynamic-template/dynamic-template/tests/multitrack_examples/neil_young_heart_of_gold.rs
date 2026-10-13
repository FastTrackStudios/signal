use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn neil_young_heart_of_gold_expected() -> daw_proto::TrackHierarchy {
    let oh = TrackGroup::folder("OH")
        .track("L")
        .item("03.OH L 260VU_01.wav")
        .track("R")
        .item("04.OH R 260VU_01.wav")
        .end();

    let drums = TrackGroup::folder("Drums")
        .track("Kick")
        .item("01.Kick OptoComp_01.wav")
        .track("Snare")
        .item("02.Snare No Compression_01.wav")
        .group(oh)
        .end();

    // --- Bass ---
    // DI + two amp mics (44A ribbon, 421 dynamic) through 260VU
    let bass = TrackGroup::folder("Bass")
        .track("Bass")
        .item("05.Bass DI OptoComp_01.wav")
        .track("Amp 1")
        .item("06.Bass Amp 44A. 260VU_01.wav")
        .track("Amp 2")
        .item("07.Bass Amp 421 260VU_01.wav")
        .end();

    // --- Guitars ---
    // Acoustic folder: main, double, harmonics technique, outro strum section
    // Mandolin nested as a sub-instrument of acoustic guitar family
    let acoustic = TrackGroup::folder("Acoustic")
        .track("Mando")
        .item("12.Mando OptoComp_01.wav")
        .track("Acoustic 1")
        .item("08.Acoustic OptoComp_01.wav")
        .track("Acoustic 2")
        .item("09.Acoustic Dbl OptoComp_01.wav")
        .track("Harmonics")
        .item("10.Acoustic Harmonics OptoComp_01.wav")
        .track("Outro Strum")
        .item("11.Acoustic Outro Strum OptoComp_01.wav")
        .end();

    let guitars = TrackGroup::folder("Guitars")
        .group(acoustic)
        .track("Steel")
        .item("13.Steel Guitar OptoComp_01.wav")
        .end();

    // --- Keys ---
    let keys = TrackGroup::folder("Keys")
        .track("Piano 1")
        .item("14.Piano 260VU_01.wav")
        .track("Piano 2")
        .item("15.Piano Lead 260VU_01.wav")
        .end();

    // --- Vocals ---
    // Lead: main, triple (3-part stack), double
    // BGVs: harmony main and harmony double
    let lead = TrackGroup::folder("Lead")
        .track("Main")
        .item("16.Vocal OptoComp_01.wav")
        .track("Triple")
        .item("18.Vocal.Triple OptoComp_01.wav")
        .track("DBL")
        .item("17.Vocal.DBL OptoComp_01.wav")
        .end();

    let bgvs = TrackGroup::folder("BGVs")
        .track("Main")
        .item("19.Vocal.HARMONY OptoComp_01.wav")
        .track("DBL")
        .item("20.Vocal.HARMONY Dbl OptoComp_01.wav")
        .end();

    let vocals = TrackGroup::folder("Vocals").group(lead).group(bgvs).end();

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(bass)
        .group(guitars)
        .group(keys)
        .group(vocals)
        .track("Reference")
        .item("21.Heart Of Gold Mix_01.wav")
        .build();
    expected
}

#[test]
fn neil_young_heart_of_gold() {
    // -- Setup & Fixtures
    // Neil Young - Heart of Gold: Simple folk session with acoustic guitars, mandolin,
    // steel guitar, piano, and vocals. All tracked through OptoComp/260VU preamps.
    let items = vec![
        "01.Kick OptoComp_01.wav",
        "02.Snare No Compression_01.wav",
        "03.OH L 260VU_01.wav",
        "04.OH R 260VU_01.wav",
        "05.Bass DI OptoComp_01.wav",
        "06.Bass Amp 44A. 260VU_01.wav",
        "07.Bass Amp 421 260VU_01.wav",
        "08.Acoustic OptoComp_01.wav",
        "09.Acoustic Dbl OptoComp_01.wav",
        "10.Acoustic Harmonics OptoComp_01.wav",
        "11.Acoustic Outro Strum OptoComp_01.wav",
        "12.Mando OptoComp_01.wav",
        "13.Steel Guitar OptoComp_01.wav",
        "14.Piano 260VU_01.wav",
        "15.Piano Lead 260VU_01.wav",
        "16.Vocal OptoComp_01.wav",
        "17.Vocal.DBL OptoComp_01.wav",
        "18.Vocal.Triple OptoComp_01.wav",
        "19.Vocal.HARMONY OptoComp_01.wav",
        "20.Vocal.HARMONY Dbl OptoComp_01.wav",
        "21.Heart Of Gold Mix_01.wav",
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
    // Simple kit: kick, snare, OH L/R stereo pair
    let expected = neil_young_heart_of_gold_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
