//! The plugin editor: what is loaded, and the catalog to change it from.
//!
//! The whole reason this editor exists is that a headless NAM plugin had no
//! way to choose a model — it read a path out of an environment variable. So
//! the editor is deliberately not a faceplate with two knobs: it is the tone
//! browser, with the current model named above it.
//!
//! Everything below the header is [`signal_tone3000_ui::ToneBrowser`], the
//! same component the desktop app and the browser remote mount. It renders
//! from the vox clients in context, which here come from the in-process
//! engine ([`crate::engine`]).

use audiocore_core::prelude::*;
use signal_tone3000_ui::{ToneBrowser, UrlOpener};

use crate::state::NamUi;

/// Editor root. Props-less, as the plugin surface requires — everything
/// arrives through context.
#[component]
pub fn App() -> Element {
    let shared = use_context::<SharedState>();
    let Some(ui) = shared.get::<NamUi>() else {
        return rsx! {
            div { style: "padding:16px;font:13px system-ui;color:#e8e8ec;background:#141418;",
                "NAM editor state missing."
            }
        };
    };

    // The engine's clients, provided once so the browser (and everything
    // under it) can consume them the way it does in every other shell.
    use_hook(|| {
        if let Some(engine) = crate::engine::get() {
            let _ = provide_context(engine.client);
            let _ = provide_context(engine.stream);
        }
        let _ = provide_context(UrlOpener::new(crate::engine::open_externally));
    });

    let loaded = ui.loaded_name();

    rsx! {
        div {
            style: "display:flex;flex-direction:column;height:100vh;background:#141418;
                    color:#e8e8ec;font:13px/1.4 system-ui,-apple-system,sans-serif;",

            // The header answers the question the old build could not: what
            // is this insert actually playing?
            div {
                style: "display:flex;align-items:center;gap:10px;padding:10px 14px;
                        border-bottom:1px solid #2a2a32;",
                span {
                    style: "font-size:11px;letter-spacing:0.08em;text-transform:uppercase;
                            color:#8b8b97;",
                    "Amp"
                }
                span {
                    style: "font-size:14px;font-weight:600;overflow:hidden;
                            text-overflow:ellipsis;white-space:nowrap;",
                    if loaded.is_empty() { "— no model —" } else { "{loaded}" }
                }
            }

            div { style: "flex:1;min-height:0;",
                ToneBrowser {
                    on_loaded: move |(name, path): (String, String)| {
                        // The browser has already put the file on this
                        // machine; all that is left is to play it.
                        ui.load(&name, &path);
                    },
                }
            }
        }
    }
}

/// The editor window's initial size.
///
/// Sized for a grid of tone cards rather than for two knobs: three columns
/// and a couple of rows is the smallest window in which browsing a catalog
/// feels like browsing rather than scrolling.
pub const SIZE: (u32, u32) = (760, 560);
