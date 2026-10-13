use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn def_leppard_rock_of_ages_expected() -> daw_proto::TrackHierarchy {
    let toms = TrackGroup::folder("Toms")
        .track("T1")
        .item("03_Tom1.wav")
        .track("T2")
        .item("04_Tom2.wav")
        .end();

    let oh = TrackGroup::folder("OH")
        .track("Ride")
        .item("07_Ovh_Ride.wav")
        .track("OH")
        .item("06_Ovh_Snare.wav")
        .end();

    let cymbals = TrackGroup::folder("Cymbals")
        .group(oh)
        .track("Hi Hat")
        .item("05_Hat.wav")
        .end();

    let drums = TrackGroup::folder("Drums")
        .track("Kick")
        .item("01_Kick.wav")
        .track("Snare")
        .item("02_Snare.wav")
        .group(toms)
        .group(cymbals)
        .end();

    // --- Percussion ---
    // Cowbell-GangVox now correctly classified as Percussion (cowbell),
    // plus chorus claps
    let clap = TrackGroup::folder("Clap")
        .track("Chorus Chorus 1")
        .item("18_Chorus-Clap1.wav")
        .track("Chorus Chorus 2")
        .item("19_Chorus-Clap2.wav")
        .end();

    let percussion = TrackGroup::folder("Percussion")
        .track("Cowbell")
        .item("20_Cowbell-GangVox.wav")
        .group(clap)
        .end();

    // --- Bass ---
    // Acoustic bass + 3 synth bass variants
    let bass = TrackGroup::folder("Bass")
        .track("Bass 1")
        .item("08_Bass.wav")
        .track("Bass 2")
        .item("09_BassSynth1Saw.wav")
        .track("Bass 3")
        .item("10_BassSynth2FM.wav")
        .track("Bass 4")
        .item("11_BassSynth3.wav")
        .end();

    // --- Guitars ---
    // KeyGtr (keyboard-guitar hybrid), rhythm guitars, and solo
    let inner_guitars = TrackGroup::folder("Guitars")
        .track("Electric 1")
        .item("13_KeyGtr1.wav")
        .track("Electric 2")
        .item("14_KeyGtr2.wav")
        .track("Electric 3")
        .item("15_Gtr-Rh1.wav")
        .track("Electric 4")
        .item("16_Gtr-Rh2.wav")
        .end();

    let guitars = TrackGroup::folder("Guitars")
        .track("Solo")
        .item("17_Gtr-Solo.wav")
        .group(inner_guitars)
        .end();

    // --- Synths ---
    let synths = TrackGroup::single_track("Synths", "12_SynthFM.wav");

    // --- Vocals ---
    let vocals = TrackGroup::folder("Vocals")
        .track("Lead 1")
        .item("21_LeadVox.wav")
        .track("Lead 2")
        .item("22_LeadVox2.wav")
        .end();

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(percussion)
        .group(bass)
        .group(guitars)
        .group(synths)
        .group(vocals)
        .build();
    expected
}

#[test]
fn def_leppard_rock_of_ages() {
    // -- Setup & Fixtures
    // Def Leppard - Rock of Ages: 22-track 80s rock session with synth bass variants
    // (Saw, FM, generic), FM synth, "KeyGtr" hybrid keyboard-guitar tracks, cowbell with
    // gang vocals, chorus claps, and dual lead vocals. Tests unusual instrument naming.
    let items = vec![
        "01_Kick.wav",
        "02_Snare.wav",
        "03_Tom1.wav",
        "04_Tom2.wav",
        "05_Hat.wav",
        "06_Ovh_Snare.wav",
        "07_Ovh_Ride.wav",
        "08_Bass.wav",
        "09_BassSynth1Saw.wav",
        "10_BassSynth2FM.wav",
        "11_BassSynth3.wav",
        "12_SynthFM.wav",
        "13_KeyGtr1.wav",
        "14_KeyGtr2.wav",
        "15_Gtr-Rh1.wav",
        "16_Gtr-Rh2.wav",
        "17_Gtr-Solo.wav",
        "18_Chorus-Clap1.wav",
        "19_Chorus-Clap2.wav",
        "20_Cowbell-GangVox.wav",
        "21_LeadVox.wav",
        "22_LeadVox2.wav",
    ];
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    // ============================================================================
    // Expected structure
    // ============================================================================

    // --- Drums ---
    let expected = def_leppard_rock_of_ages_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
