//! The velocity panel, pointed at the expression editor's own selection.
//!
//! [`VelocityPanel`](crate::VelocityPanel) has always taken its notes
//! through a [`VelocitySink`], and there have been two: a REAPER-backed
//! one and the demo take the standalone example runs on. Not one for the
//! editor the panel ships inside — so the panel could shape a synthetic
//! take or somebody else's, but not the notes on screen.
//!
//! The reason was a trait bound. `VelocitySink` was `Send + Sync`, and
//! the editor lives in a dioxus `Signal`, which is neither. Nothing ever
//! needed the bound — no sink is used off the UI thread — so it is gone,
//! and this is what it was in the way of.
//!
//! ## What "the take" means here
//!
//! The selection when there is one, the whole part when there is not.
//! The panel is a bulk tool; opening it with nothing selected and being
//! told there is nothing to do would be true and useless, and "shape
//! everything" is the only other reading.

use std::cell::RefCell;

use dioxus::prelude::*;

use crate::theme;
use expression_editor_core::Editor;
use expression_editor_core::doc::NoteId;
use expression_editor_core::edit::Edit;
use expression_editor_tools::VelocitySink;
use expression_editor_tools::velocity::{Note as VNote, Session};

/// A sink over a live [`Editor`] signal.
pub struct EditorSink {
    editor: Signal<Editor>,
    /// Engine index → document id, from the last [`VelocitySink::open`].
    ///
    /// The engine numbers notes from zero within whatever it was handed
    /// and never learns their real identity, so the mapping has to live
    /// on this side. `RefCell` because the trait takes `&self` — a sink
    /// is a service, and making every method `&mut` to carry a lookup
    /// table would be the tail wagging the dog.
    ids: RefCell<Vec<NoteId>>,
}

impl EditorSink {
    pub fn new(editor: Signal<Editor>) -> Self {
        Self {
            editor,
            ids: RefCell::new(Vec::new()),
        }
    }

    /// The notes this sink is bound to, in time order.
    ///
    /// Time order because every engine is positional — an accent pattern
    /// cycles through them, a curve is drawn across them — and the
    /// selection is a set carrying whatever order the clicks gave it.
    fn take(&self) -> (Vec<NoteId>, Vec<VNote>) {
        let ed = self.editor.read();
        let selected = &ed.selection.notes;
        let mut rows: Vec<(f64, NoteId, u8, bool)> = ed
            .doc
            .notes
            .iter()
            .filter(|n| selected.is_empty() || selected.contains(&n.id))
            .map(|n| {
                (
                    n.start,
                    n.id,
                    (n.velocity * 127.0).round().clamp(1.0, 127.0) as u8,
                    selected.contains(&n.id),
                )
            })
            .collect();
        rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal));

        let ids = rows.iter().map(|(_, id, _, _)| *id).collect();
        let notes = rows
            .iter()
            .enumerate()
            .map(|(i, (_, _, vel, sel))| {
                // Everything the sink hands over is fair game, so the
                // engines' own "selected" flag is set for all of it. It
                // means "in scope" to them, and the scope was decided
                // above.
                let _ = sel;
                VNote::selected(i as u32, *vel)
            })
            .collect();
        (ids, notes)
    }

    /// Write absolute velocities back, as one undo step.
    fn write(&self, notes: &[VNote]) -> usize {
        let ids = self.ids.borrow();
        // `Signal` is `Copy`, so a local copy is what makes a write
        // possible through `&self` — which the trait requires, and
        // rightly: a sink is a service, not something you own mutably.
        let mut editor = self.editor;
        let mut ed = editor.write();
        let mut moved = 0;
        ed.begin_gesture();
        for n in notes {
            let Some(id) = ids.get(n.index as usize) else {
                continue;
            };
            ed.apply_live(&Edit::SetVelocity {
                notes: vec![*id],
                velocity: n.velocity as f64 / 127.0,
            });
            moved += 1;
        }
        moved
    }
}

impl VelocitySink for EditorSink {
    fn open(&self) -> Result<Session, String> {
        let (ids, notes) = self.take();
        if notes.is_empty() {
            return Err("no notes to shape".to_string());
        }
        *self.ids.borrow_mut() = ids;
        Ok(Session::new(notes))
    }

    fn commit(&self, session: &Session) -> Result<usize, String> {
        Ok(self.write(&session.resolve()))
    }

    fn revert(&self, session: &Session) -> Result<usize, String> {
        Ok(self.write(session.baseline()))
    }

    fn resync(&self, session: &mut Session) -> Result<(), String> {
        let (ids, notes) = self.take();
        if notes.is_empty() {
            return Err("no notes to shape".to_string());
        }
        *self.ids.borrow_mut() = ids;
        session.resync(notes);
        Ok(())
    }
}

/// Whether the velocity window is up.
///
/// A context, not a prop: what opens it is a key press inside the
/// canvas, several components down, and the window is the canvas's
/// sibling rather than its child. Threading a prop between two cousins
/// is how the component in between ends up carrying a feature it has
/// nothing to do with.
#[derive(Clone, Copy)]
pub struct PanelOpen(pub Signal<bool>);

/// The velocity panel, in a window you can move.
///
/// Not a separate OS window, which is what it wants to be.
///
/// **dioxus-native has no multi-window support**, and the gap is deeper
/// than it looks. `DioxusNativeApplication::add_window` exists and is
/// public, but it forwards to `BlitzApplication::add_window`, which only
/// pushes to a queue that Blitz drains with `View::init` + `resume`.
/// Every piece of Dioxus wiring — the document provider, the event
/// handlers, the shell provider, history, the renderer, the winit
/// window, and `initial_build()` — lives in
/// `DioxusNativeApplication::can_create_surfaces` and runs for exactly
/// one `pending_window: Option<_>`. A window added the other way comes
/// up with an unbuilt vdom and no providers. Upstream marks the same
/// spot: *"todo(jon): we should actually mess with the pending windows
/// instead of passing along the contexts"*.
///
/// The canonical Dioxus API is `DesktopContext::new_window(dom, cfg)` —
/// build a pending window, push it to a shared queue, poke the event
/// loop through the proxy — but that is `dioxus-desktop`, which means
/// WebKit, which this editor does not use. Porting that pattern to
/// native is three changes and would need a dioxus fork beside the blitz
/// one; parked until dioxus's next Blitz sync, which the fork bump is
/// waiting on anyway.
///
/// So: a window in every respect the surface itself controls. It floats
/// over the editor, it has a title bar, you drag it where you want it,
/// and it closes.
#[component]
pub fn VelocityWindow() -> Element {
    let mut open = use_context::<PanelOpen>().0;
    // Where the window sits, in pixels from the top-left of the editor.
    let mut at = use_signal(|| (120.0_f64, 90.0_f64));
    // Pointer position when the title bar was grabbed, and the window
    // position at that moment. Both, because a drag is a *delta* — using
    // the pointer alone would snap the window's corner to the cursor the
    // instant you touched the bar.
    let mut grab = use_signal(|| None::<((f64, f64), (f64, f64))>);

    let (x, y) = at();
    rsx! {
        div {
            "data-testid": "velocity-window",
            // The grab state, so a test can tell "the bar was never
            // pressed" from "the move never arrived" — two failures that
            // look identical from the outside.
            "data-grabbed": "{grab().is_some()}",
            style: "position: absolute; left: {x}px; top: {y}px; z-index: 60; \
                    width: 420px; height: 460px; display: flex; \
                    flex-direction: column; overflow: hidden; \
                    border: 1px solid {theme::PANEL_BORDER}; border-radius: 8px; \
                    background: {theme::PANEL}; \
                    box-shadow: 0 18px 48px rgba(0,0,0,0.55);",
            // The move is tracked on the whole window, not on the bar
            // that starts it. A pointer that has travelled fifty pixels
            // is no longer over a twenty-pixel-tall title bar, so
            // handlers living there see the press and then nothing —
            // which is the browser's `setPointerCapture` case, and Blitz
            // exposes no such thing. Watching the window instead covers
            // every drag that stays inside it, which is every ordinary
            // one; a fast throw beyond the edge still drops the grab.
            onpointermove: move |e: PointerEvent| {
                if let Some((from, was)) = grab() {
                    let p = e.data().page_coordinates();
                    at.set((was.0 + (p.x - from.0), was.1 + (p.y - from.1)));
                }
            },
            onpointerup: move |_| grab.set(None),

            // The title bar, which is also the handle.
            div {
                "data-testid": "velocity-window-title",
                style: "display: flex; align-items: center; gap: 8px; \
                        padding: 5px 8px; cursor: move; user-select: none; \
                        background: {theme::SURFACE_BAR}; \
                        border-bottom: 1px solid {theme::PANEL_BORDER}; \
                        color: {theme::TEXT}; font-size: 11px; \
                        font-family: system-ui, sans-serif;",
                onpointerdown: move |e: PointerEvent| {
                    let p = e.data().page_coordinates();
                    grab.set(Some(((p.x, p.y), at())));
                },

                // Transparent to the pointer, so a press on the word
                // lands on the *bar*. Otherwise the grab only starts on
                // the bare strip either side of the label, which is a
                // title bar that mostly does not drag.
                span {
                    style: "font-weight: 600; pointer-events: none;",
                    "Velocity"
                }
                span {
                    style: "margin-left: auto; cursor: pointer; padding: 0 4px; \
                            color: {theme::TEXT_DIM};",
                    onclick: move |_| open.set(false),
                    "✕"
                }
            }

            div {
                style: "flex: 1 1 auto; min-height: 0; overflow: auto;",
                crate::VelocityPanel {}
            }
        }
    }
}
