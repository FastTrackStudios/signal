//! The piano roll — the surface every gesture lands on.
//!
//! One component, and the only place that turns a pointer position into
//! a document coordinate. Split out of `lib.rs`, where it was 1500 of
//! 2000 lines and the reason the sizing model was hard to reason about.
//!
//! ## Roll space
//!
//! Every handler here works in *roll space*: element coordinates less
//! the keyboard gutter down the left and the ruler across the top, which
//! is what [`local`] does and the only place that subtraction happens.
//! The camera therefore never has to know the chrome exists.
//!
//! That mapping is exact rather than approximate, and deliberately so —
//! see [`crate::sizing`] for why the svg carries no `viewBox` and takes
//! its box from layout.
//!
//! ## What decides a gesture
//!
//! Not the armed tool by itself. `interaction::pointer_down` resolves
//! `Editor::mouse` first, giving the armed tool first refusal on the
//! plain gestures it claims, and falls through to the tool-driven path
//! only where the map says nothing.

use dioxus::prelude::*;
use dioxus_elements::input_data::MouseButton;
use expression_editor_core::Editor;
use expression_editor_core::memagic;
use expression_editor_core::tools::Mods;
use input::InputCommand;
use keyboard_types::Modifiers;

use crate::interaction::{self, Drag};
use crate::menu_ui::{self, ContextMenu};
use crate::multitool_ui::{self, MultiTool};
use crate::{BendFlow, draw_hint_of, velocity_ramp};
use crate::{
    canvas, cursor, drawer, drawer::ModDrawer, keys, paint, roll_widget, scroll, text, theme,
};
/// Pointer coordinates in **roll** space — element coordinates minus
/// the keyboard gutter and timeline ruler.
///
/// Every interaction handler works in roll space, so the camera never
/// has to know the chrome exists.
fn local(e: &PointerEvent) -> (f64, f64) {
    let c = e.data().element_coordinates();
    (c.x - canvas::GUTTER_W, c.y - canvas::RULER_H)
}

/// Where in the chrome a press landed, if it did.
enum Chrome {
    Roll,
    /// The ruler: clicking it moves the playhead.
    Ruler(f64),
    /// A piano key: clicking it selects every note on that row.
    Key(i32),
}

/// Which region the pointer is in, and what to anchor on.
///
/// The anchor follows MeMagic's priority chain: the playhead while the
/// transport is running, else the pointer, else the middle of the view.
/// Zooming during playback should frame the music, not wherever the
/// mouse happened to be left.
///
/// With no pointer at all — a toolbar press, a key with the mouse
/// elsewhere — the region is `Elsewhere`, which fits the whole item
/// rather than guessing at a position.
fn memagic_at(ed: &Editor, hover: Option<(f64, f64)>) -> (memagic::Region, memagic::Anchor) {
    let Some((x, y)) = hover else {
        return (
            memagic::Region::Elsewhere,
            memagic::Anchor {
                t: ed
                    .playhead
                    .unwrap_or_else(|| ed.camera.t_at(ed.viewport.w * 0.5)),
                row: None,
            },
        );
    };
    let region = match chrome_at(ed, x, y) {
        Chrome::Ruler(_) => memagic::Region::Ruler,
        Chrome::Key(_) => memagic::Region::Piano,
        Chrome::Roll => memagic::Region::NoteArea,
    };
    let pointer_t = ed.camera.t_at(x - canvas::GUTTER_W);
    (
        region,
        memagic::Anchor {
            t: ed.playhead.unwrap_or(pointer_t),
            row: Some(ed.camera.pitch_at(y - canvas::RULER_H, ed.viewport)),
        },
    )
}

fn chrome_at(ed: &Editor, x: f64, y: f64) -> Chrome {
    if y < canvas::RULER_H {
        Chrome::Ruler(ed.camera.t_at(x - canvas::GUTTER_W))
    } else if x < canvas::GUTTER_W {
        Chrome::Key(ed.camera.pitch_at(y - canvas::RULER_H, ed.viewport).round() as i32)
    } else {
        Chrome::Roll
    }
}

fn mods_of(m: Modifiers) -> Mods {
    Mods {
        // Cmd and Ctrl are the same gesture; normalizing here means no
        // handler downstream has to know what platform it is on.
        ctrl: m.contains(Modifiers::CONTROL) || m.contains(Modifiers::META),
        shift: m.contains(Modifiers::SHIFT),
        alt: m.contains(Modifiers::ALT),
    }
}

#[component]
pub fn Canvas(
    editor: Signal<Editor>,
    drag: Signal<Drag>,
    drawer: Signal<ModDrawer>,
    multi: Signal<MultiTool>,
    menu_state: Signal<ContextMenu>,
    pending: Signal<Option<menu_ui::Pending>>,
    draft: Signal<Option<expression_editor_core::PitchDraft>>,
) -> Element {
    let mut multi = multi;
    let mut editor = editor;
    let mut drag = drag;
    let mut drawer = drawer;
    let mut menu_state = menu_state;
    let mut draft = draft;

    // Where the pointer last was, in element coordinates.
    //
    // Needed because a keypress carries no position and MeMagic is
    // anchored on one — "zoom to what I am pointing at" has to know
    // where that is. `None` until the pointer has been over the canvas.
    let mut hover = use_signal(|| None::<(f64, f64)>);

    // Modifiers as of the last pointer or key event.
    //
    // Tracked separately from `hover` because holding Alt has to change
    // the painted cursor *without* the pointer moving — the whole point
    // of the glyph is that it previews the gesture, and a preview that
    // waits for a wiggle before admitting Alt is bound to Copy is not
    // one.
    let mut hover_mods = use_signal(Mods::default);

    // What can follow the keys typed so far. Empty means no sequence is
    // live, which is the overlay's cue to stay hidden.
    let mut which_key = use_signal(Vec::<keys::Continuation>::new);

    // The tool to go back to when `z` is released.
    //
    // `Some` means a spring-loaded zoom is live. It is set the moment
    // `z` goes down — the toolbar reads `ed.tool`, so arming late meant
    // holding `z` looked like nothing was happening.
    let mut spring_from = use_signal(|| None::<expression_editor_core::Tool>);

    // Whether the hold got *used*, which is a separate question from
    // whether it is live now that arming happens on the way down.
    //
    // A drag sets it, a key press does not, and the release reads it to
    // decide whether the which-key tree is finished or still waiting for
    // its second key. That is what keeps `z`-as-prefix and `z`-as-tool
    // from having to be told apart by a timer.
    let mut spring_used = use_signal(|| false);

    // The live velocity shape, if one is open.
    //
    // A signal rather than a field on the `Editor` because it holds a
    // `velocity::Session`, which lives in `expression-editor-tools` —
    // and `tools` depends on `core`, so `core` cannot hold one without
    // a cycle. It is gesture state either way, which is what the other
    // signals here are.
    let mut ramp = use_signal(|| None::<crate::velocity_ramp::VelocityRamp>);

    // The viewport is followed by `ExpressionEditor`, which is the only
    // component that knows the whole chrome. See the effect there.

    // Where each render leaves the drawing for the renderer to pick up.
    let slot = use_hook(roll_widget::SceneSlot::new);

    // The shaper, kept across renders because that is the whole point of
    // it: the same twenty pitch names are drawn every frame and must be
    // shaped once, not once per frame.
    let labels = use_hook(|| std::rc::Rc::new(std::cell::RefCell::new(text::Labeller::new())));

    // The widget, created once. `CustomWidgetAttr` is write-once — the
    // DOM takes the widget out of it on the first mutation — so building
    // a new one per render would hand the second render an empty
    // attribute and the roll would go blank.
    let frames = use_context::<roll_widget::Frames>();
    let widget = use_hook(|| {
        dioxus_native_dom::CustomWidgetAttr::new(roll_widget::RollWidget::new(
            slot.clone(),
            frames.clone(),
        ))
    });

    // While the drawer is open its target is locked: editing gestures
    // are blocked, but every navigation path stays live so the preview
    // can be auditioned in context.
    let locked = drawer.read().open;

    let marquee = match &*drag.read() {
        Drag::Marquee {
            origin, current, ..
        } => Some((
            origin.0.min(current.0),
            origin.1.min(current.1),
            (current.0 - origin.0).abs(),
            (current.1 - origin.1).abs(),
        )),
        _ => None,
    };

    // The razor being swept, already snapped. Read from the drag rather
    // than recomputed here: `resolve_razor` decided it on the last move
    // and the release will commit that same value, so drawing anything
    // else would be drawing a third opinion.
    let razor = match &*drag.read() {
        Drag::RazorCreate { pending, .. } => *pending,
        _ => None,
    };

    let ed = editor.read();
    let vp = ed.viewport;
    let cc_editing = ed.cc_edit;
    let microtonal = !ed.tuning.temperament.is_equal();
    let temperament_name = ed.tuning.temperament.name;
    let empty = ed.doc.notes.is_empty();
    // The empty-roll hint names the *actual* draw binding, read from
    // the live mouse map — which the host may have overlaid with the
    // user's own REAPER mouse modifiers. A hardcoded "press D" hint
    // goes stale the moment the map differs.
    let draw_hint = draw_hint_of(&ed);

    // ── the drawing ──────────────────────────────────────────────────
    //
    // Built here, in render, where reading a signal is ordinary. The
    // widget below only replays what this leaves in the slot — see
    // `crate::roll_widget` for why the renderer must not reach back into
    // dioxus to fetch it.
    //
    // Rebuilding on every render is the same cadence the svg markup was
    // rebuilt at, and far cheaper: a `Scene` is a vector of draw
    // commands, where the markup had to be re-parsed into a usvg tree
    // before anything reached the screen.
    frames.built();
    slot.put(paint::roll_scene(
        &ed,
        vp.w + canvas::GUTTER_W,
        vp.h + canvas::RULER_H,
        &paint::Overlay {
            marquee,
            razor,
            draft: draft.read().as_ref().map(|d| canvas::draft_view(&ed, d)),
            // Prototype (#161): read deep in the drawing and nothing
            // between here and there cares, so it arrives by context.
            flow: try_consume_context::<BendFlow>().unwrap_or_default(),
        },
        &mut labels.borrow_mut(),
    ));
    drop(ed);

    rsx! {
        div {
            // `flex: 1 1 0` + a floor. The zero basis is load-bearing:
            // with `auto`, the basis comes from the content, and the
            // content is an svg whose intrinsic size is the `viewBox` —
            // which is `Editor::viewport`, which the measure below sets
            // *from this cell*. That is a loop: cell sizes svg, svg
            // sizes cell, and the measure drives it round every tick.
            // It showed up as `wgpu error: Out of Memory`, because each
            // pass is a full re-render.
            //
            // The floor is what the old `auto` basis was really for: a
            // parent whose height does not resolve (a plugin window
            // before its first resize, a headless mount) would otherwise
            // collapse the canvas to nothing, and the svg no longer
            // volunteers any height of its own.
            style: "position: relative; flex: 1 1 0; min-height: 360px; \
                    overflow: hidden; outline: none;",
            // The cell is what gets measured to drive `Editor::resize`.
            // `onresize` cannot do it: dioxus-native's
            // `convert_resize_data` is `unimplemented!()`, so an element
            // resize event never arrives in the renderer the plugin, the
            // REAPER panel and the desktop runner all use. The measure
            // below polls this element instead, which is what the REAPER
            // panel already did from outside.
            "data-testid": "canvas-cell",
            tabindex: "0",
            onkeydown: move |e: KeyboardEvent| {
                // A held key is one press.
                //
                // The OS repeats `keydown` while a key is down, and the
                // sequence resolver counts presses — so holding `z` typed
                // `z z z…` and fired the `z z` binding without the user
                // ever pressing it twice. Every held-key behaviour here
                // depends on this: the spring-loaded zoom tool is a
                // *hold*, and a hold that re-enters the keymap forty
                // times a second is not one.
                if e.is_auto_repeating() {
                    e.prevent_default();
                    return;
                }
                let key = e.key().to_string();
                let m = mods_of(e.modifiers());
                // Pressing a modifier is itself a keydown, so this is
                // what makes the painted cursor answer to Alt and Ctrl
                // with the pointer sitting still.
                hover_mods.set(m);
                // The toolbar lights up the tool the modifiers would
                // use, so it has to hear about them. Written only when
                // it actually changes: `Signal::write` notifies whether
                // or not the value differs, and a keydown that renotified
                // every subscriber would repaint the whole surface per
                // key.
                if editor.read().held_mods != m {
                    editor.write().held_mods = m;
                }

                // The shared keymap gets first refusal, so which-key
                // sequences resolve before any hardcoded binding. `z`
                // used to be hardcoded here, which ate the first key of
                // every zoom sequence the REAPER profile defines.
                if key == "Escape" && keys::is_pending() {
                    keys::cancel();
                    which_key.set(Vec::new());
                    e.prevent_default();
                    return;
                }
                let commands = keys::resolve(&key, m);
                let mut ran = false;
                for cmd in &commands {
                    let action = match cmd {
                        InputCommand::Action(a) => Some(a.0.as_str()),
                        InputCommand::ActionWithArgs { action, .. } => Some(action.0.as_str()),
                        _ => None,
                    };
                    if let Some(action) = action {
                        // Velocity commands first: they own a live shape
                        // that outlives the keypress, which a function
                        // taking only `&mut Editor` cannot hold.
                        if let Some(hit) = velocity_action(&mut editor, &mut ramp, action) {
                            ran |= hit;
                            continue;
                        }
                        let (region, anchor) = memagic_at(&editor.read(), hover());
                        ran |= keys::dispatch(&mut editor.write(), action, region, anchor);
                    }
                }
                // A half-typed sequence owns the key too: `z` on its way
                // to `z i` must not also fire the tool shortcut.
                which_key.set(keys::continuations());

                // Arm the spring-loaded tool the instant its prefix goes
                // down, not at the first drag. The toolbar reads
                // `ed.tool`, so arming late left the button dark while
                // the surface was already in zoom mode.
                //
                // Tap and hold stay one idea: this only arms, and the
                // key-up decides. A tap arms and disarms without you
                // seeing it, and leaves the tree open for `z i`.
                if let Some(armed) = keys::held_prefix().as_deref().and_then(spring_tool)
                    && spring_from.read().is_none()
                {
                    let previous = editor.read().tool;
                    if previous != armed {
                        spring_from.set(Some(previous));
                        editor.write().tool = armed;
                    }
                }

                if ran || keys::is_pending() {
                    e.prevent_default();
                    return;
                }

                // A pitch drawing is modal, so it takes its keys first
                // and Ctrl+Z means *its* undo — the document's history
                // has nothing in it to undo yet, by design.
                if draft.read().is_some() {
                    let mut handled = true;
                    match (key.as_str(), m.ctrl) {
                        ("Enter", _) => {
                            let d = draft.read().clone();
                            if let Some(d) = d {
                                editor.write().apply_draft(&d);
                            }
                            draft.set(None);
                        }
                        ("Escape", _) => {
                            let d = draft.read().clone();
                            if let Some(d) = d {
                                editor.write().dismiss_draft(&d);
                            }
                            draft.set(None);
                        }
                        ("z", true) => {
                            let mut dw = draft.write();
                            if let Some(dr) = dw.as_mut() {
                                if m.shift { dr.redo(); } else { dr.undo(); }
                                let preview = dr.clone();
                                drop(dw);
                                editor.write().preview_draft(&mut { preview });
                            }
                        }
                        _ => handled = false,
                    }
                    if handled {
                        e.prevent_default();
                        return;
                    }
                }

                // Open a drawing on the selected note.
                if key == "3" && !m.ctrl {
                    let opened = {
                        let ed = editor.read();
                        ed.selection
                            .notes
                            .first()
                            .and_then(|id| expression_editor_core::PitchDraft::open(&ed.doc, *id))
                    };
                    if opened.is_some() {
                        draft.set(opened);
                        e.prevent_default();
                        return;
                    }
                }
                // B opens the drawer; Escape closes it. Both work
                // whether or not it is already open.
                if key == "b" && !m.ctrl {
                    let mut dw = drawer.write();
                    if dw.open {
                        dw.cancel(&mut editor.write());
                    } else if dw.open_on(&editor.read()) {
                        dw.preview(&mut editor.write());
                    }
                    e.prevent_default();
                    return;
                }
                // The Multi Tool arms over the selection and takes its
                // own gestures while armed.
                if key == "a" && !m.ctrl {
                    let armed = multi.read().armed;
                    if armed {
                        multi.write().disarm();
                    } else {
                        let ok = multi.write().arm(&editor.read());
                        if !ok {
                            multi.write().disarm();
                        }
                    }
                    e.prevent_default();
                    return;
                }
                // Drum-mode keys, before the general ones: `f` means
                // flam here and nothing elsewhere.
                if key == "f" && !m.ctrl && editor.read().mode == expression_editor_core::Mode::Drums {
                    let made = editor.write().flam_selection();
                    if made > 0 {
                        e.prevent_default();
                        return;
                    }
                }

                if multi.read().armed {
                    match key.as_str() {
                        "m" => {
                            let mut t = multi.write();
                            t.toggle_bend(&mut editor.write());
                            e.prevent_default();
                            return;
                        }
                        "s" => {
                            let mut t = multi.write();
                            t.toggle_symmetric(&mut editor.write());
                            e.prevent_default();
                            return;
                        }
                        "Escape" => {
                            multi.write().disarm();
                            e.prevent_default();
                            return;
                        }
                        _ => {}
                    }
                }
                if key == "Escape" && drawer.read().open {
                    drawer.write().cancel(&mut editor.write());
                    e.prevent_default();
                    return;
                }
                // Abandon a sweep in progress rather than clearing the
                // areas behind it.
                //
                // Escape reads as "not that" — and while you are still
                // dragging, the thing you mean is the rectangle under
                // the pointer, not the set you drew earlier. Dropping
                // the drag leaves nothing committed, because the area
                // only lands on release.
                if key == "Escape" && matches!(&*drag.read(), Drag::RazorCreate { .. }) {
                    drag.set(Drag::None);
                    e.prevent_default();
                    return;
                }
                if locked {
                    return;
                }
                let d = drag.read().clone();
                if interaction::key_down(&mut editor.write(), &d, &key, m) {
                    e.prevent_default();
                }
            },
            onkeyup: move |e: KeyboardEvent| {
                let m = mods_of(e.modifiers());
                hover_mods.set(m);
                // Releasing Ctrl has to put the razor highlight back as
                // surely as pressing it lit one.
                if editor.read().held_mods != m {
                    editor.write().held_mods = m;
                }
                // The processor has to hear about the release, or it
                // goes on believing the key is held and OS auto-repeat
                // walks the sequence tree on its own. See `keys::release`.
                // The processor owns sticky-prefix behaviour — holding `g`
                // and tapping `q`, `w`, `e` in turn is `sticky_anchor`,
                // which it maintains itself. All the surface has to do is
                // report the release honestly and act on the answer:
                // `true` means a sticky run that fired at least one
                // action has ended, so the overlay should go. A bare
                // hold-and-release returns `false` and the tree stays up,
                // because you were reading it rather than using it.
                if keys::release(&e.key().to_string(), mods_of(e.modifiers())) {
                    which_key.set(Vec::new());
                } else {
                    which_key.set(keys::continuations());
                }
                // `R` is momentary: references drop back the instant it
                // is released, so it can never be left on by accident.
                match e.key().to_string().as_str() {
                    "r" => editor.write().refs_to_front = false,
                    "m" => editor.write().reference_to_front = false,
                    // Releasing `z` always puts the tool back — it was
                    // armed on the way down, so the spring is loaded
                    // whether or not you did anything with it.
                    //
                    // What the release still has to decide is the
                    // *tree*: a hold that got used is finished, and a
                    // tap is only half a sequence. So a drag closes the
                    // overlay and a bare tap leaves it open for `z i`.
                    //
                    // No timer decides which. The gesture does — which
                    // is the point. Hold-versus-tap by timeout is where
                    // this pattern usually goes wrong: it turns a slow
                    // tap into a hold.
                    key if spring_tool(key).is_some() => {
                        if let Some(previous) = spring_from.take() {
                            editor.write().tool = previous;
                        }
                        if spring_used.take() {
                            which_key.set(Vec::new());
                            keys::cancel();
                        }
                    }
                    _ => {}
                }
            },

            // ── the roll ─────────────────────────────────────────────
            //
            // A custom widget, painted straight into the renderer's
            // scene by `crate::paint`. It replaced an inline `<svg>`
            // subtree, for two reasons that were both structural:
            //
            // - Blitz paints an inline svg as a replaced element with a
            //   hardcoded `object-fit: contain`, so the drawing was
            //   scaled by (element box / declared size) — and an svg
            //   that declares no size takes it from its own content, so
            //   the roll rescaled as it scrolled. A widget is handed its
            //   box; there is no ratio to get wrong.
            // - Every camera move rebuilt the roll's markup, which Blitz
            //   re-parsed into a usvg tree before drawing anything. The
            //   scene is a recording: rebuilt when the state changes,
            //   replayed by the renderer every frame.
            //
            // The events stay on the element. `<object>` is an ordinary
            // DOM node, so `element_coordinates()` still arrives here —
            // and with nothing scaling, those *are* document
            // coordinates. `interaction.rs` did not have to change.
            object {
                "data-testid": "roll",
                "data": widget,
                // The box the scene was built for, as an attribute
                // rather than a readout in the status bar.
                //
                // It is the only way to ask the *mounted* surface what it
                // is drawing for, which is what pins the scene and the
                // element to one box — but a size in the corner of the
                // window is a debug aid, and debug aids do not belong in
                // a shipped chrome. `tests/geometry.rs` reads it here.
                "data-viewport": "{vp.w:.0}x{vp.h:.0}",
                // Explicit, and the same box the scene was built for.
                // A widget reports no intrinsic size, so without this it
                // has none — and blitz-paint skips a widget whose box is
                // zero, silently.
                // `cursor: none` because `crate::cursor` paints the real
                // one: CSS has no `[`, no `]`, no pencil and no razor,
                // and Blitz supports no `cursor: url(…)` to supply them.
                style: "position: absolute; left: 0; top: 0; display: block; \
                        width: {vp.w + canvas::GUTTER_W:.0}px; \
                        height: {vp.h + canvas::RULER_H:.0}px; \
                        touch-action: none; user-select: none; cursor: none;",
                onpointerdown: move |e: PointerEvent| {
                    let raw = e.data().element_coordinates();
                    // Resolve against a snapshot: the read guard must
                    // be dropped before any arm can write.
                    let where_ = chrome_at(&editor.read(), raw.x, raw.y);
                    match where_ {
                        Chrome::Ruler(t) => {
                            editor.write().playhead = Some(t);
                            return;
                        }
                        Chrome::Key(row) => {
                            let ids: Vec<_> = editor
                                .read()
                                .doc
                                .notes
                                .iter()
                                .filter(|n| n.row == row)
                                .map(|n| n.id)
                                .collect();
                            editor.write().selection.notes = ids;
                            return;
                        }
                        Chrome::Roll => {}
                    }
                    let (x, y) = local(&e);
                    let m = mods_of(e.modifiers());
                    // A drag while the zoom prefix is held spends the
                    // hold. The tool is already armed (see the keydown),
                    // so all this does is tell the release that the
                    // sequence is over and must not wait for `z i`.
                    if keys::zoom_prefix_held() && spring_from.read().is_some() {
                        spring_used.set(true);
                    }
                    // The lock stops you *editing*, not looking.
                    //
                    // `cursor_at` has always said so — it forbids a
                    // gesture only when `action.is_edit()` — but the
                    // handler refused everything, so the pointer drew a
                    // hand over a surface that would not pan. Navigation
                    // goes through first, and the lock applies to what is
                    // left.
                    let navigating = matches!(e.trigger_button(), Some(MouseButton::Auxiliary))
                        || editor.read().tool.is_view();
                    if locked && !navigating {
                        return;
                    }
                    let button = match e.trigger_button() {
                        Some(MouseButton::Secondary) => 2,
                        // Middle is a real gesture (hand-scroll pan);
                        // collapsing it onto left made a middle-drag
                        // edit notes.
                        Some(MouseButton::Auxiliary) => 1,
                        _ => 0,
                    };
                    // A pitch drawing owns the surface while it is up:
                    // a click is an anchor and nothing else.
                    if draft.read().is_some() {
                        let mut dw = draft.write();
                        let Some(dr) = dw.as_mut() else { return };
                        let next = interaction::draft_press(&editor.read(), dr, x, y);
                        let preview = dr.clone();
                        drop(dw);
                        editor.write().preview_draft(&mut { preview });
                        drag.set(next);
                        return;
                    }
                    let d = interaction::pointer_down(&mut editor.write(), x, y, m, button);
                    // A right-click resolves to a menu request rather
                    // than a drag. Opening it here — not in `interaction`
                    // — keeps the core pointer path free of UI state.
                    if let Drag::ContextMenu { x, y, under, t, row } = d {
                        menu_state.write().show(x, y, under, t, row);
                        return;
                    }
                    menu_state.write().close();
                    drag.set(d);
                },
                onpointermove: move |e: PointerEvent| {
                    // Recorded before the drag guard: the anchor has to
                    // follow the pointer whether or not a drag is live.
                    hover.set(Some(local(&e)));
                    let m = mods_of(e.modifiers());
                    hover_mods.set(m);
                    if editor.read().held_mods != m {
                        editor.write().held_mods = m;
                    }
                    if !drag.read().is_active() {
                        return;
                    }
                    let (x, y) = local(&e);
                    let m = mods_of(e.modifiers());
                    // Read the drag out before touching it: the guard
                    // would otherwise still be alive when `drag.set`
                    // wants the mutable borrow.
                    let current = drag.read().clone();
                    if let Drag::DraftAnchor { index } = current {
                        let mut dw = draft.write();
                        if let Some(dr) = dw.as_mut() {
                            let moved = interaction::draft_move(&editor.read(), dr, index, x, y);
                            let preview = dr.clone();
                            drop(dw);
                            editor.write().preview_draft(&mut { preview });
                            if let Some(i) = moved {
                                drag.set(Drag::DraftAnchor { index: i });
                            }
                        }
                        return;
                    }
                    let mut d = drag.write();
                    interaction::pointer_move(&mut editor.write(), &mut d, x, y, m);
                },
                onpointerup: move |e: PointerEvent| {
                    let (x, y) = local(&e);
                    let m = mods_of(e.modifiers());
                    let d = drag.read().clone();
                    let next = interaction::pointer_up(&mut editor.write(), d, x, y, m);
                    drag.set(next);
                },
                onpointerleave: move |_| {
                    // The painted cursor is only correct while the
                    // pointer is over the roll. Left set, it would be
                    // stranded at the last position the pointer had
                    // *and* the OS cursor would be visible next to it.
                    hover.set(None);
                },
                onwheel: move |e: WheelEvent| {
                    // Normalised to notches: a touchpad reports pixels and a
                    // mouse reports lines, and the gain constants
                    // downstream are tuned for lines. See `scroll::notches`.
                    let (dx, dy) = scroll::notches(&e.delta());
                    let m = mods_of(e.modifiers());
                    // Anchored on the last pointer position when there
                    // is one: a wheel event carries none of its own, and
                    // zooming about the middle of the view when the
                    // mouse is somewhere else is the wrong place.
                    // A live ramp takes the wheel while `v` is held.
                    //
                    // This is the half of the gesture that makes a
                    // preset usable: you get a shape, then you dial how
                    // hard it leans. Gated on the hold rather than on
                    // the ramp merely existing, so scrolling the view
                    // after you have shaped something still scrolls the
                    // view.
                    if keys::held_prefix().as_deref() == Some("v") {
                        // Taken out before it is used: a `write()` guard
                        // held across the body would still be live at
                        // `ramp.set`, which is the second borrow.
                        let taken = ramp.write().take();
                        if let Some(mut live) = taken {
                            let moved = live.nudge(&mut editor.write(), dy);
                            ramp.set(Some(live));
                            if moved {
                                e.prevent_default();
                                return;
                            }
                        }
                    }
                    let (x, y) = hover().unwrap_or((vp.w * 0.5, vp.h * 0.5));
                    interaction::wheel(&mut editor.write(), x, y, dx, dy, m);
                    e.prevent_default();
                },
            }

            if empty {
                div {
                    style: "position: absolute; inset: 0; display: flex; \
                            align-items: center; justify-content: center; \
                            pointer-events: none; color: {theme::TEXT_DIM}; \
                            font-size: 12px; text-align: center; line-height: 1.7;",
                    div {
                        div { style: "color: {theme::TEXT}; font-size: 14px;", "No notes" }
                        div { "{draw_hint}" }
                        // Read from the bindings, not restated here:
                        // this line still said "Scroll zooms ·
                        // Alt+scroll pans" after the shared config made
                        // that wrong, which is what a hardcoded hint is
                        // for.
                        div { "{scroll::hint()}" }
                    }
                }
            }

            // The painted pointer. Above the roll and below the modal
            // overlays, which have their own (real) cursors — and its
            // own component, so a pointer move repaints one 48px box
            // instead of rebuilding the whole roll scene. See
            // `crate::cursor`.
            cursor::CursorLayer {
                editor,
                hover,
                mods: hover_mods,
                drag,
                locked,
            }

            multitool_ui::MultiToolOverlay { editor, tool: multi }

            menu_ui::ContextMenuOverlay { editor, menu_state, pending }

            drawer::ModulationDrawer { editor, drawer }

            // A non-equal tuning is always visibly flagged — silently
            // editing in a temperament you forgot about is how you ship
            // a detuned take.
            if let Some(number) = cc_editing {
                div {
                    style: "position: absolute; top: 6px; left: 60px; display: flex; \
                            align-items: center; gap: 8px; background: {theme::BANNER_INFO}; \
                            border: 1px solid {theme::ACCENT}; border-radius: 4px; \
                            color: {theme::ACCENT}; font-size: 10px; padding: 3px 9px;",
                    span { "CC edit — CC{number}" }
                    button {
                        style: "background: none; border: none; color: {theme::ACCENT}; \
                                cursor: pointer; font-size: 11px; padding: 0;",
                        onclick: move |_| editor.write().exit_cc_edit(),
                        "✕"
                    }
                }
            }

            if microtonal {
                div {
                    style: "position: absolute; top: 6px; right: 8px; \
                            background: {theme::BANNER_WARN}; border: 1px solid {theme::GOLD}; \
                            border-radius: 4px; color: {theme::GOLD}; \
                            font-size: 10px; padding: 2px 7px; pointer-events: none;",
                    "{temperament_name}"
                }
            }
            // ── which-key ────────────────────────────────────────
            //
            // A half-typed sequence lists what can follow it. A live
            // razor lists its own verbs, for the same reason and in the
            // same place: those are bare letters with no prefix to type,
            // so nothing would ever have prompted you with them and the
            // only way to learn them would have been to be told.
            //
            // The sequence wins when both could show — you are part-way
            // through saying something specific, and answering a
            // different question would be the wrong help.
            if let Some(panel) = key_panel(&editor.read(), which_key()) {
                KeyPanel { title: panel.0, rows: panel.1 }
            }
        }
    }
}

/// A key/description list, bottom-right.
///
/// Shared by the which-key sequence overlay and the razor's verb list so
/// the two cannot drift into looking like different features — they are
/// the same promise, that the surface will tell you what it can do
/// without you having to look it up.
///
/// **Bottom-right**, not bottom-left. The status bar's own readouts are
/// on the right, so a panel on the left sat over the roll's low
/// register; and the pointer, which is what a pending gesture is
/// anchored on, is far likelier to be on the left half of a piano roll
/// than the right, since that is where the material you just clicked is.
#[component]
fn KeyPanel(title: String, rows: Vec<keys::Continuation>) -> Element {
    // Every branch resolved *before* the markup, so this component has
    // exactly one shape. `rsx!` conditionals add and remove nodes, and a
    // template whose node count depends on its props is what blitz-dom
    // walks a stale path through — the "invalid key" panic out of
    // `node_at_path`. A heading that is sometimes absent and a label
    // that is sometimes prefixed were two such conditionals; both are
    // now styles and strings, which change without moving anything.
    let heading = if title.is_empty() {
        "display: none;".to_string()
    } else {
        format!(
            "display: block; padding: 2px 10px 5px; margin-bottom: 3px; \
             border-bottom: 1px solid {}; color: {}; \
             font-weight: 600; letter-spacing: 0.4px;",
            theme::PANEL_BORDER,
            theme::TEXT_DIM,
        )
    };
    let rows: Vec<(String, String, &'static str)> = rows
        .into_iter()
        .map(|c| {
            let label = if c.is_group {
                format!("+{}", c.label)
            } else {
                c.label
            };
            let colour = if c.is_group {
                theme::TEXT_DIM
            } else {
                theme::TEXT
            };
            (c.key, label, colour)
        })
        .collect();

    rsx! {
        div {
            "data-testid": "which-key",
            style: format!(
                "position: absolute; right: 10px; bottom: 10px; z-index: 40; \
                 min-width: 220px; max-height: 60%; overflow-y: auto; \
                 padding: 6px 0; border-radius: 6px; \
                 border: 1px solid {}; background: {}; color: {}; \
                 font-size: 11px; box-shadow: 0 6px 24px rgba(0,0,0,0.45);",
                theme::PANEL_BORDER, theme::SURFACE_INSET, theme::TEXT,
            ),
            div { style: heading, "{title}" }
            for (key, label, colour) in rows.into_iter() {
                div {
                    key: "{key}",
                    style: "display: flex; align-items: baseline; gap: 8px; \
                            padding: 2px 10px;",
                    span {
                        style: format!(
                            "min-width: 34px; font-weight: 600; color: {};",
                            theme::ACCENT,
                        ),
                        "{key}"
                    }
                    span { style: format!("color: {colour};"), "{label}" }
                }
            }
        }
    }
}

/// What the razor help offers right now, which depends on the tool.
///
/// The panel is up whenever a razor exists — an area on screen is a
/// standing instruction, and the keys that act on it should be in front
/// of you the whole time it is, not only once you have found the tool.
///
/// But *which* keys work depends on what is armed, and a panel that
/// listed keys which do nothing would be worse than no panel. With the
/// razor armed, the verbs are bare letters. From any other tool those
/// letters are that tool's shortcuts, so what actually works is the `k`
/// prefix — and that is what gets listed, read from the keymap rather
/// than from a copy of it, so a rebound prefix relabels itself.
///
/// The side effect is the useful one: draw a razor from Select and the
/// surface teaches you `k`, which works everywhere and which you would
/// otherwise have had to go looking for.
fn razor_help_rows(ed: &Editor) -> Vec<keys::Continuation> {
    if interaction::razor_mode_live(ed) {
        return interaction::RAZOR_KEYS
            .iter()
            .map(|(key, label)| keys::Continuation {
                key: (*key).to_string(),
                label: (*label).to_string(),
                is_group: false,
            })
            .collect();
    }
    let mut rows: Vec<keys::Continuation> = keys::continuations_after("a")
        .into_iter()
        .map(|c| keys::Continuation {
            key: format!("a {}", c.key),
            ..c
        })
        .collect();
    // The way out of the long spelling, since the panel is the only
    // place that would ever mention it.
    rows.push(keys::Continuation {
        key: "x".to_string(),
        label: "Razor tool — then single keys".to_string(),
        is_group: false,
    });
    rows
}

fn razor_help_title(ed: &Editor) -> String {
    if interaction::razor_mode_live(ed) {
        "Razor".to_string()
    } else {
        "Razor · a".to_string()
    }
}

/// The tool a held which-key prefix springs into, if it has one.
///
/// Two keys are both a prefix and a tool. Tap `z` and it waits for a
/// zoom target; hold it and drag, and it *is* the zoom tool for the
/// length of the hold. `v` is the same shape for velocity: tap for the
/// tree, hold and drag to set velocity by hand — which is the gesture
/// REAPER spells `Alt`+drag, and which is better on a prefix because the
/// one key then covers both "shape this by hand" and every velocity
/// command there is.
///
/// A table rather than a field on `Tool`, because it is a fact about the
/// *keymap* — which key opens which tree — and the keymap is
/// configuration. A tool does not know what letter reaches it.
fn spring_tool(key: &str) -> Option<expression_editor_core::Tool> {
    use expression_editor_core::Tool;
    match key {
        "z" => Some(Tool::Zoom),
        "v" => Some(Tool::Velocity),
        _ => None,
    }
}

/// Run a `velocity.*` action, if `action` is one.
///
/// `None` means "not mine", so the caller falls through to the ordinary
/// dispatch. These are handled here rather than in [`keys::dispatch`]
/// because a ramp *outlives its keypress* — it stays live so the wheel
/// can go on adjusting it — and a function handed only `&mut Editor` has
/// nowhere to keep one.
///
/// Pressing a ramp command while that same ramp is already live inverts
/// it rather than opening a second one. Which direction you wanted is
/// something you find out by looking at it, and re-pressing is a faster
/// answer than undo-and-pick-the-other-command. Pressing a *different*
/// one replaces it, from the same baseline, so trying four shapes in a
/// row costs one undo rather than four.
fn velocity_action(
    editor: &mut Signal<Editor>,
    ramp: &mut Signal<Option<crate::velocity_ramp::VelocityRamp>>,
    action: &str,
) -> Option<bool> {
    use expression_editor_tools::velocity::CurvePreset;

    use expression_editor_core::actions::velocity as v;

    let preset = match action {
        a if a == v::RAMP_UP.id => Some(CurvePreset::Rise),
        a if a == v::RAMP_DOWN.id => Some(CurvePreset::Fall),
        a if a == v::RAMP_SMOOTH.id => Some(CurvePreset::RiseSmooth),
        _ => None,
    };

    if let Some(preset) = preset {
        // Already showing this shape? Turn it over.
        let same = ramp.read().as_ref().map(|r| r.preset()) == Some(preset);
        // Taken out before use: a `write()` guard held across the body
        // would still be live at `ramp.set`, which is a second borrow.
        let taken = ramp.write().take();
        if same && let Some(mut live) = taken {
            live.invert(&mut editor.write());
            ramp.set(Some(live));
            return Some(true);
        } else if let Some(live) = taken {
            // A different shape replaces the old one — but from the
            // *original* velocities, so the shapes do not compound.
            live.revert(&mut editor.write());
        }
        let notes = editor.read().selection.notes.clone();
        let opened = crate::velocity_ramp::VelocityRamp::open(&mut editor.write(), preset, &notes);
        let ok = opened.is_some();
        ramp.set(opened);
        return Some(ok);
    }

    // The rest act once and leave nothing live to adjust, so they close
    // any open ramp first — committing it, not reverting it. You asked
    // for the ramp; the next command is the one after it.
    let closing = [
        v::ACCENT.id,
        v::COMPRESS.id,
        v::EXPAND.id,
        v::HUMANISE.id,
        v::FLATTEN.id,
        v::PANEL.id,
    ]
    .contains(&action);
    if !closing {
        return None;
    }
    ramp.set(None);

    if action == v::PANEL.id {
        // Chrome, not an edit. `try_consume_context` because a host that
        // mounts the canvas without the editor's own chrome — the
        // screenshot harness does — has no window to toggle, and that is
        // a supported configuration rather than a missing one.
        if let Some(panel) = try_consume_context::<crate::velocity_sink::PanelOpen>() {
            let mut open = panel.0;
            let now = !open();
            open.set(now);
            return Some(true);
        }
        return Some(false);
    }

    let notes = editor.read().selection.notes.clone();
    let mut ed = editor.write();
    Some(match action {
        a if a == v::ACCENT.id => velocity_ramp::accent(&mut ed, &notes),
        a if a == v::COMPRESS.id => velocity_ramp::dynamics(&mut ed, &notes, -0.35),
        a if a == v::EXPAND.id => velocity_ramp::dynamics(&mut ed, &notes, 0.35),
        a if a == v::HUMANISE.id => velocity_ramp::humanise(&mut ed, &notes),
        a if a == v::FLATTEN.id => velocity_ramp::flatten(&mut ed, &notes),
        _ => false,
    })
}

/// Put the real chord names on the chord tree's degree rows.
///
/// The keymap can only say "fire the chord on this degree", because it
/// is a static file and the answer depends on the key, the scale, the
/// depth and the inversion. Which makes the generic label almost
/// useless: the entire point of choosing by degree is that you are
/// thinking in `I` and `vi`, and a panel that will not tell you the
/// third degree of F Dorian is a panel you have to do theory to use.
///
/// Rewritten here rather than in `keys::label_for` because that has no
/// editor to ask — it maps an action id to a string and nothing else.
fn name_chord_rows(ed: &Editor, rows: Vec<keys::Continuation>) -> Vec<keys::Continuation> {
    rows.into_iter()
        .map(|c| {
            let Ok(degree) = c.key.parse::<usize>() else {
                return c;
            };
            if !(1..=7).contains(&degree) {
                return c;
            }
            let name = ed.chord_gun.chord_name(degree);
            if name.is_empty() {
                return c;
            }
            keys::Continuation { label: name, ..c }
        })
        .collect()
}

/// Name the chord tree after the scale it is currently firing from.
fn chord_panel_title(ed: &Editor) -> String {
    // Only while the chord prefix is the one being typed — every other
    // sequence keeps the bare panel it has always had. Empty rather than
    // `None`, because the heading is always *rendered*; see `KeyPanel`.
    if keys::held_prefix().as_deref() != Some("c") {
        return String::new();
    }
    let gun = &ed.chord_gun;
    format!(
        "{} · {} · inv {}",
        gun.scale_name(),
        gun.depth.name(),
        gun.inversion,
    )
}

/// What the key panel should be showing, if anything.
///
/// One decision in one place, so the panel is *one* mounted component
/// rather than a choice between two. It used to be an `if`/`else if`
/// with a `KeyPanel` in each arm, and switching arms swapped a titled
/// panel for an untitled one — the same component with a structurally
/// different body. That is what a template diff walks a stale path
/// through, and what the "invalid key" panic out of blitz-dom's
/// `node_at_path` is: a mutation aimed at a node the last render's
/// shape had and this one does not.
fn key_panel(
    ed: &Editor,
    which_key: Vec<keys::Continuation>,
) -> Option<(String, Vec<keys::Continuation>)> {
    if !which_key.is_empty() {
        return Some((chord_panel_title(ed), name_chord_rows(ed, which_key)));
    }
    if !ed.razor.is_empty() {
        return Some((razor_help_title(ed), razor_help_rows(ed)));
    }
    None
}
