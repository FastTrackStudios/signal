//! Adapter from nice_plug's `ParamPtr` + `ParamContext` to
//! [`fts_audio_ui::ParamHandle`].
//!
//! Lives here rather than in `fts-audio-ui` because it depends on nice_plug;
//! `fts-audio-ui` itself is provider-agnostic. Every plugin editor in the
//! suite uses this — it was copied verbatim into `eq-ui`, `comp-ui` and
//! `trigger-ui` before being lifted.

use fts_audio_ui::ParamHandle;
use nice_plug::prelude::ParamPtr;
use nice_plug_dioxus::prelude::ParamContext;

/// Build a [`ParamHandle`] from a nice_plug `ParamPtr` and the editor's
/// [`ParamContext`].
///
/// The ONE adapter for the plugin suite — every `*-ui` crate re-exports it
/// (`primitives.drift-is-a-bug`: it was copied verbatim into seven crates
/// before being lifted here, and the copies drifted).
///
/// Pulls the param's default + bipolar metadata off of `nice_plug`:
/// `default_normalized_value()` populates the alt-click reset target;
/// `step_count() == None && unit() == "dB"` is taken as a hint that the
/// parameter is bipolar (centre-detent visualization). Override with the
/// `_with_options` variant if you need a different mapping.
pub fn param_handle(ptr: ParamPtr, ctx: ParamContext) -> ParamHandle {
    let default = unsafe { ptr.default_normalized_value() };
    let bipolar = is_bipolar_heuristic(ptr);
    param_handle_with_options(ptr, ctx, default, bipolar)
}

/// Builder variant that lets the caller pin the default + bipolar flag
/// explicitly when the heuristic doesn't fit.
pub fn param_handle_with_options(
    ptr: ParamPtr,
    ctx: ParamContext,
    default_normalized: f32,
    bipolar: bool,
) -> ParamHandle {
    let ctx_begin = ctx.clone();
    let ctx_set = ctx.clone();
    let ctx_end = ctx.clone();
    ParamHandle::new(
        move || unsafe { ptr.modulated_normalized_value() },
        move || ctx_begin.begin_set_raw(ptr),
        move |v| ctx_set.set_normalized_raw(ptr, v),
        move || ctx_end.end_set_raw(ptr),
        move || unsafe {
            let n = ptr.modulated_normalized_value();
            ptr.normalized_value_to_string(n, true)
        },
        move || unsafe { ptr.name() }.to_string(),
        move |s| unsafe { ptr.string_to_normalized_value(s) },
    )
    .with_default(default_normalized)
    .with_bipolar(bipolar)
    // Stepped params snap one step per wheel notch / arrow key; the unit
    // drives the typed-value conventions (`1k`, `A4`, `2x`) — see
    // `fts_audio_ui::gesture`.
    .with_step_count(unsafe { ptr.step_count() })
    .with_unit(unsafe { ptr.unit() })
}

/// Treat parameters whose unit is dB as bipolar by default — gain
/// staging knobs benefit from a centre detent. Anything else gets the
/// unipolar arc.
fn is_bipolar_heuristic(ptr: ParamPtr) -> bool {
    unsafe { ptr.unit() }.trim() == "dB"
}
