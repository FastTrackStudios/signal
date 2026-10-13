//! The expression editor as a desktop app, served.
//!
//! ```sh
//! just ee-serve
//! # = dx serve -p expression-editor-standalone --example serve \
//! #       --platform desktop --renderer native
//! ```
//!
//! The window comes from `dioxus_native::launch_cfg` — Blitz → Vello →
//! winit, which is dioxus's own documented desktop path and what
//! `eq-standalone` already uses. Not `nice-plug-dioxus`: that opens a
//! *plugin editor* window through baseview, which is the right host for
//! a VST3/CLAP editor and the wrong one for an application that is not a
//! plugin. The document model and the components are identical either
//! way — only the windowing differs — so this keeps rendering parity
//! with the plugin without pretending to be one.
//!
//! Not `dioxus::desktop` either: that is WebKit/WRY, which would render
//! the surface through a completely different engine from the one the
//! plugin and the REAPER panel use.
//!
//! What makes it *served* rather than one-shot is that `dx` watches the
//! tree and hot-reloads `rsx!` edits into the running window, and the
//! file being edited is chosen in the app rather than on the command
//! line. Point it at material with `EXPRESSION_EDITOR_LIBRARY`; it scans
//! the working directory otherwise.

use daw::standalone::Standalone;
use dioxus::prelude::*;
use dioxus_native::{Config, LogicalSize, WindowAttributes, launch_cfg};
use expression_editor_core::Viewport;
use expression_editor_standalone::cli::ArgsError;
use expression_editor_standalone::library::Entry;
use expression_editor_standalone::{Args, Runner, Target, library, stage};
use expression_editor_ui::{ExpressionEditor, theme};

fn main() {
    // Arguments are optional here, unlike `--example editor`: the whole
    // point is that the window can open on nothing and be told what to
    // load afterwards. A bad argument is still worth reporting rather
    // than silently ignoring.
    let args = match Args::from_env() {
        Ok(a) => Some(a),
        Err(e @ (ArgsError::Help | ArgsError::List)) => {
            print!("{e}");
            return;
        }
        Err(e) => {
            eprintln!("{e}\n\nopening empty — pick a file in the window");
            None
        }
    };

    let (width, height) = args
        .as_ref()
        .map(|a| (a.width, a.height))
        .unwrap_or((1200, 760));

    // Staging is the same hand-off `App` uses, so an argument opens the
    // window on that document and the chooser takes over from there.
    let mut title = String::from("Expression editor");
    let mut daw = None;
    if let Some(args) = &args {
        match Runner::open(&args.source, &args.target, args.viewport(), args.mode) {
            Ok(runner) => {
                println!(
                    "{} — {} notes, mode {}",
                    runner.label,
                    runner.loaded.editor().doc.notes.len(),
                    runner.loaded.editor().mode.label()
                );
                title = format!("Expression editor — {}", runner.label);
                daw = runner.daw;
                stage(runner.loaded.into_editor());
            }
            Err(e) => eprintln!("{e}\n\nopening empty — pick a file in the window"),
        }
    }

    let root = library::root();
    println!(
        "library: {} ({} openable)",
        root.display(),
        library::scan(&root).len()
    );

    // Outlives the window for the same reason `--example editor` holds
    // it: the session's location is only meaningful against the backend
    // it was read from.
    let _daw = daw;

    let window = WindowAttributes::default()
        .with_title(title)
        .with_surface_size(LogicalSize::new(width as f64, height as f64));
    launch_cfg(
        DevApp,
        vec![],
        vec![Box::new(Config::new().with_window_attributes(window))],
    );
}

// ── the runner's root component ─────────────────────────────────────

/// Hand the editor the whole window.
///
/// The runner has no chrome of its own left to subtract: its chooser
/// lives inside the editor's panel rather than in a bar above it. What
/// the editor draws around the roll is the editor's business, and
/// `sizing::Chrome` accounts for it — a host that tried to subtract the
/// toolbar too would be the second source of truth all over again.
fn report_space(window_w: f64, window_h: f64) {
    expression_editor_ui::available_space(window_w, window_h.max(1.0));
}

/// The runner's window: a chooser, and the editor under it.
#[component]
pub fn DevApp() -> Element {
    // The staged document, exactly as `App` takes it, so `dx serve` with
    // a file argument still opens on that file.
    let editor = use_signal(|| {
        expression_editor_standalone::app::take_staged().unwrap_or_else(|| {
            expression_editor_ui::demo::editor(
                expression_editor_ui::demo::Scene::Phrase,
                viewport(),
            )
        })
    });

    // The backend behind the current document. Held for as long as the
    // document is: a session's location is only meaningful against the
    // `Standalone` it was read from, so dropping this on the next load
    // would strand any write-back. Replaced, not accumulated.
    let mut backend = use_signal(|| None::<Standalone>);

    // Jobs first, then the fixtures, then whatever is on disk.
    //
    // The scenes are named after what they *demonstrate* — "Q zones",
    // "Channel conflict", "Guitar Pro import" — which is right for the
    // screenshot suite and wrong for a person: none of them is a thing
    // anyone sets out to do. The workflows are that list, and they come
    // first because they are the answer to "what am I opening this for".
    // The fixtures stay because they are the coverage.
    let entries = use_signal(|| {
        let mut all = library::workflows();
        all.extend(library::scenes());
        all.extend(library::scan(&library::root()));
        all
    });
    let mut status = use_signal(|| String::from("ready"));
    let mut current = use_signal(|| String::new());
    // Whether the chooser's list is showing.
    let mut picking = use_signal(|| false);

    // The editor measures its canvas when told to, because
    // dioxus-native delivers no element resize event. winit does report
    // the *window*, so the runner is what turns a drag of the window
    // edge into "your space changed" — the desktop equivalent of the
    // REAPER panel's dock callback.
    let window = dioxus_native::use_window();

    // The size we already have, at mount. `SurfaceResized` fires when
    // the size *changes*, so a window that opens at its final size and
    // is never dragged may never report one — and the editor would keep
    // the viewport its document was built with forever.
    {
        let window = window.clone();
        use_hook(move || {
            let scale = window.scale_factor();
            let size = window.surface_size();
            report_space(size.width as f64 / scale, size.height as f64 / scale);
        });
    }

    dioxus_native::use_window_event(move |event, _| {
        if let dioxus_native::winit::event::WindowEvent::SurfaceResized(size) = event {
            // Physical pixels from winit, CSS pixels for the editor.
            let scale = window.scale_factor();
            report_space(size.width as f64 / scale, size.height as f64 / scale);
        }
    });

    let mut open = move |entry: Entry| {
        let source = match entry.source() {
            Ok(s) => s,
            Err(e) => {
                status.set(format!("{}: {e}", entry.label));
                return;
            }
        };
        match Runner::open(&source, &Target::default(), viewport(), None) {
            Ok(runner) => {
                let notes = runner.loaded.editor().doc.notes.len();
                let mode = runner.loaded.editor().mode.label();
                status.set(format!("{} — {notes} notes, {mode}", runner.label));
                current.set(entry.arg.clone());
                backend.set(runner.daw);
                editor.clone().set(runner.loaded.into_editor());
            }
            // A file that will not open is the single most likely thing
            // to happen here, and it must not take the window with it —
            // the point of the runner is to still be up afterwards.
            Err(e) => status.set(format!("{}: {e}", entry.label)),
        }
    };

    // What to open, and what just happened. Built here and handed to the
    // editor's panel slot below.
    //
    // A custom popup rather than a `<select>`: Blitz treats `select` as a
    // focusable form control but implements no dropdown for it, so a
    // native one renders as a dead box.
    let chooser = rsx! {
            div {
                style: format!(
                    "display: flex; flex-direction: column; align-items: stretch; \
                     gap: 6px; padding: 8px 10px; position: relative; \
                     border-bottom: 1px solid {};",
                    theme::PANEL_BORDER,
                ),

                button {
                    "data-testid": "chooser",
                    style: format!(
                        "flex: none; height: 20px; width: 100%; padding: 0 8px; \
                         display: flex; align-items: center; justify-content: space-between; \
                         gap: 8px; font-size: 10px; border-radius: 4px; cursor: pointer; \
                         border: 1px solid {}; background: {}; color: {};",
                        theme::PANEL_BORDER, theme::CONTROL, theme::TEXT,
                    ),
                    onclick: move |_| {
                        let now = picking();
                        picking.set(!now);
                    },
                    span {
                        if current().is_empty() { "Open…" } else { "{current()}" }
                    }
                    span { style: "color: {theme::TEXT_DIM};", "▾" }
                }

                // The runner's stdout is not visible under `dx serve`, so
                // the line `--example editor` prints goes here — under
                // the chooser in the panel, where it costs the roll
                // nothing.
                span {
                    "data-testid": "status",
                    style: "font-size: 9px; color: {theme::TEXT_DIM}; \
                            overflow: hidden; word-break: break-word;",
                    "{status()}"
                }

                if picking() {
                    div {
                        "data-testid": "chooser-list",
                        style: format!(
                            "position: absolute; right: 10px; left: 10px; top: 34px; \
                             z-index: 50; max-height: 70vh; overflow-y: auto; \
                             padding: 4px 0; border-radius: 6px; \
                             border: 1px solid {}; background: {}; \
                             box-shadow: 0 8px 28px rgba(0,0,0,0.5);",
                            theme::PANEL_BORDER, theme::PANEL,
                        ),
                        for (i, entry) in entries().into_iter().enumerate() {
                        div {
                            // Keyed by *kind and* arg. The args are not
                            // unique across kinds — `drums` and `guitar`
                            // are both a job and a fixture — and a
                            // duplicate key is not a warning here, it is
                            // `invalid key` and a dead window.
                            key: "{entry.kind.label()}:{entry.arg}",
                            // A caption whenever the kind changes, so the
                            // jobs read as a group rather than as the
                            // first ten of a long list.
                            if i == 0 || entries()[i - 1].kind != entry.kind {
                                div {
                                    style: "font-size: 9px; letter-spacing: 0.08em; \
                                            text-transform: uppercase; \
                                            color: {theme::TEXT_DIM}; padding: 6px 10px 2px;",
                                    "{entry.kind.label()}"
                                }
                            }
                            button {
                                "data-testid": "open-{entry.arg}",
                                title: "{entry.arg}",
                                style: format!(
                                    "display: block; width: 100%; text-align: left; \
                                     border: none; padding: 4px 10px; font-size: 11px; \
                                     cursor: pointer; background: {}; color: {};",
                                    if current() == entry.arg { theme::CONTROL_ACTIVE } else { "transparent" },
                                    theme::TEXT,
                                ),
                                onclick: {
                                    let entry = entry.clone();
                                    move |_| {
                                        open(entry.clone());
                                        picking.set(false);
                                    }
                                },
                                "{entry.label}"
                            }
                        }
                        }
                    }
                }
            }

    };

    rsx! {
        style {
            "html, body {{ width: 100%; height: 100%; margin: 0; padding: 0; \
              overflow: hidden; background: {theme::BG}; }}"
        }
        div {
            style: "width: 100vw; height: 100vh; display: flex; flex-direction: column;",

            div {
                style: "flex: 1; min-height: 0;",
                // The chooser goes *inside* the editor's panel. A bar of
                // its own above the roll is vertical space, which is the
                // scarcest thing on this surface; the panel down the
                // right has room to spare and is already where
                // session-scoped choices live.
                ExpressionEditor { editor, panel_top: chooser }
            }
        }
    }
}

/// The viewport a freshly loaded document starts at.
///
/// The *current* window, not the opening one. This used to be a constant,
/// so every scene or file opened reset the surface to 1200x760 however
/// wide the window had been dragged — the roll snapped back to its
/// opening aspect on each load.
fn viewport() -> Viewport {
    expression_editor_ui::current_viewport(expression_editor_ui::viewport_in(1200.0, 760.0))
}
