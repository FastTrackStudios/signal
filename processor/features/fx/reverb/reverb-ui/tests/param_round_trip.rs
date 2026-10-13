//! Every parameter's displayed value can be typed back in.
//!
//! A host's generic parameter list is text in, text out: it shows whatever
//! `normalized_value_to_string` returns and hands whatever the user types to
//! `string_to_normalized_value`. A parameter that formats but does not parse
//! is read-only in every DAW's parameter panel and in automation-lane entry.
//!
//! This caught a real one. Percentage parameters used `v2s_f32_percentage`,
//! which renders 0.5 as "50" — and nothing else. No `.with_unit("%")`, so the
//! panel read a bare "50" with no clue what kind of 50 it was, and no
//! `.with_string_to_value`, so typing "50" back did nothing at all. Forty-one
//! parameters across the four FX were like this.

#![cfg(feature = "native")]

use nice_plug::prelude::Params;

/// Normalized points to probe. The ends matter as much as the middle: a
/// formatter that rounds to zero digits turns 0.004 into "0", and a parser
/// that cannot read its own "0" fails only there.
const PROBES: [f32; 7] = [0.0, 0.004, 0.25, 0.5, 0.75, 0.996, 1.0];

#[test]
fn every_parameter_parses_the_string_it_prints() {
    let params = reverb_ui::params::ReverbParams::default();
    let mut broken: Vec<String> = Vec::new();

    for (id, ptr, _group) in params.param_map() {
        // SAFETY: `params` outlives this loop, and `param_map` hands back
        // pointers into it.
        let name = unsafe { ptr.name() }.to_string();

        // Stepped parameters (choices, band shapes, profile pickers) are
        // rendered as a list by every host, not as a text field, and their
        // display is a LABEL — "Bell", "Impulse Response". Asking those to
        // parse a number back would be asking for the wrong thing. All that
        // is required of them here is that they print something.
        if unsafe { ptr.step_count() }.is_some() {
            let shown = unsafe { ptr.normalized_value_to_string(0.5, true) };
            if shown.trim().is_empty() {
                broken.push(format!("{id} ({name}): stepped parameter prints nothing"));
            }
            continue;
        }

        for probe in PROBES {
            // The host shows the value WITH its unit, so that is the string
            // the user edits and the one that has to parse.
            let shown = unsafe { ptr.normalized_value_to_string(probe, true) };
            let Some(parsed) = (unsafe { ptr.string_to_normalized_value(&shown) }) else {
                broken.push(format!("{id} ({name}): {shown:?} does not parse"));
                break;
            };
            // Compare what the user SEES, not the raw normalized value.
            // A display rounds — "1.1:1" on a skewed ratio is a couple of
            // percent off the value behind it — and that loss is fine as
            // long as it is stable. What is not fine is a string that means
            // something else when typed back, which is what this catches.
            let again = unsafe { ptr.normalized_value_to_string(parsed, true) };
            if again != shown {
                broken.push(format!(
                    "{id} ({name}): {probe} printed {shown:?}, which parsed back \
                     and printed as {again:?}"
                ));
                break;
            }
        }
    }

    assert!(
        broken.is_empty(),
        "Reverb parameters that cannot read their own display:\n  {}",
        broken.join("\n  ")
    );
}
