//! `fts-themer-ui` — the FastTrackStudio REAPER theme editor, in a browser.
//!
//! ```sh
//! just reaper theme-edit          # dx serve --platform web
//! ```
//!
//! Colours are edited in the browser against the `.ReaperTheme` text itself,
//! so the preview re-renders per keystroke with no round trip. Anything that
//! needs a filesystem — reading the theme, saving it, recoloring artwork —
//! goes through a server function (`server.rs`).

use dioxus::prelude::*;

mod editor;
mod preview;
mod server;

const STYLE: Asset = asset!("/assets/editor.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Stylesheet { href: STYLE }
        editor::ThemeEditor {}
    }
}
