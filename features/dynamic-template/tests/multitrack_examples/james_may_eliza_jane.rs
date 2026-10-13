use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn james_may_eliza_jane_expected() -> daw_proto::TrackHierarchy {
    let toms = TrackGroup::folder("Toms")
        .track("T1")
        .item("04_Tom1.wav")
        .track("T2")
        .item("05_Tom2.wav")
        .end();

    let cymbals = TrackGroup::folder("Cymbals")
        .track("OH")
        .item("06_Overheads.wav")
        .track("Hi Hat")
        .item("03_HiHat.wav")
        .end();

    let drums = TrackGroup::folder("Drums")
        .track("Kick")
        .item("01_Kick.wav")
        .track("Snare")
        .item("02_Snare.wav")
        .group(toms)
        .group(cymbals)
        .end();

    // --- Bass ---
    let bass = TrackGroup::single_track("Bass", "07_Bass.wav");

    // --- Guitars ---
    // Acoustic guitars + mandolins nested under Acoustic, plus pedal steel
    let mando = TrackGroup::folder("Mando")
        .track("Mando 1")
        .item("10_Mandolin1.wav")
        .track("Mando 2")
        .item("11_Mandolin2.wav")
        .end();

    let acoustic = TrackGroup::folder("Acoustic")
        .group(mando)
        .track("Acoustic 1")
        .item("08_AcousticGtr1.wav")
        .track("Acoustic 2")
        .item("09_AcousticGtr2.wav")
        .end();

    let guitars = TrackGroup::folder("Guitars")
        .group(acoustic)
        .track("Steel")
        .item("14_PedalSteel.wav")
        .end();

    // --- Keys ---
    // Hammond organ dual-mic (hi/lo rotary speaker)
    let keys = TrackGroup::folder("Keys")
        .track("Organ 1")
        .item("12_HammondMic1Hi.wav")
        .track("Organ 2")
        .item("13_HammondMic2Lo.wav")
        .end();

    // --- Vocals ---
    let vocals = TrackGroup::folder("Vocals")
        .track("Lead")
        .item("15_LeadVox.wav")
        .track("BGVs")
        .item("16_BackingVox.wav")
        .end();

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(bass)
        .group(guitars)
        .group(keys)
        .group(vocals)
        .build();
    expected
}

#[test]
fn james_may_eliza_jane() {
    // -- Setup & Fixtures
    // James May - Eliza Jane: 16-track country/folk session with standard drum kit,
    // bass, 2 acoustic guitars, 2 mandolins, Hammond organ (dual-mic: hi/lo rotary),
    // pedal steel guitar, lead vocals, and backing vocals. Tests country instrumentation
    // with pedal steel and Hammond organ classification. Cambridge-MT multitrack library.
    let items = vec![
        "01_Kick.wav",
        "02_Snare.wav",
        "03_HiHat.wav",
        "04_Tom1.wav",
        "05_Tom2.wav",
        "06_Overheads.wav",
        "07_Bass.wav",
        "08_AcousticGtr1.wav",
        "09_AcousticGtr2.wav",
        "10_Mandolin1.wav",
        "11_Mandolin2.wav",
        "12_HammondMic1Hi.wav",
        "13_HammondMic2Lo.wav",
        "14_PedalSteel.wav",
        "15_LeadVox.wav",
        "16_BackingVox.wav",
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
    let expected = james_may_eliza_jane_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
