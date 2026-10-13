//! The track panel's geometry that the DOM around the painted panel
//! shares with it.
//!
//! What is left of the component panel: the panel is painted by
//! `session_daw::widget::ArrangementWidget`, and folders fold (see
//! [`super::folded`]) against where its row tint ends.

/// Where the row's tint ends and REAPER's meter gutter begins.
pub const TINT_W: f64 = 296.0;
