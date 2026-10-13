//! Dioxus components for the which-key overlay.
//!
//! The generic overlay component and its data types now live in
//! `input-dioxus` (`input_dioxus::which_key`) so app-agnostic consumers can
//! reuse them. reaper-input keeps a thin wrapper that additionally injects the
//! embedded REAPER Tailwind stylesheet (Blitz doesn't load external CSS), and
//! re-exports the shared types at their historical paths so every caller in
//! this crate is unchanged.

use crate::ui::tailwind::TailwindStyle;
use reaper_dioxus::prelude::*;

pub use input_dioxus::which_key::{OverlayEntry, WhichKeyState};

/// REAPER which-key overlay root. Injects the compiled Tailwind stylesheet
/// (so the shared component's class-based colours resolve under Blitz) and
/// then renders the generic [`input_dioxus::WhichKeyOverlay`], which reads its
/// [`WhichKeyState`] from component context (provided by the native overlay
/// builder).
#[component]
pub fn WhichKeyOverlay() -> Element {
    rsx! {
        TailwindStyle {}
        input_dioxus::WhichKeyOverlay {}
    }
}
