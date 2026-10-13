//! Tailwind CSS integration for dioxus-native components.
//!
//! Embeds the compiled Tailwind CSS at compile time and provides a component
//! to inject it into any dioxus-native panel. Use `TailwindStyle` as the first
//! child of your root component to enable Tailwind class-based styling.
//!
//! # Build
//!
//! Run `just tailwind` to regenerate `assets/tailwind.css` from Rust source files.
//! The CSS is compiled by Tailwind CLI v3 scanning `crates/reaper-input/src/**/*.rs`.
//!
//! # Usage
//!
//! ```rust,ignore
//! use reaper_input::ui::tailwind::TailwindStyle;
//!
//! #[component]
//! fn MyPanel() -> Element {
//!     rsx! {
//!         TailwindStyle {}
//!         div { class: "flex flex-col bg-zinc-900 text-zinc-100 p-4",
//!             h1 { class: "text-xl font-bold", "Hello Tailwind!" }
//!         }
//!     }
//! }
//! ```

use reaper_dioxus::prelude::*;

/// Compiled Tailwind CSS, embedded at compile time.
/// Regenerate with `just tailwind`.
const TAILWIND_CSS: &str = include_str!("../../assets/tailwind.css");

/// Injects Tailwind CSS into the dioxus-native document.
/// Place this as the first child of your root component.
#[component]
pub fn TailwindStyle() -> Element {
    rsx! {
        document::Style { {TAILWIND_CSS} }
    }
}
