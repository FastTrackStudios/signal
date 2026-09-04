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
//! - **The guide** is a vault, and it is *static*. `docs/guides/signal/*.md`
//!   are notes with frontmatter; `build.rs` renders them to HTML at build
//!   time through Task's `ssg-build`, and `src/bin/bake.rs` writes each
//!   one out as a scriptless `index.html`. Reading the documentation
//!   never loads the wasm bundle.
//!
//! The site is static. It derives any URL it needs from `window.location`
//! at runtime, so one bundle serves any hostname and a new domain is an
//! ingress change rather than a rebuild.
//!
//! ## Why this is a library as well as a binary
//!
//! `main.rs` is only a launcher, and the interesting configuration on it
//! — the incremental renderer that static generation writes through —
//! reads better next to nothing else. Keeping the app itself in a
//! library also means the routes and the vault are addressable by name
//! from outside `main`, which is what [`static_routes`] does.

pub mod demos;
pub mod guide;
pub mod routes;

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

/// The site's stylesheet.
///
/// Inlined rather than linked because it is small, and one round trip
/// before the page can be read is worse than a few kilobytes inside the
/// document it is already fetching. [`App`] emits it through
/// `document::Style`, which the server renderer resolves into the
/// `<head>` of every pre-rendered page.
pub const SITE_CSS: &str = include_str!("../assets/site.css");

/// The paths `dx build --ssg` should pre-render.
///
/// The CLI looks for a server function at exactly this endpoint, calls
/// it once, and then requests every path it returns — which is what
/// writes them into the incremental renderer's directory as HTML.
///
/// Two sources, and the second is the point. `Route::static_routes()`
/// gives the routes with no parameters in them — `/` and `/guide`. It
/// cannot give the guide's pages, because `/guide/:slug` is a *single*
/// parameterised route and only the vault knows what the slugs are; so
/// the vault supplies them. Everything not in this list — the rig demo
/// pages, the catch-all — is left alone and stays a client-side route.
#[cfg(feature = "server")]
#[dioxus::prelude::server(endpoint = "static_routes")]
pub async fn static_routes() -> dioxus::prelude::ServerFnResult<Vec<String>> {
    let mut routes: Vec<String> = Route::static_routes()
        .iter()
        .map(ToString::to_string)
        .collect();

    for route in guide::VAULT.routes(routes::guide_page::BASE) {
        if !routes.contains(&route) {
            routes.push(route);
        }
    }

    Ok(routes)
}

// The lint fires inside dioxus's `asset!` expansion, on a `&[u8]` the macro
// generates. There is no hand-written code to change, and rsx! does not take
// attributes on an element, so it is scoped to the function.
#[expect(
    clippy::volatile_composites,
    reason = "fires on dioxus's asset! expansion, not on code written here"
)]
#[component]
pub fn App() -> Element {
    rsx! {
        // Dark only. `color-scheme` reaches the browser before the
        // stylesheet is parsed, so scrollbars and the initial canvas are
        // dark from the first frame rather than flashing white;
        // `theme-color` tints the mobile browser chrome to match the plate.
        document::Meta { name: "color-scheme", content: "dark" }
        document::Meta { name: "theme-color", content: "#0a0a0c" }
        document::Link { rel: "icon", href: asset!("/assets/icon.svg") }
        // The vault sheet first, so the site's own rules win a tie: the
        // guide components' `ssg-*` classes are a default to build on,
        // not a theme to fight.
        document::Style { {ssg_ui::VAULT_CSS} }
        document::Style { {SITE_CSS} }
        Router::<Route> {}
    }
}

