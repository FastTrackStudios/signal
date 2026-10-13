use daw_proto::{assert_tracks_equal, TrackStructureBuilder};
use dynamic_template::*;

fn jared_james_nichols_expected_a() -> TrackStructureBuilder {
    TrackStructureBuilder::new()
        .folder("Drums")
        .folder("Kick")
        .folder("SUM")
        .track("In")
        .item("01 Kick In .wav")
        .track("Out")
        .item("02 Kick Out .wav")
        .end()
        .track("Kick")
        .item("03 Sub Kick .wav")
        .end()
        .folder("Snare")
        .folder("Top")
        .track("Top 1")
        .item("04 Snare Top .wav")
        .track("Top 2")
        .item("05 Snare Top.dup2 .wav")
        .end()
        .track("Bottom")
        .item("06 Snare Bot .wav")
        .end()
        .folder("Toms")
        .track("Floor")
        .item("09 Floor .wav")
        .track("Toms")
        .item("08 Rack .wav")
        .end()
        .folder("Cymbals")
        .folder("Hi Hat")
        .track("Hi Hat 1")
        .item("07 Hat .wav")
        .track("Hi Hat 2")
        .item("14 Close RM Hat .wav")
        .track("Hi Hat 3")
        .item("16 Mid RM Hat .wav")
        .track("Hi Hat 4")
        .item("18 Far RM Hat .wav")
        .end()
        .folder("Ride")
        .track("Ride 1")
        .item("15 Close RM Ride .wav")
        .track("Ride 2")
        .item("17 Mid RM Ride .wav")
        .track("Ride 3")
        .item("19 Far RM Ride .wav")
        .end()
        .folder("OH")
        .track("Hi Hat")
        .item("10 OH Hat .wav")
        .track("Ride")
        .item("11 OH Ride .wav")
        .track("OH")
        .item("12 OH Mono .wav")
        .end()
        .end()
        .track("Rooms")
        .item("13 Crotch .wav")
        .end()
}

fn jared_james_nichols_expected() -> daw_proto::TrackHierarchy {
    jared_james_nichols_expected_a()
        .folder("Bass")
        .track("Bass 1")
        .item("22 Bass DI .wav")
        .track("Bass 2")
        .item("23 Bass Mic .wav")
        .end()
        .folder("Guitars")
        .folder("Solo")
        .track("Left")
        .item("27 SoloGtr RM Left .wav")
        .track("Right")
        .item("28 SoloGtr RM Right .wav")
        .track("Electric")
        .item("26 SoloGtr Bus.d .wav")
        .end()
        .folder("Guitars")
        .track("Electric 1")
        .item("24 Gtr Bus .wav")
        .track("Electric 2")
        .item("25 Gtr Bus.dup1 .wav")
        .end()
        .end()
        .track("SFX")
        .item("29 Talk Box .wav")
        .track("Guide")
        .item("Smart Tempo Multitrack Set 1.wav")
        .track("Reference")
        .item("Man In the Box Print 20220502 v2 .wav")
        .folder("Unsorted")
        .track("Mono")
        .item("20 Mono .wav")
        .track("Mono")
        .item("21 Mono U47 .wav")
        .track("Jared")
        .item("30 Jared .wav")
        .track("Jared Call Back")
        .item("31 Jared call back .wav")
        .track("Jared Harm 1")
        .item("32 Jared Harm .wav")
        .track("Jared Harm 2")
        .item("33 Jared Harm.dup1 .wav")
        .end()
        .build()
}

#[test]
fn jared_james_nichols() {
    // -- Setup & Fixtures
    // Jared James Nichols: 35-track rock session with detailed drum room mics and guitar buses
    let items = vec![
        "01 Kick In .wav",
        "02 Kick Out .wav",
        "03 Sub Kick .wav",
        "04 Snare Top .wav",
        "05 Snare Top.dup2 .wav",
        "06 Snare Bot .wav",
        "07 Hat .wav",
        "08 Rack .wav",
        "09 Floor .wav",
        "10 OH Hat .wav",
        "11 OH Ride .wav",
        "12 OH Mono .wav",
        "13 Crotch .wav",
        "14 Close RM Hat .wav",
        "15 Close RM Ride .wav",
        "16 Mid RM Hat .wav",
        "17 Mid RM Ride .wav",
        "18 Far RM Hat .wav",
        "19 Far RM Ride .wav",
        "20 Mono .wav",
        "21 Mono U47 .wav",
        "22 Bass DI .wav",
        "23 Bass Mic .wav",
        "24 Gtr Bus .wav",
        "25 Gtr Bus.dup1 .wav",
        "26 SoloGtr Bus.d .wav",
        "27 SoloGtr RM Left .wav",
        "28 SoloGtr RM Right .wav",
        "29 Talk Box .wav",
        "30 Jared .wav",
        "31 Jared call back .wav",
        "32 Jared Harm .wav",
        "33 Jared Harm.dup1 .wav",
        "Man In the Box Print 20220502 v2 .wav",
        "Smart Tempo Multitrack Set 1.wav",
    ];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    // Expected structure:
    // Drums/
    //   ├─ Kick/
    //   │   ├─ SUM/                 ← In/Out
    //   │   └─ Kick                 ← Sub Kick
    //   ├─ Snare/
    //   │   ├─ Top/                 ← Top 1/2 (dup)
    //   │   └─ Bottom               ← Snare Bot
    //   ├─ Toms/
    //   │   ├─ Floor                ← 09 Floor
    //   │   └─ Toms                 ← 08 Rack
    //   ├─ Cymbals/
    //   │   ├─ Hi Hat/              ← Hat, Close RM Hat, Mid RM Hat, Far RM Hat
    //   │   ├─ Ride/                ← Close/Mid/Far RM Ride
    //   │   └─ OH/                  ← OH Hat, OH Ride, OH Mono
    //   └─ Rooms                    ← Crotch (now matches Drums/Rooms group)
    // Bass/                         ← Bass DI, Bass Mic
    // Guitars/                      ← Solo folder (SoloGtr tracks) + Guitars folder (Gtr Bus)
    // Guide                         ← Smart Tempo Multitrack Set 1
    // Reference                     ← Man In the Box Print
    // Unsorted/                     ← Mono, Mono U47, Talk Box, Jared vox
    let expected = jared_james_nichols_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
