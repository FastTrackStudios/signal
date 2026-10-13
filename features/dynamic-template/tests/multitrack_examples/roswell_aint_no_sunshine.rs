use daw_proto::{assert_tracks_equal, TrackStructureBuilder};
use dynamic_template::*;

fn roswell_aint_no_sunshine_expected() -> daw_proto::TrackHierarchy {
    let expected = TrackStructureBuilder::new()
        .folder("Drums")
        .track("Kick")
        .item("Kick.wav")
        .track("Snare")
        .item("Snare.wav")
        .folder("Toms")
        .track("Tom 3")
        .item("Tom  3.wav")
        .track("Tom 4")
        .item("Tom  4.wav")
        .end()
        .folder("Cymbals")
        .track("OH")
        .item("Overhead.wav")
        .track("Hi Hat")
        .item("HiHat.wav")
        .end()
        .end()
        .folder("Percussion")
        .folder("Djembe")
        .track("Bottom")
        .item("01.Djembe Bottom 47X_01.wav")
        .track("Top 1")
        .item("02.Djembe Top 67X_01.wav")
        .track("Top 2")
        .item("Djembe Top Quantized.wav")
        .end()
        .track("Sidestick")
        .item("Sidestick.wav")
        .end()
        .folder("Bass")
        .track("Bass")
        .item("03.Bass DI.01_02.wav")
        .track("Amp")
        .item("04.Bass Amp 47X_01.wav")
        .end()
        .folder("Guitars")
        .folder("Electric")
        .track("Warren")
        .item("07.Gtr Warren Amp.01_02.wav")
        .folder("Solo")
        .track("Amp")
        .item("09.Gtr Solo Amp.67X_01.wav")
        .track("DI")
        .item("08.Gtr Solo DI.01_02.wav")
        .end()
        .folder("Electric")
        .track("Amp")
        .item("06.Gtr Amp.67X_01.wav")
        .track("DI")
        .item("05.Gtr DI.01_02.wav")
        .end()
        .end()
        .track("Acoustic")
        .item("10.Acoustic 47X_01.wav")
        .end()
        .track("Vocals")
        .item("11.Vocal.47X_01.wav")
        .folder("Orchestra")
        .track("Strings 1")
        .item("STRINGS HIGH.wav")
        .track("Strings 2")
        .item("STRINGS MAIN.wav")
        .end()
        .track("Reference")
        .item("12.Ain't No Sunshine Mix_Roswell Mini K47x Mini K67x_02.wav")
        .build();
    expected
}

#[test]
fn roswell_aint_no_sunshine() {
    // -- Setup & Fixtures
    // Roswell - Ain't No Sunshine: Mix with djembe, guitars, strings and reference
    let items = vec![
        "01.Djembe Bottom 47X_01.wav",
        "02.Djembe Top 67X_01.wav",
        "03.Bass DI.01_02.wav",
        "04.Bass Amp 47X_01.wav",
        "05.Gtr DI.01_02.wav",
        "06.Gtr Amp.67X_01.wav",
        "07.Gtr Warren Amp.01_02.wav",
        "08.Gtr Solo DI.01_02.wav",
        "09.Gtr Solo Amp.67X_01.wav",
        "10.Acoustic 47X_01.wav",
        "11.Vocal.47X_01.wav",
        "12.Ain't No Sunshine Mix_Roswell Mini K47x Mini K67x_02.wav",
        "Djembe Top Quantized.wav",
        "HiHat.wav",
        "Kick.wav",
        "Overhead.wav",
        "Sidestick.wav",
        "Snare.wav",
        "STRINGS HIGH.wav",
        "STRINGS MAIN.wav",
        "Tom  3.wav",
        "Tom  4.wav",
    ];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    // Expected structure:
    // Drums/
    //   ├─ Kick                    ← Kick.wav
    //   ├─ Snare                   ← Snare.wav
    //   ├─ Toms/
    //   │   ├─ Tom 3               ← Tom  3.wav
    //   │   └─ Tom 4               ← Tom  4.wav
    //   └─ Cymbals/
    //       ├─ OH                  ← Overhead.wav
    //       └─ Hi Hat              ← HiHat.wav
    // Percussion/
    //   ├─ Djembe/
    //   │   ├─ Bottom              ← 01.Djembe Bottom 47X_01.wav
    //   │   ├─ Top 1               ← 02.Djembe Top 67X_01.wav
    //   │   └─ Top 2               ← Djembe Top Quantized.wav
    //   └─ Sidestick               ← Sidestick.wav (now matches Percussion group)
    // Bass/
    //   ├─ Bass                    ← 03.Bass DI.01_02.wav (DI)
    //   └─ Amp                     ← 04.Bass Amp 47X_01.wav
    // Guitars/
    //   ├─ Electric/               ← all electric guitars grouped
    //   │   ├─ Warren              ← 07.Gtr Warren Amp (performer=Warren, MultiMic=Amp)
    //   │   ├─ Solo/
    //   │   │   ├─ Amp             ← 09.Gtr Solo Amp.67X
    //   │   │   └─ DI              ← 08.Gtr Solo DI.01
    //   │   └─ Electric/
    //   │       ├─ Amp             ← 06.Gtr Amp.67X
    //   │       └─ DI              ← 05.Gtr DI.01
    //   └─ Acoustic                ← 10.Acoustic 47X_01.wav
    // Vocals                       ← 11.Vocal.47X_01.wav
    // Orchestra/
    //   ├─ Strings 1               ← STRINGS HIGH.wav
    //   └─ Strings 2               ← STRINGS MAIN.wav
    // Reference                    ← 12.Ain't No Sunshine Mix_Roswell Mini K47x Mini K67x_02.wav
    let expected = roswell_aint_no_sunshine_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
