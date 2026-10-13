use daw_proto::{assert_tracks_equal, TrackStructureBuilder};
use dynamic_template::*;

fn led_zeppelin_the_ocean_expected() -> daw_proto::TrackHierarchy {
    let expected = TrackStructureBuilder::new()
        .folder("Drums")
        .track("Kick")
        .item("01.Kick_01.wav")
        .track("Snare")
        .item("02.SNR_01.wav")
        .folder("OH")
        .track("OH 1")
        .item("03. OHL_01.wav")
        .track("OH 2")
        .item("04. OHR_01.wav")
        .end()
        .folder("Rooms")
        .track("L")
        .item("05. Room_01 L.wav")
        .track("R")
        .item("06. Room_01 R.wav")
        .end()
        .end()
        .folder("Bass")
        .track("Bass")
        .item("07. Bass DI_01.wav")
        .track("Amp")
        .item("08. Bass ReAmp_01.wav")
        .end()
        .folder("Guitars")
        .folder("L")
        .track("Electric 1")
        .item("09. Gtr L Main Riff.A1_01.wav")
        .track("Electric 2")
        .item("10. Gtr L Main Riff.A2_01.wav")
        .track("Electric 3")
        .item("11. Gtr_L.A1_01.wav")
        .track("Electric 4")
        .item("12. Gtr_L.A2_01.wav")
        .end()
        .folder("R")
        .track("Electric 1")
        .item("13. Gtr_R.A1_01.wav")
        .track("Electric 2")
        .item("14. Gtr_R.A2_01.wav")
        .end()
        .end()
        .folder("Vocals")
        .folder("Lead")
        .folder("Outro")
        .track("Outro 1")
        .item("23. Outro Vocal 1_01.wav")
        .track("Outro 2")
        .item("24. Outro Vocal 2_01.wav")
        .track("Outro 3")
        .item("25. Outro Vocal 3_01.wav")
        .track("Outro 4")
        .item("26. Outro Vocal 4_01.wav")
        .track("Outro 5")
        .item("27. Outro Vocal 5_01.wav")
        .end()
        .track("Lead")
        .item("19. lead_Vox_01.wav")
        .track("Lead 1")
        .item("20. LA LA Voc 1_01.wav")
        .track("Lead 2")
        .item("21. LA LA Voc 2_01.wav")
        .end()
        .track("BGVs")
        .item("22. Vocal Adlib_01.wav")
        .end()
        .folder("Unsorted")
        .track("Lead_1.A1")
        .item("15. lead_1.A1_01.wav")
        .track("Lead_1.A2")
        .item("16. lead_1.A2_01.wav")
        .track("Lead_2.A1")
        .item("17. lead_2.A1_01.wav")
        .track("Lead_2.A2")
        .item("18. lead_2.A2_01.wav")
        .track("The Ocean")
        .item("28. The Ocean_01.wav")
        .end()
        .build();
    expected
}

#[test]
fn led_zeppelin_the_ocean() {
    // -- Setup & Fixtures
    // Led Zeppelin - The Ocean: 28-track classic rock session with multi-tracked guitars and layered vocals
    let items = vec![
        "01.Kick_01.wav",
        "02.SNR_01.wav",
        "03. OHL_01.wav",
        "04. OHR_01.wav",
        "05. Room_01 L.wav",
        "06. Room_01 R.wav",
        "07. Bass DI_01.wav",
        "08. Bass ReAmp_01.wav",
        "09. Gtr L Main Riff.A1_01.wav",
        "10. Gtr L Main Riff.A2_01.wav",
        "11. Gtr_L.A1_01.wav",
        "12. Gtr_L.A2_01.wav",
        "13. Gtr_R.A1_01.wav",
        "14. Gtr_R.A2_01.wav",
        "15. lead_1.A1_01.wav",
        "16. lead_1.A2_01.wav",
        "17. lead_2.A1_01.wav",
        "18. lead_2.A2_01.wav",
        "19. lead_Vox_01.wav",
        "20. LA LA Voc 1_01.wav",
        "21. LA LA Voc 2_01.wav",
        "22. Vocal Adlib_01.wav",
        "23. Outro Vocal 1_01.wav",
        "24. Outro Vocal 2_01.wav",
        "25. Outro Vocal 3_01.wav",
        "26. Outro Vocal 4_01.wav",
        "27. Outro Vocal 5_01.wav",
        "28. The Ocean_01.wav",
    ];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    // Expected structure:
    // Drums/
    //   ├─ Kick               ← 01.Kick_01.wav
    //   ├─ Snare              ← 02.SNR_01.wav
    //   ├─ OH/
    //   │   ├─ OH 1           ← 03. OHL_01.wav (now recognized via "ohl" pattern)
    //   │   └─ OH 2           ← 04. OHR_01.wav (now recognized via "ohr" pattern)
    //   └─ Rooms/
    //       ├─ L              ← 05. Room_01 L.wav
    //       └─ R              ← 06. Room_01 R.wav
    // Bass/
    //   ├─ Bass               ← 07. Bass DI_01.wav (DI)
    //   └─ Amp                ← 08. Bass ReAmp_01.wav (ReAmp)
    // Guitars/
    //   ├─ L/                  ← Channel L group (all Gtr L tracks)
    //   │   ├─ Electric 1      ← 09. Gtr L Main Riff.A1_01.wav
    //   │   ├─ Electric 2      ← 10. Gtr L Main Riff.A2_01.wav
    //   │   ├─ Electric 3      ← 11. Gtr_L.A1_01.wav
    //   │   └─ Electric 4      ← 12. Gtr_L.A2_01.wav
    //   └─ R/                  ← Channel R group
    //       ├─ Electric 1      ← 13. Gtr_R.A1_01.wav
    //       └─ Electric 2      ← 14. Gtr_R.A2_01.wav
    // Vocals/
    //   ├─ Outro/
    //   │   ├─ Outro 1        ← 23. Outro Vocal 1_01.wav
    //   │   ├─ Outro 2        ← 24. Outro Vocal 2_01.wav
    //   │   ├─ Outro 3        ← 25. Outro Vocal 3_01.wav
    //   │   ├─ Outro 4        ← 26. Outro Vocal 4_01.wav
    //   │   └─ Outro 5        ← 27. Outro Vocal 5_01.wav
    //   ├─ BGVs/
    //   │   └─ Vocal Adlib    ← 22. Vocal Adlib_01.wav (now matches BGVs via "adlib")
    //   ├─ Vocals/
    //   │   └─ Lead           ← 19. lead_Vox_01.wav
    //   ├─ Vocals 1           ← 20. LA LA Voc 1_01.wav
    //   └─ Vocals 2           ← 21. LA LA Voc 2_01.wav
    // Unsorted/
    //   ├─ 15. Lead_1.A1      ← 15. lead_1.A1_01.wav (guitar lead - ambiguous)
    //   ├─ 16. Lead_1.A2      ← 16. lead_1.A2_01.wav
    //   ├─ 17. Lead_2.A1      ← 17. lead_2.A1_01.wav
    //   ├─ 18. Lead_2.A2      ← 18. lead_2.A2_01.wav
    //   └─ 28. The Ocean      ← 28. The Ocean_01.wav (mix reference)
    let expected = led_zeppelin_the_ocean_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
