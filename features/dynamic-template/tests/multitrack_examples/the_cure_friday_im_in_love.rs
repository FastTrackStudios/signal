use daw_proto::{assert_tracks_equal, TrackStructureBuilder};
use dynamic_template::*;

fn the_cure_friday_im_in_love_expected() -> daw_proto::TrackHierarchy {
    let expected = TrackStructureBuilder::new()
        .folder("Drums")
        .track("Kick")
        .item("01.Kick_01.wav")
        .track("Snare")
        .item("02.Snare_01.wav")
        .folder("OH")
        .track("L")
        .item("03.Ohd_01 L.wav")
        .track("R")
        .item("04.Ohd_01 R.wav")
        .end()
        .end()
        .track("Percussion")
        .item("05.Tambo_01.wav")
        .folder("Bass")
        .track("Bass")
        .item("06.Bass DI_01.wav")
        .track("Amp")
        .item("07.Bass Amp_01.wav")
        .end()
        .folder("Guitars")
        .folder("Electric")
        .folder("Solo")
        .track("Amp")
        .item("19.Gtr Solo Amp_01.wav")
        .track("DI")
        .item("18.Gtr Solo DI_01.wav")
        .end()
        .folder("Electric")
        .track("Amp")
        .item("21.Gtr Root Note Amp_01.wav")
        .track("DI")
        .item("20.Gtr Root Note DI_01.wav")
        .end()
        .folder("Electric 1")
        .track("Amp")
        .item("13.Gtr 1 Amp_01.wav")
        .track("DI")
        .item("12.Gtr 1 DI_01.wav")
        .end()
        .folder("Electric 2")
        .folder("Amp")
        .track("Amp 1")
        .item("15.Gtr 2 Amp_01.wav")
        .track("Amp 2")
        .item("17.Gtr 2 Amp Overlap_01.wav")
        .end()
        .folder("DI")
        .track("Electric 1")
        .item("14.Gtr 2 DI_01.wav")
        .track("Electric 2")
        .item("16.Gtr 2 DI Overlap_01.wav")
        .end()
        .end()
        .end()
        .folder("Acoustic")
        .track("Acoustic 1")
        .item("08.Acoustic_01.wav")
        .track("Acoustic 2")
        .item("09.Acoustic.dbl_01.wav")
        .track("Acoustic 3")
        .item("10.Acoustic OD_01.wav")
        .track("Acoustic 4")
        .item("11.Acoustic OD Dbl_01.wav")
        .end()
        .end()
        .track("Synths")
        .item("23.Pad_01.wav")
        .folder("Vocals")
        .folder("Main")
        .track("Lead 1")
        .item("24.Vocal_01.wav")
        .track("Lead 2")
        .item("25.Vocal Overlap_01.wav")
        .end()
        .track("DBL")
        .item("26. Vocal Dbl_01.wav")
        .end()
        .track("Reference")
        .item("29.Friday I'm In Love Mix_01.wav")
        .folder("Unsorted")
        .track("HARM")
        .item("27.HARM_01.wav")
        .track("Harm Overlap")
        .item("28.Harm Overlap_01.wav")
        .end()
        .build();
    expected
}

#[test]
fn the_cure_friday_im_in_love() {
    // -- Setup & Fixtures
    // The Cure - Friday I'm In Love: Pop rock with multiple guitar overdubs and harmonies
    let items = vec![
        "01.Kick_01.wav",
        "02.Snare_01.wav",
        "03.Ohd_01 L.wav",
        "04.Ohd_01 R.wav",
        "05.Tambo_01.wav",
        "06.Bass DI_01.wav",
        "07.Bass Amp_01.wav",
        "08.Acoustic_01.wav",
        "09.Acoustic.dbl_01.wav",
        "10.Acoustic OD_01.wav",
        "11.Acoustic OD Dbl_01.wav",
        "12.Gtr 1 DI_01.wav",
        "13.Gtr 1 Amp_01.wav",
        "14.Gtr 2 DI_01.wav",
        "15.Gtr 2 Amp_01.wav",
        "16.Gtr 2 DI Overlap_01.wav",
        "17.Gtr 2 Amp Overlap_01.wav",
        "18.Gtr Solo DI_01.wav",
        "19.Gtr Solo Amp_01.wav",
        "20.Gtr Root Note DI_01.wav",
        "21.Gtr Root Note Amp_01.wav",
        "23.Pad_01.wav",
        "24.Vocal_01.wav",
        "25.Vocal Overlap_01.wav",
        "26. Vocal Dbl_01.wav",
        "27.HARM_01.wav",
        "28.Harm Overlap_01.wav",
        "29.Friday I'm In Love Mix_01.wav",
    ];

    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    let expected = the_cure_friday_im_in_love_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
