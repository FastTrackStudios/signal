//! Every parameter of FTS Mic parses the string it prints.
//!
//! A host's generic parameter list is text in, text out; a parameter that
//! formats but does not parse is read-only there. The choices are held to
//! the same line as the continuous controls: a mic model or a pattern step
//! has to read its own name back ("LD-87 Vintage", "Sub-Card"), since that
//! is what a host shows and what scripts set.

use mic_ui::params::MicParams;
use nice_plug::prelude::Params;

/// Normalized points to probe; the ends matter as much as the middle.
const PROBES: [f32; 7] = [0.0, 0.004, 0.25, 0.5, 0.75, 0.996, 1.0];

fn broken(params: &dyn Params) -> Vec<String> {
    let mut out = Vec::new();
    for (id, ptr, _group) in params.param_map() {
        // SAFETY: `params` outlives this loop, and `param_map` hands back
        // pointers into it.
        let name = unsafe { ptr.name() }.to_string();
        // A stepped parameter is checked at every step, a continuous one at
        // the probes.
        let points: Vec<f32> = unsafe { ptr.step_count() }.map_or_else(
            || PROBES.to_vec(),
            |steps| {
                let n = u16::try_from(steps).unwrap_or(u16::MAX).max(1);
                (0..=n).map(|k| f32::from(k) / f32::from(n)).collect()
            },
        );
        for at in points {
            let shown = unsafe { ptr.normalized_value_to_string(at, true) };
            let Some(parsed) = (unsafe { ptr.string_to_normalized_value(&shown) }) else {
                out.push(format!("{id} ({name}): {shown:?} does not parse"));
                break;
            };
            let again = unsafe { ptr.normalized_value_to_string(parsed, true) };
            if again != shown {
                out.push(format!("{id} ({name}): {at} printed {shown:?}, which parsed back as {again:?}"));
                break;
            }
        }
    }
    out
}

#[test]
fn every_fts_mic_parameter_parses_the_string_it_prints() {
    let bad = broken(&MicParams::default());
    assert!(bad.is_empty(), "FTS Mic:\n  {}", bad.join("\n  "));
}
