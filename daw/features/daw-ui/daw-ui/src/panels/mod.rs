//! # The WALTER panels are gone — the native components are the UI
//!
//! Deleted 2026-08-19, at Cody's direction: `arrange_view.rs`,
//! `track_control_panel.rs`, `mixer_control_panel.rs`, `workspace.rs`
//! (DawWorkspace), `mcp_strip.rs` (the WALTER-executing strip),
//! `envcp_row.rs`, `transport_bar.rs`, and the `model.rs` view-model
//! (`TrackView`/`ClipView`/`MarkerView`/…).
//!
//! Those panels rendered a *REAPER theme* — WALTER layout, PNG atlas —
//! and they were this crate's first implementation. The one UI is now
//! the **native components family** (`crate::components`, PR #279): the
//! traced vector TCP, arrangement, mixer and controls that the shipped
//! REAPER theme's art is *exported from* (`daw-theme-art`). Building a
//! window on the WALTER panels again is the mistake this deletion
//! exists to prevent — outside a theme editor they render placeholder
//! art, because executing somebody's theme is a different job from
//! being the product's UI.
//!
//! If you are building a theme *editor* and need to render a
//! `.ReaperTheme`'s own layout faithfully, that is a real use — the
//! WALTER interpreter still lives in `crate::theming::walter` (parser
//! and layout evaluation), and the deleted rendering components are in
//! git history at this file's deletion commit. Revive them under the
//! themer, not here.
//!
//! What remains is [`native`]: vector controls that belong with the
//! components family and predate its module ([`NativeTransportBar`] is
//! mounted by `components::main_window`).

pub mod native;

pub use native::NativeTransportBar;
