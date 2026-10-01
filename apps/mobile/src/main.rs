//! Signal on a phone: the guitar rig's phone pages, a remote of a signal
//! engine.
//!
//! The same `GuitarRigRemote` the desktop app and the browser remote mount,
//! over vox WebSocket to `signal-desktop --engine` — the phone is one more
//! remote of the headless core. Its window's size picks the phone form
//! factor (a phone held sideways lays the chain out a page at a time).
//!
//! The engine: `SIGNAL_ENGINE_URL`, else on Android the host the emulator
//! runs on (`ws://10.0.2.2:4040/vox`), else the local engine.

use dioxus::prelude::*;
use signal_guitar_proto::audio::AudioSettingsClient;
use signal_guitar_proto::rig::{RigClient, RigStreamClient};
use signal_guitar_ui::GuitarRigRemote;

/// The signal UI's compiled Tailwind (the desktop app's sheet).
const SIGNAL_TAILWIND: &str = include_str!("../../desktop/assets/tailwind-signal.css");

fn main() {
    init_tracing();
    launch();
}

/// Android's entry: NativeActivity loads libmain.so and calls this.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: dioxus_native::AndroidApp) {
    init_tracing();
    // The faces are frame's files, read from `SIGNAL_FRAME_DIR`: here the
    // app's own external files (`scripts/push-faces.sh` puts them there
    // while the faces are designed; they ship in the APK later).
    if let Some(dir) = app.external_data_path() {
        // SAFETY: before any thread the app starts.
        unsafe { std::env::set_var("SIGNAL_FRAME_DIR", dir.join("frame")) };
    }
    dioxus_native::set_android_app(app);
    launch();
}

fn init_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,vox_core=warn".into()))
        .try_init();
}

fn launch() {
    use dioxus_native::{Config, LogicalSize, WindowAttributes, launch_cfg};
    // On a desktop, an iPhone 16 Pro held sideways (its points); on a phone
    // the screen is the window.
    let window = WindowAttributes::default().with_title("Signal").with_surface_size(LogicalSize::new(874.0, 402.0));
    launch_cfg(App, vec![], vec![Box::new(Config::new().with_window_attributes(window))]);
}

/// Where the engine is.
fn engine_url() -> String {
    std::env::var("SIGNAL_ENGINE_URL").unwrap_or_else(|_| {
        if cfg!(target_os = "android") { "ws://10.0.2.2:4040/vox" } else { "ws://127.0.0.1:4040/vox" }.to_string()
    })
}

/// One typed client over its own link.
async fn establish<C: vox_core::FromVoxLane>(url: &str) -> Option<C> {
    let link = vox_websocket::WsLink::connect(url).await.map_err(|e| tracing::debug!("ws connect {url}: {e:?}")).ok()?;
    vox_core::initiator_on(link).establish::<C>().await.map_err(|e| tracing::debug!("vox handshake: {e:?}")).ok()
}

type Clients = (RigClient, RigStreamClient, AudioSettingsClient);

async fn connect(url: &str) -> Option<Clients> {
    Some((establish(url).await?, establish(url).await?, establish(url).await?))
}

#[component]
fn App() -> Element {
    // The window's size in points (and its aspect): the form factor.
    let mut aspect = use_signal(|| 874.0 / 402.0);
    use_context_provider(|| signal_guitar_ui::WindowAspect(aspect));
    let mut size = use_signal(|| (874.0_f64, 402.0_f64));
    use_context_provider(|| signal_guitar_ui::WindowSize(size));
    let window = dioxus_native::use_window();
    use_future(move || {
        let window = window.clone();
        async move {
            loop {
                let px = window.surface_size();
                if px.height > 0 {
                    let scale = window.scale_factor().max(0.1);
                    let pt = (f64::from(px.width) / scale, f64::from(px.height) / scale);
                    let was = *size.peek();
                    if (was.0 - pt.0).abs() > 1.0 || (was.1 - pt.1).abs() > 1.0 {
                        size.set(pt);
                        aspect.set(pt.0 / pt.1);
                    }
                }
                architect::platform::sleep(std::time::Duration::from_millis(300)).await;
            }
        }
    });

    // Dial the engine until it answers; a watchdog remounts if it goes.
    let mut generation = use_signal(|| 0u32);
    let mut lost = use_signal(|| false);
    let url = engine_url();
    let clients = use_resource({
        let url = url.clone();
        move || {
            let generation = generation();
            let url = url.clone();
            async move {
                loop {
                    if let Some(c) = connect(&url).await {
                        return (generation, c);
                    }
                    architect::platform::sleep(std::time::Duration::from_millis(1200)).await;
                }
            }
        }
    });
    use_future(move || async move {
        let mut fails = 0u32;
        loop {
            architect::platform::sleep(std::time::Duration::from_millis(1500)).await;
            let Some((g, (rig, _, _))) = clients.peek().as_ref().cloned() else { continue };
            if g != *generation.peek() {
                continue;
            }
            if rig.status().await.is_ok() {
                fails = 0;
                lost.set(false);
            } else {
                fails += 1;
                if fails >= 2 {
                    fails = 0;
                    lost.set(true);
                    generation += 1;
                }
            }
        }
    });

    let state = clients.read().as_ref().filter(|(g, _)| *g == generation()).map(|(_, c)| c.clone());
    rsx! {
        document::Style { {SIGNAL_TAILWIND} }
        // The viewport, a column; the remote fills it (as the desktop's rig
        // view does).
        div { style: "width: 100vw; height: 100vh; overflow: hidden; display: flex; flex-direction: column; background: #0b0b0d; color: #e4e4e7; font-family: Inter, sans-serif;",
            match state {
                Some((rig, stream, settings)) => {
                    let _ = provide_context(rig);
                    let _ = provide_context(stream);
                    let _ = provide_context(settings);
                    rsx! {
                        div { style: "flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column;",
                            GuitarRigRemote { key: "{generation}" }
                        }
                    }
                }
                None => rsx! {
                    div { style: "flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 10px;",
                        span { style: if lost() { "width: 12px; height: 12px; border-radius: 999px; background: #ef4444;" } else { "width: 12px; height: 12px; border-radius: 999px; background: #22c55e;" } }
                        span { style: "font-size: 15px; font-weight: 600;", if lost() { "Engine down — reconnecting…" } else { "Looking for the signal engine…" } }
                        span { style: "font-size: 12px; font-family: monospace; color: #71717a;", "{url}" }
                    }
                },
            }
        }
    }
}
