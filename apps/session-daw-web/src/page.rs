//! The Session DAW view in a browser: the web demo's bundle.
//!
//! Built with cargo and wasm-bindgen (`just web-daw`), served as static
//! files: `index.html` and the module. The song comes from a Task share
//! link to its session folder — `?share=<link>` — opened the one way songs
//! open (its prepared `.session`, mirrored into memory) with its proxies
//! streaming in by range, what will be heard first first; `&guide=<link>`
//! is the guide sample library's, for the click, the count and the cues.
//! Without a link it opens the copy bundled under `session/`, silent. See
//! `session_daw::web_engine`.

use dioxus::prelude::*;

/// The compiled Tailwind the session-ui panels (the progress bar, the
/// transport buttons) are styled with — the desktop app's sheet.
const TAILWIND: &str = include_str!("../../desktop/assets/tailwind-signal.css");

/// Start the page: its logging, then the app.
pub fn run() {
    {
        console_error_panic_hook::set_once();
        // Loro's own block diagnostics come at info, a few hundred lines a
        // sync: kept to warnings.
        use tracing_subscriber::layer::SubscriberExt as _;
        let filter = tracing_subscriber::filter::Targets::new()
            .with_default(tracing::Level::INFO)
            .with_target("loro_internal", tracing::Level::WARN)
            .with_target("loro", tracing::Level::WARN);
        let subscriber =
            tracing_subscriber::registry()
                .with(filter)
                .with(tracing_wasm::WASMLayer::new(
                    tracing_wasm::WASMLayerConfigBuilder::new().build(),
                ));
        let _ = tracing::subscriber::set_global_default(subscriber);
    }
    dioxus::launch(App);
}

/// The query parameter a share link comes in.
const SHARE: &str = "share";
/// The query parameter a live link (a set Task keeps) comes in.
const LIVE: &str = "live";

/// A visitor's name, when the page is not told one: Guest and four
/// letters, so two guests are told apart.
fn guest_name() -> String {
    let n = (js_sys::Math::random() * 65_536.0) as u32;
    format!("Guest {n:04x}")
}

/// What the page opens — a set Task keeps, joined by its live link
/// (`?live=<link>&name=<who>`), the song a share link shares
/// (`?share=<link>`), or the copy bundled under `session/`, silent — and
/// the guide library's link (`&guide=<link>`).
fn source() -> (session_daw::web_engine::WebSource, Option<String>) {
    use session_daw::web_engine::WebSource;
    let query = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .and_then(|q| web_sys::UrlSearchParams::new_with_str(&q).ok());
    let param = |key: &str| {
        query
            .as_ref()
            .and_then(|q| q.get(key))
            .filter(|v| !v.is_empty())
    };
    let source = match (param(LIVE), param(SHARE)) {
        (Some(link), _) => WebSource::Live {
            link,
            name: param("name").unwrap_or_else(guest_name),
        },
        (None, Some(link)) => WebSource::Shared { link },
        (None, None) => WebSource::Bundled {
            name: "Always On Time".to_owned(),
            rpp_url: "session/demo.RPP".to_owned(),
            chart_url: Some("session/demo.kf".to_owned()),
        },
    };
    (source, param("guide"))
}

#[component]
fn App() -> Element {
    let (source, guide) = use_hook(source);
    rsx! {
        style { {TAILWIND} }
        session_daw::web_host::WebDemo { source, guide }
    }
}
