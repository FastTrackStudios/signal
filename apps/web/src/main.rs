//! session.fasttrackstudio.app
//!
//! Two things: a landing page that shows what Session is, and the live
//! demo — `/demo` opens the Session app itself (served under `/app/`) in
//! the public playground set Task keeps: everyone who comes is in one
//! session, which starts over every few minutes.

mod guide;
mod praise;
mod routes;

use dioxus::prelude::*;

#[derive(Routable, Clone, PartialEq)]
#[rustfmt::skip]
pub enum Route {
    #[route("/")]
    Home {},
    #[route("/reference")]
    ReferencePage {},
    #[route("/demo")]
    Demo {},
    #[route("/guide")]
    GuideIndex {},
    #[route("/guide/:slug")]
    GuidePage { slug: String },
    #[route("/:..segments")]
    NotFound { segments: Vec<String> },
}

use routes::{Demo, GuideIndex, GuidePage, Home, NotFound, ReferencePage};

fn main() {
    #[cfg(target_arch = "wasm32")]
    {
        console_error_panic_hook::set_once();
        // INFO, not TRACE — dioxus traces every template/signal/mount, and
        // the demo's transport re-renders continuously once playing.
        tracing_wasm::set_as_global_default_with_config(
            tracing_wasm::WASMLayerConfigBuilder::new()
                .set_max_level(tracing::Level::INFO)
                .build(),
        );
    }
    #[cfg(not(target_arch = "wasm32"))]
    tracing_subscriber::fmt::init();

    dioxus::LaunchBuilder::new()
        .with_cfg(server_only! {
            dioxus::server::ServeConfig::builder().incremental(
                dioxus::server::IncrementalRendererConfig::new()
                    // `public` beside the executable is where the CLI
                    // also puts the web bundle, so the pre-rendered
                    // pages and the assets they reference land in one
                    // directory — and that directory is what deploys.
                    .static_dir(
                        std::env::current_exe()
                            .expect("the server knows its own path")
                            .parent()
                            .expect("an executable has a parent directory")
                            .join("public"),
                    )
                    // Emphatically false. The cache directory is shared
                    // with the wasm bundle and every asset; clearing it
                    // per render would delete the site around the pages
                    // being written into it.
                    .clear_cache(false),
            )
        })
        .launch(App);
}

/// The paths `dx build --ssg` should pre-render.
///
/// The CLI looks for a server function at exactly this endpoint, calls
/// it once, and requests every path it returns — which is what writes
/// them to disk as HTML.
///
/// Every page the site has: the landing page, the reference, the guide,
/// and `/demo` — which is a redirect into the Session app now (`/app/`),
/// not the in-process backend it once booted, so it has a finished form
/// like any other page.
///
/// A path left out is served as the bare shell, and this is a fullstack
/// build: its client reads the page's hydration data on load, finds none
/// in the shell, and stops (`atob` on nothing) — a blank page. That is
/// what the landing page and `/demo` did while only the guide was here.
#[cfg(feature = "server")]
#[server(endpoint = "static_routes")]
async fn static_routes() -> ServerFnResult<Vec<String>> {
    let mut routes = vec!["/".to_owned(), "/reference".to_owned(), "/demo".to_owned()];
    routes.extend(guide::VAULT.routes(guide::BASE));
    Ok(routes)
}

#[component]
fn App() -> Element {
    rsx! {
        document::Title { "Session — your setlist, live" }
        // Dark-only site: force it ahead of any CSS load so native chrome
        // (form controls, scrollbars) never flashes light before
        // tailwind.css's `color-scheme: dark` takes over.
        document::Meta { name: "color-scheme", content: "dark" }
        // Session's own icon — the app's (apps/desktop/ios/icon.svg), served
        // from public/ at a fixed path so the page shell and /app/ use it too.
        document::Link { rel: "icon", r#type: "image/svg+xml", href: "/favicon.svg" }
        document::Link { rel: "preconnect", href: "https://fonts.googleapis.com" }
        document::Link {
            rel: "preconnect",
            href: "https://fonts.gstatic.com",
            crossorigin: "anonymous",
        }
        document::Link {
            rel: "stylesheet",
            href: "https://fonts.googleapis.com/css2?family=Geist:wght@300..800&family=Geist+Mono:wght@400..600&display=swap",
        }
        document::Link { rel: "stylesheet", href: asset!("/assets/tailwind.css") }
        // The guide components' own sheet — prose, contents rail,
        // chapter nav. Its colours are custom properties with inherited
        // fallbacks, so Tailwind's palette wins where the two meet.
        document::Stylesheet { href: ssg_ui::VAULT_STYLE }
        Router::<Route> {}
    }
}
