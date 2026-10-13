//! Washed — Elevation Rhythm (B). 18-stem session: click + spoken guide, live
//! drums + loop, acoustic + electric guitar, five keys layers + piano, two FX
//! returns, two percussion, and a saxophone.
//!
//! Asserts the grouping contract (membership + top-level folder). See
//! `worship_common` for the placement helper and the shared guards.

use super::worship_common::{in_group, organize};

#[test]
fn washed() {
    let items = vec![
        "Washed - AG.wav",
        "Washed - Bass.wav",
        "Washed - Click.wav",
        "Washed - Drums (Live).wav",
        "Washed - EG.wav",
        "Washed - FX 1.wav",
        "Washed - FX 2.wav",
        "Washed - Guide.wav",
        "Washed - Keys 1.wav",
        "Washed - Keys 2.wav",
        "Washed - Keys 3.wav",
        "Washed - Keys 4.wav",
        "Washed - Keys 5.wav",
        "Washed - Loop.wav",
        "Washed - Perc 1.wav",
        "Washed - Perc 2.wav",
        "Washed - Piano.wav",
        "Washed - Saxophone.wav",
    ];
    let (placement, _) = organize("Washed", &items).unwrap();
    let ing = |f: &str, g: &str| in_group(&placement, f, g);

    assert!(ing("Washed - Click.wav", "Guide"));
    assert!(ing("Washed - Guide.wav", "Guide"));
    assert!(ing("Washed - Loop.wav", "Tracks"));
    assert!(ing("Washed - Bass.wav", "Bass"));
    // Guitars: acoustic + electric.
    assert!(ing("Washed - AG.wav", "Guitars"));
    assert!(ing("Washed - EG.wav", "Guitars"));
    // Keys: five layers + piano.
    for f in [
        "Washed - Keys 1.wav",
        "Washed - Keys 2.wav",
        "Washed - Keys 3.wav",
        "Washed - Keys 4.wav",
        "Washed - Keys 5.wav",
        "Washed - Piano.wav",
    ] {
        assert!(ing(f, "Keys"), "{f} should be under Keys");
    }
    // Percussion (two perc stems → a Percussion folder).
    assert!(ing("Washed - Perc 1.wav", "Percussion"));
    assert!(ing("Washed - Perc 2.wav", "Percussion"));
    // Drums.
    assert!(ing("Washed - Drums (Live).wav", "Drums"));
    // Saxophone → Horns; the two FX returns → SFX (documenting current behavior).
    assert!(ing("Washed - Saxophone.wav", "Horns"));
    assert!(ing("Washed - FX 1.wav", "SFX"));
}
