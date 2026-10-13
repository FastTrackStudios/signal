//! The canonical FastTrackStudio theme.
//!
//! One authored palette ([`Theme`]) that every FTS surface resolves from —
//! the Dioxus panels, the expression editor, and REAPER itself.
//!
//! ```text
//!            fts-theme.styx        (authored, one vocabulary)
//!                  │
//!        ┌─────────┼──────────────┐
//!        ▼         ▼              ▼
//!   panel tokens  .ReaperTheme   editor tokens
//!                 (generated)    (pitch / grid / zone)
//! ```
//!
//! **Why the canonical theme is not a REAPER theme.** REAPER's palette has
//! no slot for a pitch class, a razor area or a structural zone, so a
//! REAPER-first design leaves the editor with a second, unsynced palette —
//! which is the problem this crate exists to remove. Instead REAPER is an
//! *output*: [`Theme::reaper_palette`] emits the keys it determines, and
//! `daw_ui::theming::reaper_import` still handles the other direction for
//! adopting an existing theme as a starting point.
//!
//! ```no_run
//! use daw_theme::Theme;
//!
//! let theme = Theme::default();
//! for assignment in theme.reaper_palette() {
//!     // apply via fts_themer::ThemeIni::set_color
//!     println!("{}={}", assignment.key, assignment.color.to_hex());
//! }
//! ```

pub mod color;
pub mod defaults;
pub mod palette;
pub mod ramp;
pub mod reaper_export;
pub mod swell_export;

pub use color::Color;
pub use palette::{Chrome, Editor, Metrics, Signal, Theme};
pub use ramp::Ramp;
pub use reaper_export::Assignment;
pub use swell_export::SwellSetting;
