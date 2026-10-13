use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn bon_jovi_you_give_love_a_bad_name_expected() -> daw_proto::TrackHierarchy {
    let drums = TrackGroup::folder("Drums")
        .track("Drum Kit 1")
        .item("Drums.PNT.L.wav")
        .track("Drum Kit 2")
        .item("Drums.PNT.R.wav")
        .end();

    // --- Percussion ---
    // 095 BPM tambourine stereo pair
    let percussion = TrackGroup::folder("Percussion")
        .track("Tambourine 1")
        .item("095 Pop Tamb.L.wav")
        .track("Tambourine 2")
        .item("095 Pop Tamb.R.wav")
        .end();

    // --- Bass ---
    let bass = TrackGroup::single_track("Bass", "Bass.wav");

    // --- Guitars ---
    // Electric: Slide guitar and Solo guitar (distinct arrangements)
    let electric = TrackGroup::folder("Electric")
        .track("Slide")
        .item("Guitar Slide.wav")
        .track("Solo")
        .item("Guitar Solo.wav")
        .end();

    // Acoustic: Mandolin sub-instrument, plus two acoustic takes (Right and main)
    let acoustic = TrackGroup::folder("Acoustic")
        .track("Mando")
        .item("Mando.wav")
        .track("Acoustic 1")
        .item("Acoustic.Right.wav")
        .track("Acoustic 2")
        .item("Acoustic.wav")
        .end();

    // Banjo: three tracks (One/Two as numbered takes, Solo as arrangement)
    let banjo = TrackGroup::folder("Banjo")
        .track("Banjo 1")
        .item("banjo.One.wav")
        .track("Solo")
        .item("banjo.Solo.wav")
        .track("Banjo 2")
        .item("banjo.Two.wav")
        .end();

    let guitars = TrackGroup::folder("Guitars")
        .group(electric)
        .group(acoustic)
        .group(banjo)
        .end();

    // --- Fiddle ---
    // New Fiddle group captures session fiddle stereo print
    let fiddle = TrackGroup::folder("Fiddle")
        .track("Fiddle 1")
        .item("Fiddle PNT.L.wav")
        .track("Fiddle 2")
        .item("Fiddle PNT.R.wav")
        .end();

    // --- Vocals ---
    let bgvs = TrackGroup::folder("BGVs")
        .track("BGVs 1")
        .item("Vocal.Harmony.One.wav")
        .track("BGVs 2")
        .item("Vocal.Harmony.Two.wav")
        .end();

    let vocals = TrackGroup::folder("Vocals")
        .track("Lead")
        .item("Vocal.Tune.Lead.wav")
        .group(bgvs)
        .end();

    // --- Reference ---
    let reference = TrackGroup::folder("Reference")
        .track("You Give Love A Bad Name. 1")
        .item("You Give Love A Bad Name.PRINT.L.wav")
        .track("You Give Love A Bad Name. 2")
        .item("You Give Love A Bad Name.PRINT.R.wav")
        .end();

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(drums)
        .group(percussion)
        .group(bass)
        .group(guitars)
        .group(fiddle)
        .group(vocals)
        .group(reference)
        .build();
    expected
}

#[test]
fn bon_jovi_you_give_love_a_bad_name() {
    // -- Setup & Fixtures
    // Bon Jovi - You Give Love A Bad Name: Live/session recording with fiddle, banjo,
    // slide guitar, and full folk-rock arrangement.
    let items = vec![
        "095 Pop Tamb.L.wav",
        "095 Pop Tamb.R.wav",
        "Acoustic.Right.wav",
        "Acoustic.wav",
        "banjo.One.wav",
        "banjo.Solo.wav",
        "banjo.Two.wav",
        "Bass.wav",
        "Drums.PNT.L.wav",
        "Drums.PNT.R.wav",
        "Fiddle PNT.L.wav",
        "Fiddle PNT.R.wav",
        "Guitar Slide.wav",
        "Guitar Solo.wav",
        "Mando.wav",
        "Vocal.Harmony.One.wav",
        "Vocal.Harmony.Two.wav",
        "Vocal.Tune.Lead.wav",
        "You Give Love A Bad Name.PRINT.L.wav",
        "You Give Love A Bad Name.PRINT.R.wav",
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
    // Stereo print drum bus (L/R)
    let expected = bon_jovi_you_give_love_a_bad_name_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
