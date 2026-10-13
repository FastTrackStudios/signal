use daw_proto::{assert_tracks_equal, TrackStructureBuilder};
use dynamic_template::*;

fn iron_maiden_the_trooper_expected() -> daw_proto::TrackHierarchy {
    let expected = TrackStructureBuilder::new()
        .folder("Drums")
        .track("Kick")
        .item("08-Kick-TheTrooper.wav")
        .track("Snare")
        .item("09-Snare-TheTrooper.wav")
        .track("OH")
        .item("10-OH-TheTrooper.wav")
        .end()
        .folder("Bass")
        .track("Bass")
        .item("12-Bass DI-TheTrooper.wav")
        .track("Amp")
        .item("13-Bass Amp-TheTrooper.wav")
        .end()
        .folder("Guitars")
        .track("L")
        .item("15-Rhy Gtr L-TheTrooper.wav")
        .track("R")
        .item("16-Rhy Gtr R-TheTrooper.wav")
        .end()
        .folder("Vocals")
        .track("Vocals 1")
        .item("21-Vocal 1-TheTrooper.wav")
        .track("Vocals 2")
        .item("22-Vocal 2-TheTrooper.wav")
        .track("Vocals 3")
        .item("23-Vocal 3-TheTrooper.wav")
        .end()
        .track("Reference")
        .item("Trooper-mix1.wav")
        .folder("Unsorted")
        .track("Solo 1")
        .item("17-Solo 1-TheTrooper.wav")
        .track("Solo 2")
        .item("18-Solo 2-TheTrooper.wav")
        .track("Solo 3")
        .item("19-Solo 3-TheTrooper.wav")
        .end()
        .build();
    expected
}

#[test]
fn iron_maiden_the_trooper() {
    // -- Setup & Fixtures
    // Iron Maiden - The Trooper: 14-track metal session with stereo rhythm guitars and layered vocals
    let items = vec![
        "08-Kick-TheTrooper.wav",
        "09-Snare-TheTrooper.wav",
        "10-OH-TheTrooper.wav",
        "12-Bass DI-TheTrooper.wav",
        "13-Bass Amp-TheTrooper.wav",
        "15-Rhy Gtr L-TheTrooper.wav",
        "16-Rhy Gtr R-TheTrooper.wav",
        "17-Solo 1-TheTrooper.wav",
        "18-Solo 2-TheTrooper.wav",
        "19-Solo 3-TheTrooper.wav",
        "21-Vocal 1-TheTrooper.wav",
        "22-Vocal 2-TheTrooper.wav",
        "23-Vocal 3-TheTrooper.wav",
        "Trooper-mix1.wav",
    ];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    // Expected structure:
    // Note: "TheTrooper"/"Trooper" detected as song name via fuzzy matching and stripped
    // Drums/
    //   ├─ Kick                    ← 08-Kick-TheTrooper.wav
    //   ├─ Snare                   ← 09-Snare-TheTrooper.wav
    //   └─ OH                      ← 10-OH-TheTrooper.wav
    // Bass/
    //   ├─ Bass                    ← 12-Bass DI-TheTrooper.wav (DI)
    //   └─ Amp                     ← 13-Bass Amp-TheTrooper.wav
    // Guitars/
    //   ├─ L                       ← 15-Rhy Gtr L-TheTrooper.wav ("Rhy" arrangement collapses since only one)
    //   └─ R                       ← 16-Rhy Gtr R-TheTrooper.wav
    // Vocals/
    //   ├─ Vocals 1                ← 21-Vocal 1-TheTrooper.wav
    //   ├─ Vocals 2                ← 22-Vocal 2-TheTrooper.wav
    //   └─ Vocals 3                ← 23-Vocal 3-TheTrooper.wav
    // Reference                    ← Trooper-mix1.wav
    // Unsorted/
    //   ├─ Solo 1                  ← 17-Solo 1-TheTrooper.wav (song name stripped)
    //   ├─ Solo 2                  ← 18-Solo 2-TheTrooper.wav (song name stripped)
    //   └─ Solo 3                  ← 19-Solo 3-TheTrooper.wav (song name stripped)
    let expected = iron_maiden_the_trooper_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
