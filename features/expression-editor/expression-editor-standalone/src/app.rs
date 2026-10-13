//! The root component, and the hand-off that gets a document into it.
//!
//! `open_standalone_with_state` takes a bare `fn() -> Element`, and the
//! REAPER panel mounts a component with no props for the same reason:
//! a root that needed props would be a root only one host could build.
//! So the loaded document is *staged* before the window opens and the
//! component takes it on mount.
//!
//! Staging is a take, not a read: the document moves into the
//! component's signal and the slot is left empty. A component that
//! re-mounted would otherwise silently reset the user's edits back to
//! the loaded state.

use std::sync::Mutex;

use dioxus::prelude::*;
use expression_editor_core::{Editor, ExpressionDoc, TimeBase, Viewport};
use expression_editor_ui::{ExpressionEditor, theme};

use crate::drum_host::SharedDrumHost;

/// The document waiting for a window.
static STAGED: Mutex<Option<Editor>> = Mutex::new(None);

/// The drum host waiting beside it, when the window is a drum
/// workspace. Staged the same way and for the same reason: the root
/// takes no props.
static STAGED_HOST: Mutex<Option<SharedDrumHost>> = Mutex::new(None);

/// Hand a loaded document to the next [`App`] that mounts.
pub fn stage(editor: Editor) {
    *STAGED.lock().unwrap() = Some(editor);
}

/// Stage a drum workspace: the document *and* its write half, so the
/// panel's Apply and the slip drag land on the daw.
// r[impl drums.quantize.apply]
pub fn stage_with_host(editor: Editor, host: SharedDrumHost) {
    *STAGED.lock().unwrap() = Some(editor);
    *STAGED_HOST.lock().unwrap() = Some(host);
}

/// Take the staged document, if there is one.
pub fn take_staged() -> Option<Editor> {
    STAGED.lock().unwrap().take()
}

/// Take the staged drum host, if the staged document came with one.
pub fn take_staged_host() -> Option<SharedDrumHost> {
    STAGED_HOST.lock().unwrap().take()
}

/// An empty document, for the case where nothing was staged.
///
/// Better than panicking: a window that opens empty is diagnosable, and
/// the runner has already printed what it loaded.
/// An empty document, for a window that has opened before its material
/// has finished loading.
pub fn fallback() -> Editor {
    let doc = ExpressionDoc::new(TimeBase::Ppq { ppq: 960.0 }, 0.0, 960.0 * 8.0);
    Editor::new(doc, Viewport::new(1100.0, 520.0))
}

pub(crate) use expression_editor_ui::host::HostCallbacks;

pub(crate) fn host_callbacks(
    editor: Signal<Editor>,
    host: Option<SharedDrumHost>,
    bins: Signal<Vec<expression_editor_ui::quantize_panel::Bin>>,
    previews: Signal<Vec<expression_editor_ui::quantize_panel::HitPreview>>,
    fills: Signal<Vec<(f64, f64)>>,
) -> HostCallbacks {
    let mut callbacks =
        expression_editor_ui::host::use_drum_callbacks(editor, host.clone(), bins, previews, fills);
    callbacks.on_save = host.map(|host| {
        EventHandler::new(move |_| match host.save() {
            Ok(path) => tracing::info!(path = %path.display(), "saved project copy"),
            Err(error) => tracing::warn!(%error, "save failed"),
        })
    });
    callbacks
}

/// The whole window: the editor, and nothing else.
///
/// No arrangement view, no mixer, no transport. The editor is the
/// product being built here; the composed window with the arrange view
/// and the mixer is [`crate::workstation`].
#[component]
pub fn App() -> Element {
    let editor = use_signal(|| take_staged().unwrap_or_else(fallback));
    let host = use_signal(take_staged_host);
    // The panel's data channel: bins and previews recomputed by the
    // host on every control change. Empty without a host — the panel
    // is then purely visual, which is what a demo scene wants.
    let bins = use_signal(Vec::new);
    let previews = use_signal(Vec::new);
    // The fills, as (start, end) seconds, for the bands the stack draws.
    let fills = use_signal(Vec::<(f64, f64)>::new);
    let HostCallbacks {
        error,
        on_change,
        on_apply,
        on_save,
        on_hit,
        on_undo,
        on_redo,
    } = host_callbacks(editor, host.read().clone(), bins, previews, fills);
    rsx! {
        style {
            // Blitz sizes the root from these; without them the editor
            // lays out at its intrinsic height and the status bar
            // leaves the frame.
            "html, body {{ width: 100%; height: 100%; margin: 0; padding: 0; \
              overflow: hidden; background: {theme::BG}; }}"
        }
        div {
            // `vh`/`vw` rather than `100%`: percentage heights resolve
            // against the parent, and a headless Blitz mount gives
            // `body` no resolved height, so the editor would lay out to
            // its content and leave a band of background under the
            // status bar in every screenshot. The viewport units are
            // the window either way.
            style: "width: 100vw; height: 100vh;",
            ExpressionEditor {
                editor,
                quantize_bins: bins(),
                quantize_previews: previews(),
                on_quantize_change: on_change,
                on_quantize_apply: on_apply,
                on_hit,
                on_save,
                on_undo,
                on_redo,
                host_error: error.and_then(|error| error()),
                fills: fills(),
            }
        }
    }
}
