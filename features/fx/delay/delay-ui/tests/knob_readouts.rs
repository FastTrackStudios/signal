//! Every knob on the delay's face prints its value.
//!
//! The face silkscreened only the control's NAME, so the panel told you a knob
//! was called TIME L and not what it was set to. For a delay that is close to
//! unusable — a time with no milliseconds on it cannot be dialled to a tempo,
//! only nudged by ear — and the panel is the only place these parameters
//! surface, so there was nowhere else to look the value up.

#![cfg(feature = "native")]

use std::sync::Arc;

#[path = "support/mod.rs"]
mod support;

use delay_ui::params::DelayParams;
use support::mount_with;

/// The text a knob's readout is showing, with the wrapper markup stripped.
fn readout(fx: &support::Fixture, param: &str) -> Option<String> {
    let html = fx
        .tester
        .query_all(dioxus_test::by_testid(format!("knob-value-{param}")))
        .immediately()
        .first()
        .map(dioxus_test::ResolvedElement::inner_html)?;
    // The readout is a Silkscreen div inside the tagged wrapper; take its text.
    let mut text = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    Some(text.trim().to_string())
}

/// Every knob the default (digital) face draws.
///
/// `character-a` / `character-b` are relabelled per profile (Width / Sync
/// here), so the param id and the silkscreen legend deliberately differ.
const DIGITAL_PARAMS: [&str; 8] = [
    "time-l",
    "time-r",
    "feedback",
    "tone",
    "character-a",
    "character-b",
    "duck",
    "mix",
];

#[tokio::test]
async fn the_face_prints_a_value_beside_every_knob_it_draws() -> dioxus_test::Result<()> {
    let fx = mount_with(Arc::new(DelayParams::default()), 1280, 440);
    let _ = fx.tester.pump().await;
    fx.tester.relayout();

    for param in DIGITAL_PARAMS {
        let text = readout(&fx, param)
            .unwrap_or_else(|| panic!("no readout element for {param}"));
        assert!(
            text.chars().any(|c| c.is_ascii_digit()),
            "the {param} readout should print a number, got {text:?}"
        );
    }
    Ok(())
}

/// Each knob shows its OWN parameter.
///
/// The presence test above would still pass if every readout were wired to the
/// same handle, or to a literal — which is the realistic copy-paste failure for
/// eight near-identical blocks. A face whose knobs all read the same number is
/// not more useful than one that reads none.
#[tokio::test]
async fn the_readouts_are_not_all_the_same_value() -> dioxus_test::Result<()> {
    let fx = mount_with(Arc::new(DelayParams::default()), 1280, 440);
    let _ = fx.tester.pump().await;
    fx.tester.relayout();

    let values: Vec<String> = DIGITAL_PARAMS
        .iter()
        .filter_map(|p| readout(&fx, p))
        .collect();
    assert_eq!(values.len(), DIGITAL_PARAMS.len(), "a readout went missing");

    let distinct: std::collections::BTreeSet<&String> = values.iter().collect();
    assert!(
        distinct.len() > 1,
        "every knob is showing the same value {values:?} — the readouts are \
         not reading their own parameters"
    );
    Ok(())
}
