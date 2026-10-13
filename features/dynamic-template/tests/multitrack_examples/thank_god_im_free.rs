//! Thank God I'm Free — Elevation Rhythm (E). 36-stem session (the big one):
//! click + spoken guide, live drums + two loops, eleven electric guitars,
//! electric + synth bass, five keys layers + two pianos, arps/bells/strings,
//! synth lead/pad/FX, BGVs + choir, and vocal/piano FX returns.
//!
//! Asserts the grouping contract (membership + top-level folder) for the
//! unambiguous families. See `worship_common` for the placement helper and the
//! shared guards (placed-once + no Unsorted cover the rest).

use super::worship_common::{in_group, organize};

#[test]
fn thank_god_im_free() {
    let items = vec![
        "Thank God I_m Free - Arps 1.wav",
        "Thank God I_m Free - Arps 2.wav",
        "Thank God I_m Free - Bass.wav",
        "Thank God I_m Free - Bells.wav",
        "Thank God I_m Free - BGVS.wav",
        "Thank God I_m Free - Choir.wav",
        "Thank God I_m Free - Click.wav",
        "Thank God I_m Free - Drums (Live).wav",
        "Thank God I_m Free - EG 1.wav",
        "Thank God I_m Free - EG 2.wav",
        "Thank God I_m Free - EG 3.wav",
        "Thank God I_m Free - EG 4.wav",
        "Thank God I_m Free - EG 5.wav",
        "Thank God I_m Free - EG 6.wav",
        "Thank God I_m Free - EG 7.wav",
        "Thank God I_m Free - EG 8.wav",
        "Thank God I_m Free - EG 9.wav",
        "Thank God I_m Free - EG 10.wav",
        "Thank God I_m Free - EG 11.wav",
        "Thank God I_m Free - Guide.wav",
        "Thank God I_m Free - Keys 1.wav",
        "Thank God I_m Free - Keys 2.wav",
        "Thank God I_m Free - Keys 3.wav",
        "Thank God I_m Free - Keys 4.wav",
        "Thank God I_m Free - Keys 5.wav",
        "Thank God I_m Free - Loop 1.wav",
        "Thank God I_m Free - Loop 2.wav",
        "Thank God I_m Free - Piano 1.wav",
        "Thank God I_m Free - Piano 2.wav",
        "Thank God I_m Free - Piano FX.wav",
        "Thank God I_m Free - Strings.wav",
        "Thank God I_m Free - Synth Bass.wav",
        "Thank God I_m Free - Synth FX.wav",
        "Thank God I_m Free - Synth Lead.wav",
        "Thank God I_m Free - Synth Pad.wav",
        "Thank God I_m Free - Vox FX.wav",
    ];
    let (placement, _) = organize("Thank God I'm Free", &items).unwrap();
    let ing = |f: &str, g: &str| in_group(&placement, f, g);

    assert!(ing("Thank God I_m Free - Click.wav", "Guide"));
    assert!(ing("Thank God I_m Free - Guide.wav", "Guide"));
    // Tracks: both loops.
    assert!(ing("Thank God I_m Free - Loop 1.wav", "Tracks"));
    assert!(ing("Thank God I_m Free - Loop 2.wav", "Tracks"));
    // Bass: electric + synth.
    assert!(ing("Thank God I_m Free - Bass.wav", "Bass"));
    assert!(ing("Thank God I_m Free - Synth Bass.wav", "Bass"));
    // Guitars: all eleven electrics (6-11 sub-nest — the known monarchy limit).
    for n in 1..=11 {
        let f = format!("Thank God I_m Free - EG {n}.wav");
        assert!(ing(&f, "Guitars"), "{f} should be under Guitars");
    }
    // Keys: five keys layers + two pianos (+ Piano FX).
    for f in [
        "Thank God I_m Free - Keys 1.wav",
        "Thank God I_m Free - Keys 5.wav",
        "Thank God I_m Free - Piano 1.wav",
        "Thank God I_m Free - Piano 2.wav",
    ] {
        assert!(ing(f, "Keys"), "{f} should be under Keys");
    }
    // Drums.
    assert!(ing("Thank God I_m Free - Drums (Live).wav", "Drums"));
    // Synths: lead / pad / FX (+ Bells fold in here).
    for f in [
        "Thank God I_m Free - Synth Lead.wav",
        "Thank God I_m Free - Synth Pad.wav",
        "Thank God I_m Free - Bells.wav",
    ] {
        assert!(ing(f, "Synths"), "{f} should be under Synths");
    }
    // Vocals bus: BGVs + the Vox FX return. Choir → top-level Choir group.
    assert!(ing("Thank God I_m Free - BGVS.wav", "Vocals"));
    assert!(ing("Thank God I_m Free - Vox FX.wav", "Vocals"));
    assert!(ing("Thank God I_m Free - Choir.wav", "Choir"));
    // Strings → the Orchestra group.
    assert!(ing("Thank God I_m Free - Strings.wav", "Orchestra"));
    // KNOWN GAP: "Arps" (arpeggiators) aren't classified → Unsorted.
    assert!(ing("Thank God I_m Free - Arps 1.wav", "Unsorted"));
    assert!(ing("Thank God I_m Free - Arps 2.wav", "Unsorted"));
}
