//! fts-clap-host — open any CLAP plugin's embedded GUI in a real window.
//!
//! This is the REAPER-parity iteration loop for FTS plugin editors: the exact
//! embedded-GUI path a DAW drives (`gui.create` → `set_parent` → `show`, with
//! `on_main_thread` pumped at UI rate), so what you see here is what REAPER
//! shows — unlike eq-standalone's dioxus-native shell, which renders through
//! a different windowing stack.
//!
//! The parent window is a **baseview** window (crates.io 0.2, core-X11), not
//! winit: winit selects XInput2 on its window, which starves the plugin's
//! core-X11-only baseview child of pointer/key events — REAPER's plain X11
//! parent is what makes input work there, and this shell reproduces it.
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

use std::cell::RefCell;
use std::path::PathBuf;

use baseview::{Event, EventStatus, Window, WindowEvent, WindowHandler, WindowOpenOptions};
use daw_standalone::audio_engine::plugin_host::{ClapHost, LoadedClapPlugin};
use raw_window_handle::HasWindowHandle;

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

struct HostHandler {
    /// `WindowHandler` methods take `&self` in baseview 0.2 — the plugin
    /// lives behind a RefCell. Everything runs on the one GUI thread.
    plugin: RefCell<Option<LoadedClapPlugin>>,
    /// This host's own window, so a resize the plugin asks for can be applied
    /// to the frame around it. `WindowContext` is cheap to clone and is the
    /// only handle a `WindowHandler` gets to the window in baseview 0.2.
    window: baseview::WindowContext,
}

impl WindowHandler for HostHandler {
    fn on_frame(&self) {
        // The DAW-timer equivalent: run the plugin's deferred main-thread
        // work every frame (~60 Hz) so param/GUI tasks keep flowing.
        if let Some(plugin) = self.plugin.borrow_mut().as_mut() {
            plugin.pump_main_thread();
            // GUI-only host: drain GUI-issued param gestures every frame —
            // the process() loop that normally applies them doesn't exist
            // here, and without this the editor's edits never take effect.
            plugin.flush_params();

            // …and drain resizes the plugin asked for. FTS Comp changes its
            // own editor size when you switch profiles — a 4:1 rack face and
            // a tall control surface are different shapes — and a host that
            // does not do this leaves the new face rendering inside the old
            // frame, which is exactly what it looked like.
            //
            // Both halves are needed: resize the frame, then tell the plugin
            // the size it now has.
            if let Some((w, h)) = plugin.take_requested_resize() {
                if std::env::var_os("FTS_HOST_TRACE").is_some() {
                    eprintln!("[host] plugin requested resize: {w}x{h}");
                }
                self.window
                    .resize(baseview::dpi::PhysicalSize::new(w as f64, h as f64));
                plugin.gui_set_size(w, h);
            }
        }
    }

    fn resized(&self, new_size: baseview::WindowSize) {
        if let Some(plugin) = self.plugin.borrow_mut().as_mut() {
            let (w, h) = (new_size.physical.width, new_size.physical.height);
            if w > 0 && h > 0 {
                plugin.gui_set_size(w, h);
            }
        }
    }

    fn on_event(&self, event: Event) -> EventStatus {
        // Diagnostic (FTS_HOST_TRACE=1): any mouse/key event that reaches the
        // PARENT window was NOT delivered to the plugin's child window —
        // routing telemetry for embedded-input debugging. Notably, keyboard
        // events currently land HERE (the child never gets focus) — keyboard
        // forwarding is a known gap.
        if std::env::var_os("FTS_HOST_TRACE").is_some() {
            match &event {
                Event::Mouse(m) => eprintln!("[parent] mouse: {m:?}"),
                Event::Keyboard(k) => eprintln!("[parent] key: {k:?}"),
                _ => {}
            }
        }
        if let Event::Window(WindowEvent::WillClose) = event {
            if let Some(plugin) = self.plugin.borrow_mut().as_mut() {
                plugin.close_gui();
            }
        }
        EventStatus::Ignored
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

    // open_blocking runs the build closure and the event loop on THIS
    // (main) thread — where CLAP requires all main-thread calls to happen.
    let mut options = WindowOpenOptions::default();
    options.title = title;
    options.size = baseview::dpi::LogicalSize::new(800.0, 500.0).into();
    options.scale = baseview::WindowScalePolicy::SystemScaleFactor;
    Window::open_blocking(
        options,
        move |ctx| {
            let mut plugin = ClapHost::default()
                .load(&bundle, plugin_index)
                .unwrap_or_else(|e| panic!("loading {}: {e:?}", bundle.display()));
            eprintln!(
                "hosting {} ({}) — embedded GUI",
                plugin.descriptor().name,
                plugin.descriptor().id
            );

            let raw = ctx
                .window_handle()
                .expect("host window handle")
                .as_raw();
            // Size the window before the editor is parented into it, the way a
            // DAW does — so the editor's first frame is at its real size and
            // no resize follows to paper over a missing first paint.
            match plugin.open_gui_embedded(raw, |w, h| {
                ctx.resize(baseview::dpi::PhysicalSize::new(w, h));
            }) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("error: plugin GUI failed to embed: {e:?}");
                    ctx.request_close();
                }
            }

            HostHandler {
                plugin: RefCell::new(Some(plugin)),
                window: ctx.clone(),
            }
        },
    );
    Ok(())
}
