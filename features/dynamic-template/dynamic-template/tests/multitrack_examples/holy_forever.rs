//! Holy Forever — Bethel Music (Bb). 13-stem modern-worship session:
//! click + spoken guide, live drums + loop, one acoustic and two electric
//! guitars, electric + synth bass, three keys layers + piano.
//!
//! Asserts the grouping contract (membership + top-level folder), not the
//! exact interior numbering. See `worship_common` for the placement helper and
//! the shared Unsorted / placed-once guards.

use super::worship_common::{in_group, organize};

#[test]
fn holy_forever() {
    let items = vec![
        "Holy Forever - AG.wav",
        "Holy Forever - Bass.wav",
        "Holy Forever - Click.wav",
        "Holy Forever - Drums.wav",
        "Holy Forever - EG 1.wav",
        "Holy Forever - EG 2.wav",
        "Holy Forever - Guide.wav",
        "Holy Forever - Keys 1.wav",
        "Holy Forever - Keys 2.wav",
        "Holy Forever - Keys 3.wav",
        "Holy Forever - Loop.wav",
        "Holy Forever - Piano.wav",
        "Holy Forever - Synth Bass.wav",
    ];
    let (placement, _) = organize("Holy Forever", &items).unwrap();
    let ing = |f: &str, g: &str| in_group(&placement, f, g);

    // Guide folder: click + spoken guide.
    assert!(ing("Holy Forever - Click.wav", "Guide"));
    assert!(ing("Holy Forever - Guide.wav", "Guide"));
    // Tracks: the loop.
    assert!(ing("Holy Forever - Loop.wav", "Tracks"));
    // Bass: electric + synth.
    assert!(ing("Holy Forever - Bass.wav", "Bass"));
    assert!(ing("Holy Forever - Synth Bass.wav", "Bass"));
    // Guitars: acoustic + both electrics.
    for f in [
        "Holy Forever - AG.wav",
        "Holy Forever - EG 1.wav",
        "Holy Forever - EG 2.wav",
    ] {
        assert!(ing(f, "Guitars"), "{f} should be under Guitars");
    }
    // Keys: three keys layers + piano.
    for f in [
        "Holy Forever - Keys 1.wav",
        "Holy Forever - Keys 2.wav",
        "Holy Forever - Keys 3.wav",
        "Holy Forever - Piano.wav",
    ] {
        assert!(ing(f, "Keys"), "{f} should be under Keys");
    }
    // Drums.
    assert!(ing("Holy Forever - Drums.wav", "Drums"));
}
