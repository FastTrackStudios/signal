use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn stevie_wonder_superstition_expected() -> daw_proto::TrackHierarchy {
    let oh = TrackGroup::folder("OH")
        .track("Ride")
        .item("009 Overhead Ride_01.wav")
        .track("OH")
        .item("008 Overhead Hats_01.wav")
        .end();

    let drums = TrackGroup::folder("Drums")
        .track("Kick")
        .item("006 Kick_01.wav")
        .track("Snare")
        .item("007 Snare_01.wav")
        .group(oh)
        .end();

    // --- Bass ---
    let bass = TrackGroup::folder("Bass")
        .track("Bass")
        .item("015 Bass DI_01.wav")
        .track("Amp")
        .item("016 Bass Amp Sim_01.wav")
        .end();

    // --- Guitars ---
    // 4 electric guitarists, each with DI + amp sim pair
    // "Elec Gui N" maps to "Guitars N" folder with Amp and DI children
    let gtr1 = TrackGroup::folder("Guitars 1")
        .track("Amp")
        .item("018 Elec Gui 1 Amp Sim_01.wav")
        .track("DI")
        .item("017 Elec Gui 1 DI_01.wav")
        .end();

    let gtr2 = TrackGroup::folder("Guitars 2")
        .track("Amp")
        .item("020 Elec Gui 2 Amp Sim_01.wav")
        .track("DI")
        .item("019 Elec Gui 2 DI_01.wav")
        .end();

    let gtr3 = TrackGroup::folder("Guitars 3")
        .track("Amp")
        .item("024 Elec Gui 3 Amp Sim B_01.wav")
        .track("DI")
        .item("021 Elec Gui 3 DI_01.wav")
        .end();

    let gtr4 = TrackGroup::folder("Guitars 4")
        .track("Amp")
        .item("022 Elec Gui 4 Amp Sim_01.wav")
        .track("DI")
        .item("023 Elec Gui 4 DI_01.wav")
        .end();

    let guitars = TrackGroup::folder("Guitars")
        .group(gtr1)
        .group(gtr2)
        .group(gtr3)
        .group(gtr4)
        .end();

    // --- Horns ---
    // Saxophone section: Soprano, Alto, Tenor, Bari (sax subgroup collapses into Horns
    // since all horn tracks are saxophones, making the Saxophone folder redundant)
    let horns = TrackGroup::folder("Horns")
        .track("Saxophone 1")
        .item("001 Soprano sax_01.wav")
        .track("Saxophone 2")
        .item("002 Alto sax_01.wav")
        .track("Saxophone 3")
        .item("004Tenor sax_01.wav")
        .track("Saxophone 4")
        .item("005 Bari sax_01.wav")
        .end();

    // --- Vocals ---
    // Lead Vocal and Horn Vocal (unnumbered) share the "Vocal" display name and
    // are auto-grouped together. Horn Vocal 1/2/3 appear as numbered Vocals siblings.
    let vocals_inner = TrackGroup::folder("Vocals")
        .track("Lead 1")
        .item("010 Lead Vocal_01.wav")
        .track("Lead 2")
        .item("014 Horn Vocal_01.wav")
        .end();

    let vocals = TrackGroup::folder("Vocals")
        .group(vocals_inner)
        .track("Vocals 1")
        .item("011 Horn Vocal 1_01.wav")
        .track("Vocals 2")
        .item("012 Horn Vocal 2_01.wav")
        .track("Vocals 3")
        .item("013 Horn Vocal 3_01.wav")
        .end();

    // --- Choir ---
    // "003 Alto (uni tenor)" = alto vocal doubling tenor unison, classified as Choir
    let choir = TrackGroup::single_track("Choir", "003 Alto (uni tenor)_01.wav");

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(bass)
        .group(guitars)
        .group(horns)
        .group(vocals)
        .group(choir)
        .track("Reference")
        .item("Superstition Mix 1_01.wav")
        .build();
    expected
}

#[test]
fn stevie_wonder_superstition() {
    // -- Setup & Fixtures
    // Stevie Wonder - Superstition: dense funk session with horn section, 4 electric
    // guitarists (DI + amp sim each), drums, bass, lead vocal, and horn vocals (BGVs).
    let items = vec![
        "001 Soprano sax_01.wav",
        "002 Alto sax_01.wav",
        "003 Alto (uni tenor)_01.wav",
        "004Tenor sax_01.wav",
        "005 Bari sax_01.wav",
        "006 Kick_01.wav",
        "007 Snare_01.wav",
        "008 Overhead Hats_01.wav",
        "009 Overhead Ride_01.wav",
        "010 Lead Vocal_01.wav",
        "011 Horn Vocal 1_01.wav",
        "012 Horn Vocal 2_01.wav",
        "013 Horn Vocal 3_01.wav",
        "014 Horn Vocal_01.wav",
        "015 Bass DI_01.wav",
        "016 Bass Amp Sim_01.wav",
        "017 Elec Gui 1 DI_01.wav",
        "018 Elec Gui 1 Amp Sim_01.wav",
        "019 Elec Gui 2 DI_01.wav",
        "020 Elec Gui 2 Amp Sim_01.wav",
        "021 Elec Gui 3 DI_01.wav",
        "022 Elec Gui 4 Amp Sim_01.wav",
        "023 Elec Gui 4 DI_01.wav",
        "024 Elec Gui 3 Amp Sim B_01.wav",
        "Superstition Mix 1_01.wav",
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
    // Simple live kit: kick, snare, overhead hats and ride
    let expected = stevie_wonder_superstition_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
