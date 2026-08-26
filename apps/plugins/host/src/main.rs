//! fts-clap-host — open any CLAP plugin's embedded GUI in a real window.
//!
//! This is the REAPER-parity iteration loop for FTS plugin editors: the exact
//! embedded-GUI path a DAW drives (`gui.create` → `set_parent` → `show`, with
//! `on_main_thread` pumped at UI rate), so what you see here is what REAPER
//! shows — unlike eq-standalone's dioxus-native shell, which renders through
//! a different windowing stack.
//!
//! The parent window is a **baseview** window (crates.io 0.2), not winit. On
//! Linux that matters for input: winit selects XInput2 on its window, which
//! starves the plugin's core-X11-only baseview child of pointer/key events —
//! REAPER's plain X11 parent is what makes input work there, and this shell
//! reproduces it. On macOS baseview gives an NSView parent, the same shape
//! REAPER embeds into.
//!
//! Runs on Linux and macOS. Bare plugin names resolve against whichever
//! per-user CLAP directory this platform uses (`~/.clap`, or
//! `~/Library/Audio/Plug-Ins/CLAP` on macOS) and then `target/bundled`.
//!
//! ```sh
//! fts-clap-host "FTS EQ"                    # resolves the installed bundle
//! fts-clap-host "FTS Guide" --note-names    # print piano-roll key labels, no GUI
//! fts-clap-host --probe "FTS Comp"          # headless: what a DAW sees, no window
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
use daw_standalone::audio_engine::plugin_host::{
    gui_api_uses_logical_size, ClapHost, LoadedClapPlugin,
};
use raw_window_handle::HasWindowHandle;

/// Resize this window to a size the PLUGIN reported.
///
/// CLAP sizes are in the GUI API's unit — logical pixels on macOS (Cocoa),
/// physical on X11/Win32 — so handing them straight to `PhysicalSize` is only
/// right on Linux and Windows. On a 2x Retina Mac it produced an editor at
/// half the size it asked for.
fn resize_to_plugin_size(ctx: &baseview::WindowContext, width: u32, height: u32) {
    if gui_api_uses_logical_size() {
        ctx.resize(baseview::dpi::LogicalSize::new(width as f64, height as f64));
    } else {
        ctx.resize(baseview::dpi::PhysicalSize::new(
            width as f64,
            height as f64,
        ));
    }
}

/// The size to hand BACK to a plugin for a window that is now `size`, in
/// whichever unit that plugin's GUI API speaks. The mirror of the above: a
/// host reporting physical pixels to a Cocoa plugin tells it it is twice as
/// big as it is.
fn plugin_size_of(size: &baseview::WindowSize) -> (u32, u32) {
    if gui_api_uses_logical_size() {
        (size.logical.width as u32, size.logical.height as u32)
    } else {
        (size.physical.width, size.physical.height)
    }
}

/// Make the host window user-resizable.
///
/// baseview builds its NSWindow with `Titled | Closable | Miniaturizable` and
/// no `Resizable` bit, so on macOS the frame cannot be dragged to a new size —
/// which makes it useless for the thing this host exists to check, since a
/// plugin's response to being resized is most of its editor's layout
/// behaviour. X11 imposes no such restriction, so Linux windows already
/// resize and this is macOS-only.
///
/// Adding the bit after the fact is the smallest fix available: the
/// alternative is patching baseview, which would also change every plugin's
/// own window.
#[cfg(target_os = "macos")]
fn make_window_resizable(handle: &raw_window_handle::RawWindowHandle) {
    use objc2_app_kit::{NSView, NSWindowStyleMask};

    let raw_window_handle::RawWindowHandle::AppKit(appkit) = handle else {
        return;
    };
    // SAFETY: baseview handed us this NSView pointer for the window it just
    // created and keeps it alive for the window's lifetime; we are on the main
    // thread (the build closure runs there), which is where AppKit requires
    // window mutation.
    let view: &NSView = unsafe { appkit.ns_view.cast().as_ref() };
    let Some(window) = view.window() else {
        eprintln!("[host] view has no window yet — not made resizable");
        return;
    };
    let mask = window.styleMask() | NSWindowStyleMask::Resizable;
    window.setStyleMask(mask);
}

#[cfg(not(target_os = "macos"))]
fn make_window_resizable(_handle: &raw_window_handle::RawWindowHandle) {
    // X11 windows are resizable unless the client sets size hints forbidding
    // it, and baseview sets none.
}

/// Turn `"FTS EQ"` into a bundle path, searching the places CLAP plugins are
/// actually installed on this platform, plus the repo's own build output.
///
/// An existing path is taken as-is. Otherwise the name is resolved against the
/// per-user plugin dir — `~/.clap` on Linux, `~/Library/Audio/Plug-Ins/CLAP`
/// on macOS, which is why bare names never resolved there — then the system
/// dir, then `target/bundled` so a freshly built plugin can be opened without
/// installing it first.
fn resolve_bundle(arg: &str) -> PathBuf {
    let direct = PathBuf::from(arg);
    if direct.exists() {
        return direct;
    }
    let file = format!("{}.clap", arg.trim_end_matches(".clap"));
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        if cfg!(target_os = "macos") {
            candidates.push(home.join("Library/Audio/Plug-Ins/CLAP"));
        } else {
            candidates.push(home.join(".clap"));
        }
    }
    if cfg!(target_os = "macos") {
        candidates.push(PathBuf::from("/Library/Audio/Plug-Ins/CLAP"));
    } else {
        candidates.push(PathBuf::from("/usr/lib/clap"));
    }
    candidates.push(PathBuf::from("target/bundled"));
    for dir in candidates {
        let named = dir.join(&file);
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
        // work every frame (~60 Hz) so param/GUI tasks keep flowing, and drain
        // any resize the plugin asked for. FTS Comp changes its own editor size
        // when you switch profiles — a 4:1 rack face and a tall control surface
        // are different shapes — and a host that ignores that leaves the new
        // face rendering inside the old frame.
        //
        // The borrow is released before the window is touched. On macOS
        // `resize()` synchronously calls back into `resized()` on this same
        // handler, which borrows the plugin again — holding the borrow across
        // it panicked with "RefCell already borrowed" the moment any plugin
        // requested a resize. X11 hid this: there the resize is a request and
        // the ConfigureNotify arrives on a later turn of the loop, so the
        // re-entrancy never happened.
        let pending_resize = {
            let mut guard = self.plugin.borrow_mut();
            let Some(plugin) = guard.as_mut() else {
                return;
            };
            plugin.pump_main_thread();
            // GUI-only host: drain GUI-issued param gestures every frame —
            // the process() loop that normally applies them doesn't exist
            // here, and without this the editor's edits never take effect.
            plugin.flush_params();
            plugin.take_requested_resize()
        };

        if let Some((w, h)) = pending_resize {
            if std::env::var_os("FTS_HOST_TRACE").is_some() {
                eprintln!("[host] plugin requested resize: {w}x{h}");
            }
            // May re-enter `resized()` below, which is why nothing is borrowed
            // here. That callback tells the plugin its new size; on platforms
            // where it does not fire, the explicit call after it does.
            resize_to_plugin_size(&self.window, w, h);
            if let Some(plugin) = self.plugin.borrow_mut().as_mut() {
                plugin.gui_set_size(w, h);
            }
        }
    }

    fn resized(&self, new_size: baseview::WindowSize) {
        // `try_borrow_mut`, not `borrow_mut`: this can be re-entered from
        // inside a window resize we ourselves initiated. Skipping is correct —
        // whoever holds the borrow is mid-resize and applies the size itself.
        let Ok(mut guard) = self.plugin.try_borrow_mut() else {
            return;
        };
        if let Some(plugin) = guard.as_mut() {
            let (w, h) = plugin_size_of(&new_size);
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
    let mut plugin_index = 0usize;
    let mut note_names = false;
    let mut probe = false;
    // The bundle is the first non-flag argument, so flags may come before or
    // after it — `--probe "FTS EQ"` and `"FTS EQ" --probe` both work.
    let mut bundle_arg: Option<String> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--index" => {
                plugin_index = args.next().and_then(|v| v.parse().ok()).unwrap_or_default();
            }
            "--note-names" => note_names = true,
            "--probe" => probe = true,
            other if bundle_arg.is_none() && !other.starts_with("--") => {
                bundle_arg = Some(other.to_string());
            }
            _ => {}
        }
    }
    let Some(bundle_arg) = bundle_arg else {
        eprintln!(
            "usage: fts-clap-host <bundle.clap | plugin name> [--index N] [--note-names] [--probe]"
        );
        std::process::exit(2);
    };

    let bundle = resolve_bundle(&bundle_arg);
    eyre::ensure!(bundle.exists(), "no such bundle: {}", bundle.display());
    let title = format!(
        "fts-clap-host — {}",
        bundle.file_stem().unwrap_or_default().to_string_lossy()
    );

    // Headless probe: instantiate and report what a DAW would find, without
    // ever opening a window. This is the sweep mode — it runs over SSH and in
    // CI, on machines with no display, which the embedded path cannot.
    if probe {
        let mut plugin = ClapHost::default().load(&bundle, plugin_index)?;
        let (name, id) = {
            let d = plugin.descriptor();
            (d.name.clone(), d.id.clone())
        };
        let (audio_in, audio_out) = plugin.audio_port_count();
        let (note_in, note_out) = plugin.note_port_count();
        let params = plugin.params().len();
        let unit = if gui_api_uses_logical_size() {
            "logical"
        } else {
            "physical"
        };
        let gui = match plugin.probe_gui_size() {
            Ok((w, h)) => format!("{w}x{h} {unit}"),
            Err(e) => format!("none ({e:?})"),
        };
        println!(
            "{name}\t{id}\taudio={audio_in}in/{audio_out}out\tnote={note_in}in/{note_out}out\tparams={params}\tgui={gui}"
        );
        return Ok(());
    }

    // Inspection mode: instantiate, dump the `note-name` extension, exit.
    // No window, so this answers "does the plugin actually serve its key
    // labels" without a DAW — the question you otherwise can't separate
    // from "is the host configured to show them".
    if note_names {
        let mut plugin = ClapHost::default().load(&bundle, plugin_index)?;
        let names = plugin.note_names();
        println!("{} ({})", plugin.descriptor().name, plugin.descriptor().id);
        if names.is_empty() {
            println!("  no note names (plugin does not implement clap.note-name)");
            return Ok(());
        }
        println!("  {} note names:", names.len());
        for n in names {
            let wildcard = |v: i32| {
                if v < 0 {
                    "*".to_string()
                } else {
                    v.to_string()
                }
            };
            println!(
                "    key {:>3}  ch {:>3}  port {:>3}   {}",
                wildcard(n.key),
                wildcard(n.channel),
                wildcard(n.port),
                n.name
            );
        }
        return Ok(());
    }

    // open_blocking runs the build closure and the event loop on THIS
    // (main) thread — where CLAP requires all main-thread calls to happen.
    let mut options = WindowOpenOptions::default();
    options.title = title;
    options.size = baseview::dpi::LogicalSize::new(800.0, 500.0).into();
    // FTS_HOST_SCALE forces a DPI scale instead of taking the display's.
    // Scale is the one condition that cannot be reproduced by moving the
    // window: an SSH session, a CI box, and a non-Retina panel all report 1,
    // so every scale-dependent bug is invisible there while being the normal
    // case on the Mac laptops these plugins run on.
    //
    // LINUX AND WINDOWS ONLY. baseview's macOS backend takes the scale from
    // the NSWindow's `backingScaleFactor()` and never reads `options.scale`
    // (platform/macos/window.rs), so this cannot fake a Retina session on the
    // platform that most needs it — a macOS scale-2 repro has to run on an
    // actual Retina display, not over SSH.
    options.scale = match std::env::var("FTS_HOST_SCALE")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
    {
        Some(scale) if scale > 0.0 => baseview::WindowScalePolicy::ScaleFactor(scale),
        _ => baseview::WindowScalePolicy::SystemScaleFactor,
    };
    Window::open_blocking(options, move |ctx| {
        let mut plugin = ClapHost::default()
            .load(&bundle, plugin_index)
            .unwrap_or_else(|e| panic!("loading {}: {e:?}", bundle.display()));
        eprintln!(
            "hosting {} ({}) — embedded GUI",
            plugin.descriptor().name,
            plugin.descriptor().id
        );
        // The scale factor is the whole story behind macOS sizing: the
        // plugin reports its editor size in logical pixels there, so on a
        // 2x display the window is twice the pixel size of the same
        // plugin on Linux. Print it so a wrong-looking window can be
        // read off the log instead of guessed at.
        {
            let size = ctx.size();
            eprintln!(
                "[host] window: {}x{} logical, {}x{} physical, scale {} — plugin sizes are {}",
                size.logical.width,
                size.logical.height,
                size.physical.width,
                size.physical.height,
                size.scale_factor,
                if gui_api_uses_logical_size() {
                    "LOGICAL"
                } else {
                    "PHYSICAL"
                },
            );
        }

        let raw = ctx.window_handle().expect("host window handle").as_raw();
        // Let the frame be dragged to a new size — the plugin's response
        // to that is most of what this host is for.
        make_window_resizable(&raw);
        // Size the window before the editor is parented into it, the way a
        // DAW does — so the editor's first frame is at its real size and
        // no resize follows to paper over a missing first paint.
        match plugin.open_gui_embedded(raw, |w, h| {
            resize_to_plugin_size(&ctx, w, h);
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
    });
    Ok(())
}
