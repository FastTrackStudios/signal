//! The site's entry point — and, for the length of a build, its server.
//!
//! Everything the site *is* lives in the library half of this crate (see
//! `lib.rs`). This binary starts it, and carries the one piece of
//! configuration that only an entry point can: the incremental renderer
//! that `dx build --ssg` writes the guide through.
//!
//! ## How the guide becomes static
//!
//! `dx build --ssg` builds this crate twice — once for the browser, once
//! for the host with the `server` feature — then runs the server binary,
//! asks it for [`signal_web::static_routes`], and requests each path.
//! Rendering a path fills the incremental cache, and the cache is a
//! directory of `index.html` files. So the guide ships as HTML that is
//! finished before any script runs, and the bundle hydrates it into the
//! ordinary app afterwards.
//!
//! Nothing deploys the server. It exists during the build, to render.

// `server_only!` lives in the prelude, and expands to its body only when
// the crate is built for the server — so the ServeConfig below does not
// have to be cfg'd by hand, and does not reach the wasm build at all.
use dioxus::prelude::*;
use signal_web::App;

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

    dioxus::LaunchBuilder::new()
        .with_cfg(server_only! {
            dioxus::server::ServeConfig::builder().incremental(
                dioxus::server::IncrementalRendererConfig::new()
                    // `public` beside the executable is where the CLI
                    // also puts the web bundle, so the rendered pages and
                    // the assets they reference land in one directory —
                    // and that directory is the thing to deploy.
                    .static_dir(
                        std::env::current_exe()
                            .expect("the server knows its own path")
                            .parent()
                            .expect("an executable has a parent directory")
                            .join("public"),
                    )
                    // Emphatically false. The cache directory is shared
                    // with the wasm bundle and every asset; clearing it
                    // on each render would delete the site around the
                    // pages being written into it.
                    .clear_cache(false),
            )
        })
        .launch(App);
}
