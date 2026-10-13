use daw_proto::{assert_tracks_equal, TrackStructureBuilder};
use dynamic_template::*;

fn katie_ferrara_valerie_expected() -> daw_proto::TrackHierarchy {
    let expected = TrackStructureBuilder::new()
        .folder("Drums")
        .track("Kick")
        .item("01.Kick_01-01.wav")
        .track("Snare")
        .item("02.Snare_01-01.wav")
        .track("OH")
        .item("03.OH_01-01.wav")
        .end()
        .folder("Bass")
        .track("Bass")
        .item("04.Bass DI_01-01.wav")
        .track("Amp")
        .item("05.Bass Amp_01-01.wav")
        .end()
        .folder("Guitars")
        .folder("Electric")
        .folder("Solo")
        .track("Amp")
        .item("12.Gtr Amp.Solo_01-01.wav")
        .track("DI")
        .item("11.Gtr DI.Solo_01-01.wav")
        .end()
        .folder("Electric")
        .folder("Amp")
        .track("Amp 1")
        .item("08.Gtr Amp_01-01.wav")
        .track("Amp 2")
        .item("10.Gtr Amp.Riff_01-01.wav")
        .end()
        .folder("DI")
        .track("DI 1")
        .item("07.Gtr DI_01-01.wav")
        .track("DI 2")
        .item("09.Gtr DI.Riff_01-01.wav")
        .end()
        .end()
        .end()
        .track("Acoustic")
        .item("06.Acoustic_01-01.wav")
        .end()
        .folder("Vocals")
        .folder("Lead")
        .track("Lead 1")
        .item("13.Vocal_01-01.wav")
        .track("Plate")
        .item("19.Vocal.Eko.Plate_01-01.wav")
        .track("Lead 2")
        .item("20.Vocal.Magic_01-01.wav")
        .end()
        .folder("BGVs")
        .track("Main")
        .item("14.KD.BV_01-01.wav")
        .track("DBL")
        .item("15.KD.BV Dbl_01-01.wav")
        .end()
        .end()
        .folder("SFX")
        .track("H3000.One")
        .item("16.H3000.One_01-01.wav")
        .track("H3000.Two")
        .item("17.H3000.Two_01-01.wav")
        .track("H3000.Three")
        .item("18.H3000.Three_01-01.wav")
        .end()
        .track("Reference")
        .item("21.Valerie Mix_01-01.wav")
        .build();
    expected
}

#[test]
fn katie_ferrara_valerie() {
    // -- Setup & Fixtures
    // Katie Ferrara - Valerie: 21-track pop cover with DI/Amp guitar pairs and vocal effects tracks
    let items = vec![
        "01.Kick_01-01.wav",
        "02.Snare_01-01.wav",
        "03.OH_01-01.wav",
        "04.Bass DI_01-01.wav",
        "05.Bass Amp_01-01.wav",
        "06.Acoustic_01-01.wav",
        "07.Gtr DI_01-01.wav",
        "08.Gtr Amp_01-01.wav",
        "09.Gtr DI.Riff_01-01.wav",
        "10.Gtr Amp.Riff_01-01.wav",
        "11.Gtr DI.Solo_01-01.wav",
        "12.Gtr Amp.Solo_01-01.wav",
        "13.Vocal_01-01.wav",
        "14.KD.BV_01-01.wav",
        "15.KD.BV Dbl_01-01.wav",
        "16.H3000.One_01-01.wav",
        "17.H3000.Two_01-01.wav",
        "18.H3000.Three_01-01.wav",
        "19.Vocal.Eko.Plate_01-01.wav",
        "20.Vocal.Magic_01-01.wav",
        "21.Valerie Mix_01-01.wav",
    ];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    // Expected structure:
    // Drums/
    //   ├─ Kick                     ← 01.Kick
    //   ├─ Snare                    ← 02.Snare
    //   └─ OH                       ← 03.OH
    // Bass/
    //   ├─ Bass                     ← 04.Bass DI
    //   └─ Amp                      ← 05.Bass Amp
    // Guitars/
    //   ├─ Electric/
    //   │   ├─ Solo/
    //   │   │   ├─ Amp              ← 12.Gtr Amp.Solo
    //   │   │   └─ DI               ← 11.Gtr DI.Solo
    //   │   └─ Electric/            ← no-arrangement group
    //   │       ├─ Amp/
    //   │       │   ├─ Amp 1        ← 08.Gtr Amp
    //   │       │   └─ Amp 2        ← 10.Gtr Amp.Riff
    //   │       └─ DI/
    //   │           ├─ DI 1         ← 07.Gtr DI
    //   │           └─ DI 2         ← 09.Gtr DI.Riff
    //   └─ Acoustic                 ← 06.Acoustic
    // Vocals/
    //   ├─ Lead/
    //   │   ├─ Lead 1               ← 13.Vocal
    //   │   ├─ Plate                ← 19.Vocal.Eko.Plate (vocal effect)
    //   │   └─ Lead 2               ← 20.Vocal.Magic (vocal effect)
    //   └─ BGVs/
    //       ├─ Main                 ← 14.KD.BV
    //       └─ DBL                  ← 15.KD.BV Dbl
    // SFX/                          ← H3000 effects (Eventide H3000)
    // Reference                     ← 21.Valerie Mix
    let expected = katie_ferrara_valerie_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
