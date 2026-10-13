//! Every knob on a reverb face prints its value.
//!
//! The face silkscreened only the control's NAME, so the panel told you a knob
//! was called Decay and not that it was set to 2.4 s. A reverb is dialled
//! against a tempo and a room — a decay you can only nudge by ear is close to
//! unusable — and this face is the only place these parameters surface, so
//! there was nowhere else to look the number up. Same gap the saturator and
//! delay faces had; same fix.

#![cfg(feature = "native")]

use std::sync::Arc;

#[path = "support/mod.rs"]
mod support;

use reverb_ui::params::ReverbParams;
use support::{Fixture, mount_with};

/// Open the editor already on `profile_id`, the way a host restoring a session
/// would — the persisted id is what the face resolves its design from.
async fn on_profile(profile_id: &str) -> Fixture {
    let params = Arc::new(ReverbParams::default());
    params.store_profile_id(reverb_profiles::profile_index(profile_id).unwrap());
    let mut fx = mount_with(
        params,
        reverb_ui::control_view::EDITOR_W,
        reverb_ui::control_view::EDITOR_H,
    );
    fx.settle().await;
    fx
}

/// The text a knob's readout is showing, with the wrapper markup stripped.
fn readout(fx: &Fixture, param: &str) -> Option<String> {
    let testid = format!("knob-value-{}", param.replace('_', "-"));
    let html = fx
        .tester
        .query_all(dioxus_test::by_testid(testid))
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

/// Every parameter the face for `profile_id` actually draws, extras included.
///
/// Taken from the face's own specs rather than a hand-copied list, so a knob
/// added to a design is covered the day it appears instead of the day someone
/// remembers to update this file.
fn params_on_face(profile_id: &str) -> Vec<&'static str> {
    reverb_ui::faces::design_for(profile_id)
        .knobs
        .iter()
        .chain(reverb_ui::faces::extras_for(profile_id).iter())
        .map(|k| k.param)
        .collect()
}

/// One profile per design family, plus the ones carrying an extra knob — the
/// extras sit on the same readout row and are the easiest thing to forget.
const PROFILES: [&str; 6] = [
    "hall_concert",
    "plate_classic",
    "room_medium",
    "spring_classic",
    "shimmer",
    "ir",
];

#[tokio::test]
async fn every_face_prints_a_value_beside_every_knob_it_draws() -> dioxus_test::Result<()> {
    for profile_id in PROFILES {
        let fx = on_profile(profile_id).await;
        let params = params_on_face(profile_id);
        assert!(!params.is_empty(), "{profile_id} draws no knobs at all");

        for param in params {
            let text = readout(&fx, param)
                .unwrap_or_else(|| panic!("no readout element for {profile_id}/{param}"));
            assert!(
                text.chars().any(|c| c.is_ascii_digit()),
                "the {profile_id}/{param} readout should print a number, got {text:?}"
            );
        }
    }
    Ok(())
}

/// Each knob shows its OWN parameter.
///
/// The presence test above would still pass if every readout were wired to the
/// same handle, or to a literal — the realistic copy-paste failure for a row of
/// near-identical blocks. A face whose knobs all read the same number is no
/// more useful than one that reads none.
#[tokio::test]
async fn the_readouts_are_not_all_the_same_value() -> dioxus_test::Result<()> {
    let profile_id = "hall_concert";
    let fx = on_profile(profile_id).await;

    let params = params_on_face(profile_id);
    let values: Vec<String> = params.iter().filter_map(|p| readout(&fx, p)).collect();
    assert_eq!(values.len(), params.len(), "a readout went missing");

    let distinct: std::collections::BTreeSet<&String> = values.iter().collect();
    assert!(
        distinct.len() > 1,
        "every knob is showing the same value {values:?} — the readouts are \
         not reading their own parameters"
    );
    Ok(())
}

/// A readout carries its UNIT, not just a number.
///
/// "50" on a Decay knob says nothing — 50 what? Every percentage parameter
/// formatted with `v2s_f32_percentage`, which renders 0.5 as "50" and stops
/// there; the "%" only appears if the parameter also declares
/// `.with_unit("%")`, and none of them did.
#[tokio::test]
async fn the_readouts_carry_their_units() -> dioxus_test::Result<()> {
    let profile_id = "hall_concert";
    let fx = on_profile(profile_id).await;

    // Decay, Size, Damping, Tone and Mix are all percentages on this face;
    // Pre-Delay is the milliseconds one, and already had its unit.
    for (param, unit) in [
        ("decay", "%"),
        ("size", "%"),
        ("damping", "%"),
        ("tone", "%"),
        ("mix", "%"),
        ("predelay", "ms"),
    ] {
        let text = readout(&fx, param).unwrap_or_else(|| panic!("no readout element for {param}"));
        assert!(
            text.to_ascii_lowercase().contains(unit),
            "the {param} readout should carry its unit ({unit}), got {text:?}"
        );
    }
    Ok(())
}
