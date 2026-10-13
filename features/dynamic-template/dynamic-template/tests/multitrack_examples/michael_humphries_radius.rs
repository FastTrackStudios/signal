use daw_proto::{assert_tracks_equal, TrackStructureBuilder};
use dynamic_template::*;

fn michael_humphries_radius_expected() -> daw_proto::TrackHierarchy {
    let expected = TrackStructureBuilder::new()
        .folder("Drums")
        .track("Kick")
        .item("01.Kick_01.wav")
        .folder("Snare")
        .track("Top")
        .item("02.SN Top_01.wav")
        .track("Bottom")
        .item("03.SN Bot_01.wav")
        .end()
        .folder("Toms")
        .track("Tom 1")
        .item("04.Rack.Tom_01.wav")
        .track("Tom 2")
        .item("05.Flr.Tom_01.wav")
        .end()
        .folder("Cymbals")
        .track("OH")
        .item("07.OH_01.wav")
        .track("Hi Hat")
        .item("06.Hi Hat_01.wav")
        .end()
        .track("Rooms")
        .item("08.Room_01.wav")
        .end()
        .track("Bass")
        .item("09.Bass DI_01.wav")
        .track("Guitars")
        .item("10.Gtr Amp_01.wav")
        .track("Keys")
        .item("11.Nord Piano_01.wav")
        .track("Reference")
        .item("12.Radius Mix_01.wav")
        .build();
    expected
}

#[test]
fn michael_humphries_radius() {
    // -- Setup & Fixtures
    // Michael Humphries - Radius: A clean 12-track session with standard rock instrumentation
    let items = vec![
        "01.Kick_01.wav",
        "02.SN Top_01.wav",
        "03.SN Bot_01.wav",
        "04.Rack.Tom_01.wav",
        "05.Flr.Tom_01.wav",
        "06.Hi Hat_01.wav",
        "07.OH_01.wav",
        "08.Room_01.wav",
        "09.Bass DI_01.wav",
        "10.Gtr Amp_01.wav",
        "11.Nord Piano_01.wav",
        "12.Radius Mix_01.wav",
    ];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    // Expected structure:
    // Drums/
    //   ├─ Kick            ← 01.Kick_01.wav
    //   ├─ Snare/
    //   │   ├─ Top         ← 02.SN Top_01.wav
    //   │   └─ Bottom      ← 03.SN Bot_01.wav
    //   ├─ Toms/
    //   │   ├─ Tom 1       ← 04.Rack.Tom_01.wav (Rack tom)
    //   │   └─ Tom 2       ← 05.Flr.Tom_01.wav (Floor tom)
    //   ├─ Cymbals/
    //   │   ├─ OH          ← 07.OH_01.wav (Overheads)
    //   │   └─ Hi Hat      ← 06.Hi Hat_01.wav
    //   └─ Rooms           ← 08.Room_01.wav
    // Bass                 ← 09.Bass DI_01.wav
    // Guitars              ← 10.Gtr Amp_01.wav
    // Keys                 ← 11.Nord Piano_01.wav (Nord keyboard → Keys)
    // Reference            ← 12.Radius Mix_01.wav (Mix → Reference)
    let expected = michael_humphries_radius_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
