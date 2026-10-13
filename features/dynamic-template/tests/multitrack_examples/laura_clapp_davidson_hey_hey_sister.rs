use daw_proto::{assert_tracks_equal, TrackStructureBuilder};
use dynamic_template::*;

fn laura_clapp_davidson_hey_hey_sister_expected() -> daw_proto::TrackHierarchy {
    let expected = TrackStructureBuilder::new()
        .folder("Drums")
        .track("Kick")
        .item("01.KICK_01.wav")
        .track("Snare")
        .item("02.SNARE_01.wav")
        .folder("Cymbals")
        .track("OH")
        .item("04.OHS_01.wav")
        .track("Hi Hat")
        .item("03.HH_01.wav")
        .end()
        .track("Rooms")
        .item("05.ROOM_01.wav")
        .end()
        .folder("Bass")
        .track("Bass 1")
        .item("06.Hey_Sister_Bass DI_01.wav")
        .track("Bass 2")
        .item("07.Hey_Sister_Bass_Tone_X_01.wav")
        .end()
        .folder("Guitars")
        .folder("Electric")
        .track("Electric 1")
        .item("10.Hey_Sister_Gtr_1_L_01.wav")
        .track("Electric 2")
        .item("11.Hey_Sister_Gtr_2_R_01.wav")
        .end()
        .folder("Acoustic")
        .track("Acoustic 1")
        .item("08.Hey_Sister_Acoustic_L_01.wav")
        .track("Acoustic 2")
        .item("09.Hey_Sister_Acoustic_R_01.wav")
        .end()
        .end()
        .folder("Vocals")
        .track("Lead")
        .item("14.Laura Lead Vocal_01.wav")
        .folder("BGVs")
        .folder("Laura")
        .folder("Chorus")
        .track("Harmony 1")
        .item("17.Laura Chorus Harmony 1_01.wav")
        .track("Harmony 2")
        .item("18.Laura Chorus Harmony 2_01.wav")
        .end()
        .track("Verse")
        .item("15.Laura Verse Harmony_01.wav")
        .folder("Low")
        .track("Low 1")
        .item("23.Laura Hey Hey Low 1_01.wav")
        .track("Low 2")
        .item("24.Laura Hey Hey Low 2_01.wav")
        .end()
        .folder("High")
        .track("High 1")
        .item("19.Laura Hey Hey High 1_01.wav")
        .track("High 2")
        .item("20.Laura Hey Hey High 2_01.wav")
        .end()
        .folder("Highest")
        .track("Highest 1")
        .item("21.Laura Hey Hey Highest 1_01.wav")
        .track("Highest 2")
        .item("22.Laura Hey Hey Highest 2_01.wav")
        .end()
        .folder("Mid")
        .track("Mid 1")
        .item("25.Laura Hey Hey Mid 1_01.wav")
        .track("Mid 2")
        .item("26.Laura Hey Hey Mid 2_01.wav")
        .end()
        .end()
        .track("BGVs")
        .item("27. Hey Hey Sister_01.wav")
        .end()
        .end()
        .track("Choir")
        .item("16.Laura Chorus Double_01.wav")
        .folder("Unsorted")
        .track("Hey_Sister_Licks")
        .item("12.Hey_Sister_Licks_01.wav")
        .track("Hey_Sister_Solo")
        .item("13.Hey_Sister_Solo_01.wav")
        .end()
        .build();
    expected
}

#[test]
fn laura_clapp_davidson_hey_hey_sister() {
    // -- Setup & Fixtures
    // Laura Clapp Davidson - Hey Hey Sister: Folk/pop with harmony layers
    let items = vec![
        "01.KICK_01.wav",
        "02.SNARE_01.wav",
        "03.HH_01.wav",
        "04.OHS_01.wav",
        "05.ROOM_01.wav",
        "06.Hey_Sister_Bass DI_01.wav",
        "07.Hey_Sister_Bass_Tone_X_01.wav",
        "08.Hey_Sister_Acoustic_L_01.wav",
        "09.Hey_Sister_Acoustic_R_01.wav",
        "10.Hey_Sister_Gtr_1_L_01.wav",
        "11.Hey_Sister_Gtr_2_R_01.wav",
        "12.Hey_Sister_Licks_01.wav",
        "13.Hey_Sister_Solo_01.wav",
        "14.Laura Lead Vocal_01.wav",
        "15.Laura Verse Harmony_01.wav",
        "16.Laura Chorus Double_01.wav",
        "17.Laura Chorus Harmony 1_01.wav",
        "18.Laura Chorus Harmony 2_01.wav",
        "19.Laura Hey Hey High 1_01.wav",
        "20.Laura Hey Hey High 2_01.wav",
        "21.Laura Hey Hey Highest 1_01.wav",
        "22.Laura Hey Hey Highest 2_01.wav",
        "23.Laura Hey Hey Low 1_01.wav",
        "24.Laura Hey Hey Low 2_01.wav",
        "25.Laura Hey Hey Mid 1_01.wav",
        "26.Laura Hey Hey Mid 2_01.wav",
        "27. Hey Hey Sister_01.wav",
    ];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    // Note: "Hey Hey" patterns now match BGVs, so the harmony parts are properly organized
    // The "Hey Hey Sister" reference also matches due to containing "hey hey"
    let expected = laura_clapp_davidson_hey_hey_sister_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
