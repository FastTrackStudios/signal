use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn night_panther_fire_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::folder("Drums")
        .track("Drum Kit")
        .item("05_Drums.wav")
        .track("Electronic Kit")
        .item("04_DrumMachine.wav")
        .end();

    let percussion = TrackGroup::single_track("Percussion", "08_AuxPerc.wav");

    // ElecBass now correctly classified as Bass
    let bass = TrackGroup::single_track("Bass", "01_ElecBass.wav");

    let synths = TrackGroup::folder("Synths")
        .track("Synth1")
        .item("10_Synth1.wav")
        .track("Synth2")
        .item("11_Synth2.wav")
        .track("Synth3")
        .item("12_Synth3.wav")
        .end();

    let bgvs = TrackGroup::folder("BGVs")
        .track("BGVs 1")
        .item("02_BackingVox1.wav")
        .track("BGVs 2")
        .item("03_BackingVox2.wav")
        .end();

    let vocals = TrackGroup::folder("Vocals")
        .track("Lead")
        .item("07_LeadVox.wav")
        .group(bgvs)
        .end();

    let orchestra = TrackGroup::folder("Orchestra")
        .track("Strings")
        .item("09_Strings.wav")
        .track("Brass")
        .item("06_BrassSection.wav")
        .end();

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(percussion)
        .group(bass)
        .group(synths)
        .group(vocals)
        .group(orchestra)
        .build();
    expected
}

#[test]
fn night_panther_fire() {
    // -- Setup & Fixtures
    // Night Panther - Fire: 12-stem pop/funk session from MedleyDB. Electric bass,
    // male + female vocals, drum machine + live drums, brass section, string section,
    // 3 synths, and auxiliary percussion. Tests layered electronic/live hybrid with
    // orchestral elements.
    let items = vec![
        "01_ElecBass.wav",
        "02_BackingVox1.wav",
        "03_BackingVox2.wav",
        "04_DrumMachine.wav",
        "05_Drums.wav",
        "06_BrassSection.wav",
        "07_LeadVox.wav",
        "08_AuxPerc.wav",
        "09_Strings.wav",
        "10_Synth1.wav",
        "11_Synth2.wav",
        "12_Synth3.wav",
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

    let expected = night_panther_fire_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
