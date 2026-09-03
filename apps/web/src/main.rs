//! signal.fasttrackstudio.app — the Signal site.
//!
//! Three things live here, and the split between them is the whole design:
//!
//! - **The landing page** ([`routes::Home`]) is a hero plus one stripe per
//!   rig. Each stripe carries an animated mock of that rig's interface —
//!   [`demos`] — which is deliberately CSS and SVG with **no backend and
//!   no engine**. It is a moving picture of the product, and it must stay
//!   that way: a landing page that boots a DSP graph to show a meter
//!   moving is a landing page that white-screens when the graph fails.
//!
//! - **The rig pages** (`/rigs/{guitar,keys,drums}`) are where the real
//!   thing goes. They are stubs today, and the module docs in
//!   [`routes::rig`] say what has to arrive before they are not: sample
//!   and NAM-file streaming over iroh, and the actual rig UI crates.
//!
//! - **The guide** is a vault. `docs/guides/signal/*.md` are notes with
//!   frontmatter; `build.rs` compiles them in and [`guide`] serves them.
//!
//! The site is static. It derives any URL it needs from `window.location`
//! at runtime, so one bundle serves any hostname and a new domain is an
//! ingress change rather than a rebuild.

mod demos;
mod guide;
mod routes;

use dioxus::prelude::*;

/// Every screen the site has.
#[derive(Routable, Clone, PartialEq, Eq, Debug)]
#[rustfmt::skip]
pub enum Route {
    #[route("/")]
    Home {},
    // `/rigs/:rig` rather than three routes: the three pages differ by
    // content, not by shape, and a `Rig` enum keeps an unknown slug from
    // rendering a blank page.
    #[route("/rigs/:rig")]
    RigDemo { rig: String },
    #[route("/guide")]
    GuideIndex {},
    #[route("/guide/:slug")]
    GuidePage { slug: String },
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}

use routes::{GuideIndex, GuidePage, Home, NotFound, RigDemo};

fn main() {
    #[cfg(target_arch = "wasm32")]
    {
        console_error_panic_hook::set_once();
        // INFO, not the default TRACE: dioxus traces every template
        // creation and mount at TRACE, and the stripes animate, so TRACE
        // is a function of frame rate rather than of anything useful.
        tracing_wasm::set_as_global_default_with_config(
            tracing_wasm::WASMLayerConfigBuilder::new()
                .set_max_level(tracing::Level::INFO)
                .build(),
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "info".into()),
            )
            .init();
    }

    dioxus::launch(App);
}

// The lint fires inside dioxus's `asset!` expansion, on a `&[u8]` the macro
// generates. There is no hand-written code to change, and rsx! does not take
// attributes on an element, so it is scoped to the function.
#[expect(
    clippy::volatile_composites,
    reason = "fires on dioxus's asset! expansion, not on code written here"
)]
#[component]
fn App() -> Element {
    rsx! {
        // Dark only. `color-scheme` reaches the browser before the
        // stylesheet is parsed, so scrollbars and the initial canvas are
        // dark from the first frame rather than flashing white;
        // `theme-color` tints the mobile browser chrome to match the plate.
        document::Meta { name: "color-scheme", content: "dark" }
        document::Meta { name: "theme-color", content: "#0a0a0c" }
        document::Link { rel: "icon", href: asset!("/assets/icon.svg") }
        document::Style { {include_str!("../assets/site.css")} }
        Router::<Route> {}
    }
}
