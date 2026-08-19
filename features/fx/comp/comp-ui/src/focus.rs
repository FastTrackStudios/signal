//! Which stack stage the editor is editing (`fx.stack.focus`).
//!
//! One signal, provided at the editor root and read wherever a component
//! needs "the stage the user is on": the rail's plain click replaces this
//! stage's profile, the face edits this stage's params, the graph draws this
//! stage's curve, the meters read this stage's telemetry. UI state — the
//! session persists it via `CompParams::focused_stage`, and the audio thread
//! learns it through `CompUiState::focused_stage` (an atomic), never through
//! this signal.

use nice_plug_dioxus::prelude::{try_consume_context, ReadableExt, Signal};

/// The focused stage, 0-based.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FocusedStage(pub usize);

/// The focused stage index, or stage 0 where no provider is mounted (unit
/// tests, isolated components).
pub fn use_focused_stage() -> usize {
    match try_consume_context::<Signal<FocusedStage>>() {
        Some(sig) => sig.read().0,
        None => 0,
    }
}

/// The focus signal itself, for components that *set* focus (the strip).
pub fn use_focus_signal() -> Option<Signal<FocusedStage>> {
    try_consume_context::<Signal<FocusedStage>>()
}
