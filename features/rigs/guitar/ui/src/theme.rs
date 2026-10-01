//! The rig's look, as values: colours, type sizes, radii and the few style
//! fragments every surface repeats.
//!
//! Inline, because the rig must lay out without Tailwind (CLAUDE.md), and
//! the rig's own greys rather than the app theme's so every surface — the
//! sidebars, the library, the menus — reads as one instrument. Each surface
//! used to carry its own copy of these (two different `LINE`s among them);
//! a colour is changed here or nowhere.
//!
//! The values live in `signal_widgets::theme`, shared with every rig.

pub use signal_widgets::theme::*;
