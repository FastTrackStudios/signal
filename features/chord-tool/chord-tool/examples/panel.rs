//! Run the chord panel in a desktop window.
//!
//! ```sh
//! dx serve --package chord-tool --example panel --platform linux --hot-patch false
//! cargo run -p chord-tool --example panel
//! ```
//!
//! The iteration loop, not the shipping target. This uses Dioxus's
//! desktop (`WebView`) renderer, while the REAPER panel renders through
//! Blitz — CSS support differs, so keep styles simple and check the
//! REAPER panel before believing a layout.
//!
//! Window config mirrors `apps/desktop`: undecorated and
//! menu-less, because the panel draws its own surface and a host
//! titlebar is not part of what ships inside REAPER.
use dioxus::desktop::tao::dpi::LogicalSize;
use dioxus::desktop::{Config, WindowBuilder};

fn main() {
    let window = WindowBuilder::new()
        .with_title("FTS Chord Tool")
        .with_decorations(false)
        .with_inner_size(LogicalSize::new(1100.0, 620.0))
        .with_min_inner_size(LogicalSize::new(560.0, 360.0));

    dioxus::LaunchBuilder::new()
        .with_cfg(Config::new().with_window(window).with_menu(None))
        .launch(chord_tool::ChordToolPanel);
}
