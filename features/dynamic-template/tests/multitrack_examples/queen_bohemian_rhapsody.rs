use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn queen_bohemian_rhapsody_expected() -> daw_proto::TrackHierarchy {
    let cymbals = TrackGroup::folder("Cymbals")
        .track("OH")
        .item("05_Overheads.wav")
        .track("Hi Hat")
        .item("03_HiHat.wav")
        .end();

    // Toms now correctly classified in Drums
    let drums = TrackGroup::folder("Drums")
        .track("Kick")
        .item("01_Kick.wav")
        .track("Snare")
        .item("02_Snare.wav")
        .track("Toms")
        .item("04_Toms.wav")
        .group(cymbals)
        .end();

    // Gong + timpani dual-classified as percussion and orchestral
    let percussion = TrackGroup::folder("Percussion")
        .track("Orch 1")
        .item("12_Gong.wav")
        .track("Orch 2")
        .item("13_Timpani.wav")
        .end();

    let bass = TrackGroup::single_track("Bass", "06_Bass.wav");

    let lead_gtrs = TrackGroup::folder("Lead")
        .track("Electric 1")
        .item("08_LeadGtr1.wav")
        .track("Electric 2")
        .item("09_LeadGtr2.wav")
        .end();

    let guitars = TrackGroup::folder("Guitars")
        .group(lead_gtrs)
        .track("Rhythm")
        .item("07_RhythmGtr.wav")
        .track("Guitars")
        .item("10_GtrHarmony.wav")
        .end();

    let keys = TrackGroup::single_track("Keys", "11_Piano.wav");

    // Opera vocals classified as Lead (contains "Vox")
    let lead = TrackGroup::folder("Lead")
        .track("Lead 1")
        .item("14_LeadVox1.wav")
        .track("Lead 2")
        .item("15_LeadVox2.wav")
        .track("Lead 3")
        .item("16_LeadVox3.wav")
        .track("Lead 4")
        .item("23_OperaVox1.wav")
        .track("Lead 5")
        .item("24_OperaVox2.wav")
        .end();

    let bgvs = TrackGroup::folder("BGVs")
        .track("BGVs 1")
        .item("17_BackingVox1.wav")
        .track("BGVs 2")
        .item("18_BackingVox2.wav")
        .track("BGVs 3")
        .item("19_BackingVox3.wav")
        .track("BGVs 4")
        .item("20_BackingVox4.wav")
        .track("BGVs 5")
        .item("21_BackingVox5.wav")
        .track("BGVs 6")
        .item("22_BackingVox6.wav")
        .end();

    let vocals = TrackGroup::folder("Vocals").group(lead).group(bgvs).end();

    // Orchestra: gong + timpani as orchestral percussion
    let orchestra = TrackGroup::folder("Orchestra")
        .track("Percussion 1")
        .item("12_Gong.wav")
        .track("Percussion 2")
        .item("13_Timpani.wav")
        .end();

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(percussion)
        .group(bass)
        .group(guitars)
        .group(keys)
        .group(vocals)
        .group(orchestra)
        .build();
    expected
}

#[test]
fn queen_bohemian_rhapsody() {
    // -- Setup & Fixtures
    // Queen - Bohemian Rhapsody: 24-stem multi-section epic rock session. Features
    // full drum kit, bass, layered guitars (rhythm, lead, harmonized), piano, gong,
    // timpani, multiple lead vocal takes (ballad, opera, rock sections), and
    // extensive multi-part backing vocal choir. Tests complex vocal layering.
    let items = vec![
        "01_Kick.wav",
        "02_Snare.wav",
        "03_HiHat.wav",
        "04_Toms.wav",
        "05_Overheads.wav",
        "06_Bass.wav",
        "07_RhythmGtr.wav",
        "08_LeadGtr1.wav",
        "09_LeadGtr2.wav",
        "10_GtrHarmony.wav",
        "11_Piano.wav",
        "12_Gong.wav",
        "13_Timpani.wav",
        "14_LeadVox1.wav",
        "15_LeadVox2.wav",
        "16_LeadVox3.wav",
        "17_BackingVox1.wav",
        "18_BackingVox2.wav",
        "19_BackingVox3.wav",
        "20_BackingVox4.wav",
        "21_BackingVox5.wav",
        "22_BackingVox6.wav",
        "23_OperaVox1.wav",
        "24_OperaVox2.wav",
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

    let expected = queen_bohemian_rhapsody_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
