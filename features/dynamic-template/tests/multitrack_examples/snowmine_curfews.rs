use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn snowmine_curfews_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::single_track("Drums", "05_Drums.wav");

    // Vibraphone dual-classified
    let percussion = TrackGroup::folder("Percussion")
        .track("Aux")
        .item("01_AuxPerc.wav")
        .track("Orch")
        .item("04_Vibraphone.wav")
        .end();

    // ElecBass now correctly classified as Bass
    let bass = TrackGroup::single_track("Bass", "02_ElecBass.wav");

    let guitars = TrackGroup::folder("Guitars")
        .track("Electric 1")
        .item("07_ElecGtr1.wav")
        .track("Electric 2")
        .item("08_ElecGtr2.wav")
        .end();

    let keys = TrackGroup::single_track("Keys", "11_TackPiano.wav");

    // Sampler now classified as Synths, along with Synth
    let synths = TrackGroup::folder("Synths")
        .track("Sampler")
        .item("06_Sampler.wav")
        .track("Synth")
        .item("09_Synth.wav")
        .end();

    let vocals = TrackGroup::folder("Vocals")
        .track("Lead")
        .item("03_LeadVox.wav")
        .track("BGVs")
        .item("10_BackingVox.wav")
        .end();

    let orchestra = TrackGroup::single_track("Orchestra", "04_Vibraphone.wav");

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(percussion)
        .group(bass)
        .group(guitars)
        .group(keys)
        .group(synths)
        .group(vocals)
        .group(orchestra)
        .build();
    expected
}

#[test]
fn snowmine_curfews() {
    // -- Setup & Fixtures
    // Snowmine - Curfews: 11-stem indie pop session from MedleyDB. Auxiliary percussion,
    // electric bass, lead + backing vocals, vibraphone, drums, sampler, 2 clean electric
    // guitars, synthesizer, and tack piano. Tests eclectic indie instrumentation with
    // vibraphone and sampler classification.
    let items = vec![
        "01_AuxPerc.wav",
        "02_ElecBass.wav",
        "03_LeadVox.wav",
        "04_Vibraphone.wav",
        "05_Drums.wav",
        "06_Sampler.wav",
        "07_ElecGtr1.wav",
        "08_ElecGtr2.wav",
        "09_Synth.wav",
        "10_BackingVox.wav",
        "11_TackPiano.wav",
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

    let expected = snowmine_curfews_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
