//! `SIGNAL_BLOCK_SET`: a block's parameters set by their ids, in their own
//! units, whether or not a host slot shows them (an EQ's bands past the
//! sixth, any block past the slots' end).
//!
//! The values go into the state (what a chain read, a save and a render's
//! copy see) and into the running graph as parameter changes — no rebuild,
//! so nothing playing is cut. A parameter a slot shows moves its slot too:
//! the slots are written back into the state on every read, and the host
//! reads its value there. A bypass changes the graph's shape, and rebuilds.

use std::sync::atomic::Ordering;

use daw_clap_ext::ExtError;
use nice_plug::prelude::Param as _;
use serde_json::Value;
use signal_core::param::{ParamSpec, Unit};

use crate::ext::{chain, find_block};
use crate::plugin::{FtsSignal, Shared};

/// One parameter to set: where it is in the running graph (lane, block,
/// parameter), its id, its plain value.
struct Change {
    param: usize,
    id: &'static str,
    value: f32,
}

/// Run `SIGNAL_BLOCK_SET` (`args`: `block`, `lane`?, `params`?, `bypass`?):
/// the new chain.
pub fn run(shared: &Shared, args: &Value, on_main: bool) -> Result<String, ExtError> {
    let block = args
        .get("block")
        .and_then(Value::as_str)
        .ok_or_else(|| ExtError::Failed("SIGNAL_BLOCK_SET wants a block".into()))?;
    let lane = args.get("lane").and_then(Value::as_str);
    let params = match args.get("params") {
        None | Some(Value::Null) => serde_json::Map::new(),
        Some(Value::Object(m)) => m.clone(),
        Some(_) => {
            return Err(ExtError::Failed(
                "`params` is an object: {\"<param id>\": <value>}".into(),
            ));
        }
    };
    let bypass = args.get("bypass").and_then(Value::as_bool);
    if params.is_empty() && bypass.is_none() {
        return Err(ExtError::Failed(
            "SIGNAL_BLOCK_SET wants `params`, `bypass` or both".into(),
        ));
    }

    let mut state = shared.state();
    let (li, bi) = find_block(&state, block, lane)?;
    let lane_name = state.preset.lanes[li].name.clone();
    let table = shared.params.shown.get();
    // The running graph's lane (the lanes this instance runs, in order)
    // and the block's parameters.
    let graph_lane = table.lanes.iter().position(|n| *n == lane_name);
    let specs = table
        .blocks
        .iter()
        .find(|b| Some(b.lane) == graph_lane && b.block == bi)
        .map(|b| b.specs)
        .ok_or_else(|| ExtError::NotFound(format!("block {block} is not running")))?;
    let changes = changes(specs, &params)?;

    let spec = &mut state.preset.lanes[li].blocks[bi];
    for c in &changes {
        spec.params.insert(c.id.to_owned(), c.value);
    }
    let rebuild = bypass.is_some_and(|b| b != spec.bypass);
    if let Some(b) = bypass {
        spec.bypass = b;
    }
    let text = String::from_utf8(state.to_blob()).map_err(|e| ExtError::Failed(e.to_string()))?;
    if rebuild {
        shared.load_state_text(text, on_main);
    } else {
        apply(shared, &table, graph_lane, bi, &changes, text);
    }
    Ok(chain(shared).to_string())
}

/// Each of `params` against the block's `specs`: its id and plain value.
fn changes(
    specs: &'static [ParamSpec],
    params: &serde_json::Map<String, Value>,
) -> Result<Vec<Change>, ExtError> {
    params
        .iter()
        .map(|(id, raw)| {
            let (param, spec) = specs
                .iter()
                .enumerate()
                .find(|(_, s)| s.id == id)
                .ok_or_else(|| {
                    let ids: Vec<&str> = specs.iter().map(|s| s.id).take(24).collect();
                    ExtError::NotFound(format!(
                        "parameter {id} (this block has {}{})",
                        ids.join(", "),
                        if specs.len() > ids.len() { ", …" } else { "" }
                    ))
                })?;
            let value = value_of(spec, raw)
                .ok_or_else(|| ExtError::Failed(format!("{id}: {raw} is not a value of it")))?;
            Ok(Change {
                param,
                id: spec.id,
                value,
            })
        })
        .collect()
}

/// A value in a parameter's own units: a number (Hz, dB, ms, a choice's
/// index, 0..1 for a percentage), a boolean for a switch, or its text as
/// the block writes it (`450 Hz`, `-6 dB`, `Bell`, `50%`, `On`). Clamped
/// to its range.
fn value_of(spec: &ParamSpec, raw: &Value) -> Option<f32> {
    let value = match raw {
        Value::Number(n) => n.as_f64()? as f32,
        Value::Bool(b) => f32::from(u8::from(*b)),
        Value::String(s) => return spec.parse(s),
        _ => return None,
    };
    if !value.is_finite() {
        return None;
    }
    let value = match spec.unit {
        Unit::Choice(_) | Unit::Toggle => value.round(),
        _ => value,
    };
    Some(spec.clamp(value))
}

/// Put `text` in as the running state (no rebuild) and hand `changes` to
/// what runs: the slot showing each (moved, so the next write-back keeps
/// it) and the graph's block, as parameter changes.
fn apply(
    shared: &Shared,
    table: &crate::slots::Table,
    graph_lane: Option<usize>,
    block: usize,
    changes: &[Change],
    text: String,
) {
    for c in changes {
        let slot = table
            .slots
            .iter()
            .position(|s| Some(s.lane) == graph_lane && s.block == block && s.param == c.param);
        if let Some((def, p)) =
            slot.and_then(|i| Some((table.slots.get(i)?, shared.params.slots.get(i)?)))
        {
            // SAFETY: the slot lives as long as `shared.params`.
            unsafe {
                p.as_ptr()
                    ._internal_set_normalized_value(def.spec.normalize(c.value))
            };
        }
        if let Some(lane) = graph_lane {
            shared.send_param(lane, block, c.param, c.value);
        }
    }
    shared.params.state.store_running(text);
    shared.params.values_revision.fetch_add(1, Ordering::AcqRel);
    // A chain read while the revision holds is the host's cached one.
    shared.ext.revision.fetch_add(1, Ordering::AcqRel);
    let plugin = shared.ext.plugin.load(Ordering::Acquire);
    if !plugin.is_null() {
        // SAFETY: the instance's own pointer, alive as long as it is.
        unsafe { nice_plug::wrapper::clap::clap_request_rescan::<FtsSignal>(plugin, false, true) };
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use signal_core::state::{BlockSpec, Phase, Preset, SignalState};

    use super::*;

    /// A one-lane instance with a Tone EQ and a Tone comp, built.
    fn instance() -> std::sync::Arc<Shared> {
        let mut preset = Preset::passthrough();
        preset.lanes[0].blocks = vec![
            BlockSpec::signal("eq", "eq").phase(Phase::Tone),
            BlockSpec::signal("comp", "comp").phase(Phase::Tone),
        ];
        let plugin = FtsSignal::default();
        let shared = plugin.shared();
        let text = String::from_utf8(SignalState::new(preset).to_blob()).expect("utf-8");
        shared.load_state_text(text, false);
        shared
    }

    fn chain_param(chain: &Value, block: &str, id: &str) -> Option<f64> {
        chain["phases"]
            .as_array()?
            .iter()
            .flat_map(|p| p["blocks"].as_array().cloned().unwrap_or_default())
            .find(|b| b["id"] == block)?["params"]
            .as_array()?
            .iter()
            .find(|p| p["id"] == id)?["value"]
            .as_f64()
    }

    fn state_param(shared: &Shared, block: &str, id: &str) -> Option<f32> {
        let state = shared.state();
        state.preset.lanes[0]
            .blocks
            .iter()
            .find(|b| b.id == block)?
            .params
            .get(id)
            .copied()
    }

    #[test]
    fn a_block_set_reaches_params_past_the_slots_and_the_comps_makeup() {
        let shared = instance();
        let answer = run(
            &shared,
            &json!({ "block": "tone.eq", "params": {
                "band.7.used": true, "band.7.freq": 450, "band.7.gain": "-6 dB", "band.7.shape": "Bell",
            }}),
            false,
        )
        .expect("the EQ set");
        let chain: Value = serde_json::from_str(&answer).expect("a chain");
        assert_eq!(chain_param(&chain, "tone.eq", "band.7.freq"), Some(450.0));
        assert_eq!(chain_param(&chain, "tone.eq", "band.7.gain"), Some(-6.0));
        // Read again (the slots are written back on every read): it holds.
        assert_eq!(state_param(&shared, "eq", "band.7.freq"), Some(450.0));
        assert_eq!(state_param(&shared, "eq", "band.7.used"), Some(1.0));

        // A comp's own makeup — a slot: the write-back keeps the new value.
        let answer = run(
            &shared,
            &json!({ "block": "comp", "params": { "makeup": 4.5 } }),
            false,
        )
        .expect("the comp set");
        let chain: Value = serde_json::from_str(&answer).expect("a chain");
        assert_eq!(chain_param(&chain, "tone.comp", "makeup"), Some(4.5));
        assert_eq!(state_param(&shared, "comp", "makeup"), Some(4.5));
        let saved = SignalState::from_blob(shared.state().to_blob().as_slice()).expect("state");
        assert_eq!(
            saved.preset.lanes[0].blocks[1].params.get("makeup"),
            Some(&4.5)
        );

        // Out of range clamps; an unknown id says what the block has.
        run(
            &shared,
            &json!({ "block": "comp", "params": { "makeup": 99 } }),
            false,
        )
        .unwrap();
        assert_eq!(state_param(&shared, "comp", "makeup"), Some(24.0));
        let err = run(
            &shared,
            &json!({ "block": "comp", "params": { "nope": 1 } }),
            false,
        )
        .unwrap_err();
        assert!(format!("{err:?}").contains("threshold"), "{err:?}");

        // A bypass rebuilds, and keeps the values.
        let answer = run(
            &shared,
            &json!({ "block": "tone.comp", "bypass": true }),
            false,
        )
        .unwrap();
        let chain: Value = serde_json::from_str(&answer).unwrap();
        let comp = chain["phases"][0]["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["id"] == "tone.comp")
            .cloned()
            .unwrap();
        assert_eq!(comp["bypass"], true);
        assert_eq!(chain_param(&chain, "tone.comp", "makeup"), Some(24.0));
        assert_eq!(chain_param(&chain, "tone.eq", "band.7.freq"), Some(450.0));
    }
}
