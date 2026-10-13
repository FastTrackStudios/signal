//! The expression editor as a REAPER module.
//!
//! A dockable panel plus the actions that put a take into it and write
//! one back. The panel holds a [`Session`], which is what keeps "open
//! the editor on the selected item" and "save it" talking about the
//! same take.
//!
//! Everything the panel does goes through the `daw` facade, so the
//! logic is identical to the standalone path and is tested there. What
//! *this* crate adds — and what its REAPER test covers — is the part
//! that only exists inside REAPER: the panel registering, docking,
//! opening on a selection, and its edits reaching a real take.

use std::sync::{Mutex, OnceLock};

use daw::module::{
    ActionDef, DawModule, DockPosition, ModuleContext, PanelComponent, PanelDef, PanelRenderer,
};
use daw::service::ProjectContext;
use dioxus::prelude::*;
use expression_editor_audio::{AudioSession, TakeConfig};
use expression_editor_core::Viewport;
// The MPE bend range a take is loaded with. Taken from the adapter
// rather than restated here, so the range the editor reads at is the
// same one the MPE fixture declares on the wire — a mismatch rescales
// every pitch curve silently.
use expression_editor_daw::{Session, DEFAULT_BEND_RANGE};
use expression_editor_ui::ExpressionEditor;

pub const PANEL_ID: &str = "FTS_EXPRESSION_EDITOR";

/// What the panel currently holds.
///
/// One panel, two kinds of take. Which one is loaded is decided by the
/// item — a MIDI take gets the MPE editor, an audio take gets the
/// Melodyne surface — rather than by a mode the user has to remember to
/// set, because loading a vocal into the MIDI editor produces an empty
/// roll and no explanation.
pub enum Loaded {
    Midi(Box<Session>),
    Audio(Box<AudioSession>),
}

impl Loaded {
    fn editor(&self) -> &expression_editor_core::Editor {
        match self {
            Loaded::Midi(s) => &s.editor,
            Loaded::Audio(s) => &s.editor,
        }
    }

    fn editor_mut(&mut self) -> &mut expression_editor_core::Editor {
        match self {
            Loaded::Midi(s) => &mut s.editor,
            Loaded::Audio(s) => &mut s.editor,
        }
    }

    fn is_dirty(&self) -> bool {
        match self {
            Loaded::Midi(s) => s.is_dirty(),
            Loaded::Audio(s) => s.is_dirty(),
        }
    }
}

/// The panel's session, shared between the actions and the component.
///
/// A global rather than a signal because the actions that load and
/// write are REAPER actions — they run outside any component, and
/// cannot reach a hook's state.
static SESSION: OnceLock<Mutex<Option<Loaded>>> = OnceLock::new();

fn session() -> &'static Mutex<Option<Loaded>> {
    SESSION.get_or_init(|| Mutex::new(None))
}

/// What the header calls the loaded take — the item's label when it has
/// one, else the kind of take. Kept beside the session rather than in it
/// because the label belongs to the *item*, which the sessions
/// deliberately don't model.
static LABEL: OnceLock<Mutex<String>> = OnceLock::new();

fn label() -> &'static Mutex<String> {
    LABEL.get_or_init(|| Mutex::new(String::new()))
}

/// The header label for the loaded take, empty when nothing is loaded.
pub fn loaded_label() -> String {
    label().lock().unwrap().clone()
}

/// The item the session was loaded from, so the open action can tell
/// "e on the same item" (close) from "e on a different one" (switch).
static LOADED_GUID: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn loaded_guid() -> &'static Mutex<Option<String>> {
    LOADED_GUID.get_or_init(|| Mutex::new(None))
}

/// The first selected item's guid, if any.
fn selected_item_guid() -> Option<String> {
    use daw::service::Items;
    daw::reaper::Reaper
        .get_selected_items(ProjectContext::Current)
        .into_iter()
        .next()
        .map(|i| i.guid)
}

/// Load the selected item into the panel.
///
/// Audio is tried first, then MIDI. The order matters only because a
/// take is one or the other, and asking the audio path first means an
/// audio item never falls through to a MIDI reader that would find no
/// notes and report an empty take.
///
/// Returns false when nothing is selected or the item is neither, which
/// the caller should report — an editor opened on nothing looks like a
/// failed load.
pub fn load_selected() -> bool {
    use daw::service::Items;

    let reaper = daw::reaper::Reaper;
    let viewport = Viewport::new(1100.0, 520.0);

    // The item's own identity, resolved up front so both session kinds
    // share it: the label feeds the header (falling back to the kind of
    // take below), the guid feeds the open action's same-item check.
    let item = reaper
        .get_selected_items(ProjectContext::Current)
        .into_iter()
        .next();
    let item_guid = item.as_ref().map(|i| i.guid.clone());
    let item_label = item.and_then(|i| i.label.filter(|l| !l.is_empty()));

    if let Some(s) = AudioSession::load_selected(
        &reaper,
        ProjectContext::Current,
        viewport,
        TakeConfig::default(),
    ) {
        tracing::info!(
            notes = s.editor.doc.notes.len(),
            rate = s.sample_rate(),
            "analysed audio take into editor"
        );
        *label().lock().unwrap() = item_label.unwrap_or_else(|| "Audio take".into());
        *loaded_guid().lock().unwrap() = item_guid;
        *session().lock().unwrap() = Some(Loaded::Audio(Box::new(s)));
        return true;
    }

    match Session::load_selected(
        &reaper,
        ProjectContext::Current,
        DEFAULT_BEND_RANGE,
        viewport,
    ) {
        Some(s) => {
            tracing::info!(
                notes = s.editor.doc.notes.len(),
                "loaded MIDI take into editor"
            );
            *label().lock().unwrap() = item_label.unwrap_or_else(|| "MIDI take".into());
            *loaded_guid().lock().unwrap() = item_guid;
            *session().lock().unwrap() = Some(Loaded::Midi(Box::new(s)));
            true
        }
        None => {
            tracing::warn!("no editable item selected");
            false
        }
    }
}

/// Write the panel's document back to the take it came from.
///
/// MIDI is rewritten in place — the take *is* the document.
///
/// Audio splits by what changed. Timing always goes to the host as
/// stretch markers, losslessly and reversibly. Only a pitch, formant or
/// gain edit renders, and then to a new file beside the original rather
/// than over it, because a recording is the only copy of a performance
/// and the resynthesis is lossy. A take that was only retimed is never
/// resynthesised at all.
pub fn write_back() -> bool {
    let reaper = daw::reaper::Reaper;
    let mut guard = session().lock().unwrap();
    let Some(loaded) = guard.as_mut() else {
        tracing::warn!("nothing loaded");
        return false;
    };
    match loaded {
        Loaded::Midi(s) => {
            // Warn before overwriting, not after: expression on
            // ambiguous notes is dropped on the way out, and the user
            // should hear that while they can still fix it.
            for w in s.warnings() {
                tracing::warn!("{w}");
            }
            let indices = s.write_back(&reaper);
            tracing::info!(notes = indices.len(), "wrote MIDI take");
            true
        }
        Loaded::Audio(s) => match s.write_back(&reaper) {
            Ok(expression_editor_audio::WriteOutcome::Unchanged) => {
                tracing::info!("no edits to write");
                true
            }
            // The good case: the host carries the warp and the
            // recording is untouched.
            Ok(expression_editor_audio::WriteOutcome::Retimed { markers }) => {
                tracing::info!(markers, "retimed take with stretch markers");
                true
            }
            Ok(expression_editor_audio::WriteOutcome::Rendered { path, markers }) => {
                tracing::info!(%path, markers, "rendered audio take");
                true
            }
            Err(e) => {
                tracing::error!("audio write-back failed: {e}");
                false
            }
        },
    }
}

/// State for the live-sync poll: the last document the poll saw, so a
/// write only happens once a gesture has settled, not mid-drag.
struct PollState {
    last_seen: Option<expression_editor_core::ExpressionDoc>,
    stable_ticks: u32,
    /// Last measured canvas-cell size, and how long it has held.
    cell: Option<(f64, f64)>,
    cell_stable: u32,
}

static POLL: OnceLock<Mutex<PollState>> = OnceLock::new();

fn poll_state() -> &'static Mutex<PollState> {
    POLL.get_or_init(|| {
        Mutex::new(PollState {
            last_seen: None,
            stable_ticks: 0,
            cell: None,
            cell_stable: 0,
        })
    })
}

/// How many ticks the measured cell size must hold before the editor is
/// resized to it — one remount at the end of a dock drag, not thirty
/// during it.
const RESIZE_AFTER_STABLE_TICKS: u32 = 3;

/// Match the editor's viewport to the canvas cell's real layout size.
///
/// dioxus-native never delivers element resize events, so the panel
/// measures the cell from the host tick and pushes the size into the
/// session, then remounts so the component re-reads it. The roll svg is
/// sized 1:1 to the viewport, which keeps the mouse exact even while
/// this is catching up; what this poll restores is the roll actually
/// *filling* the panel after a resize.
fn follow_cell_size() {
    use expression_editor_ui::canvas::{GUTTER_W, RULER_H};

    let Some((w, h)) =
        daw::reaper_ui::dock::panel_element_size(PANEL_ID, "data-testid", "canvas-cell")
    else {
        return;
    };
    let mut st = poll_state().lock().unwrap();
    let same = st
        .cell
        .is_some_and(|(cw, ch)| (cw - w).abs() < 0.5 && (ch - h).abs() < 0.5);
    if !same {
        st.cell = Some((w, h));
        st.cell_stable = 0;
        return;
    }
    st.cell_stable += 1;
    if st.cell_stable != RESIZE_AFTER_STABLE_TICKS {
        return;
    }
    drop(st);

    let want = Viewport::new((w - GUTTER_W).max(50.0), (h - RULER_H).max(50.0));
    let resized = with_editor(|ed| {
        if (ed.viewport.w - want.w).abs() > 0.5 || (ed.viewport.h - want.h).abs() > 0.5 {
            ed.resize(want);
            true
        } else {
            false
        }
    })
    .unwrap_or(false);
    if resized {
        daw::reaper_ui::dock::remount_panel(PANEL_ID);
    }
}

/// How many ~30Hz ticks a document must sit unchanged before it is
/// written to the take. ~250ms: short enough to feel live, long enough
/// that a drag in progress is not rendered on every frame — which
/// matters for audio, where a settled pitch edit is a resynthesis.
const WRITE_AFTER_STABLE_TICKS: u32 = 8;

/// The live-sync tick, called from the extension timer (~30Hz).
///
/// The editor has no Load/Write/Reload chrome; this is what replaces
/// it. While the panel is visible:
///
/// - **Selection follow**: selecting a different item in REAPER loads
///   it into the editor (flushing any settled-but-unwritten edits to
///   the old take first). Deselecting everything keeps the current
///   take — an editor that blanked on every arrange-view click would
///   be unusable.
/// - **Auto write-back**: once the document has been stable for
///   `WRITE_AFTER_STABLE_TICKS` (8 polls — the const is private, since
///   the settling delay is this module's business and not API), it is
///   written to the take. MIDI
///   replaces the take's events; audio timing goes out as stretch
///   markers, and a settled pitch edit renders once, not per frame.
pub fn poll() {
    if !daw::reaper_ui::dock::is_panel_visible(PANEL_ID) {
        return;
    }

    follow_cell_size();

    // Follow the selection.
    if let Some(sel) = selected_item_guid() {
        let current = loaded_guid().lock().unwrap().clone();
        if current.as_deref() != Some(sel.as_str()) {
            if is_dirty() {
                write_back();
            }
            if load_selected() {
                daw::reaper_ui::dock::remount_panel(PANEL_ID);
                let mut st = poll_state().lock().unwrap();
                st.last_seen = None;
                st.stable_ticks = 0;
            }
            return;
        }
    }

    // Debounced write-back of settled edits.
    let (dirty, doc) = {
        let guard = session().lock().unwrap();
        match guard.as_ref() {
            Some(s) => (s.is_dirty(), Some(s.editor().doc.clone())),
            None => (false, None),
        }
    };
    let mut st = poll_state().lock().unwrap();
    if !dirty {
        st.last_seen = None;
        st.stable_ticks = 0;
        return;
    }
    if st.last_seen.as_ref() == doc.as_ref() {
        st.stable_ticks += 1;
        if st.stable_ticks >= WRITE_AFTER_STABLE_TICKS {
            st.last_seen = None;
            st.stable_ticks = 0;
            drop(st);
            write_back();
        }
    } else {
        st.last_seen = doc;
        st.stable_ticks = 0;
    }
}

/// Discard local edits and re-read the take.
pub fn reload() -> bool {
    let reaper = daw::reaper::Reaper;
    let mut guard = session().lock().unwrap();
    match guard.as_mut() {
        Some(Loaded::Midi(s)) => {
            s.reload(&reaper);
            true
        }
        // Audio re-reads its own recording rather than the host: the
        // samples have not changed, only the analysis is being redone,
        // and pulling them again would cost a decode for nothing.
        Some(Loaded::Audio(s)) => {
            s.reanalyze(TakeConfig::default());
            true
        }
        None => false,
    }
}

/// Whether the panel has unsaved edits.
pub fn is_dirty() -> bool {
    session()
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|s| s.is_dirty())
}

/// Note count currently loaded — cheap state a test can assert on
/// without reaching into the panel's rendering.
pub fn loaded_note_count() -> usize {
    session()
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.editor().doc.notes.len())
        .unwrap_or(0)
}

pub struct ExpressionEditorModule;

impl DawModule for ExpressionEditorModule {
    fn name(&self) -> &str {
        "expression-editor"
    }

    fn display_name(&self) -> &str {
        "FTS Expression Editor"
    }

    fn actions(&self) -> Vec<ActionDef> {
        vec![
            ActionDef::new(
                "FTS_EXPRESSION_EDITOR_TOGGLE",
                "FTS: Toggle Expression Editor",
                || {
                    daw::reaper_ui::dock::toggle_panel(PANEL_ID);
                },
            )
            .in_menu(),
            ActionDef::new(
                "FTS_EXPRESSION_EDITOR_OPEN",
                "FTS: Open Expression Editor on selected item",
                || {
                    // One key in, same key out. While the panel is
                    // open the live-sync poll already follows the item
                    // selection, so the only job `e` has left on an
                    // open panel is to close it.
                    if daw::reaper_ui::dock::is_panel_visible(PANEL_ID) {
                        daw::reaper_ui::dock::hide_panel(PANEL_ID);
                        return;
                    }
                    // Load before showing so the panel mounts on the
                    // selection; shown even when nothing loads, because
                    // the empty state explains what to do next — more
                    // help than a key that silently does nothing.
                    load_selected();
                    daw::reaper_ui::dock::show_panel(PANEL_ID);
                },
            )
            .in_menu(),
            ActionDef::new(
                "FTS_EXPRESSION_EDITOR_WRITE",
                "FTS: Write Expression Editor back to take",
                || {
                    write_back();
                },
            ),
            ActionDef::new(
                "FTS_EXPRESSION_EDITOR_RELOAD",
                "FTS: Reload Expression Editor from take",
                || {
                    reload();
                },
            ),
            // Write a known gain ride to the selected take's volume
            // envelope. The test binary is a separate process and
            // cannot see this extension's memory, so the only way to
            // prove the take-envelope path works is to make a change
            // REAPER itself then reports back.
            ActionDef::new(
                "FTS_EXPRESSION_EDITOR_TEST_DYNAMICS",
                "FTS: Expression Editor — write test dynamics envelope (test)",
                || {
                    write_test_dynamics();
                },
            ),
            // An edit the integration test can trigger from outside the
            // process. The test binary talks to REAPER over a socket and
            // cannot see this extension's memory, so "did the editor
            // load and edit correctly" is only answerable by making a
            // change that shows up in the take.
            ActionDef::new(
                "FTS_EXPRESSION_EDITOR_TEST_TRANSPOSE",
                "FTS: Expression Editor — transpose loaded notes +12 (test)",
                || {
                    with_editor(|ed| {
                        let ids: Vec<_> = ed.doc.notes.iter().map(|n| n.id).collect();
                        ed.apply(&expression_editor_core::Edit::Transpose {
                            notes: ids,
                            semitones: 12,
                        });
                    });
                },
            ),
        ]
    }

    fn panels(&self) -> Vec<PanelDef> {
        vec![PanelDef {
            id: PANEL_ID,
            title: "FTS Expression Editor",
            component: PanelComponent::from_fn_ptr(EditorPanel as fn() -> _ as *const ()),
            default_dock: DockPosition::Floating,
            renderer: PanelRenderer::Native,
            default_size: (1180.0, 640.0),
            toggle_action: Some("FTS_EXPRESSION_EDITOR_TOGGLE"),
        }]
    }

    fn init(&self, _ctx: &ModuleContext) {
        tracing::info!("FTS Expression Editor module initialized");
    }
}

/// What kind of take is loaded, for the header. `None` when empty.
fn loaded_kind() -> Option<&'static str> {
    session().lock().unwrap().as_ref().map(|s| match s {
        Loaded::Midi(_) => "MIDI",
        Loaded::Audio(_) => "audio",
    })
}

#[component]
pub fn EditorPanel() -> Element {
    // The component mirrors the global session at mount. The actions
    // that load a take run outside any component and cannot reach a
    // hook's state; instead they remount this panel (see the OPEN
    // action), which re-runs these initializers against the fresh
    // global.
    let editor = use_signal(|| {
        session()
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.editor().clone())
            .unwrap_or_else(empty_editor)
    });
    let loaded = use_signal(|| session().lock().unwrap().is_some());
    let take_label = use_signal(loaded_label);
    let take_kind = use_signal(|| loaded_kind().unwrap_or(""));

    // Mirror the panel's whole editor — camera included — down to the
    // session on every change. The live-sync poll ([`poll`]) owns
    // everything from there: it writes settled edits to the take,
    // follows the REAPER selection, and matches the viewport to the
    // panel, remounting this component to surface what changed. The
    // full mirror is what lets a remount restore the camera instead of
    // resetting the user's zoom.
    use_effect(move || {
        let mut guard = session().lock().unwrap();
        if let Some(s) = guard.as_mut() {
            let panel = editor.read();
            let mirror = s.editor_mut();
            if mirror.doc != panel.doc {
                // A real edit: mirror everything, it is what the
                // auto-write sends to the take.
                *mirror = panel.clone();
            } else {
                // View-only change (a pan frame, a marquee): copy the
                // light state so a remount restores it, without cloning
                // a many-thousand-note document per frame.
                mirror.camera = panel.camera;
                mirror.viewport = panel.viewport;
                if mirror.selection != panel.selection {
                    mirror.selection = panel.selection.clone();
                }
            }
        }
    });

    let note_count = editor.read().doc.notes.len();

    rsx! {
        document::Style { {PANEL_CSS} }
        div {
            // Viewport units, not `height: 100%`: the panel's Blitz
            // document has no definite height on the root element, so a
            // percentage chain collapses to zero (a black panel under
            // the header). 100vh is the panel surface itself — the same
            // root the standalone runner uses. Everything inside is an
            // ordinary responsive flex column.
            style: "width: 100vw; height: 100vh; display: flex; flex-direction: column; \
                    background: #101016;",
            div {
                style: "display: flex; align-items: center; gap: 6px; padding: 4px 8px; \
                        flex: 0 0 auto; background: #15151c; border-bottom: 1px solid #2b2b38; \
                        color: #c8cede; font-size: 11px; font-family: system-ui, sans-serif;",
                if loaded() {
                    span {
                        // No text-overflow: Blitz doesn't support it;
                        // overflow:hidden alone truncates a long label.
                        style: "color: #e8ecf6; font-weight: 600; overflow: hidden;",
                        "{take_label}"
                    }
                    span {
                        style: "color: #7b8397;",
                        "{take_kind} · {note_count} notes"
                    }
                } else {
                    span {
                        style: "color: #7b8397;",
                        "no item"
                    }
                }
                span {
                    style: "margin-left: auto; color: #7b8397;",
                    if is_dirty() { "syncing…" } else { "live" }
                }
            }
            div {
                style: "flex: 1 1 0; min-height: 0;",
                if loaded() {
                    ExpressionEditor { editor }
                } else {
                    div {
                        style: "height: 100%; display: flex; flex-direction: column; \
                                align-items: center; justify-content: center; gap: 8px; \
                                color: #7b8397; font-size: 13px; \
                                font-family: system-ui, sans-serif;",
                        span {
                            style: "font-size: 15px; color: #c8cede;",
                            "Nothing loaded"
                        }
                        span { "Select an audio or MIDI item in REAPER — the editor" }
                        span { "follows your selection. Press E to close it." }
                    }
                }
            }
        }
    }
}

/// Document-level fixes for the embedded Blitz view, mirroring the
/// standalone runner's shell: a definite root size for the percentage
/// chain, and dark-scheme defaults.
const PANEL_CSS: &str = "html, body { width: 100%; height: 100%; margin: 0; padding: 0; \
                         background: #101016; } \
                         button { cursor: pointer; } \
                         :root { color-scheme: dark; }";

fn empty_editor() -> expression_editor_core::Editor {
    use expression_editor_core::{ExpressionDoc, TimeBase};
    expression_editor_core::Editor::new(
        ExpressionDoc::new(TimeBase::Ppq { ppq: 960.0 }, 0.0, 960.0 * 8.0),
        Viewport::new(1100.0, 520.0),
    )
}

/// Write a known two-lane gain ride to the loaded audio take.
///
/// Exists for the REAPER integration test. The values are fixed so the
/// test can assert exact numbers: a gate at -3 dB and a sibilance lane
/// at -5 dB sum to -8, which is 0.398 as the linear multiplier a take
/// volume envelope holds.
pub fn write_test_dynamics() -> bool {
    use expression_editor_audio::dynamics::GainPoint;
    use expression_editor_audio::{DynamicsLane, Lanes};

    let reaper = daw::reaper::Reaper;
    let mut guard = session().lock().unwrap();
    let Some(Loaded::Audio(s)) = guard.as_mut() else {
        tracing::warn!("no audio take loaded");
        return false;
    };
    let frames = s.analysis().frames.frames.len();
    // Logged because a zero here is the difference between "the write
    // path is broken" and "nothing was loaded to write about", and the
    // test binary cannot see either.
    tracing::info!(
        frames,
        samples = s.source().len(),
        rate = s.sample_rate(),
        "test dynamics: analysed take"
    );
    if frames == 0 {
        return false;
    }
    let flat =
        |db: f64| -> Vec<GainPoint> { (0..frames).map(|frame| GainPoint { frame, db }).collect() };
    let mut lanes = Lanes::from_dynamics(&Default::default(), frames);
    lanes.set(DynamicsLane::Gate, flat(-3.0));
    lanes.set(DynamicsLane::Sibilance, flat(-5.0));

    let written = s.write_dynamics(&reaper, &lanes, &Default::default(), true);
    tracing::info!(
        points = written.points,
        markers = written.markers,
        "wrote test dynamics"
    );
    written.points > 0
}

/// Run a closure against the loaded editor.
///
/// Exists for the REAPER integration test, which needs to make an edit
/// through the editor's own path without a pointer — driving the real
/// `Edit` pipeline rather than poking the document is the only way the
/// test proves the pipeline works.
pub fn with_editor<R>(f: impl FnOnce(&mut expression_editor_core::Editor) -> R) -> Option<R> {
    let mut guard = session().lock().unwrap();
    guard.as_mut().map(|s| f(s.editor_mut()))
}

/// The module, for the extension's registry.
pub fn module() -> Box<dyn DawModule> {
    Box::new(ExpressionEditorModule)
}
