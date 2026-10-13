//! Who Else — Gateway Worship (Ab). 23-stem session: click + spoken guide,
//! drums, acoustic + five electric guitars, electric + synth bass, organ +
//! eight keys layers + piano, percussion, and a synth FX.
//!
//! Asserts the grouping contract (membership + top-level folder). See
//! `worship_common` for the placement helper and the shared guards.

use super::worship_common::{in_group, organize};

#[test]
fn who_else() {
    let items = vec![
        "Who Else - AG.wav",
        "Who Else - Bass.wav",
        "Who Else - Click.wav",
        "Who Else - Drums.wav",
        "Who Else - EG 1.wav",
        "Who Else - EG 2.wav",
        "Who Else - EG 3.wav",
        "Who Else - EG 4.wav",
        "Who Else - EG 5.wav",
        "Who Else - Guide.wav",
        "Who Else - Keys 1.wav",
        "Who Else - Keys 2.wav",
        "Who Else - Keys 3.wav",
        "Who Else - Keys 4.wav",
        "Who Else - Keys 5.wav",
        "Who Else - Keys 6.wav",
        "Who Else - Keys 7.wav",
        "Who Else - Keys 8.wav",
        "Who Else - Organ.wav",
        "Who Else - Percussion.wav",
        "Who Else - Piano.wav",
        "Who Else - Synth Bass.wav",
        "Who Else - Synth FX.wav",
    ];
    let (placement, _) = organize("Who Else", &items).unwrap();
    let ing = |f: &str, g: &str| in_group(&placement, f, g);

    assert!(ing("Who Else - Click.wav", "Guide"));
    assert!(ing("Who Else - Guide.wav", "Guide"));
    assert!(ing("Who Else - Bass.wav", "Bass"));
    assert!(ing("Who Else - Synth Bass.wav", "Bass"));
    // Guitars: acoustic + five electrics.
    for f in [
        "Who Else - AG.wav",
        "Who Else - EG 1.wav",
        "Who Else - EG 2.wav",
        "Who Else - EG 3.wav",
        "Who Else - EG 4.wav",
        "Who Else - EG 5.wav",
    ] {
        assert!(ing(f, "Guitars"), "{f} should be under Guitars");
    }
    // Keys: organ + eight keys layers + piano.
    for f in [
        "Who Else - Organ.wav",
        "Who Else - Keys 1.wav",
        "Who Else - Keys 8.wav",
        "Who Else - Piano.wav",
    ] {
        assert!(ing(f, "Keys"), "{f} should be under Keys");
    }
    assert!(ing("Who Else - Percussion.wav", "Percussion"));
    assert!(ing("Who Else - Drums.wav", "Drums"));
}
