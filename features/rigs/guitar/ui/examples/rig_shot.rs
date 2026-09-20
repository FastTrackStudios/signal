//! The rig UI, rendered to a picture — no window, no compositor, no device.
//!
//! ```sh
//! nix develop -c cargo run -r -p signal-guitar-ui --example rig_shot -- /tmp/rig.png
//! nix develop -c cargo run -r -p signal-guitar-ui --example rig_shot -- /tmp/rig.png 2560 1440
//! ```
//!
//! # Why render rather than screenshot
//!
//! A screenshot needs a compositor willing to give one up, and KWin hands
//! `grim` nothing (no `wlr-screencopy`) while Spectacle goes through the portal
//! and wants a human. It would also put the whole desktop in frame — everything
//! else that happens to be open — to show one window.
//!
//! Rendering the tree answers a narrower question honestly: what does *this UI*
//! look like, at a size chosen here, with nothing else in the picture. It is
//! the same path the window takes — `blitz_paint::paint_scene` into
//! `anyrender_vello` — so this and a photograph of the window are one
//! renderer's output, which is what makes it worth looking at.
//!
//! Design mode is forced, so the shot opens no audio device and no MIDI node:
//! it can be taken while the real rig is playing, as often as you like, on a
//! machine with no interface at all.
//!
//! # The poll loop
//!
//! Twice is not enough. The first poll builds the tree; the effects that run on
//! mount — the status seed, the chain fetch, the node tree — land on later
//! ones, and the vox calls behind them answer on another thread. A picture
//! taken too early is a UI mid-construction, which reads as a renderer dropping
//! content it was never given.

use anyrender::ImageRenderer as _;
use anyrender_vello::VelloImageRenderer;
use architect::rig::RigBackend as _;
use architect::{LocalServer, Scope};
use blitz_dom::{Document as _, DocumentConfig};
use blitz_traits::shell::{ColorScheme, Viewport};
use dioxus::prelude::*;
use dioxus_native_dom::DioxusDocument;
use signal_guitar::GuitarRigBackend;
use signal_guitar_proto::audio::AudioSettingsClient;
use signal_guitar_proto::rig::{RigClient, RigStreamClient};

/// The app's compiled Tailwind — the same sheet `rig_view` mounts.
///
/// Read from the app's assets rather than duplicated: a shot styled by a
/// different stylesheet is a picture of a UI nobody runs.
const SIGNAL_TAILWIND: &str =
    include_str!("../../../../../apps/desktop/assets/tailwind-signal.css");

/// The clients the page needs, established against an in-process server.
///
/// A static rather than props: this process takes one picture and stops, and a
/// prop would have to be `PartialEq` for a value that is a set of live clients.
static WIRED: std::sync::OnceLock<Wired> = std::sync::OnceLock::new();

struct Wired {
    rig: RigClient,
    stream: RigStreamClient,
    settings: AudioSettingsClient,
}

fn main() {
    // SAFETY: single-threaded, before the rig or any thread is created.
    unsafe {
        std::env::set_var("SIGNAL_RIG_DESIGN", "1");
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();

    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "rig.png".to_string());
    let w: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1600);
    let h: u32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(1000);

    // The rig and its wire, on a runtime of their own. The render below is
    // synchronous and Blitz drives the component's own tasks, so this runtime
    // exists only to serve the RPCs the page makes.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let wired = runtime.block_on(async {
        let backend = GuitarRigBackend::new();
        backend.open_blocking();
        // Leaked on purpose: the server has to outlive the render, and this
        // process exists to take one picture and stop.
        let server: &'static LocalServer = Box::leak(Box::new(LocalServer::serve(
            backend.router(),
            Scope::new(),
        )));
        let rig: RigClient = server.establish().await.expect("rig client");
        let stream: RigStreamClient = server.establish().await.expect("stream client");
        let settings: AudioSettingsClient =
            server.establish().await.expect("settings client");
        Wired {
            rig,
            stream,
            settings,
        }
    });
    let _ = WIRED.set(wired);

    let _guard = runtime.enter();
    let vdom = VirtualDom::new(Shot);
    let mut document = DioxusDocument::new(
        vdom,
        DocumentConfig {
            viewport: Some(Viewport::new(w, h, 1.0, ColorScheme::Dark)),
            ..Default::default()
        },
    );
    document.initial_build();
    for _ in 0..25 {
        document.poll(None);
        {
            let mut inner = document.inner_mut();
            inner.resolve(0.0);
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
    }

    let mut image = VelloImageRenderer::new(w, h);
    let mut buffer = Vec::new();
    image.render_to_vec(
        |painter| {
            let mut inner = document.inner_mut();
            blitz_paint::paint_scene(painter, &mut inner, 1.0, w, h, 0, 0);
        },
        &mut buffer,
    );
    let Some(image) = image::RgbaImage::from_raw(w, h, buffer) else {
        eprintln!("the renderer returned a buffer of the wrong size");
        std::process::exit(1);
    };
    image.save(&out).expect("write the picture");
    println!("wrote {out} — {w}x{h}");
}

/// The rig remote with its clients in context, the way the window mounts it.
#[component]
fn Shot() -> Element {
    use_hook(|| {
        if let Some(w) = WIRED.get() {
            let _ = provide_context(w.rig.clone());
            let _ = provide_context(w.stream.clone());
            let _ = provide_context(w.settings.clone());
        }
    });
    rsx! {
        // The same two stylesheets the window mounts. Without them the shot is
        // a picture of the UI with its layout missing — every Tailwind class
        // undefined, so flex rows stack and the type falls back to a serif —
        // which looks like a broken renderer rather than a missing stylesheet.
        // A real `<style>` node, not `document::Style`: the head mechanism
        // routes through the document provider, and a headless document has
        // no head to route to — the sheet arrives nowhere and every Tailwind
        // class stays undefined, which reads as a broken renderer.
        style {
            {"html,body{margin:0;padding:0;height:100%;background:#0a0a0a;overflow:hidden;}*{box-sizing:border-box;}"}
        }
        style { {SIGNAL_TAILWIND} }
        div { style: "width: 100%; height: 100%;",
            signal_guitar_ui::GuitarRigRemote {}
        }
    }
}
