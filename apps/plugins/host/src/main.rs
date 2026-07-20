//! fts-clap-host — open any CLAP plugin's embedded GUI in a real window.
//!
//! This is the REAPER-parity iteration loop for FTS plugin editors: the exact
//! embedded-GUI path a DAW drives (`gui.create` → `set_parent` → `show`, with
//! `on_main_thread` pumped at UI rate), so what you see here is what REAPER
//! shows — unlike eq-standalone's dioxus-native shell, which renders through
//! a different windowing stack.
//!
//! ```sh
//! fts-clap-host "FTS EQ"                    # resolves ~/.clap/FTS EQ.clap
//! fts-clap-host ~/.clap/"FTS EQ.clap"       # explicit bundle path
//! fts-clap-host target/bundled/"FTS EQ.clap" --index 0
//! ```
//!
//! v1 is GUI-only (no audio I/O): the plugin is instantiated but not
//! activated, exactly enough for editor iteration. Meters/spectra that need
//! audio stay idle until we wire a cpal/PipeWire stream through
//! `daw-standalone`'s engine in a follow-up.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use daw_standalone::audio_engine::plugin_host::{ClapHost, LoadedClapPlugin};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

/// How often we run the plugin's deferred main-thread work. REAPER's UI
/// timer runs at ~30 Hz; 60 Hz keeps dioxus editors snappy.
const PUMP_INTERVAL: Duration = Duration::from_millis(16);

fn resolve_bundle(arg: &str) -> PathBuf {
    let direct = PathBuf::from(arg);
    if direct.exists() {
        return direct;
    }
    // "FTS EQ" → ~/.clap/FTS EQ.clap
    if let Some(home) = std::env::var_os("HOME") {
        let named = PathBuf::from(home)
            .join(".clap")
            .join(format!("{}.clap", arg.trim_end_matches(".clap")));
        if named.exists() {
            return named;
        }
    }
    direct
}

struct HostApp {
    bundle: PathBuf,
    plugin_index: usize,
    title: String,
    plugin: Option<LoadedClapPlugin>,
    window: Option<Window>,
    next_pump: Instant,
}

impl ApplicationHandler for HostApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        // Parent window first — the plugin needs its handle at set_parent
        // time. Sized provisionally; we adopt the plugin's reported size
        // right after the GUI is created.
        let window = event_loop
            .create_window(
                Window::default_attributes()
                    .with_title(self.title.clone())
                    .with_inner_size(PhysicalSize::new(800, 500)),
            )
            .expect("creating the host window");

        let raw: RawWindowHandle = window
            .window_handle()
            .expect("host window handle")
            .as_raw();

        let mut plugin = ClapHost::default()
            .load(&self.bundle, self.plugin_index)
            .unwrap_or_else(|e| panic!("loading {}: {e:?}", self.bundle.display()));
        eprintln!(
            "hosting {} ({}) — embedded GUI",
            plugin.descriptor().name,
            plugin.descriptor().id
        );

        match plugin.open_gui_embedded(raw) {
            Ok((w, h)) => {
                let _ = window.request_inner_size(PhysicalSize::new(w, h));
            }
            Err(e) => {
                eprintln!("error: plugin GUI failed to embed: {e:?}");
                event_loop.exit();
                return;
            }
        }

        self.plugin = Some(plugin);
        self.window = Some(window);
        self.next_pump = Instant::now();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                if let Some(plugin) = &mut self.plugin {
                    plugin.close_gui();
                }
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                // Host-side resize → offer the new size to the plugin. The
                // plugin may clamp it (fixed-size editors refuse).
                if let Some(plugin) = &mut self.plugin {
                    if size.width > 0 && size.height > 0 {
                        plugin.gui_set_size(size.width, size.height);
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(plugin) = &mut self.plugin {
            let now = Instant::now();
            if now >= self.next_pump {
                plugin.pump_main_thread();
                self.next_pump = now + PUMP_INTERVAL;
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_pump));
        }
    }
}

fn main() -> eyre::Result<()> {
    let mut args = std::env::args().skip(1);
    let Some(bundle_arg) = args.next() else {
        eprintln!("usage: fts-clap-host <bundle.clap | plugin name> [--index N]");
        std::process::exit(2);
    };
    let mut plugin_index = 0usize;
    while let Some(flag) = args.next() {
        if flag == "--index" {
            plugin_index = args
                .next()
                .and_then(|v| v.parse().ok())
                .unwrap_or_default();
        }
    }

    let bundle = resolve_bundle(&bundle_arg);
    eyre::ensure!(bundle.exists(), "no such bundle: {}", bundle.display());
    let title = format!(
        "fts-clap-host — {}",
        bundle.file_stem().unwrap_or_default().to_string_lossy()
    );

    // nice-plug editors embed via X11 on Linux (no Wayland surface support
    // in baseview) — force the X11 backend so the parent handle is Xlib.
    #[cfg(target_os = "linux")]
    let event_loop = {
        use winit::platform::x11::EventLoopBuilderExtX11;
        EventLoop::builder().with_x11().build()?
    };
    #[cfg(not(target_os = "linux"))]
    let event_loop = EventLoop::new()?;

    let mut app = HostApp {
        bundle,
        plugin_index,
        title,
        plugin: None,
        window: None,
        next_pump: Instant::now(),
    };
    event_loop.run_app(&mut app)?;
    Ok(())
}
