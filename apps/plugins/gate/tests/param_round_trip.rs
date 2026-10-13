//! Every parameter's displayed value can be typed back in — choices by name.
//!
//! A host's generic parameter list is text in, text out (this plugin has no
//! editor yet, so that list is the whole UI): a parameter that formats but
//! does not parse is read-only in every DAW's panel and automation entry.

use gate_plugin::GateParams;
use nice_plug::prelude::Params;

/// Normalized points to probe, ends included.
const PROBES: [f32; 7] = [0.0, 0.004, 0.25, 0.5, 0.75, 0.996, 1.0];

#[test]
fn every_parameter_parses_the_string_it_prints() {
    let params = GateParams::default();
    let mut broken: Vec<String> = Vec::new();
    for (id, ptr, _group) in params.param_map() {
        // SAFETY: `params` outlives this loop, and `param_map` hands back
        // pointers into it.
        let name = unsafe { ptr.name() }.to_string();
        for probe in PROBES {
            let shown = unsafe { ptr.normalized_value_to_string(probe, true) };
            let Some(parsed) = (unsafe { ptr.string_to_normalized_value(&shown) }) else {
                broken.push(format!("{id} ({name}): {shown:?} does not parse"));
                break;
            };
            let again = unsafe { ptr.normalized_value_to_string(parsed, true) };
            if again != shown {
                broken.push(format!("{id} ({name}): {probe} printed {shown:?}, read back as {again:?}"));
                break;
            }
        }
    }
    assert!(broken.is_empty(), "parameters that cannot read their own display:\n  {}", broken.join("\n  "));
}

#[test]
fn choices_parse_by_name() {
    let params = GateParams::default();
    let map = params.param_map();
    let find = |want: &str| map.iter().find(|(id, _, _)| id == want).map(|(_, p, _)| *p).expect(want);
    for (id, text, norm) in [("style", "Drum", 1.0), ("mode", "Snare Top", 1.0 / 3.0), ("mode", "toms", 1.0)] {
        let got = unsafe { find(id).string_to_normalized_value(text) }.expect(text);
        assert!((got - norm).abs() < 1e-6, "{id} {text:?} -> {got}");
    }
}
