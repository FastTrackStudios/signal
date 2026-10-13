use daw_proto::{assert_tracks_equal, TrackStructureBuilder};
use dynamic_template::*;

fn gloria_anderson_households_expected() -> daw_proto::TrackHierarchy {
    let expected = TrackStructureBuilder::new()
        .folder("Drums")
        .folder("Kick")
        .track("SUM")
        .item("03.Kick Out LIM_01.wav")
        .track("Lim")
        .item("02.Kick LIM_01.wav")
        .end()
        .folder("Snare")
        .track("Snare 1")
        .item("04.Sn T_01.wav")
        .track("Snare 2")
        .item("05.Sn B_01.wav")
        .end()
        .folder("Toms")
        .track("T1")
        .item("07.Tom 1 EDT_01.wav")
        .track("T2")
        .item("08.Tom 2 EDT_01.wav")
        .end()
        .folder("Cymbals")
        .track("OH")
        .item("09.OH_01.wav")
        .track("Hi Hat")
        .item("06.Hat_01.wav")
        .end()
        .track("Rooms")
        .item("10.Room_01.wav")
        .end()
        .folder("Bass")
        .track("Bass 1")
        .item("11.Bass Body_01.wav")
        .track("Bass 2")
        .item("12.Bass Neck_01.wav")
        .end()
        .folder("Guitars")
        .folder("Electric")
        .track("L")
        .item("Households_TimHoek_NashGtrL.wav")
        .track("R")
        .item("Households_TimHoek_NashGtrR.wav")
        .end()
        .folder("Acoustic")
        .track("Acoustic 1")
        .item("01.Households ACO JC MIXX_01.wav")
        .track("Acoustic 2")
        .item("13.AG 1_01.wav")
        .end()
        .track("Steel")
        .item("15.Steel_01.wav")
        .end()
        .track("Keys")
        .item("14.Piano_01.wav")
        // Choir now routes to the top-level Choir group (separate from BGVs),
        // per the Lead / BGVs / Choir vocal taxonomy. It's a sibling of the
        // Vocals bus (route it to the vocal VCA in the DAW).
        .folder("Vocals")
        .track("Lead")
        .item("16.Vocal_01.wav")
        .track("BGVs")
        .item("Vocal harmony (D).wav")
        .end()
        .track("Choir")
        .item("Choir.wav")
        .folder("Orchestra")
        .track("Strings Tim")
        .item("Households_TimHoek_Strings.wav")
        .track("Strings")
        .item("Strings (R).wav")
        .end()
        .build();
    expected
}

#[test]
fn gloria_anderson_households() {
    // -- Setup & Fixtures
    // Gloria Anderson - Households: 22-track session with Nashville guitars and string arrangements
    let items = vec![
        "01.Households ACO JC MIXX_01.wav",
        "02.Kick LIM_01.wav",
        "03.Kick Out LIM_01.wav",
        "04.Sn T_01.wav",
        "05.Sn B_01.wav",
        "06.Hat_01.wav",
        "07.Tom 1 EDT_01.wav",
        "08.Tom 2 EDT_01.wav",
        "09.OH_01.wav",
        "10.Room_01.wav",
        "11.Bass Body_01.wav",
        "12.Bass Neck_01.wav",
        "13.AG 1_01.wav",
        "14.Piano_01.wav",
        "15.Steel_01.wav",
        "16.Vocal_01.wav",
        "Choir.wav",
        "Households_TimHoek_NashGtrL.wav",
        "Households_TimHoek_NashGtrR.wav",
        "Households_TimHoek_Strings.wav",
        "Strings (R).wav",
        "Vocal harmony (D).wav",
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
    //   │   ├─ SUM                  ← 03.Kick Out LIM
    //   │   └─ Lim                  ← 02.Kick LIM
    //   ├─ Snare/
    //   │   ├─ Snare 1              ← 04.Sn T (Top)
    //   │   └─ Snare 2              ← 05.Sn B (Bottom)
    //   ├─ Toms/
    //   │   ├─ T1                   ← 07.Tom 1 EDT
    //   │   └─ T2                   ← 08.Tom 2 EDT
    //   ├─ Cymbals/
    //   │   ├─ OH                   ← 09.OH
    //   │   └─ Hi Hat               ← 06.Hat
    //   └─ Rooms                    ← 10.Room
    // Bass/                         ← Body, Neck
    // Guitars/
    //   ├─ Electric/                ← NashGtr tracks now grouped as Electric with L/R channels
    //   │   ├─ L                    ← Households_TimHoek_NashGtrL
    //   │   └─ R                    ← Households_TimHoek_NashGtrR
    //   ├─ Acoustic/
    //   │   ├─ Acoustic 1           ← 01.Households ACO JC MIXX
    //   │   └─ Acoustic 2           ← 13.AG 1
    //   └─ Steel                    ← 15.Steel
    // Keys                          ← 14.Piano
    // Vocals/
    //   ├─ Lead                     ← 16.Vocal
    //   └─ BGVs                     ← Vocal harmony (D)
    // Choir                         ← Choir (top-level, route to vocal VCA)
    // Orchestra/
    //   ├─ Strings Tim              ← Households_TimHoek_Strings
    //   └─ Strings                  ← Strings (R)
    let expected = gloria_anderson_households_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
