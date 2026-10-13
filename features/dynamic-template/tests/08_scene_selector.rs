//! The scene `Selector` is the one vocabulary a patch-list entry and a
//! scene rule both speak (spec #48, "Scenes as data" and "The patch
//! list"): a selector round-trips through styx with every taxonomy
//! field, and an absent field stays absent rather than becoming an
//! empty constraint.

use dynamic_template::scenes::{Language, Rank, Role, Selector};

#[test]
fn selector_round_trips_through_styx() {
    let full = Selector {
        group: vec!["Drum Kit".into(), "Kick".into()],
        kind: Some("drums".into()),
        role: Role::Leaf,
        rank: Rank::TopmostPerInstrument,
        performer: Some("drummer".into()),
        layer: Some("Main".into()),
        channel: Some("L".into()),
        multi_mic: Some("In".into()),
        arrangement: Some("Rhythm".into()),
        language: Some(Language::En),
        name: None,
        depth: Some(1),
    };
    // `None` is written as the unit `@`, which the parser does not read
    // back into an `Option` — so an absent field is omitted, the way a
    // hand-written selector leaves it out.
    let text = facet_styx::to_string_with_options(
        &full,
        &facet_styx::SerializeOptions::default().omit_none(),
    )
    .expect("serialize");
    let back: Selector = facet_styx::from_str(&text).expect("parse back");
    assert_eq!(back, full, "styx text was:\n{text}");
}

#[test]
fn an_empty_selector_is_the_default() {
    let back: Selector = facet_styx::from_str("").expect("an empty document");
    assert_eq!(back, Selector::default());
    assert_eq!(back.role, Role::Any);
    assert_eq!(back.rank, Rank::All);
}

#[test]
fn a_selector_reads_from_authored_styx() {
    let text = "performer cody\nkind guitar\nmulti-mic \"Amp A 57\"\nrole @leaf";
    let back: Selector = facet_styx::from_str(text).expect("parse");
    assert_eq!(back.performer.as_deref(), Some("cody"));
    assert_eq!(back.kind.as_deref(), Some("guitar"));
    assert_eq!(back.multi_mic.as_deref(), Some("Amp A 57"));
    assert_eq!(back.role, Role::Leaf);
    assert!(back.group.is_empty());
}
