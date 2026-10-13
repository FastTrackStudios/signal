//! The DAW-wired panels — the app's own surfaces.
//!
//! These poll or subscribe to a live `daw_control::Daw` and draw the vector
//! controls from `daw_theme_art`. Nothing here blits: the same components
//! that draw these panels are rasterised to build the REAPER theme, which
//! is what keeps the two renderings honest.
//!
//! Not to be confused with [`crate::panels`], which executes a *theme's*
//! WALTER layout and blits that theme's art. That is a different job and a
//! deliberate one — see the note there.

pub mod arrangement_view;
pub mod fx_chain_tree;
pub mod fx_parameter_browser;
#[cfg(feature = "web")]
pub mod main_window;
pub mod media_browser;
pub mod mixer;
pub mod tcp;
pub mod toolbars;
pub mod track_control_panel;
