use daw_proto::{assert_tracks_equal, TrackStructureBuilder};
use dynamic_template::*;

fn steve_maggiora_its_too_late_expected() -> daw_proto::TrackHierarchy {
    let expected = TrackStructureBuilder::new()
        .folder("Drums")
        .track("Kick")
        .item("01.Kick_02.wav")
        .track("Snare")
        .item("02.SN TP_02.wav")
        .folder("OH")
        .track("L")
        .item("03.OH.01_02 L.wav")
        .track("R")
        .item("04.OH.01_02 R.wav")
        .end()
        .end()
        .folder("Bass")
        .track("Bass 1")
        .item("05.Bass DI_02.wav")
        .track("Bass 2")
        .item("06.Bass_02.wav")
        .end()
        .folder("Guitars")
        .folder("Electric")
        .folder("Michael")
        .track("Amp")
        .item("12.Gtr Amp Solo Michael_02.wav")
        .track("DI")
        .item("11.Gtr Di Solo Michael_02.wav")
        .end()
        .folder("Warren")
        .track("Amp")
        .item("14.Guitar Amp Solo Warren_01.wav")
        .track("DI")
        .item("13.Guitar DI Solo Warren_02.wav")
        .end()
        .folder("Rhythm")
        .track("DI")
        .item("08.Guitar DI Rhythm_01.wav")
        .track("Rhythm")
        .item("09.Guitar Rhythm_01.wav")
        .end()
        .end()
        .track("Acoustic")
        .item("07.Acoustic_02.wav")
        .end()
        .track("Keys")
        .item("10.Piano_02.wav")
        .folder("Vocals")
        .track("Lead 1")
        .item("15.Vocal_02.wav")
        .track("Lead Plate")
        .item("19.Vocal.Eko.Plate_02.wav")
        .track("Lead 2")
        .item("20.Vocal.Magic_02.wav")
        .end()
        .folder("SFX")
        .track("H3000.One")
        .item("16.H3000.One_02.wav")
        .track("H3000.Two")
        .item("17.H3000.Two_02.wav")
        .track("H3000.Three")
        .item("18.H3000.Three_02.wav")
        .end()
        .track("Reference")
        .item("21.It's Too Late Mix_02.wav")
        .build();
    expected
}

#[test]
fn steve_maggiora_its_too_late() {
    // -- Setup & Fixtures
    // Steve Maggiora - It's Too Late: Classic rock with multiple guitar layers and H3000 effects
    let items = vec![
        "01.Kick_02.wav",
        "02.SN TP_02.wav",
        "03.OH.01_02 L.wav",
        "04.OH.01_02 R.wav",
        "05.Bass DI_02.wav",
        "06.Bass_02.wav",
        "07.Acoustic_02.wav",
        "08.Guitar DI Rhythm_01.wav",
        "09.Guitar Rhythm_01.wav",
        "10.Piano_02.wav",
        "11.Gtr Di Solo Michael_02.wav",
        "12.Gtr Amp Solo Michael_02.wav",
        "13.Guitar DI Solo Warren_02.wav",
        "14.Guitar Amp Solo Warren_01.wav",
        "15.Vocal_02.wav",
        "16.H3000.One_02.wav",
        "17.H3000.Two_02.wav",
        "18.H3000.Three_02.wav",
        "19.Vocal.Eko.Plate_02.wav",
        "20.Vocal.Magic_02.wav",
        "21.It's Too Late Mix_02.wav",
    ];

    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    let expected = steve_maggiora_its_too_late_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
