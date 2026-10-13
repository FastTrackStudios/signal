//! Every knob on the saturator's face prints its value.
//!
//! The face silkscreened only the control's NAME, so the panel told you a knob
//! was called DRIVE and not what it was set to. The panel is the only place
//! these parameters surface, so there was nowhere else to look the value up.

#![cfg(feature = "native")]

use std::sync::Arc;

#[path = "support/mod.rs"]
mod support;

use saturate_ui::params::SatParams;
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

/// Every knob the default (Triode) face draws.
///
/// `character-a` is relabelled per profile (Heat here), so the param id and
/// the silkscreen legend deliberately differ.
const TUBE_PARAMS: [&str; 7] = [
    "drive",
    "bias",
    "sag",
    "character-a",
    "tilt",
    "mix",
    "output",
];

#[tokio::test]
async fn the_face_prints_a_value_beside_every_knob_it_draws() -> dioxus_test::Result<()> {
    let fx = mount_with(Arc::new(SatParams::default()), 1280, 440);
    let _ = fx.tester.pump().await;
    fx.tester.relayout();

    for param in TUBE_PARAMS {
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
    let fx = mount_with(Arc::new(SatParams::default()), 1280, 440);
    let _ = fx.tester.pump().await;
    fx.tester.relayout();

    let values: Vec<String> = TUBE_PARAMS
        .iter()
        .filter_map(|p| readout(&fx, p))
        .collect();
    assert_eq!(values.len(), TUBE_PARAMS.len(), "a readout went missing");

    let distinct: std::collections::BTreeSet<&String> = values.iter().collect();
    assert!(
        distinct.len() > 1,
        "every knob is showing the same value {values:?} — the readouts are \
         not reading their own parameters"
    );
    Ok(())
}
