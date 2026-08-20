//! Hardware controls that drive the engine.
//!
//! A faceplate control is not a plugin parameter. PEAK REDUCTION writes five
//! engine params on linked curves; the 1176's INPUT writes three; the LA-2A's
//! GAIN writes one, but only over the slice of its range the unit can reach.
//! `comp-profiles` already describes all of that as data
//! ([`comp_profiles::ParamMapping`]); what was missing was a
//! [`ParamHandle`] that a widget can turn.
//!
//! [`profile_control_handle`] builds one per [`ProfileControl`], so every
//! control on a face — one-to-one, stepped or compound — is driven the same
//! way and the widgets stay ordinary `fts_audio_ui` widgets.
//!
//! # Reading the position back
//!
//! Direct and Stepped controls recover their position by inverting the mapping
//! against the target param's current value. A Compound control cannot: its
//! position is spread across several params on curves that are not jointly
//! invertible. It is stored instead, in one of [`CompParams`]'s macro slots —
//! which also makes the knob the user turns the thing the host automates and
//! the session restores. See [`CompParams::macro1`].

use std::sync::Arc;

use audiocore_core::prelude::Param;
use comp_profiles::{map_control_value, ParamMapping, Profile, ProfileControl};
use fts_audio_ui::ParamHandle;
use nice_plug::prelude::ParamPtr;
use nice_plug_dioxus::prelude::ParamContext;

use crate::param_map::core_param_ptr;
use crate::params::CompParams;

/// Build a [`ParamHandle`] for one control of a profile's front panel.
///
/// `macro_slot` is the position store for a [`ParamMapping::Compound`] control
/// (see the module docs) and is ignored by the other two kinds. Returns `None`
/// when the control writes nothing this plugin exposes, or when a compound
/// control has no slot to remember its position in.
pub fn profile_control_handle(
    profile: &'static (dyn Profile + Sync),
    control: &'static ProfileControl,
    params: Arc<CompParams>,
    stage: usize,
    ctx: ParamContext,
    macro_slot: Option<ParamPtr>,
) -> Option<ParamHandle> {
    // Every param this control can write, resolved once — the fanout runs on
    // every pointer move, and the widgets need the same list to bracket the
    // gesture for the host.
    let targets: Vec<ParamPtr> = target_params(control)
        .into_iter()
        .filter_map(|name| core_param_ptr(params.stage(stage), name))
        .collect();
    if targets.is_empty() {
        return None;
    }

    let control_id = control.id;
    let label = control.label;

    // Position readback. Compound reads its slot; the rest invert their
    // mapping against the live param value, so a face opened on an existing
    // session shows where the engine actually is.
    let read: Arc<dyn Fn() -> f32 + Send + Sync> = match &control.mapping {
        ParamMapping::Compound { .. } => {
            let slot = macro_slot?;
            Arc::new(move || unsafe { slot.modulated_normalized_value() })
        }
        ParamMapping::Direct { range, .. } => {
            let ptr = targets[0];
            let (lo, hi) = (*range.start() as f32, *range.end() as f32);
            Arc::new(move || {
                let plain = unsafe { ptr.modulated_plain_value() };
                if (hi - lo).abs() < f32::EPSILON {
                    0.0
                } else {
                    ((plain - lo) / (hi - lo)).clamp(0.0, 1.0)
                }
            })
        }
        ParamMapping::Stepped { values, .. } => {
            let ptr = targets[0];
            let values = *values;
            Arc::new(move || {
                let plain = unsafe { ptr.modulated_plain_value() } as f64;
                nearest_step(values, plain)
            })
        }
    };

    // Display. A stepped control reads its own label ("Limit", "8"); the
    // others defer to the engine param they front, which already knows its
    // unit and formatting.
    let display: Arc<dyn Fn() -> String + Send + Sync> = match &control.mapping {
        ParamMapping::Stepped { labels, .. } => {
            let ptr = targets[0];
            let labels = *labels;
            let values = match &control.mapping {
                ParamMapping::Stepped { values, .. } => *values,
                _ => unreachable!(),
            };
            Arc::new(move || {
                let plain = unsafe { ptr.modulated_plain_value() } as f64;
                let index = step_index(values, plain);
                labels.get(index).map(|s| s.to_string()).unwrap_or_default()
            })
        }
        _ => {
            let ptr = targets[0];
            Arc::new(move || unsafe {
                let n = ptr.modulated_normalized_value();
                ptr.normalized_value_to_string(n, true)
            })
        }
    };

    let write = {
        let params = params.clone();
        let ctx = ctx.clone();
        move |normalized: f32| {
            if let Some(slot) = macro_slot {
                if matches!(control.mapping, ParamMapping::Compound { .. }) {
                    ctx.set_normalized_raw(slot, normalized.clamp(0.0, 1.0));
                }
            }
            for (name, plain) in
                map_control_value(profile, control_id, normalized as f64).unwrap_or_default()
            {
                let Some(ptr) = core_param_ptr(params.stage(stage), name) else {
                    continue;
                };
                let n = unsafe { ptr.preview_normalized(plain as f32) };
                ctx.set_normalized_raw(ptr, n);
            }
        }
    };

    // Gesture bracketing covers every param the control moves — a host
    // recording automation has to see one gesture per param, not one for the
    // knob.
    let gesture_ptrs: Vec<ParamPtr> = targets
        .iter()
        .copied()
        .chain(
            macro_slot
                .filter(|_| matches!(control.mapping, ParamMapping::Compound { .. })),
        )
        .collect();

    let begin = {
        let ctx = ctx.clone();
        let ptrs = gesture_ptrs.clone();
        move || {
            for ptr in &ptrs {
                ctx.begin_set_raw(*ptr);
            }
        }
    };
    let end = {
        let ctx = ctx.clone();
        let ptrs = gesture_ptrs;
        move || {
            for ptr in &ptrs {
                ctx.end_set_raw(*ptr);
            }
        }
    };

    let read_for_handle = read.clone();
    let default_normalized = default_position(control);

    Some(
        ParamHandle::new(
            move || read_for_handle(),
            begin,
            write,
            end,
            move || display(),
            move || label.to_string(),
            // Typed entry would have to run backwards through the mapping;
            // hardware panels have no text fields anyway.
            |_| None,
        )
        .with_default(default_normalized),
    )
}

/// The core params a control can write, in mapping order.
fn target_params(control: &'static ProfileControl) -> Vec<&'static str> {
    match &control.mapping {
        ParamMapping::Direct { param, .. } | ParamMapping::Stepped { param, .. } => vec![*param],
        ParamMapping::Compound { mappings, .. } => {
            mappings.iter().map(|(param, _)| *param).collect()
        }
    }
}

/// Where a control rests when nothing has moved it — the reset target for
/// alt-click. Stepped controls rest on their first detent, everything else at
/// mid-travel.
fn default_position(control: &'static ProfileControl) -> f32 {
    match &control.mapping {
        ParamMapping::Stepped { .. } => 0.0,
        _ => 0.5,
    }
}

/// Index of the detent nearest `plain`.
fn step_index(values: &[f64], plain: f64) -> usize {
    values
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (*a - plain)
                .abs()
                .partial_cmp(&(*b - plain).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Normalized position of the detent nearest `plain`.
fn nearest_step(values: &[f64], plain: f64) -> f32 {
    if values.len() < 2 {
        return 0.0;
    }
    step_index(values, plain) as f32 / (values.len() - 1) as f32
}

/// Compound controls of a profile, paired with the macro slot index that
/// stores each one's position.
pub fn macro_slot_index(profile: &'static (dyn Profile + Sync), control_id: &str) -> Option<usize> {
    profile
        .controls()
        .iter()
        .filter(|c| matches!(c.mapping, ParamMapping::Compound { .. }))
        .position(|c| c.id == control_id)
}

/// Look a control up on a profile by id.
pub fn profile_control(
    profile: &'static (dyn Profile + Sync),
    control_id: &str,
) -> Option<&'static ProfileControl> {
    profile.controls().iter().find(|c| c.id == control_id)
}

/// Build the handle for a control named by id — the form the faces use.
/// `stage` is the stack stage this face is editing (`fx.stack.focus`).
pub fn handle_for(
    profile: &'static (dyn Profile + Sync),
    control_id: &str,
    params: Arc<CompParams>,
    stage: usize,
    ctx: ParamContext,
) -> Option<ParamHandle> {
    let control = profile_control(profile, control_id)?;
    let slot = macro_slot_index(profile, control_id)
        .and_then(|i| params.stage(stage).macro_slot(i))
        .map(|p| p.as_ptr());
    profile_control_handle(profile, control, params, stage, ctx, slot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use comp_profiles::{LA2A, UREI_1176};

    #[test]
    fn the_first_compound_control_takes_the_first_macro_slot() {
        assert_eq!(macro_slot_index(&LA2A, "peak_reduction"), Some(0));
        assert_eq!(macro_slot_index(&UREI_1176, "input"), Some(0));
        // Non-compound controls never claim a slot.
        assert_eq!(macro_slot_index(&LA2A, "gain"), None);
        assert_eq!(macro_slot_index(&UREI_1176, "ratio"), None);
    }

    #[test]
    fn stepped_positions_snap_to_the_nearest_detent() {
        let values = &[4.0, 8.0, 12.0, 20.0, 32.0];
        assert_eq!(step_index(values, 4.0), 0);
        assert_eq!(step_index(values, 11.0), 2);
        assert_eq!(step_index(values, 99.0), 4);
        assert_eq!(nearest_step(values, 4.0), 0.0);
        assert_eq!(nearest_step(values, 32.0), 1.0);
    }
}
