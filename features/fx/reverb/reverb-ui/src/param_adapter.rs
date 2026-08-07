//! Adapter from nice_plug's `ParamPtr` + `ParamContext` to
//! [`fts_ui_audio::ParamHandle`].
//!
//! Lives in `reverb-ui` because it depends on nice_plug; `fts-ui-audio` itself
//! is provider-agnostic. Same shape as `eq-ui::param_adapter` — use this
//! whenever you wire a Dioxus audio widget to a plugin parameter.

use fts_ui_audio::ParamHandle;
use nice_plug::prelude::ParamPtr;
use nice_plug_dioxus::prelude::ParamContext;

/// Build a [`ParamHandle`] from a nice_plug `ParamPtr` and the editor's
/// [`ParamContext`].
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
}

/// Treat parameters whose unit is dB as bipolar by default — gain
/// staging knobs benefit from a centre detent. Anything else gets the
/// unipolar arc.
fn is_bipolar_heuristic(ptr: ParamPtr) -> bool {
    unsafe { ptr.unit() }.trim() == "dB"
}
