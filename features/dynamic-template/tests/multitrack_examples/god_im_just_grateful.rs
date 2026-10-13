//! God, I'm Just Grateful — Elevation Worship (D). 19-stem session: click +
//! spoken guide, two drum layers + loop, four electric guitars, electric +
//! synth bass, organ/keys/piano, strings, synths, BGVs and two choir layers.
//!
//! Asserts the grouping contract (membership + top-level folder). See
//! `worship_common` for the placement helper and the shared guards.

use super::worship_common::{in_group, organize};

#[test]
fn god_im_just_grateful() {
    let items = vec![
        "God, I_m Just Grateful - Bass.wav",
        "God, I_m Just Grateful - BGVS.wav",
        "God, I_m Just Grateful - Choir 1.wav",
        "God, I_m Just Grateful - Choir 2.wav",
        "God, I_m Just Grateful - Click.wav",
        "God, I_m Just Grateful - Drums 1.wav",
        "God, I_m Just Grateful - Drums 2.wav",
        "God, I_m Just Grateful - EG 1.wav",
        "God, I_m Just Grateful - EG 2.wav",
        "God, I_m Just Grateful - EG 3.wav",
        "God, I_m Just Grateful - EG 4.wav",
        "God, I_m Just Grateful - Guide.wav",
        "God, I_m Just Grateful - Keys.wav",
        "God, I_m Just Grateful - Loop.wav",
        "God, I_m Just Grateful - Organ.wav",
        "God, I_m Just Grateful - Piano.wav",
        "God, I_m Just Grateful - Strings.wav",
        "God, I_m Just Grateful - Synth Bass.wav",
        "God, I_m Just Grateful - Synths.wav",
    ];
    let (placement, _) = organize("God, I'm Just Grateful", &items).unwrap();
    let ing = |f: &str, g: &str| in_group(&placement, f, g);

    assert!(ing("God, I_m Just Grateful - Click.wav", "Guide"));
    assert!(ing("God, I_m Just Grateful - Guide.wav", "Guide"));
    assert!(ing("God, I_m Just Grateful - Loop.wav", "Tracks"));
    // Bass: electric + synth.
    assert!(ing("God, I_m Just Grateful - Bass.wav", "Bass"));
    assert!(ing("God, I_m Just Grateful - Synth Bass.wav", "Bass"));
    // Guitars: four electrics.
    for f in [
        "God, I_m Just Grateful - EG 1.wav",
        "God, I_m Just Grateful - EG 2.wav",
        "God, I_m Just Grateful - EG 3.wav",
        "God, I_m Just Grateful - EG 4.wav",
    ] {
        assert!(ing(f, "Guitars"), "{f} should be under Guitars");
    }
    // Keys: organ / keys / piano.
    for f in [
        "God, I_m Just Grateful - Organ.wav",
        "God, I_m Just Grateful - Keys.wav",
        "God, I_m Just Grateful - Piano.wav",
    ] {
        assert!(ing(f, "Keys"), "{f} should be under Keys");
    }
    // Drums: two layers.
    assert!(ing("God, I_m Just Grateful - Drums 1.wav", "Drums"));
    assert!(ing("God, I_m Just Grateful - Drums 2.wav", "Drums"));
    // Vocals bus: BGVs. Choir routes to the top-level Choir group (see the
    // Praise test's KNOWN LIMITATIONS note — Choir can't nest under Vocals).
    assert!(ing("God, I_m Just Grateful - BGVS.wav", "Vocals"));
    assert!(ing("God, I_m Just Grateful - Choir 1.wav", "Choir"));
    assert!(ing("God, I_m Just Grateful - Choir 2.wav", "Choir"));
    // Strings → the Orchestra group.
    assert!(ing("God, I_m Just Grateful - Strings.wav", "Orchestra"));
    // Bare "Synths" (plural) matches its own top-level Synths group.
    assert!(ing("God, I_m Just Grateful - Synths.wav", "Synths"));
}
