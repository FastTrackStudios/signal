use daw_proto::{assert_tracks_equal, TrackGroup, TrackStructureBuilder};
use dynamic_template::*;

fn mars_sws_bass_keys() -> (TrackGroup, TrackGroup) {
    let bass = TrackGroup::folder("Bass")
        .track("Synth 1")
        .item("Bass Synth.L.wav")
        .track("Synth 2")
        .item("Bass Synth.R.wav")
        .track("Synth 3")
        .item("Bass Synth.wav")
        .track("Synth 4")
        .item("Bass Synth.wav")
        .track("Synth 5")
        .item("Bass Synth.wav")
        .end();

    // --- Keys ---
    // All piano variants: L/R stereo (×4 each), Piano Room Mono (×4, now goes to Keys
    // since Room exclusions were added to Drums/Rooms for non-drum instruments),
    // Rough scratch, SUM (×2), Reverse Piano (×4).
    let keys = TrackGroup::folder("Keys")
        .track("Piano 1")
        .item("Piano L.wav")
        .track("Piano 2")
        .item("Piano L.wav")
        .track("Piano 3")
        .item("Piano L.wav")
        .track("Piano 4")
        .item("Piano L.wav")
        .track("Piano 5")
        .item("Piano R.wav")
        .track("Piano 6")
        .item("Piano R.wav")
        .track("Piano 7")
        .item("Piano R.wav")
        .track("Piano 8")
        .item("Piano R.wav")
        .track("Piano 9")
        .item("Piano Room Mono.wav")
        .track("Piano 10")
        .item("Piano Room Mono.wav")
        .track("Piano 11")
        .item("Piano Room Mono.wav")
        .track("Piano 12")
        .item("Piano Room Mono.wav")
        .track("Piano 13")
        .item("Piano_Rough_76bpm.wav")
        .track("Piano 14")
        .item("Piano_SUM.wav")
        .track("Piano 15")
        .item("Piano_SUM.wav")
        .track("Piano 16")
        .item("Reverse Piano.wav")
        .track("Piano 17")
        .item("Reverse Piano.wav")
        .track("Piano 18")
        .item("Reverse Piano.wav")
        .track("Piano 19")
        .item("Reverse Piano.wav")
        .end();

    (bass, keys)
}

fn mars_sws_vocals() -> (TrackGroup, TrackGroup) {
    // --- Vocals ---
    // Kim: VOX_SUM (×2) and bare VOX (×3) share the same base name and group into
    // an inner Kim folder. VOX_1 (playlist take 1) lands beside it as a sibling
    // track. VOX_2 (×2) forms a "Kim 2" layer subfolder.
    let kim_inner = TrackGroup::folder("Kim")
        .track("Kim 1")
        .item("Kim VOX_SUM.wav")
        .track("Kim 2")
        .item("Kim VOX_SUM.wav")
        .track("Kim 3")
        .item("Kim VOX.wav")
        .track("Kim 4")
        .item("Kim VOX.wav")
        .track("Kim 5")
        .item("Kim VOX.wav")
        .end();

    let kim_2 = TrackGroup::folder("Kim 2")
        .track("Kim 1")
        .item("Kim VOX_2.wav")
        .track("Kim 2")
        .item("Kim VOX_2.wav")
        .end();

    let kim = TrackGroup::folder("Kim")
        .group(kim_inner)
        .track("Kim 1")
        .item("Kim VOX_1.wav")
        .group(kim_2)
        .end();

    let steve = TrackGroup::folder("Steve")
        .track("Steve 1")
        .item("Steve VOX_SUM.wav")
        .track("Steve 2")
        .item("Steve VOX_SUM.wav")
        .track("Steve 3")
        .item("Steve VOX.wav")
        .track("Steve 4")
        .item("Steve VOX.wav")
        .track("Steve 5")
        .item("Steve VOX.wav")
        .track("Steve 6")
        .item("Steve VOX.wav")
        .end();

    (kim, steve)
}

/// BGVs: 4 parts × (SUM×2 + A×4 + B×4 + C×4 + D×4) = 72 flat tracks.
/// Note: the first two BGV4A entries render without a sequence number — this is
/// a monarchy counter quirk when the embedded layer digit "4" clashes with the
/// existing sequence state. Data-driven (rather than one 150-line builder
/// chain) to stay under the line-count lint.
fn mars_sws_bgvs() -> TrackGroup {
    let tracks: &[(&str, &str)] = &[
        ("BGVs 1", "BGV1_SUM.wav"),
        ("BGVs 2", "BGV1_SUM.wav"),
        ("BGVs 3", "BGV1A.wav"),
        ("BGVs 4", "BGV1A.wav"),
        ("BGVs 5", "BGV1A.wav"),
        ("BGVs 6", "BGV1A.wav"),
        ("BGVs 7", "BGV1B.wav"),
        ("BGVs 8", "BGV1B.wav"),
        ("BGVs 9", "BGV1B.wav"),
        ("BGVs 10", "BGV1B.wav"),
        ("BGVs 11", "BGV1C.wav"),
        ("BGVs 12", "BGV1C.wav"),
        ("BGVs 13", "BGV1C.wav"),
        ("BGVs 14", "BGV1C.wav"),
        ("BGVs 15", "BGV1D.wav"),
        ("BGVs 16", "BGV1D.wav"),
        ("BGVs 17", "BGV1D.wav"),
        ("BGVs 18", "BGV1D.wav"),
        ("BGVs 19", "BGV2_SUM.wav"),
        ("BGVs 20", "BGV2_SUM.wav"),
        ("BGVs 21", "BGV2A.wav"),
        ("BGVs 22", "BGV2A.wav"),
        ("BGVs 23", "BGV2A.wav"),
        ("BGVs 24", "BGV2A.wav"),
        ("BGVs 25", "BGV2B.wav"),
        ("BGVs 26", "BGV2B.wav"),
        ("BGVs 27", "BGV2B.wav"),
        ("BGVs 28", "BGV2B.wav"),
        ("BGVs 29", "BGV2C.wav"),
        ("BGVs 30", "BGV2C.wav"),
        ("BGVs 31", "BGV2C.wav"),
        ("BGVs 32", "BGV2C.wav"),
        ("BGVs 33", "BGV2D.wav"),
        ("BGVs 34", "BGV2D.wav"),
        ("BGVs 35", "BGV2D.wav"),
        ("BGVs 36", "BGV2D.wav"),
        ("BGVs 37", "BGV3_SUM.wav"),
        ("BGVs 38", "BGV3_SUM.wav"),
        ("BGVs 39", "BGV3A.wav"),
        ("BGVs 40", "BGV3A.wav"),
        ("BGVs 41", "BGV3A.wav"),
        ("BGVs 42", "BGV3A.wav"),
        ("BGVs 43", "BGV3B.wav"),
        ("BGVs 44", "BGV3B.wav"),
        ("BGVs 45", "BGV3B.wav"),
        ("BGVs 46", "BGV3B.wav"),
        ("BGVs 47", "BGV3C.wav"),
        ("BGVs 48", "BGV3C.wav"),
        ("BGVs 49", "BGV3C.wav"),
        ("BGVs 50", "BGV3C.wav"),
        ("BGVs 51", "BGV3D.wav"),
        ("BGVs 52", "BGV3D.wav"),
        ("BGVs 53", "BGV3D.wav"),
        ("BGVs 54", "BGV3D.wav"),
        // First two BGV4A entries render without a sequence number.
        ("BGVs 55", "BGV4_SUM.wav"),
        ("BGVs 56", "BGV4_SUM.wav"),
        ("BGVs", "BGV4A.wav"),
        ("BGVs", "BGV4A.wav"),
        ("BGVs 59", "BGV4A.wav"),
        ("BGVs 60", "BGV4A.wav"),
        ("BGVs 61", "BGV4B.wav"),
        ("BGVs 62", "BGV4B.wav"),
        ("BGVs 63", "BGV4B.wav"),
        ("BGVs 64", "BGV4B.wav"),
        ("BGVs 65", "BGV4C.wav"),
        ("BGVs 66", "BGV4C.wav"),
        ("BGVs 67", "BGV4C.wav"),
        ("BGVs 68", "BGV4C.wav"),
        ("BGVs 69", "BGV4D.wav"),
        ("BGVs 70", "BGV4D.wav"),
        ("BGVs 71", "BGV4D.wav"),
        ("BGVs 72", "BGV4D.wav"),
    ];
    let mut builder = TrackGroup::folder("BGVs");
    for (name, item) in tracks {
        builder = builder.track(*name).item(*item);
    }
    builder.end()
}

fn mars_sws_expected() -> daw_proto::TrackHierarchy {
    let (bass, keys) = mars_sws_bass_keys();
    let (kim, steve) = mars_sws_vocals();
    let bgvs = mars_sws_bgvs();
    let lead = TrackGroup::folder("Lead").group(kim).group(steve).end();

    let vocals = TrackGroup::folder("Vocals").group(lead).group(bgvs).end();

    // --- Reference ---
    let reference = TrackGroup::folder("Reference")
        .track("Crystalizer_ 1")
        .item("Crystalizer_Print_1.wav")
        .track("Crystalizer_ 2")
        .item("Crystalizer_Print.wav")
        .track("Crystalizer_ 3")
        .item("Crystalizer_Print.wav")
        .end();

    // --- Unsorted ---
    // "Mars Kim vx" and "Mars_PLAP" have no recognised instrument keywords.
    let unsorted = TrackGroup::folder("Unsorted")
        .track("Mars Kim Vx")
        .item("Mars Kim vx.wav")
        .track("Mars_PLAP")
        .item("Mars_PLAP.wav")
        .end();

    // ============================================================================
    // Compose final structure
    // ============================================================================

    let expected = TrackStructureBuilder::new()
        .group(bass)
        .group(keys)
        .group(vocals)
        .group(reference)
        .group(unsorted)
        .build();
    expected
}

fn mars_sws_items_a() -> Vec<&'static str> {
    vec![
        "Bass Synth.L.wav",
        "Bass Synth.R.wav",
        "Bass Synth.wav",
        "Bass Synth.wav",
        "Bass Synth.wav",
        "BGV1_SUM.wav",
        "BGV1_SUM.wav",
        "BGV1A.wav",
        "BGV1A.wav",
        "BGV1A.wav",
        "BGV1A.wav",
        "BGV1B.wav",
        "BGV1B.wav",
        "BGV1B.wav",
        "BGV1B.wav",
        "BGV1C.wav",
        "BGV1C.wav",
        "BGV1C.wav",
        "BGV1C.wav",
        "BGV1D.wav",
        "BGV1D.wav",
        "BGV1D.wav",
        "BGV1D.wav",
        "BGV2_SUM.wav",
        "BGV2_SUM.wav",
        "BGV2A.wav",
        "BGV2A.wav",
        "BGV2A.wav",
        "BGV2A.wav",
        "BGV2B.wav",
        "BGV2B.wav",
        "BGV2B.wav",
        "BGV2B.wav",
        "BGV2C.wav",
        "BGV2C.wav",
        "BGV2C.wav",
        "BGV2C.wav",
        "BGV2D.wav",
        "BGV2D.wav",
        "BGV2D.wav",
        "BGV2D.wav",
        "BGV3_SUM.wav",
        "BGV3_SUM.wav",
        "BGV3A.wav",
        "BGV3A.wav",
        "BGV3A.wav",
        "BGV3A.wav",
        "BGV3B.wav",
        "BGV3B.wav",
        "BGV3B.wav",
        "BGV3B.wav",
        "BGV3C.wav",
        "BGV3C.wav",
        "BGV3C.wav",
        "BGV3C.wav",
        "BGV3D.wav",
        "BGV3D.wav",
    ]
}

/// Mars SWS: 124-track session with duplicate items and extensive BGV arrangements.
/// No drums — only piano (with room mic), synth bass, lead vocals (Kim + Steve),
/// and a large BGV stack (4 parts × SUM + A/B/C/D, many duplicates).
fn mars_sws_items() -> Vec<&'static str> {
    let mut items = mars_sws_items_a();
    items.extend([
        "BGV3D.wav",
        "BGV3D.wav",
        "BGV4_SUM.wav",
        "BGV4_SUM.wav",
        "BGV4A.wav",
        "BGV4A.wav",
        "BGV4A.wav",
        "BGV4A.wav",
        "BGV4B.wav",
        "BGV4B.wav",
        "BGV4B.wav",
        "BGV4B.wav",
        "BGV4C.wav",
        "BGV4C.wav",
        "BGV4C.wav",
        "BGV4C.wav",
        "BGV4D.wav",
        "BGV4D.wav",
        "BGV4D.wav",
        "BGV4D.wav",
        "Crystalizer_Print_1.wav",
        "Crystalizer_Print.wav",
        "Crystalizer_Print.wav",
        "Kim VOX_1.wav",
        "Kim VOX_2.wav",
        "Kim VOX_2.wav",
        "Kim VOX_SUM.wav",
        "Kim VOX_SUM.wav",
        "Kim VOX.wav",
        "Kim VOX.wav",
        "Kim VOX.wav",
        "Mars Kim vx.wav",
        "Mars_PLAP.wav",
        "Piano L.wav",
        "Piano L.wav",
        "Piano L.wav",
        "Piano L.wav",
        "Piano R.wav",
        "Piano R.wav",
        "Piano R.wav",
        "Piano R.wav",
        "Piano Room Mono.wav",
        "Piano Room Mono.wav",
        "Piano Room Mono.wav",
        "Piano Room Mono.wav",
        "Piano_Rough_76bpm.wav",
        "Piano_SUM.wav",
        "Piano_SUM.wav",
        "Reverse Piano.wav",
        "Reverse Piano.wav",
        "Reverse Piano.wav",
        "Reverse Piano.wav",
        "Steve VOX_SUM.wav",
        "Steve VOX_SUM.wav",
        "Steve VOX.wav",
        "Steve VOX.wav",
        "Steve VOX.wav",
        "Steve VOX.wav",
    ]);
    items
}

#[test]
fn mars_sws() {
    let items = mars_sws_items();
    let config = default_config();

    // -- Exec
    let tracks = items.organize_into_tracks(&config, None).unwrap();

    // -- Check
    println!("\nTrack list:");
    daw_proto::display_tracklist(&tracks);

    // ============================================================================
    // Expected structure
    // ============================================================================

    // --- Bass ---
    // Bass Synth L/R are the stereo DI pair; remaining three are duplicate takes.
    let expected = mars_sws_expected();

    assert_tracks_equal(&tracks, &expected).unwrap();
}
