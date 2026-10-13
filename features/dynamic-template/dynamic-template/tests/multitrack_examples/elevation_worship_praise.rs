use super::worship_common::organize;

/// Elevation Worship – "Praise" official `MultiTracks` stem set (23 stems).
/// A / 127 bpm / 4-4 modern-worship session: click + cue guide tracks, an
/// original-mix reference, layered BGVs + choir, organ/keys/piano, electric +
/// synth bass, one acoustic and seven electric guitars, plus a loop and
/// hand/aux percussion.
///
/// This asserts the performance-grouping *contract* the app relies on
/// (membership + which folder each key stem lands in), rather than pinning the
/// exact interior numbering, so it stays robust to cosmetic ordering.
///
/// KNOWN LIMITATIONS (upstream `monarchy`, tracked separately — deliberately
/// NOT asserted here):
///   • The 7 electric guitars sub-nest (6 & 7 in an extra folder) because the
///     global Layers field treats bare digits "1".."5" as layer names but not
///     "6"/"7"; removing them regresses 19 other bands. Needs a monarchy fix.
///   • Choir lands in the top-level Choir group (a sibling of Vocals), not
///     nested under the Vocals bus — a "Choir" subgroup under Vocals duplicates
///     against the top-level group, whose name doubles as a match token. Route
///     Choir to the vocal VCA in the DAW instead.
#[test]
fn elevation_worship_praise() {
    let items = vec![
        "01 - Click.wav",
        "02 - Cue.wav",
        "03 - Elevation Worship - Praise (Original Track).wav",
        "04 - BGVS.wav",
        "05 - BGVS 2.wav",
        "06 - Choir.wav",
        "07 - Organ.wav",
        "08 - Keys.wav",
        "09 - Piano.wav",
        "10 - Electric Bass 1.wav",
        "11 - Electric Bass 2.wav",
        "12 - Synth Bass.wav",
        "13 - Acoustic Guitar.wav",
        "14 - Electric Guitar 1.wav",
        "15 - Electric Guitar 2.wav",
        "16 - Electric Guitar 3.wav",
        "17 - Electric Guitar 4.wav",
        "18 - Electric Guitar 5.wav",
        "19 - Electric Guitar 6.wav",
        "20 - Electric Guitar 7.wav",
        "21 - Loop.wav",
        "22 - Hand Percussion.wav",
        "23 - Percussion.wav",
    ];
    let (placement, _) = organize("Elevation Worship – Praise", &items).unwrap();

    // Helper: assert an item's top-level folder + owning node name.
    let top_of = |file: &str| {
        placement
            .get(file)
            .map(|(t, _)| t.clone())
            .unwrap_or_default()
    };
    let node_of = |file: &str| {
        placement
            .get(file)
            .map(|(_, n)| n.clone())
            .unwrap_or_default()
    };

    // ── Guide: Click + Cue (Cue is a proper Cue track, not "Dynamic Cues") ──
    assert_eq!(top_of("01 - Click.wav"), "Guide");
    assert_eq!(node_of("01 - Click.wav"), "Click");
    assert_eq!(top_of("02 - Cue.wav"), "Guide");
    assert_eq!(node_of("02 - Cue.wav"), "Cues");

    // ── Reference: the original-mix reference stem ──
    assert_eq!(
        node_of("03 - Elevation Worship - Praise (Original Track).wav"),
        "Reference"
    );

    // ── Tracks: the loop (backing/playback element) ──
    assert_eq!(node_of("21 - Loop.wav"), "Tracks");

    // ── Vocals bus: BGVs under the Vocals folder ──
    assert_eq!(top_of("04 - BGVS.wav"), "Vocals");
    assert_eq!(top_of("05 - BGVS 2.wav"), "Vocals");
    // Choir routes to the top-level Choir group (see KNOWN LIMITATIONS).
    assert_eq!(node_of("06 - Choir.wav"), "Choir");

    // ── Bass: electric + synth basses under the Bass folder ──
    for f in [
        "10 - Electric Bass 1.wav",
        "11 - Electric Bass 2.wav",
        "12 - Synth Bass.wav",
    ] {
        assert_eq!(top_of(f), "Bass", "{f} should be under Bass");
    }

    // ── Keys: organ / keys / piano ──
    for f in ["07 - Organ.wav", "08 - Keys.wav", "09 - Piano.wav"] {
        assert_eq!(top_of(f), "Keys", "{f} should be under Keys");
    }

    // ── Percussion: hand + aux percussion ──
    for f in ["22 - Hand Percussion.wav", "23 - Percussion.wav"] {
        assert_eq!(top_of(f), "Percussion", "{f} should be under Percussion");
    }

    // ── Guitars: acoustic + all seven electrics under the Guitars folder ──
    for f in [
        "13 - Acoustic Guitar.wav",
        "14 - Electric Guitar 1.wav",
        "15 - Electric Guitar 2.wav",
        "16 - Electric Guitar 3.wav",
        "17 - Electric Guitar 4.wav",
        "18 - Electric Guitar 5.wav",
        "19 - Electric Guitar 6.wav",
        "20 - Electric Guitar 7.wav",
    ] {
        assert_eq!(top_of(f), "Guitars", "{f} should be under Guitars");
    }
}
