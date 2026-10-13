use daw_proto::{assert_tracks_equal, TrackStructureBuilder};
use dynamic_template::*;

fn cyndi_lauper_time_after_time_expected() -> daw_proto::TrackHierarchy {
    let expected = TrackStructureBuilder::new()
        .folder("Drums")
        .track("Kick")
        .item("08.Kick.TimeAfterTime.126bpm_01.wav")
        .track("Snare")
        .item("09.Snare.TimeAfterTime.126bpm_01.wav")
        .folder("OH")
        .track("Hi Hat")
        .item("11.Overhead Hat.TimeAfterTime.126bpm_01.wav")
        .track("Ride")
        .item("12.Overhead Ride.TimeAfterTime.126bpm_01.wav")
        .end()
        .folder("Rooms")
        .track("L")
        .item("13.Room.Left.TimeAfterTime.126bpm_01.wav")
        .track("R")
        .item("14.Room.Right.TimeAfterTime.126bpm_01.wav")
        .track("Mono")
        .item("10.Kit.Mono.Room.TimeAfterTime.126bpm_01.wav")
        .end()
        .end()
        .folder("Percussion")
        .track("Shaker 1")
        .item("15.SHAKER.Left_01.wav")
        .track("Shaker 2")
        .item("16.SHAKER.Right_01.wav")
        .end()
        .folder("Bass")
        .track("Warren Amp")
        .item("06.BASS AMP.Warren.TimeAfterTime.126bpm_01.wav")
        .track("Warren DI")
        .item("07.BASS DI.Warren.TimeAfterTime.126bpm_01.wav")
        .end()
        .folder("Guitars")
        .folder("Electric")
        .track("Left")
        .item("04.Electric.Gtr.Left.Warren.TimeAfterTime.126bpm_01.wav")
        .track("Right")
        .item("05.Electric.Gtr.Right.Warren.TimeAfterTime.126bpm_01.wav")
        .end()
        .track("Acoustic")
        .item("03.Acoustic Guitar.Nick.TimeAfterTime.126bpm_01.wav")
        .end()
        .folder("Vocals")
        .track("Lead")
        .item("01.LV BECCA.TimeAfterTime.126bpm_01.wav")
        .track("BGVs")
        .item("02.BV LUCA.TimeAfterTime.126bpm_01.wav")
        .end()
        .track("Guide")
        .item("17.Click.Print_01.wav")
        .track("Reference")
        .item("18.Time After Time Presonus HD8 Mix_01.wav")
        .build();
    expected
}

#[test]
fn cyndi_lauper_time_after_time() {
    // -- Setup & Fixtures
    // Cyndi Lauper - Time After Time: 18-track pop session with performer names in filenames
    let items = vec![
        "01.LV BECCA.TimeAfterTime.126bpm_01.wav",
        "02.BV LUCA.TimeAfterTime.126bpm_01.wav",
        "03.Acoustic Guitar.Nick.TimeAfterTime.126bpm_01.wav",
        "04.Electric.Gtr.Left.Warren.TimeAfterTime.126bpm_01.wav",
        "05.Electric.Gtr.Right.Warren.TimeAfterTime.126bpm_01.wav",
        "06.BASS AMP.Warren.TimeAfterTime.126bpm_01.wav",
        "07.BASS DI.Warren.TimeAfterTime.126bpm_01.wav",
        "08.Kick.TimeAfterTime.126bpm_01.wav",
        "09.Snare.TimeAfterTime.126bpm_01.wav",
        "10.Kit.Mono.Room.TimeAfterTime.126bpm_01.wav",
        "11.Overhead Hat.TimeAfterTime.126bpm_01.wav",
        "12.Overhead Ride.TimeAfterTime.126bpm_01.wav",
        "13.Room.Left.TimeAfterTime.126bpm_01.wav",
        "14.Room.Right.TimeAfterTime.126bpm_01.wav",
        "15.SHAKER.Left_01.wav",
        "16.SHAKER.Right_01.wav",
        "17.Click.Print_01.wav",
        "18.Time After Time Presonus HD8 Mix_01.wav",
    ];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    // Expected structure:
    // Drums/
    //   ├─ Kick                ← 08.Kick.TimeAfterTime.126bpm_01.wav
    //   ├─ Snare               ← 09.Snare.TimeAfterTime.126bpm_01.wav
    //   ├─ OH/
    //   │   ├─ Hi Hat          ← 11.Overhead Hat.TimeAfterTime.126bpm_01.wav
    //   │   └─ Ride            ← 12.Overhead Ride.TimeAfterTime.126bpm_01.wav
    //   └─ Rooms/
    //       ├─ L               ← 13.Room.Left.TimeAfterTime.126bpm_01.wav
    //       ├─ R               ← 14.Room.Right.TimeAfterTime.126bpm_01.wav
    //       └─ Mono            ← 10.Kit.Mono.Room.TimeAfterTime.126bpm_01.wav
    // Percussion/
    //   ├─ Shaker 1            ← 15.SHAKER.Left_01.wav
    //   └─ Shaker 2            ← 16.SHAKER.Right_01.wav
    // Bass/
    //   ├─ Warren Amp          ← 06.BASS AMP.Warren.TimeAfterTime.126bpm_01.wav
    //   └─ Warren DI           ← 07.BASS DI.Warren.TimeAfterTime.126bpm_01.wav
    // Guitars/
    //   ├─ Electric/
    //   │   ├─ Left            ← 04.Electric.Gtr.Left.Warren.TimeAfterTime.126bpm_01.wav
    //   │   └─ Right           ← 05.Electric.Gtr.Right.Warren.TimeAfterTime.126bpm_01.wav
    //   └─ Acoustic            ← 03.Acoustic Guitar.Nick.TimeAfterTime.126bpm_01.wav
    // Vocals/
    //   ├─ Lead                ← 01.LV BECCA.TimeAfterTime.126bpm_01.wav (LV = Lead Vocal)
    //   └─ BGVs                ← 02.BV LUCA.TimeAfterTime.126bpm_01.wav (BV = Background Vocal)
    // Guide                    ← 17.Click.Print_01.wav
    // Reference                ← 18.Time After Time Presonus HD8 Mix_01.wav
    let expected = cyndi_lauper_time_after_time_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
