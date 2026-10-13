use daw_proto::{assert_tracks_equal, TrackStructureBuilder};
use dynamic_template::*;

fn chad_burton_nashville_expected_a() -> TrackStructureBuilder {
    TrackStructureBuilder::new()
        .folder("Drums")
        .folder("Kick")
        .track("In")
        .item("Nashville_Kick_In_02.wav")
        .track("Out")
        .item("Nashville_Kick_Out.dup1_01.wav")
        .end()
        .folder("Snare")
        .track("Top")
        .item("Nashville_Snare_Top_02.wav")
        .track("Bottom")
        .item("Nashville_Snare_Bottom_02.wav")
        .end()
        .folder("Cymbals")
        .folder("OH")
        .track("L")
        .item("Nashville_OH_Left_02.wav")
        .track("R")
        .item("Nashville_OH_Right_02.wav")
        .end()
        .track("Cymbals")
        .item("Nashville_Cymbal_02.wav")
        .end()
        .folder("Rooms")
        .track("L")
        .item("Nashville_Room_Left_02.wav")
        .track("R")
        .item("Nashville_Room_Right_02.wav")
        .track("Rooms 1")
        .item("AMB Slide_02.wav")
        .track("Rooms 2")
        .item("Ambient Loop_03.wav")
        .end()
        .track("Drum Kit")
        .item("Nashville_Center_Kit_Mic_02.wav")
        .end()
}

fn chad_burton_nashville_expected() -> daw_proto::TrackHierarchy {
    chad_burton_nashville_expected_a()
        .folder("Percussion")
        .track("Shaker")
        .item("Nashville_Shaker_02.wav")
        .track("Tambourine")
        .item("Nashville_Tambourine_02.wav")
        .track("Clave")
        .item("Nashville_Clave_02.wav")
        .folder("Conga")
        .track("Conga 1")
        .item("Nashville_Conga_High_02.wav")
        .track("Conga 2")
        .item("Nashville_Conga_Low_02.wav")
        .end()
        .track("Woodblock")
        .item("Nashville_Woodblock_02.wav")
        .end()
        .track("Bass")
        .item("Nashville_BassShawn_02.wav")
        .folder("Guitars")
        .folder("Acoustic")
        .track("Acoustic 1")
        .item("ACO L1_02.wav")
        .track("Acoustic 2")
        .item("ACO R1_02.wav")
        .track("Acoustic 3")
        .item("ACO RHY_02.wav")
        .track("Solo L 1")
        .item("ACO Slide SOLO L1_02.wav")
        .track("Solo R 1")
        .item("ACo slide solo r1_02.wav")
        .track("Solo R 2")
        .item("ACO Slide Solo_02 R.wav")
        .track("Solo L 2")
        .item("ACO Sllide Solo_02 L.wav")
        .track("Acoustic 4")
        .item("ACO_02.wav")
        .end()
        .track("Steel")
        .item("Steel GTR_02.wav")
        .end()
        .folder("Keys")
        .folder("Piano")
        .track("Delay")
        .item("Nashville - DELAY PIANO_STRING SYNTH (might be too pop)_02.wav")
        .track("Delay 2")
        .item("Nashville - DELAY PIANO_STRING SYNTH (might be too pop)(2)_02.wav")
        .end()
        .track("Rhodes")
        .item("Nashville - RHODES (take 2)_02.wav")
        .end()
        .folder("Vocals")
        .folder("Main")
        .track("Lead 1")
        .item("BU VOC1_02.wav")
        .track("Lead 2")
        .item("LD VOC_02.wav")
        .end()
        .track("DBL")
        .item("BU VOC DBL_02.wav")
        .end()
        .track("Reference")
        .item("Nashville_MIP2_MDN.wav")
        .track("Unsorted")
        .item("Slide RHY_02.wav")
        .build()
}

#[test]
fn chad_burton_nashville() {
    // -- Setup & Fixtures
    // Chad Burton - Nashville: Country session with detailed percussion and piano
    let items = vec![
        "ACO L1_02.wav",
        "ACO R1_02.wav",
        "ACO RHY_02.wav",
        "ACO Slide SOLO L1_02.wav",
        "ACo slide solo r1_02.wav",
        "ACO Slide Solo_02 R.wav",
        "ACO Sllide Solo_02 L.wav",
        "ACO_02.wav",
        "AMB Slide_02.wav",
        "Ambient Loop_03.wav",
        "BU VOC DBL_02.wav",
        "BU VOC1_02.wav",
        "LD VOC_02.wav",
        "Nashville - DELAY PIANO_STRING SYNTH (might be too pop)_02.wav",
        "Nashville - DELAY PIANO_STRING SYNTH (might be too pop)(2)_02.wav",
        "Nashville - RHODES (take 2)_02.wav",
        "Nashville_BassShawn_02.wav",
        "Nashville_Center_Kit_Mic_02.wav",
        "Nashville_Clave_02.wav",
        "Nashville_Conga_High_02.wav",
        "Nashville_Conga_Low_02.wav",
        "Nashville_Cymbal_02.wav",
        "Nashville_Kick_In_02.wav",
        "Nashville_Kick_Out.dup1_01.wav",
        "Nashville_MIP2_MDN.wav",
        "Nashville_OH_Left_02.wav",
        "Nashville_OH_Right_02.wav",
        "Nashville_Room_Left_02.wav",
        "Nashville_Room_Right_02.wav",
        "Nashville_Shaker_02.wav",
        "Nashville_Snare_Bottom_02.wav",
        "Nashville_Snare_Top_02.wav",
        "Nashville_Tambourine_02.wav",
        "Nashville_Woodblock_02.wav",
        "Slide RHY_02.wav",
        "Steel GTR_02.wav",
    ];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    let expected = chad_burton_nashville_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
