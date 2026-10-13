use daw_proto::{assert_tracks_equal, TrackStructureBuilder};
use dynamic_template::*;

fn derek_and_the_dominos_layla_expected_a() -> TrackStructureBuilder {
    TrackStructureBuilder::new()
        .track("Drums")
        .item("01. Drums_01.wav")
        .folder("Bass")
        .track("Bass")
        .item("02.Bass DI_01.wav")
        .track("Amp")
        .item("03.Bass Amp_01.wav")
        .end()
        .folder("Guitars")
        .folder("Electric")
        .folder("Lead")
        .folder("Main")
        .folder("Amp")
        .track("Amp 1")
        .item("09.Lead Harm Gtr Amp_02.wav")
        .track("Amp 2")
        .item("13. Lead Gtr Amp_02.wav")
        .end()
        .folder("DI")
        .track("DI 1")
        .item("08.Lead Harm Gtr DI_02.wav")
        .track("DI 2")
        .item("12. Lead Gtr DI_02.wav")
        .end()
        .end()
        .folder("DBL")
        .folder("Amp")
        .track("Amp 1")
        .item("11.Lead Harm Gtr Amp Dbl_02.wav")
        .track("Amp 2")
        .item("15. Lead Gtr Amp Dbl_02.wav")
        .end()
        .folder("DI")
        .track("Lead 1")
        .item("10.Lead Harm Gtr DI Dbl_02.wav")
        .track("Lead 2")
        .item("14. Lead Gtr DI Dbl_02.wav")
        .end()
        .end()
        .end()
        .folder("Rhythm")
        .folder("Main")
        .track("Amp")
        .item("05.Rhythm Gtr Amp_02.wav")
        .track("DI")
        .item("04.Rhythm Gtr DI_02.wav")
        .end()
        .folder("DBL")
        .track("Amp")
        .item("07.Rhythm Gtr Amp Dbl_02.wav")
        .track("DI")
        .item("06.Rhythm Gtr DI Dbl_02.wav")
        .end()
        .end()
        .folder("Solo")
        .track("Amp")
        .item("16. Solo Gtr Amp_02.wav")
        .track("DI")
        .item("16. Solo Gtr DI_02.wav")
        .end()
        .folder("Outro")
        .folder("Main")
        .track("Amp")
        .item("18. Outro Gtr Amp_01.wav")
        .track("DI")
        .item("17. Outro Gtr DI_01.wav")
        .end()
        .folder("DBL")
        .track("Amp")
        .item("20. Outro Gtr Amp Dbl_01.wav")
        .track("DI")
        .item("19. Outro Gtr DI Dbl_01.wav")
        .end()
        .end()
        .end()
        .folder("Acoustic")
        .track("Acoustic 1")
        .item("21. Acoustic_01.wav")
        .track("Acoustic 2")
        .item("22. Acoustic Dbl_01.wav")
        .track("Acoustic 3")
        .item("23.Acoustic Lead Line_01.wav")
        .track("Acoustic 4")
        .item("24.Acoustic Lead Line Dbl_01.wav")
        .track("Outro")
        .item("25.Acoustic Outro_01.wav")
        .end()
        .end()
}

fn derek_and_the_dominos_layla_expected() -> daw_proto::TrackHierarchy {
    derek_and_the_dominos_layla_expected_a()
        .track("Keys")
        .item("26. Piano_01.wav")
        .folder("Vocals")
        .track("Lead")
        .item("27. Vocal_02.wav")
        .folder("BGVs")
        .folder("A")
        .track("A 1")
        .item("28.BG A Harm 1_02.wav")
        .track("A 2")
        .item("29.BG A Harm 2_02.wav")
        .track("A 3")
        .item("30.BG A Harm 3_02.wav")
        .track("A 4")
        .item("31.BG A Harm 4_02.wav")
        .end()
        .folder("B")
        .track("B 1")
        .item("32.BG B Harm 1_02.wav")
        .track("B 2")
        .item("33.BG B Harm 2_02.wav")
        .track("B 3")
        .item("34.BG B Harm 3_02.wav")
        .track("B 4")
        .item("35.BG B Harm 4_02.wav")
        .end()
        .folder("Harm 1")
        .track("C")
        .item("36.BG C Harm 1_02.wav")
        .track("Harm 1")
        .item("40.BG D Harm 1_02.wav")
        .track("Harm 2")
        .item("48.BG E Harm 1_02.wav")
        .end()
        .folder("Harm 2")
        .track("C")
        .item("37.BG C Harm 2_02.wav")
        .track("Harm 1")
        .item("41.BG D Harm 2_02.wav")
        .track("Harm 2")
        .item("49.BG E Harm 2_02.wav")
        .end()
        .folder("Harm 3")
        .track("C")
        .item("38.BG C Harm 3_02.wav")
        .track("Harm 1")
        .item("42.BG D Harm 3_02.wav")
        .track("Harm 2")
        .item("50.BG E Harm 3_02.wav")
        .end()
        .folder("BGVs")
        .track("BGVs 1")
        .item("45.BG D Harm 6_02.wav")
        .track("BGVs 2")
        .item("46.BG D Harm 7_02.wav")
        .track("BGVs 3")
        .item("47.BG D Harm 8_02.wav")
        .end()
        .folder("BGVs 4")
        .track("C")
        .item("39.BG C Harm 4_02.wav")
        .track("4 1")
        .item("43.BG D Harm 4_02.wav")
        .track("4 2")
        .item("51.BG E Harm 4_02.wav")
        .end()
        .track("BGVs 5")
        .item("44.BG D Harm 5_02.wav")
        .end()
        .end()
        .folder("Reference")
        .track("Soyuz Bomblet Layla Cover Mix 1")
        .item("52.Soyuz Bomblet Layla Cover Mix_01.L.wav")
        .track("Soyuz Bomblet Layla Cover Mix 2")
        .item("52.Soyuz Bomblet Layla Cover Mix_01.R.wav")
        .end()
        .build()
}

#[test]
fn derek_and_the_dominos_layla() {
    // -- Setup & Fixtures
    // Derek and the Dominos - Layla: 52-track session with extensive guitar multi-tracking and BG harmonies
    let items = vec![
        "01. Drums_01.wav",
        "02.Bass DI_01.wav",
        "03.Bass Amp_01.wav",
        "04.Rhythm Gtr DI_02.wav",
        "05.Rhythm Gtr Amp_02.wav",
        "06.Rhythm Gtr DI Dbl_02.wav",
        "07.Rhythm Gtr Amp Dbl_02.wav",
        "08.Lead Harm Gtr DI_02.wav",
        "09.Lead Harm Gtr Amp_02.wav",
        "10.Lead Harm Gtr DI Dbl_02.wav",
        "11.Lead Harm Gtr Amp Dbl_02.wav",
        "12. Lead Gtr DI_02.wav",
        "13. Lead Gtr Amp_02.wav",
        "14. Lead Gtr DI Dbl_02.wav",
        "15. Lead Gtr Amp Dbl_02.wav",
        "16. Solo Gtr Amp_02.wav",
        "16. Solo Gtr DI_02.wav",
        "17. Outro Gtr DI_01.wav",
        "18. Outro Gtr Amp_01.wav",
        "19. Outro Gtr DI Dbl_01.wav",
        "20. Outro Gtr Amp Dbl_01.wav",
        "21. Acoustic_01.wav",
        "22. Acoustic Dbl_01.wav",
        "23.Acoustic Lead Line_01.wav",
        "24.Acoustic Lead Line Dbl_01.wav",
        "25.Acoustic Outro_01.wav",
        "26. Piano_01.wav",
        "27. Vocal_02.wav",
        "28.BG A Harm 1_02.wav",
        "29.BG A Harm 2_02.wav",
        "30.BG A Harm 3_02.wav",
        "31.BG A Harm 4_02.wav",
        "32.BG B Harm 1_02.wav",
        "33.BG B Harm 2_02.wav",
        "34.BG B Harm 3_02.wav",
        "35.BG B Harm 4_02.wav",
        "36.BG C Harm 1_02.wav",
        "37.BG C Harm 2_02.wav",
        "38.BG C Harm 3_02.wav",
        "39.BG C Harm 4_02.wav",
        "40.BG D Harm 1_02.wav",
        "41.BG D Harm 2_02.wav",
        "42.BG D Harm 3_02.wav",
        "43.BG D Harm 4_02.wav",
        "44.BG D Harm 5_02.wav",
        "45.BG D Harm 6_02.wav",
        "46.BG D Harm 7_02.wav",
        "47.BG D Harm 8_02.wav",
        "48.BG E Harm 1_02.wav",
        "49.BG E Harm 2_02.wav",
        "50.BG E Harm 3_02.wav",
        "51.BG E Harm 4_02.wav",
        "52.Soyuz Bomblet Layla Cover Mix_01.L.wav",
        "52.Soyuz Bomblet Layla Cover Mix_01.R.wav",
    ];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    // Expected structure:
    // Drums                         ← 01. Drums_01.wav
    // Bass/
    //   ├─ Bass                     ← 02.Bass DI_01.wav
    //   └─ Amp                      ← 03.Bass Amp_01.wav
    // Guitars/
    //   ├─ Electric/
    //   │   ├─ Lead/
    //   │   │   ├─ Main/ → Amp(×2), DI(×2)
    //   │   │   └─ DBL/ → Amp(×2), DI(×2)
    //   │   ├─ Rhythm/
    //   │   │   ├─ Main/ → Amp, DI
    //   │   │   └─ DBL/ → Amp, DI
    //   │   ├─ Solo/ → Amp, DI
    //   │   └─ Outro/
    //   │       ├─ Main/ → Amp, DI
    //   │       └─ DBL/ → Amp, DI
    //   └─ Acoustic/
    //       ├─ Acoustic 1-4
    //       └─ Outro
    // Keys                          ← 26. Piano_01.wav
    // Vocals/
    //   ├─ Lead                     ← 27. Vocal_02.wav
    //   └─ BGVs/                    ← BG A-E Harm 1-8 (now recognized via "bg" pattern)
    //       ├─ A/                   ← A 1-4
    //       ├─ B/                   ← B 1-4
    //       ├─ Harm 1-3/            ← C/D/E Harm 1-3 (grouped by harmony number)
    //       ├─ BGVs/                ← D Harm 6-8 (fallback for unmatched)
    //       ├─ BGVs 4/              ← C/D/E Harm 4
    //       └─ BGVs 5               ← D Harm 5
    // Reference/
    //   ├─ 52.Soyuz... Mix 1        ← L channel
    //   └─ 52.Soyuz... Mix 2        ← R channel
    let expected = derek_and_the_dominos_layla_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
