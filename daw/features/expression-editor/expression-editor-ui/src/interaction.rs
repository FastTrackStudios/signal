//! Drag state and the pointer/keyboard logic that drives it.
//!
//! Kept out of the rsx so it stays readable and testable: every handler
//! here is a plain function over `&mut Editor`, and the component just
//! routes events into them.

use expression_editor_core::doc::{Dimension, NoteId, Point, Target};
use expression_editor_core::edit::Edit;
use expression_editor_core::handles::{self, Handle};
use expression_editor_core::menu::Command;
use expression_editor_core::mouse::{Action, Context, Gesture};
use expression_editor_core::razor::{RazorArea, RazorAxis};
use expression_editor_core::tools::{self, Hit, Mods};
use expression_editor_core::zoom::ZoomModes;
use expression_editor_core::{Editor, Mode, Shape, Tool};

/// What the pointer is currently doing. `None` between gestures.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Drag {
    #[default]
    None,
    /// Right-drag or ctrl+shift-drag.
    /// A freshly inserted note, still under the pointer.
    ///
    /// One gesture with two phases, switchable *while it runs*: without
    /// Shift you are moving the note, with Shift you are sizing it. Alt
    /// alone therefore places a note of the current grid length and lets
    /// you carry it to where it belongs; Alt+Shift starts you sizing it
    /// instead.
    ///
    /// The switch is the point. Insert, carry it into place, hold Shift
    /// to pin it there and pull out its length, release Shift and carry
    /// it again at that new length — without lifting the button or
    /// making a second gesture of it.
    ///
    /// `anchor` and `base` are re-taken every time the phase changes,
    /// which is what stops the note jumping when it does: each phase
    /// measures from where the pointer was when *it* began, not from
    /// where the whole gesture began.
    InsertNote {
        note: NoteId,
        /// The pointer position when the current phase began.
        anchor: (f64, f64),
        /// The note's span when the current phase began.
        base: (f64, f64),
        /// Whether the current phase is sizing rather than moving.
        sizing: bool,
    },
    /// The zoom *tool*'s drag.
    ///
    /// Distinct from [`Drag::Zoom`], which is REAPER's anchored zoom on
    /// `Ctrl+Alt` and maps a vertical drag to *time*. This one reads the
    /// direction as the axis: up and down zoom pitch, left and right
    /// zoom time, a diagonal does both. You point at the axis you want
    /// instead of remembering a modifier for it.
    ///
    /// Everything is measured against the state at press — the units per
    /// pixel and the point under the cursor — so the drag is a ratio
    /// against a fixed base rather than a series that compounds each
    /// frame and runs away.
    ZoomTool {
        origin: (f64, f64),
        base_units_per_px: f64,
        base_px_per_row: f64,
        /// What was under the press, and stays under it.
        ///
        /// The vertical half is a *slot*, not a model row. Rows are what
        /// the document numbers and the fold can collapse several of
        /// them onto one slot; slots are what the camera actually
        /// measures in. Keeping a row here meant the restore below
        /// silently compared the two, which is only harmless when the
        /// fold is empty.
        anchor_t: f64,
        anchor_slot: f64,
        /// Alt: sweep a rectangle and zoom into it on release, rather
        /// than zooming continuously as you drag.
        marquee: bool,
        current: (f64, f64),
    },
    Pan {
        last: (f64, f64),
    },
    Marquee {
        origin: (f64, f64),
        current: (f64, f64),
        additive: bool,
    },
    /// Freehand: samples accumulate at pointer resolution and commit
    /// continuously, so the stroke feels like spraypaint rather than
    /// appearing on release.
    Pen {
        notes: Vec<NoteId>,
        start: (f64, f64),
        samples: Vec<(f64, f64)>,
    },
    /// A shaped ramp between two points; stays "live" after release so
    /// the shape buttons can restyle it.
    Curve {
        notes: Vec<NoteId>,
        start: (f64, f64),
        current: (f64, f64),
    },
    Erase {
        notes: Vec<NoteId>,
        start_t: f64,
        current_t: f64,
    },
    /// Shift-drag: vertical transposition against tuning targets.
    Transpose {
        notes: Vec<NoteId>,
        origin: (f64, f64),
        base_rows: Vec<i32>,
        applied: i32,
    },
    /// Note Draw / Select: note movement, on whichever axes [`Axis`]
    /// allows.
    MoveNotes {
        notes: Vec<NoteId>,
        origin: (f64, f64),
        applied_rows: i32,
        applied_time: f64,
        axis: Axis,
    },
    /// Dragging a note edge scales the whole selection — lengths for
    /// [`Action::StretchNotes`], positions for
    /// [`Action::StretchNotePositions`].
    ///
    /// Captures the notes as they were at press and rewrites from that
    /// capture every move, for the reason every other gesture here does:
    /// a factor applied sixty times is a factor of sixty.
    Stretch {
        /// `(id, start, end)` as captured.
        base: Vec<(NoteId, f64, f64)>,
        /// The fixed end — the opposite edge from the one grabbed.
        pivot: f64,
        /// Where the grabbed edge was, so the factor is a ratio of
        /// distances from the pivot rather than of raw pixels.
        origin_t: f64,
        /// Positions rather than lengths: arpeggiate.
        positions: bool,
    },
    /// Dragging an area's edge. `stretch` pulls the contents with it.
    RazorEdge {
        index: usize,
        /// Which edge was grabbed.
        start_edge: bool,
        stretch: bool,
        /// The area as it was at press.
        ///
        /// Not read back from the set each frame, because this drag
        /// rewrites that entry as it goes — so the set holds the *last*
        /// frame's rectangle, and a stretch recomputed from the
        /// gesture's start needs the rectangle the material still
        /// matches. The fixed end also has to stay put: taking it from a
        /// rewritten entry let rounding walk it a fraction per frame.
        base: RazorArea,
    },
    /// Anchored zoom: vertical drag scales the view about the press.
    Zoom {
        origin: (f64, f64),
        /// Units per pixel at press, so the drag is a ratio against a
        /// fixed base rather than a compounding series.
        base_units_per_px: f64,
        /// The time under the press, which stays under it throughout.
        anchor_t: f64,
    },
    /// Selecting whatever the pointer sweeps over, with no rectangle.
    SelectTouched {
        /// Flip what is swept rather than adding to it.
        toggle: bool,
        /// Notes this sweep has already decided on.
        seen: std::collections::HashSet<NoteId>,
    },
    /// Alt-drag: scale and invert expression about the effective
    /// center.
    Scale {
        notes: Vec<NoteId>,
        origin: (f64, f64),
        applied: f64,
    },
    Resize {
        note: NoteId,
        start_edge: bool,
        original: (f64, f64),
    },
    SplitDrag {
        note: NoteId,
        from: f64,
    },
    NoteErase,
    /// Dragging out a new razor rectangle.
    RazorCreate {
        origin: (f64, f64),
        current: (f64, f64),
        /// The area as it would be committed right now, snapped.
        ///
        /// Resolved on every move rather than recomputed at release, so
        /// the rectangle the surface draws while you sweep and the one
        /// that lands are the same object rather than two computations
        /// that have to agree. They did not: the release snapped and
        /// nothing was drawn until it happened, so the area appeared
        /// somewhere other than where the drag had been.
        ///
        /// `None` until the sweep is big enough to mean anything, which
        /// is also what keeps a click from leaving a zero-width razor.
        pending: Option<RazorArea>,
    },
    /// Moving or copying an existing area's contents.
    RazorDrag {
        area: RazorArea,
        index: usize,
        origin: (f64, f64),
        copy: bool,
        applied_t: f64,
        applied_rows: i32,
        /// Move the rectangle and leave the notes where they are.
        area_only: bool,
        axis: Axis,
    },
    /// Painting notes along a swept path.
    Paint {
        last_cell: Option<(i64, i32)>,
        snap: bool,
    },
    /// Freehand drawing into a pinned controller lane.
    CcPen {
        number: u8,
        /// Previous sample, so a fast drag interpolates instead of
        /// leaving a staircase.
        last: Option<(f64, f64)>,
    },
    /// A shaped ramp across a controller lane. Like [`Drag::Curve`] it
    /// survives release, so the toolbar's shape buttons restyle the
    /// stroke you just drew instead of the one before it.
    CcLine {
        number: u8,
        start: (f64, f64),
        current: (f64, f64),
    },
    /// Sweeping a controller lane back to its default.
    CcErase {
        number: u8,
        start_t: f64,
        current_t: f64,
    },
    /// Vertical drag scales the swept range's depth about a pivot.
    ///
    /// Rebuilt from `base` on every move rather than scaling the live
    /// curve, because `apply_live` writes straight through and a chain
    /// of live scales would compound into an exponential.
    CcScale {
        number: u8,
        t0: f64,
        t1: f64,
        /// The dimension's points as they were when the gesture opened.
        base: Vec<Point>,
        /// Widest range the drag has covered, so shrinking it back
        /// restores what the wider sweep had already scaled.
        spanned: (f64, f64),
        /// The value the range is scaled about — its own mean, so a
        /// factor of 0 flattens it to where it already sat rather than
        /// slamming it to zero.
        pivot: f64,
        origin_y: f64,
    },
    /// Dragging a pitch-drawing anchor.
    DraftAnchor {
        index: usize,
    },
    /// Dragging a timing separator. The law is captured at press, from
    /// where on the line the grab landed — reading it live would let the
    /// gesture change meaning halfway through.
    Separator {
        sep: expression_editor_core::Separator,
        law: expression_editor_core::StretchLaw,
    },
    /// One of the seven note handles, on every note it applies to.
    ///
    /// The note actually grabbed is first; the rest of the selection
    /// follows. Each carries its *own* captured base, so the gesture is
    /// a relative change applied everywhere rather than one value copied
    /// across — pull the pressure handle on one of six selected notes
    /// and all six rise by the same amount from wherever each of them
    /// was, which is the only reading that preserves what you shaped
    /// earlier.
    ///
    /// One entry when nothing is selected, or when the grabbed note is
    /// the only selected one, so the single-note case is not a special
    /// case anywhere below.
    Handle(Box<Vec<handles::HandleDrag>>),
    /// Dragging out a temporary note: a range inside one note that the
    /// handles will then address.
    TempNote {
        note: NoteId,
        origin_t: f64,
        current_t: f64,
    },
    /// Not a drag at all — the signal that the surface should open a
    /// context menu here. It rides `Drag` because that is already the
    /// one value `pointer_down` hands back to the component, and a
    /// second return channel for one gesture is not worth the churn.
    ContextMenu {
        x: f64,
        y: f64,
        under: Option<NoteId>,
        t: f64,
        /// The row clicked, so the menu can tell whether the click
        /// landed inside a razor area. Time alone cannot: an area is a
        /// rectangle, and a click at the right moment on the wrong row
        /// is outside it.
        row: i32,
    },
    /// Vertical drag over notes edits velocity.
    Velocity {
        notes: Vec<NoteId>,
        origin_y: f64,
        fine: bool,
        applied: f64,
    },
}

/// Which axes a move gesture is allowed to travel on.
///
/// The map distinguishes four move actions and, until this existed, all
/// four did the same thing — which meant Shift on a note claimed to lock
/// an axis and did not, and the Riffer preset's primary gesture
/// (`MoveNoteVertically`, its plain note drag) moved in time as well.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Axis {
    #[default]
    Both,
    Horizontal,
    Vertical,
    /// Free until the drag commits, then locked to whichever axis it
    /// travelled furthest along first. REAPER's Shift-drag, and the
    /// reason it is not simply "the larger delta each frame": a gesture
    /// that re-decides every frame flickers between axes at the corner.
    Commit(Option<bool>),
}

impl Axis {
    /// Resolve against a drag delta, committing if this is the first
    /// movement worth deciding on. Returns `(horizontal, vertical)`.
    fn allows(&mut self, dx: f64, dy: f64) -> (bool, bool) {
        match self {
            Axis::Both => (true, true),
            Axis::Horizontal => (true, false),
            Axis::Vertical => (false, true),
            Axis::Commit(decided) => {
                if decided.is_none() {
                    // A threshold, not a first-pixel decision: the first
                    // pixel of a drag is noise, and committing on it
                    // locks half of them to the wrong axis.
                    const DEAD_PX: f64 = 4.0;
                    if dx.abs().max(dy.abs()) >= DEAD_PX {
                        *decided = Some(dx.abs() >= dy.abs());
                    }
                }
                match decided {
                    Some(true) => (true, false),
                    Some(false) => (false, true),
                    None => (false, false),
                }
            }
        }
    }
}

impl Drag {
    pub fn is_active(&self) -> bool {
        !matches!(self, Drag::None)
    }

    /// The gesture that shape buttons target first — a Curve stroke
    /// stays selected after release.
    pub fn live_curve(&self) -> Option<(&[NoteId], f64, f64)> {
        match self {
            Drag::Curve {
                notes,
                start,
                current,
            } => Some((notes, start.0, current.0)),
            _ => None,
        }
    }

    /// The controller ramp the shape buttons target, same contract as
    /// [`Drag::live_curve`] but for a CC dimension.
    pub fn live_cc_line(&self) -> Option<(u8, f64, f64)> {
        match self {
            Drag::CcLine {
                number,
                start,
                current,
            } => Some((*number, start.0, current.0)),
            _ => None,
        }
    }
}

/// How close to the drawn controller curve counts as grabbing it.
const CC_EVENT_PX: f64 = 5.0;

/// Which mouse-modifier context `(x, y)` falls in.
///
/// CC edit mode outranks everything: while it is on, the roll *is* that
/// controller's dimension, so the razor and the notes behind it are not what
/// the pointer is addressing.
///
/// Razor areas outrank notes: once you have drawn a rectangle, dragging
/// inside it must operate on the region, not on whatever note happens
/// to be under the pointer.
pub fn context_at(ed: &Editor, x: f64, y: f64) -> Context {
    let t = ed.camera.t_at(x);
    let row = ed.camera.pitch_at(y, ed.viewport).round() as i32;

    if let Some(number) = ed.cc_edit {
        // Near the existing curve is `CcEvent`, open dimension is `CcLane` —
        // the same distinction REAPER draws, so a mouse map written for
        // REAPER lands on the right binding without translation.
        let on_curve = ed
            .doc
            .cc
            .get(number)
            .map(|l| {
                let v = l.curve.sample(t, l.default_value());
                (expression_editor_core::cc::cc_y(v, ed.viewport.h) - y).abs() <= CC_EVENT_PX
            })
            .unwrap_or(false);
        return if on_curve {
            Context::CcEvent
        } else {
            Context::CcLane
        };
    }

    if let Some((_, area)) = ed.razor.at(t, row) {
        let edge_px = 6.0;
        if (ed.camera.x(area.t0) - x).abs() <= edge_px
            || (ed.camera.x(area.t1) - x).abs() <= edge_px
        {
            return Context::RazorEdge;
        }
        return Context::RazorArea;
    }
    match ed.hit_test(x, y) {
        Hit::ZoneSplit { .. } => Context::ZoneSplit,
        Hit::NoteEdge { .. } => Context::NoteEdge,
        Hit::Note { .. } | Hit::CurvePoint { .. } => Context::Note,
        Hit::Empty { .. } => Context::PianoRoll,
    }
}

/// Begin a gesture at element coordinates `(x, y)`.
///
/// Resolves the binding through [`Editor::mouse`] and then executes it.
/// This function decides *nothing* about policy — which modifier does
/// what lives in the map, so the drum, guitar and Melodyne editors can
/// disagree without forking this code.
pub fn pointer_down(ed: &mut Editor, x: f64, y: f64, mods: Mods, button: u16) -> Drag {
    let gesture = match button {
        2 => Gesture::RightClick,
        1 => Gesture::MiddleClick,
        _ => Gesture::Drag,
    };

    // Timing separators outrank the notes they sit between: in timing
    // mode the boundary is what the pointer is addressing.
    if let Some(drag) = separator_press(ed, x, y, gesture) {
        return drag;
    }

    // The note handles sit in front of everything, because they are
    // drawn in front of everything: a press that visibly lands on a
    // handle must not fall through to the note or the roll behind it.
    // Right-click is the exception the manual calls out — on the
    // amplitude handle it mutes rather than dragging.
    if let Some(drag) = handle_press(ed, x, y, mods, gesture) {
        return drag;
    }

    let context = context_at(ed, x, y);
    let action = ed.mouse.resolve_for(context, gesture, mods, ed.tool);
    if action != Action::None
        && let Some(drag) = run_action(ed, action, x, y, mods)
    {
        return drag;
    }
    legacy_pointer_down(ed, x, y, mods, button)
}

/// Execute a resolved action, returning the drag it opens.
///
/// `None` means "the map had nothing useful here" and the caller falls
/// through to the tool-driven path, which still owns the expression
/// tools (pen, curve, eraser).
/// The razor area a sweep from `origin` to `current` currently means.
///
/// The one place that turns two pixel points into an area, so the
/// rectangle drawn during the drag and the one committed on release
/// cannot disagree — they are the same value, resolved once per move and
/// carried on the drag.
///
/// `None` for a sweep too small to be deliberate. A click that leaves a
/// hairline razor is worse than one that leaves nothing: it is invisible
/// and it still swallows the next operation.
fn resolve_razor(
    ed: &Editor,
    origin: (f64, f64),
    current: (f64, f64),
    mods: Mods,
) -> Option<RazorArea> {
    // In pixels, so the threshold means the same thing at every zoom.
    // A time threshold would be unreachable when zoomed out and
    // trivially exceeded when zoomed in.
    if (current.0 - origin.0).abs() < 3.0 {
        return None;
    }
    let t0 = ed.camera.t_at(origin.0);
    let t1 = ed.camera.t_at(current.0);
    let r0 = ed.camera.pitch_at(origin.1, ed.viewport).round() as i32;
    let r1 = ed.camera.pitch_at(current.1, ed.viewport).round() as i32;
    // Shift is the escape from the grid, matching every other snapped
    // gesture on the surface.
    let (t0, t1) = if ed.grid.enabled && !mods.shift {
        (ed.snap_time(t0), ed.snap_time(t1))
    } else {
        (t0, t1)
    };
    let area = RazorArea::new(t0, t1, r0, r1);
    // Snapping can collapse a short sweep onto one grid line. That is a
    // real outcome of the gesture, not an error, but an empty area is
    // still nothing worth committing or drawing.
    (!area.is_empty()).then_some(area)
}

fn run_action(ed: &mut Editor, action: Action, x: f64, y: f64, mods: Mods) -> Option<Drag> {
    let t = ed.camera.t_at(x);
    let row = ed.camera.pitch_at(y, ed.viewport).round() as i32;
    let under = match ed.hit_test(x, y) {
        Hit::Note { id, .. } | Hit::NoteEdge { id, .. } => Some(id),
        _ => None,
    };
    if action.is_edit() {
        ed.begin_gesture();
    }

    match action {
        Action::Pan => Some(Drag::Pan { last: (x, y) }),

        // ── razor ────────────────────────────────────────────────────
        Action::RazorCreate => Some(Drag::RazorCreate {
            origin: (x, y),
            current: (x, y),
            pending: None,
        }),
        Action::RazorMoveContents
        | Action::RazorMoveContentsNoSnap
        | Action::RazorCopyContents
        | Action::RazorMoveAreaOnly
        | Action::RazorMoveVertically
        | Action::RazorMoveHorizontally => {
            let (index, area) = ed.razor.at(t, row)?;
            Some(Drag::RazorDrag {
                area,
                index,
                origin: (x, y),
                copy: action == Action::RazorCopyContents,
                applied_t: 0.0,
                applied_rows: 0,
                // Moving the rectangle without its contents is what
                // makes the razor a *selection* you can reposition, and
                // it was the one razor gesture the map bound and nothing
                // performed.
                area_only: action == Action::RazorMoveAreaOnly,
                axis: match action {
                    Action::RazorMoveVertically => Axis::Vertical,
                    Action::RazorMoveHorizontally => Axis::Horizontal,
                    _ => Axis::Both,
                },
            })
        }
        Action::RazorAddArea => {
            // Adds to the set rather than replacing it: the drag that
            // follows is an ordinary create, and `RazorSet::add` merges
            // it in.
            Some(Drag::RazorCreate {
                origin: (x, y),
                current: (x, y),
                pending: None,
            })
        }
        Action::RazorResizeArea | Action::RazorStretchContents => {
            let (index, area) = ed.razor.at(t, row)?;
            let start_edge = (t - area.t0).abs() <= (t - area.t1).abs();
            Some(Drag::RazorEdge {
                index,
                start_edge,
                stretch: action == Action::RazorStretchContents,
                base: area,
            })
        }
        Action::RazorRemoveArea => {
            ed.razor.remove_at(t, row);
            Some(Drag::None)
        }
        Action::RazorDeleteContents => {
            let (_, area) = ed.razor.at(t, row)?;
            expression_editor_core::razor::delete_contents(&mut ed.doc, area);
            Some(Drag::None)
        }
        Action::RazorClearAll => {
            ed.razor.clear();
            Some(Drag::None)
        }

        // ── notes ────────────────────────────────────────────────────
        Action::SelectNote => {
            let id = under?;
            ed.selection.set_single(id);
            Some(Drag::None)
        }
        Action::AddNoteToSelection => {
            ed.selection.add(under?);
            Some(Drag::None)
        }
        Action::ToggleNoteSelection => {
            ed.selection.toggle(under?);
            Some(Drag::None)
        }
        Action::DeselectAll => {
            ed.selection.clear();
            Some(Drag::None)
        }
        Action::SelectNoteAndLater | Action::SelectNoteAndLaterSameRow => {
            let id = under?;
            let (start, note_row) = {
                let n = ed.doc.note(id)?;
                (n.start, n.row)
            };
            let same_row = action == Action::SelectNoteAndLaterSameRow;
            ed.selection.notes = ed
                .doc
                .notes
                .iter()
                .filter(|n| n.start >= start && (!same_row || n.row == note_row))
                .map(|n| n.id)
                .collect();
            Some(Drag::None)
        }
        Action::ToggleNoteMute => {
            let id = under?;
            ed.apply_live(&Edit::ToggleMuted { notes: vec![id] });
            Some(Drag::None)
        }
        Action::EraseNote => {
            let id = under?;
            ed.apply_live(&Edit::DeleteNotes(vec![id]));
            Some(Drag::None)
        }
        Action::DoubleNoteLength | Action::HalveNoteLength => {
            let id = under?;
            let factor = if action == Action::DoubleNoteLength {
                2.0
            } else {
                0.5
            };
            ed.apply_live(&Edit::ScaleLength {
                notes: vec![id],
                factor,
            });
            Some(Drag::None)
        }
        Action::EditNoteVelocity | Action::EditNoteVelocityFine => {
            let notes = tools::gesture_targets(&ed.selection, under);
            if notes.is_empty() {
                return None;
            }
            Some(Drag::Velocity {
                notes,
                origin_y: y,
                fine: action == Action::EditNoteVelocityFine,
                applied: 0.0,
            })
        }
        Action::CopyNote | Action::CopyNoteNoSnap => {
            let notes = tools::gesture_targets(&ed.selection, under);
            if notes.is_empty() {
                return None;
            }
            // Duplicate in place; the drag then moves the copies, so
            // the originals stay put exactly where they were.
            ed.apply_live(&Edit::CopyNotes {
                notes: notes.clone(),
                time_delta: 0.0,
                row_delta: 0,
            });
            let copies: Vec<NoteId> = ed
                .doc
                .notes
                .iter()
                .rev()
                .take(notes.len())
                .map(|n| n.id)
                .collect();
            ed.selection.notes = copies.clone();
            Some(Drag::MoveNotes {
                notes: copies,
                origin: (x, y),
                applied_rows: 0,
                applied_time: 0.0,
                axis: Axis::Both,
            })
        }
        Action::MoveNote
        | Action::MoveNoteNoSnap
        | Action::MoveNoteOneAxis
        | Action::MoveNoteHorizontally
        | Action::MoveNoteVertically
        | Action::MoveNoteIgnoringSelection => {
            let id = under?;
            // The whole point of `MoveNoteIgnoringSelection`: leave the
            // selection alone and move only what was grabbed.
            let notes = if action == Action::MoveNoteIgnoringSelection {
                vec![id]
            } else {
                if !ed.selection.contains(id) {
                    ed.selection.set_single(id);
                }
                ed.selection.notes.clone()
            };
            Some(Drag::MoveNotes {
                notes,
                origin: (x, y),
                applied_rows: 0,
                applied_time: 0.0,
                axis: match action {
                    Action::MoveNoteHorizontally => Axis::Horizontal,
                    Action::MoveNoteVertically => Axis::Vertical,
                    Action::MoveNoteOneAxis => Axis::Commit(None),
                    _ => Axis::Both,
                },
            })
        }
        Action::MoveNoteEdge | Action::MoveNoteEdgeNoSnap => {
            let Hit::NoteEdge { id, start_edge } = ed.hit_test(x, y) else {
                return None;
            };
            let n = ed.doc.note(id)?;
            let original = (n.start, n.end);
            Some(Drag::Resize {
                note: id,
                start_edge,
                original,
            })
        }
        // Both stretches grab an end and scale the whole selection about
        // the opposite one — lengths, or positions for the arpeggiate
        // variant. The pivot is the far edge of the *selection*, not of
        // the note grabbed: stretching a phrase about the middle of one
        // of its notes is not a gesture anyone wants.
        Action::StretchNotes | Action::StretchNotePositions => {
            let Hit::NoteEdge { id, start_edge } = ed.hit_test(x, y) else {
                return None;
            };
            if !ed.selection.contains(id) {
                ed.selection.set_single(id);
            }
            let base: Vec<(NoteId, f64, f64)> = ed
                .selection
                .notes
                .iter()
                .filter_map(|&n| ed.doc.note(n).map(|note| (n, note.start, note.end)))
                .collect();
            if base.is_empty() {
                return None;
            }
            let pivot = if start_edge {
                base.iter().map(|b| b.2).fold(f64::MIN, f64::max)
            } else {
                base.iter().map(|b| b.1).fold(f64::MAX, f64::min)
            };
            // A grab exactly on the pivot has no distance to form a
            // ratio from, and would divide by zero on the first move.
            if (t - pivot).abs() < 1e-6 {
                return None;
            }
            Some(Drag::Stretch {
                base,
                pivot,
                origin_t: t,
                positions: action == Action::StretchNotePositions,
            })
        }
        Action::InsertNote | Action::InsertNoteNoSnap | Action::InsertNoteDragToExtend => {
            Some(begin_new_note(ed, x, y, mods))
        }
        Action::EraseNotes => {
            // The sweeping eraser: delete what is under the press, then
            // keep deleting whatever the drag crosses.
            if let Some(id) = under {
                ed.apply_live(&Edit::DeleteNotes(vec![id]));
            }
            Some(Drag::NoteErase)
        }
        Action::SetNoteChannelHigher | Action::SetNoteChannelLower => {
            let notes = tools::gesture_targets(&ed.selection, under);
            if notes.is_empty() {
                return None;
            }
            ed.apply_live(&Edit::NudgeChannel {
                notes,
                delta: if action == Action::SetNoteChannelHigher {
                    1
                } else {
                    -1
                },
            });
            Some(Drag::None)
        }
        Action::SelectTouched | Action::ToggleSelectTouched => {
            let toggle = action == Action::ToggleSelectTouched;
            let mut drag = Drag::SelectTouched {
                toggle,
                seen: Default::default(),
            };
            // The note under the press counts as touched; a sweep that
            // ignored its own starting point would be surprising.
            touch_at(ed, &mut drag, x, y);
            Some(drag)
        }
        Action::SelectAllInMeasure => {
            ed.selection.notes = ed.notes_in_measure(t);
            Some(Drag::None)
        }
        Action::EditLyric => {
            // The inspector owns the text field; this arms it on the
            // note that was double-clicked.
            let id = under?;
            ed.selection.set_single(id);
            ed.editing_lyric = Some(id);
            Some(Drag::None)
        }
        Action::ZoomAnchored => Some(Drag::Zoom {
            origin: (x, y),
            base_units_per_px: ed.camera.units_per_px,
            anchor_t: t,
        }),
        Action::Audition => {
            audition(ed, x, y);
            Some(Drag::None)
        }
        Action::PaintNotes | Action::PaintNotesNoSnap => {
            let snap = action == Action::PaintNotes;
            let mut drag = Drag::Paint {
                last_cell: None,
                snap,
            };
            paint_at(ed, &mut drag, x, y);
            Some(drag)
        }
        Action::MarqueeSelect | Action::MarqueeAdd | Action::MarqueeToggle => {
            if action == Action::MarqueeSelect {
                ed.selection.clear();
            }
            Some(Drag::Marquee {
                origin: (x, y),
                current: (x, y),
                additive: action != Action::MarqueeSelect,
            })
        }
        Action::MovePlayhead | Action::MovePlayheadNoSnap => {
            ed.playhead = Some(if action == Action::MovePlayhead {
                ed.snap_time(t)
            } else {
                t
            });
            Some(Drag::None)
        }
        Action::SelectRow => {
            ed.selection.notes = ed
                .doc
                .notes
                .iter()
                .filter(|n| n.row == row)
                .map(|n| n.id)
                .collect();
            Some(Drag::None)
        }
        // ── controller lanes ─────────────────────────────────────────
        // All four need a dimension to act on. Outside CC edit mode there is
        // no active controller, so they fall through to the tool path
        // rather than inventing one.
        Action::EditCcEvents => {
            let number = ed.cc_edit?;
            let mut drag = Drag::CcPen { number, last: None };
            cc_draw(ed, &mut drag, x, y);
            Some(drag)
        }
        Action::DrawCcLine => {
            let number = ed.cc_edit?;
            Some(Drag::CcLine {
                number,
                start: (x, y),
                current: (x, y),
            })
        }
        Action::EraseCcEvents => {
            let number = ed.cc_edit?;
            Some(Drag::CcErase {
                number,
                start_t: t,
                current_t: t,
            })
        }
        Action::ScaleCcEvents => {
            let number = ed.cc_edit?;
            // The gesture opens with no width; the range grows as the
            // drag sweeps sideways while the vertical offset sets depth.
            Some(Drag::CcScale {
                number,
                t0: t,
                t1: t,
                base: ed
                    .doc
                    .cc
                    .get(number)
                    .map(|l| l.curve.points().to_vec())
                    .unwrap_or_default(),
                spanned: (t, t),
                pivot: cc_mean(ed, number, t, t),
                origin_y: y,
            })
        }

        Action::ContextMenu => Some(Drag::ContextMenu {
            x,
            y,
            under,
            t,
            row,
        }),

        // Expression tools stay with the tool-driven path: `None` here
        // is the deliberate hand-off, not a gap.
        Action::ActiveTool | Action::PenOverride => None,

        // ── deliberately not routed ──────────────────────────────────
        //
        // Listed one by one rather than swept up by a `_` arm, and that
        // is the point of this block. The wildcard that used to sit here
        // meant a binding could name an action nothing performed and
        // nobody would hear about it — ten of them had, including
        // `StretchNotes` on Shift+edge and the Riffer preset's *primary*
        // note drag, all of them quietly doing whatever the legacy path
        // did instead. With the arms explicit, a new `Action` cannot be
        // added without deciding this question, and an existing one
        // cannot be dropped by accident.
        Action::None => None,
        // Fall through to the tool path, which owns freehand insertion
        // and already reads the same modifiers.
        Action::InsertNoteDragToExtendNoSnap
        | Action::InsertNoteDragToMove
        | Action::InsertNoteDragToEditVelocity
        | Action::PaintRowOfNotes => None,
        // Handled by `legacy_pointer_down`'s Shift/Alt expression path,
        // which owns the dimension-scaling gestures.
        Action::ScaleExpression | Action::TransposeSnapped => None,
        // Guitar/bass badges: the string roll owns these, and routing
        // them here would edit a note that has no string to cycle.
        Action::CycleArticulation | Action::CycleString => None,
    }
}

/// Resolve a press against the timing separators.
///
/// Returns `None` when the press was not on one, so the caller falls
/// through to the ordinary path.
fn separator_press(ed: &mut Editor, x: f64, y: f64, gesture: Gesture) -> Option<Drag> {
    if !ed.timing_mode {
        return None;
    }
    let grab = crate::canvas::SEPARATOR_GRAB_PX;
    let line = crate::canvas::separators(ed)
        .into_iter()
        .find(|s| (s.x - x).abs() <= grab)?;

    // Double-click puts the boundary on the beat, which is the fastest
    // way to fix a phrase that drifted.
    if gesture == Gesture::DoubleClick {
        let step = ed.grid.step(ed.units_per_beat());
        let to = expression_editor_core::timing::snap_to_beat(line.sep.t, ed.doc.start, step);
        ed.begin_gesture();
        for e in expression_editor_core::timing::plan(
            &ed.doc,
            line.sep,
            to,
            expression_editor_core::StretchLaw::BothStretch,
        ) {
            ed.apply_live(&e);
        }
        return Some(Drag::None);
    }

    let law = expression_editor_core::StretchLaw::at(y, ed.viewport.h);
    ed.begin_gesture();
    Some(Drag::Separator { sep: line.sep, law })
}

/// Route a press while a pitch drawing is open.
///
/// The draft owns the surface: while it is up, a click is an anchor and
/// nothing else. That is deliberate — the drawing is a modal state with
/// an explicit apply, and letting notes be selected or moved underneath
/// it would make "what does Escape throw away?" unanswerable.
pub fn draft_press(
    ed: &Editor,
    draft: &mut expression_editor_core::PitchDraft,
    x: f64,
    y: f64,
) -> Drag {
    let t = ed.camera.t_at(x);
    let Some(note) = ed.doc.note(draft.note) else {
        return Drag::None;
    };
    // Values are semitones from the note's row, the same units the
    // pitch curve uses.
    let value = ed.camera.pitch_at(y, ed.viewport) - note.row as f64;
    let tolerance = ed.camera.units_per_px * expression_editor_core::draft::GRAB_PX;

    match draft.anchor_at(t, tolerance) {
        Some(i) => {
            // One checkpoint for the whole drag, not one per move.
            draft.begin_move();
            draft.move_to(i, t, value);
            Drag::DraftAnchor { index: i }
        }
        None => {
            draft.add(t, value);
            let i = draft.anchor_at(t, tolerance).unwrap_or(0);
            Drag::DraftAnchor { index: i }
        }
    }
}

/// Continue an anchor drag.
pub fn draft_move(
    ed: &Editor,
    draft: &mut expression_editor_core::PitchDraft,
    index: usize,
    x: f64,
    y: f64,
) -> Option<usize> {
    let note = ed.doc.note(draft.note)?;
    let t = ed.camera.t_at(x);
    let value = ed.camera.pitch_at(y, ed.viewport) - note.row as f64;
    draft.move_to(index, t, value);
    // Re-sorting can move the grabbed anchor, so the caller has to be
    // told where it went or the next frame drags the wrong one.
    let tolerance = ed.camera.units_per_px * expression_editor_core::draft::GRAB_PX;
    draft.anchor_at(t, tolerance.max(1e-9))
}

/// Resolve a press against the note handles.
///
/// Returns `None` when the press was not on a handle, so the caller
/// falls through to the ordinary map-driven path.
fn handle_press(ed: &mut Editor, x: f64, y: f64, mods: Mods, gesture: Gesture) -> Option<Drag> {
    if !ed.mode.has_handles() {
        return None;
    }
    let sets = crate::canvas::note_handles(ed);
    let (id, handle) = sets
        .iter()
        .find_map(|s| handles::hit(&s.rects, x, y).map(|h| (s.id, h)))?;

    // A horizontal drag across the *body* draws a temporary note rather
    // than moving pitch: the range gesture has to start somewhere, and
    // the body is the only handle wide enough to sweep across.
    // Committing to it on the first horizontal movement is what keeps a
    // plain vertical pitch drag from being stolen.
    if handle == Handle::Pitch && mods.alt {
        let t = ed.camera.t_at(x);
        return Some(Drag::TempNote {
            note: id,
            origin_t: t,
            current_t: t,
        });
    }

    // Right-click on the amplitude handle mutes, exactly as the manual
    // has it, and opens no drag.
    if gesture == Gesture::RightClick {
        if handle == Handle::Amplitude {
            ed.begin_gesture();
            ed.apply(&Edit::ToggleMuted { notes: vec![id] });
            return Some(Drag::None);
        }
        return None;
    }

    let note = ed.doc.note(id)?;
    let scope = ed.scope_for(id);
    if !scope.is_valid(note) {
        return None;
    }
    // Captured at press, not read live: releasing Shift mid-drag must
    // not change which spans a gesture has already started writing.
    let sibilants = ed.sibilant_scope != mods.shift;
    let mut drags = vec![handles::HandleDrag::begin_with(
        handle, note, scope, y, sibilants,
    )];

    // The rest of the selection comes along.
    //
    // Grabbing a handle on one of six selected notes and having it move
    // only that one makes the selection decorative: you would select the
    // phrase, shape it, and find you had shaped a note. Every other
    // gesture on this surface already works on the selection — moving,
    // stretching, velocity — and the handles were the exception.
    //
    // Each follower captures its own base off its own note at the same
    // pointer origin, so they all see the same delta and apply it to
    // whatever they were. A note whose scope has no span to write (an
    // amplitude drag over a note with no unvoiced material, say) is
    // dropped here rather than fudged: it has nothing this gesture can
    // change, and inventing one would be the surprise.
    if ed.selection.contains(id) {
        let followers: Vec<_> = ed
            .selection
            .notes
            .iter()
            .copied()
            .filter(|other| *other != id)
            .filter_map(|other| {
                let n = ed.doc.note(other)?;
                let scope = ed.scope_for(other);
                scope
                    .is_valid(n)
                    .then(|| handles::HandleDrag::begin_with(handle, n, scope, y, sibilants))
            })
            .collect();
        drags.extend(followers);
    }

    ed.begin_gesture();
    Some(Drag::Handle(Box::new(drags)))
}

/// Add whatever the pointer is over to a touch-select sweep.
///
/// The rectangle-free selection gesture: run the pointer through a run
/// of notes and they are selected, which beats aiming a marquee at a
/// dense passage. Idempotent per note, so the same note crossed twice by
/// a wandering sweep is not toggled back off.
fn touch_at(ed: &mut Editor, drag: &mut Drag, x: f64, y: f64) {
    let Drag::SelectTouched { toggle, seen } = drag else {
        return;
    };
    let Hit::Note { id, .. } = ed.hit_test(x, y) else {
        return;
    };
    // Each note is decided once per sweep, not once per frame: the
    // pointer rests on one note for many moves, and a per-frame toggle
    // would flicker it on and off for as long as you hovered.
    if !seen.insert(id) {
        return;
    }
    if *toggle {
        ed.selection.toggle(id);
    } else {
        ed.selection.add(id);
    }
}

/// Sound the note under the pointer without editing it.
///
/// Routed through a host hook rather than played here, for the same
/// reason [`expression_editor_core::mouse::set_host_overlay`] exists:
/// this crate has no synth, no output device and no business acquiring
/// one, and the three hosts that embed the surface each already have
/// somewhere for a note to go. With no hook registered the gesture is a
/// no-op — which is what it was before, only now it is a *stated* no-op
/// with a place for the sound to arrive.
fn audition(ed: &Editor, x: f64, y: f64) {
    let (row, velocity) = match ed.hit_test(x, y) {
        Hit::Note { id, .. } | Hit::NoteEdge { id, .. } => match ed.doc.note(id) {
            Some(n) => (n.row, n.velocity),
            None => return,
        },
        // Off a note — the key gutter, or empty roll — sounds the row
        // itself at a nominal level.
        _ => (ed.camera.pitch_at(y, ed.viewport).round() as i32, 0.63),
    };
    if let Some(sink) = AUDITION.get() {
        sink(row, velocity);
    }
}

/// Where an audition goes, when the host has somewhere to put it.
///
/// `(row, velocity)`. First caller wins, matching the mouse-map overlay
/// hook, so a test harness cannot be trampled by a late host.
static AUDITION: std::sync::OnceLock<fn(i32, f64)> = std::sync::OnceLock::new();

/// Register the audition sink.
pub fn set_audition_sink(f: fn(i32, f64)) {
    let _ = AUDITION.set(f);
}

/// The mean value a controller holds over `[t0, t1]`.
///
/// Used as the pivot for [`Drag::CcScale`]: scaling about the range's
/// own mean is what makes a factor of 0 flatten a swell to its average
/// level instead of dropping it to silence.
fn cc_mean(ed: &Editor, number: u8, t0: f64, t1: f64) -> f64 {
    let Some(dimension) = ed.doc.cc.get(number) else {
        return 0.0;
    };
    let default = dimension.default_value();
    if (t1 - t0).abs() < f64::EPSILON {
        return dimension.curve.sample(t0, default);
    }
    const STEPS: usize = 32;
    let sum: f64 = (0..=STEPS)
        .map(|i| {
            let f = i as f64 / STEPS as f64;
            dimension.curve.sample(t0 + (t1 - t0) * f, default)
        })
        .sum();
    sum / (STEPS + 1) as f64
}

/// Write a shaped ramp between the two ends of a [`Drag::CcLine`].
///
/// Endpoints snap to the grid unless shift reverses it, which is the
/// same rule the razor and note gestures follow. The interior is
/// resampled through the toolbar shape, so switching shape after
/// release restyles the ramp without redrawing it.
fn cc_line(ed: &mut Editor, number: u8, start: (f64, f64), current: (f64, f64), mods: Mods) {
    let snap = ed.grid.enabled && !mods.shift;
    let raw0 = ed.camera.t_at(start.0);
    let raw1 = ed.camera.t_at(current.0);
    let (mut t0, mut t1) = (raw0, raw1);
    if snap {
        t0 = ed.snap_time(t0);
        t1 = ed.snap_time(t1);
    }
    let v0 = expression_editor_core::cc::cc_value(start.1, ed.viewport.h);
    let v1 = expression_editor_core::cc::cc_value(current.1, ed.viewport.h);
    let (lo, hi, v_lo, v_hi) = if t0 <= t1 {
        (t0, t1, v0, v1)
    } else {
        (t1, t0, v1, v0)
    };
    if (hi - lo).abs() < f64::EPSILON {
        return;
    }
    let steps = (((hi - lo) / ed.camera.units_per_px).abs().ceil() as usize).clamp(2, 256);
    let shape = ed.shape;
    let points: Vec<Point> = (0..=steps)
        .map(|i| {
            let f = i as f64 / steps as f64;
            Point {
                t: lo + (hi - lo) * f,
                value: v_lo + (v_hi - v_lo) * shape.amount(f),
                // The pen bakes its curve into the values, so the
                // segments between the sampled points are linear.
                ..Point::default()
            }
        })
        .collect();
    ed.apply_live(&Edit::DrawCc {
        number,
        t0: lo,
        t1: hi,
        points,
    });
}

/// Write one controller sample at the pointer.
///
/// The roll's full height is 0..127, so a controller drawn here spans
/// the whole canvas — which is the point of editing it on the roll
/// rather than in a short strip.
fn cc_draw(ed: &mut Editor, drag: &mut Drag, x: f64, y: f64) {
    let Drag::CcPen { number, last } = drag else {
        return;
    };
    let t = ed.camera.t_at(x);
    let value = expression_editor_core::cc::cc_value(y, ed.viewport.h);

    // Ramp from the previous sample to this one. Holding the new value
    // flat across the gap and jumping at the end is what turns a smooth
    // stroke into a staircase — visible the moment the pointer moves
    // faster than one sample per pixel.
    let (from_t, from_v) = last.unwrap_or((t, value));
    let (lo, hi, v_lo, v_hi) = if from_t <= t {
        (from_t, t, from_v, value)
    } else {
        (t, from_t, value, from_v)
    };
    let steps = (((hi - lo) / ed.camera.units_per_px).abs().ceil() as usize).clamp(1, 256);
    let points: Vec<Point> = (0..=steps)
        .map(|i| {
            let f = i as f64 / steps as f64;
            Point {
                t: lo + (hi - lo) * f,
                value: v_lo + (v_hi - v_lo) * f,
                ..Point::default()
            }
        })
        .collect();
    ed.apply_live(&Edit::DrawCc {
        number: *number,
        t0: lo,
        t1: hi,
        points,
    });
    *last = Some((t, value));
}

/// Fill the grid cell under the pointer, once per cell.
fn paint_at(ed: &mut Editor, drag: &mut Drag, x: f64, y: f64) {
    let Drag::Paint { last_cell, snap } = drag else {
        return;
    };
    let step = ed.grid.step(ed.units_per_beat()).max(1.0);
    let raw = ed.camera.t_at(x);
    let row = ed.camera.pitch_at(y, ed.viewport).round() as i32;
    let cell = ((raw - ed.doc.start) / step).floor() as i64;
    // One note per cell per sweep, or a slow drag stacks dozens.
    if *last_cell == Some((cell, row)) {
        return;
    }
    *last_cell = Some((cell, row));

    let start = if *snap {
        ed.doc.start + cell as f64 * step
    } else {
        raw
    };
    let end = start + step;
    if ed
        .doc
        .notes
        .iter()
        .any(|n| n.row == row && n.start < end && n.end > start)
    {
        return;
    }
    let id = ed.doc.mint_id();
    let mut note = expression_editor_core::Note::new(id, start, end, row.clamp(0, 127));
    for dimension in Dimension::ALL {
        let v = dimension.default_value();
        note.curve_mut(dimension).set(start, v);
        note.curve_mut(dimension).set(end, v);
    }
    ed.apply_live(&Edit::AddNote(Box::new(note)));
}

/// The view gestures every surface owes the user, whatever it is showing.
///
/// Middle-drag pans. It is not a piano-roll feature — it is how you get
/// around — and a surface that ignored it left the user stuck with the
/// scrollbar-less view they happened to be looking at. The roll honoured
/// it and the stack and the strip did not, which is the sort of gap
/// nobody reports as a bug because it reads as "that view is just like
/// that".
///
/// Returns the drag to run, or `None` if this press is not a view
/// gesture and the surface should handle it itself.
pub fn view_press(button: u16, x: f64, y: f64) -> Option<Drag> {
    (button == 1).then_some(Drag::Pan { last: (x, y) })
}

/// The tool-driven path, for expression tools the map defers to.
fn legacy_pointer_down(ed: &mut Editor, x: f64, y: f64, mods: Mods, button: u16) -> Drag {
    // Right-drag pans regardless of tool.
    if button == 2 || (mods.ctrl && mods.shift) {
        return Drag::Pan { last: (x, y) };
    }

    let hit = ed.hit_test(x, y);

    // A split handle always wins — it is small and deliberate.
    if let Hit::ZoneSplit { id, t, .. } = hit {
        if mods.alt {
            ed.apply(&Edit::RemoveZoneSplit {
                note: id,
                t,
                tolerance: f64::INFINITY,
            });
            return Drag::None;
        }
        ed.begin_gesture();
        return Drag::SplitDrag { note: id, from: t };
    }

    if let Hit::NoteEdge { id, start_edge } = hit
        && (ed.tool == Tool::NoteDraw || ed.tool == Tool::Select)
    {
        let n = ed.doc.note(id).expect("hit test returned a live note");
        let original = (n.start, n.end);
        ed.begin_gesture();
        return Drag::Resize {
            note: id,
            start_edge,
            original,
        };
    }

    let under = match hit {
        Hit::Note { id, zone } => {
            // Clicking a selected note's active zone toggles between
            // single-zone and whole-note targeting.
            if ed.selection.contains(id) {
                if let Some(n) = ed.doc.note(id) {
                    let next = tools::toggle_target(n.target, zone);
                    ed.apply(&Edit::SetTarget {
                        note: id,
                        target: next,
                    });
                }
            } else if mods.shift {
                ed.selection.add(id);
            } else if mods.ctrl {
                ed.selection.toggle(id);
            } else {
                ed.selection.set_single(id);
                ed.apply(&Edit::SetTarget {
                    note: id,
                    target: Target::Zone(zone),
                });
            }
            Some(id)
        }
        Hit::CurvePoint { id, .. } => Some(id),
        _ => None,
    };

    if ed.tool == Tool::NoteErase {
        if let Some(id) = under {
            ed.begin_gesture();
            ed.apply_live(&Edit::DeleteNotes(vec![id]));
        }
        return Drag::NoteErase;
    }

    let notes = tools::gesture_targets(&ed.selection, under);

    // Shift-drag transposes; alt-drag scales. Both outrank the active
    // drawing tool.
    if !notes.is_empty() {
        if mods.shift && !ed.tool.edits_notes() {
            let base_rows = notes
                .iter()
                .filter_map(|&id| ed.doc.note(id).map(|n| n.row))
                .collect();
            ed.begin_gesture();
            return Drag::Transpose {
                notes,
                origin: (x, y),
                base_rows,
                applied: 0,
            };
        }
        if mods.alt && ed.tool.edits_expression() {
            ed.begin_gesture();
            return Drag::Scale {
                notes,
                origin: (x, y),
                applied: 1.0,
            };
        }
    }

    // The armed tool, and only the armed tool.
    //
    // `Ctrl` used to force the Pen here regardless of the map, from a
    // time when it was the pen's modifier. Under the FTS map `Ctrl` is
    // the razor, and a hardcoded override would have quietly outranked
    // the binding — the map would say razor and the surface would draw.
    // The Pen is a tool; arming it is how you reach it.
    let tool = ed.tool;

    match tool {
        // A view tool, so it never touches the document and never opens
        // an undo step — which is what makes it safe to spring-load onto
        // a held key and safe to leave armed.
        Tool::Zoom => Drag::ZoomTool {
            origin: (x, y),
            base_units_per_px: ed.camera.units_per_px,
            base_px_per_row: ed.camera.vertical.px_per_row,
            anchor_t: ed.camera.t_at(x),
            // The slot under the press, read straight off the camera.
            // `pitch_at` would send it through the fold and back, and
            // the round trip is not the identity.
            anchor_slot: ed.camera.vertical.center
                + (ed.viewport.h * 0.5 - y) / ed.camera.vertical.px_per_row,
            marquee: mods.alt,
            current: (x, y),
        },
        // Same drag `Ctrl` produces from any other tool — one code path,
        // so the tool cannot drift from the modifier that shares it.
        Tool::Razor => Drag::RazorCreate {
            origin: (x, y),
            current: (x, y),
            pending: None,
        },
        // The same drag the map's `EditNoteVelocity` produces, so the
        // tool and the binding cannot come apart.
        //
        // The whole selection when the note under the pointer is part of
        // it, that note alone when it is not — `gesture_targets` is the
        // rule the rest of the surface already follows, and the reason
        // is that grabbing an unselected note should act on what you
        // grabbed rather than on something else entirely.
        Tool::Velocity => {
            let notes = tools::gesture_targets(&ed.selection, under);
            if notes.is_empty() {
                Drag::None
            } else {
                ed.begin_gesture();
                Drag::Velocity {
                    notes,
                    origin_y: y,
                    fine: mods.shift,
                    applied: 0.0,
                }
            }
        }
        Tool::Select => {
            if under.is_some() {
                ed.begin_gesture();
                Drag::MoveNotes {
                    notes,
                    origin: (x, y),
                    applied_rows: 0,
                    applied_time: 0.0,
                    axis: Axis::Both,
                }
            } else {
                if !mods.shift {
                    ed.selection.clear();
                }
                Drag::Marquee {
                    origin: (x, y),
                    current: (x, y),
                    additive: mods.shift,
                }
            }
        }
        Tool::Pen if !notes.is_empty() => {
            ed.begin_gesture();
            Drag::Pen {
                notes,
                start: (x, y),
                samples: vec![(x, y)],
            }
        }
        Tool::Curve if !notes.is_empty() => {
            ed.begin_gesture();
            Drag::Curve {
                notes,
                start: (x, y),
                current: (x, y),
            }
        }
        Tool::Eraser if !notes.is_empty() => {
            let t = ed.camera.t_at(x);
            ed.begin_gesture();
            Drag::Erase {
                notes,
                start_t: t,
                current_t: t,
            }
        }
        Tool::NoteDraw => {
            if under.is_some() {
                ed.begin_gesture();
                Drag::MoveNotes {
                    notes,
                    origin: (x, y),
                    applied_rows: 0,
                    applied_time: 0.0,
                    axis: Axis::Both,
                }
            } else {
                begin_new_note(ed, x, y, mods)
            }
        }
        _ => {
            if under.is_none() && !mods.shift {
                ed.selection.clear();
            }
            Drag::None
        }
    }
}

/// Note Draw on blank canvas: fill the grid division under the pointer.
fn begin_new_note(ed: &mut Editor, x: f64, y: f64, mods: Mods) -> Drag {
    let raw_t = ed.camera.t_at(x);
    let row = ed.camera.pitch_at(y, ed.viewport).round() as i32;
    let snap = ed.grid.enabled && !mods.shift;
    let (start, end) = if snap {
        let step = ed.grid.step(ed.units_per_beat());
        let cell = ed.snap_time(raw_t - step * 0.5);
        (cell, cell + step)
    } else {
        (raw_t, raw_t + ed.units_per_beat() * 0.25)
    };

    let id = ed.doc.mint_id();
    let mut note = expression_editor_core::Note::new(id, start, end, row.clamp(0, 127));
    // A new note is playable immediately: centered pitch, MPE-default
    // pressure and timbre across its full length.
    for dimension in Dimension::ALL {
        let v = dimension.default_value();
        note.curve_mut(dimension).set(start, v);
        note.curve_mut(dimension).set(end, v);
    }
    ed.begin_gesture();
    ed.apply_live(&Edit::AddNote(Box::new(note)));
    ed.apply_live(&Edit::AssignChannels {
        notes: vec![id],
        seed: id.0,
    });
    ed.selection.set_single(id);
    Drag::InsertNote {
        note: id,
        anchor: (x, y),
        base: (start, end),
        // Shift starts you sizing; without it you are placing. Either
        // way the other phase is one modifier away for the rest of the
        // gesture.
        sizing: mods.shift,
    }
}

/// Continue a gesture.
pub fn pointer_move(ed: &mut Editor, drag: &mut Drag, x: f64, y: f64, mods: Mods) {
    match drag {
        // Not a drag; the menu is already open and owns the pointer.
        Drag::None | Drag::ContextMenu { .. } => {}
        Drag::Pan { last } => {
            ed.pan_px(x - last.0, y - last.1);
            *last = (x, y);
        }
        Drag::Marquee { current, .. } => *current = (x, y),
        Drag::Pen {
            notes,
            start,
            samples,
        } => {
            samples.push((x, y));
            write_pen(ed, notes, *start, samples, mods);
        }
        Drag::Curve {
            notes,
            start,
            current,
        } => {
            *current = (x, y);
            write_curve(ed, notes, *start, *current, ed.shape);
        }
        Drag::Erase {
            notes,
            start_t,
            current_t,
        } => {
            *current_t = ed.camera.t_at(x);
            let (t0, t1) = (start_t.min(*current_t), start_t.max(*current_t));
            for &id in notes.iter() {
                let Some(n) = ed.doc.note(id) else { continue };
                let (s, e) = n.target_span();
                ed.apply_live(&Edit::EraseDimension {
                    note: id,
                    dimension: ed.dimension,
                    t0: t0.max(s),
                    t1: t1.min(e),
                });
            }
        }
        Drag::Transpose {
            notes,
            origin,
            base_rows,
            applied,
        } => {
            let base = base_rows.first().copied().unwrap_or(60);
            let raw =
                ed.camera.pitch_at(y, ed.viewport) - ed.camera.pitch_at(origin.1, ed.viewport);
            let target = ed.tuning.snap(base as f64 + raw);
            let delta = target.row - base;
            if delta != *applied {
                ed.apply_live(&Edit::Transpose {
                    notes: notes.clone(),
                    semitones: delta - *applied,
                });
                *applied = delta;
            }
        }
        Drag::MoveNotes {
            notes,
            origin,
            applied_rows,
            applied_time,
            axis,
        } => {
            // The gesture's own axis, narrowed by a standing lock.
            //
            // `H`/`L` are sticky modes rather than modifiers, so they
            // have to be applied here rather than at the press: turning
            // one on mid-drag should take effect on the drag you are
            // already making, which is the whole reason to prefer a mode
            // over a chord.
            let (mut free_x, mut free_y) = axis.allows(x - origin.0, y - origin.1);
            match ed.razor_axis {
                Some(RazorAxis::Horizontal) => free_y = false,
                Some(RazorAxis::Vertical) => free_x = false,
                None => {}
            }
            // A locked axis holds at whatever it had already applied
            // rather than snapping back: `Axis::Commit` decides *during*
            // the drag, and undoing the first few pixels of movement at
            // the moment it commits is a visible jump.
            let rows = if free_y {
                (ed.camera.pitch_at(y, ed.viewport) - ed.camera.pitch_at(origin.1, ed.viewport))
                    .round() as i32
            } else {
                *applied_rows
            };
            if rows != *applied_rows {
                ed.apply_live(&Edit::Transpose {
                    notes: notes.clone(),
                    semitones: rows - *applied_rows,
                });
                *applied_rows = rows;
            }
            // Horizontal movement preserves the note's grid offset
            // rather than re-snapping it to the grid.
            let raw = if free_x {
                (x - origin.0) * ed.camera.units_per_px
            } else {
                *applied_time
            };
            let delta = if ed.grid.enabled && !mods.shift {
                let step = ed.grid.step(ed.units_per_beat());
                (raw / step).round() * step
            } else {
                raw
            };
            if (delta - *applied_time).abs() > 1e-9 {
                ed.apply_live(&Edit::MoveTime {
                    notes: notes.clone(),
                    delta: delta - *applied_time,
                });
                *applied_time = delta;
            }
        }
        Drag::Scale {
            notes,
            origin,
            applied,
        } => {
            // Exponential response: up expands, down compresses, and
            // continuing past flat reconstructs the gesture inverted.
            let dy = origin.1 - y;
            let factor = (dy / 120.0).exp() * if dy < -240.0 { -1.0 } else { 1.0 };
            let relative = factor / *applied;
            for &id in notes.iter() {
                let Some(n) = ed.doc.note(id) else { continue };
                let spans: Vec<(f64, f64)> = match n.target {
                    // Whole-note targeting scales each zone about its
                    // own center, so the larger melodic contour is
                    // preserved while each zone tightens or expands.
                    Target::WholeNote => n.zones(),
                    Target::Zone(_) => vec![n.target_span()],
                };
                for (t0, t1) in spans {
                    ed.apply_live(&Edit::ScaleDimension {
                        note: id,
                        dimension: ed.dimension,
                        t0,
                        t1,
                        factor: relative,
                    });
                }
            }
            *applied = factor;
        }
        Drag::Resize {
            note,
            start_edge,
            original,
        } => {
            let raw = ed.camera.t_at(x);
            let t = if ed.grid.enabled && !mods.shift {
                ed.snap_time(raw)
            } else {
                raw
            };
            let (start, end) = if *start_edge {
                (t.min(original.1 - 1.0), original.1)
            } else {
                (original.0, t.max(original.0 + 1.0))
            };
            ed.apply_live(&Edit::Resize {
                note: *note,
                start,
                end,
            });
        }
        Drag::SplitDrag { note, from } => {
            let to = ed.camera.t_at(x);
            if ed.apply_live(&Edit::MoveZoneSplit {
                note: *note,
                from: *from,
                to,
            }) {
                // Track where it actually landed — the edit clamps
                // against the neighbouring boundaries.
                if let Some(n) = ed.doc.note(*note)
                    && let Some(&s) = n.splits.iter().min_by(|a, b| {
                        (*a - to)
                            .abs()
                            .partial_cmp(&(*b - to).abs())
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                {
                    *from = s;
                }
            }
        }
        Drag::NoteErase => {
            // Writes continuously while swiping, so notes disappear
            // during the gesture rather than after release.
            if let Hit::Note { id, .. } = ed.hit_test(x, y) {
                ed.apply_live(&Edit::DeleteNotes(vec![id]));
            }
        }
        Drag::RazorCreate {
            origin,
            current,
            pending,
        } => {
            *current = (x, y);
            *pending = resolve_razor(ed, *origin, *current, mods);
        }
        Drag::RazorDrag {
            area,
            origin,
            copy,
            applied_t,
            applied_rows,
            index,
            area_only,
            axis,
        } => {
            let (free_x, free_y) = axis.allows(x - origin.0, y - origin.1);
            let raw = if free_x {
                (x - origin.0) * ed.camera.units_per_px
            } else {
                0.0
            };
            let dt = if ed.grid.enabled && !mods.shift {
                let step = ed.grid.step(ed.units_per_beat());
                (raw / step).round() * step
            } else {
                raw
            };
            let rows = if free_y {
                (ed.camera.pitch_at(y, ed.viewport) - ed.camera.pitch_at(origin.1, ed.viewport))
                    .round() as i32
            } else {
                0
            };
            if (dt - *applied_t).abs() < 1e-9 && rows == *applied_rows {
                return;
            }
            // Re-run from the document as the gesture found it.
            //
            // The area was already captured at press, and this always
            // claimed to re-run from it — but it re-ran against a
            // document its own previous frame had already carved. So
            // frame two split the *original* rectangle again, in a place
            // the material had since moved out of: the notes that moved
            // first stopped moving, notes that were never in the area
            // got dragged along, and the take came apart in pieces.
            //
            // Reverting first makes the claim true. A razor move is
            // destructive — it splits at the boundaries and clears the
            // ground it lands on — so it cannot be expressed as a delta
            // against the last frame; it has to be recomputed from a
            // fixed starting point. We own the document, so that is
            // simply the snapshot `begin_gesture` already took.
            if !*area_only {
                ed.revert_gesture();
                let insert = ed.razor_insert;
                expression_editor_core::razor::move_contents(
                    &mut ed.doc,
                    *area,
                    dt,
                    rows,
                    *copy,
                    insert,
                );
            }
            *applied_t = dt;
            *applied_rows = rows;
            let moved = area.translated(dt, rows);
            if let Some(slot) = ed.razor.areas.get_mut(*index) {
                *slot = moved;
            }
        }
        Drag::RazorEdge {
            index,
            start_edge,
            stretch,
            base,
        } => {
            let area = *base;
            let raw = ed.camera.t_at(x);
            let t = if ed.grid.enabled && !mods.shift {
                ed.snap_time(raw)
            } else {
                raw
            };
            // The dragged edge may not cross the fixed one; an inverted
            // area is not a smaller area, it is a broken one.
            let next = if *start_edge {
                RazorArea::new(t.min(area.t1 - 1.0), area.t1, area.row_lo, area.row_hi)
            } else {
                RazorArea::new(area.t0, t.max(area.t0 + 1.0), area.row_lo, area.row_hi)
            };
            if *stretch {
                // Same reason as the move above: `stretch_contents`
                // carves, so re-running it against an already-stretched
                // document compounds the scaling and re-splits material
                // that has moved. Recompute from the gesture's start.
                ed.revert_gesture();
                expression_editor_core::razor::stretch_contents(
                    &mut ed.doc,
                    area,
                    next.t0,
                    next.t1,
                );
            }
            if let Some(slot) = ed.razor.areas.get_mut(*index) {
                *slot = next;
            }
        }
        Drag::Stretch {
            base,
            pivot,
            origin_t,
            positions,
        } => {
            let raw = ed.camera.t_at(x);
            let t = if ed.grid.enabled && !mods.shift {
                ed.snap_time(raw)
            } else {
                raw
            };
            // A ratio of distances from the pivot: dragging the grabbed
            // edge twice as far from it doubles everything.
            let factor = (t - *pivot) / (*origin_t - *pivot);
            if !factor.is_finite() || factor.abs() < 1e-6 {
                return;
            }
            for &(id, start, end) in base.iter() {
                let (s, e) = if *positions {
                    // Positions scale, lengths do not — that is what
                    // makes it arpeggiate rather than time-stretch.
                    let s = *pivot + (start - *pivot) * factor;
                    (s, s + (end - start))
                } else {
                    (
                        *pivot + (start - *pivot) * factor,
                        *pivot + (end - *pivot) * factor,
                    )
                };
                let (lo, hi) = if s <= e { (s, e) } else { (e, s) };
                ed.apply_live(&Edit::Resize {
                    note: id,
                    start: lo,
                    end: hi.max(lo + 1.0),
                });
            }
        }
        Drag::Zoom {
            origin,
            base_units_per_px,
            anchor_t,
        } => {
            // Vertical drag, exponential, anchored on the press: the
            // time under the pointer when the gesture opened stays under
            // it, which is the only zoom that does not lose your place.
            let factor = ((origin.1 - y) / 200.0).exp();
            ed.camera.units_per_px = (*base_units_per_px / factor).max(1e-6);
            // `t_at(x) = t0 + x * units_per_px`, solved for the t0 that
            // puts `anchor_t` back under the press.
            ed.camera.t0 = *anchor_t - origin.0 * ed.camera.units_per_px;
            // The required ending for a direct camera write: constrain,
            // and let the adaptive grid follow the new zoom.
            ed.settle_camera();
        }
        Drag::ZoomTool {
            origin,
            base_units_per_px,
            base_px_per_row,
            anchor_t,
            anchor_slot,
            marquee,
            current,
        } => {
            *current = (x, y);
            if *marquee {
                // Sweeping a box; nothing moves until release.
                return;
            }
            // Direction is the axis. Shift is the fine control: the same
            // travel asks for a quarter as much, so a gesture that
            // overshot can be repeated slowly rather than undone.
            let gain = if mods.shift { 800.0 } else { 200.0 };
            let dx = x - origin.0;
            let dy = origin.1 - y;

            // Right and up zoom *in*, which is the direction the content
            // grows in both cases.
            ed.camera.units_per_px = (*base_units_per_px / (dx / gain).exp()).max(1e-9);
            ed.camera.vertical.px_per_row = (*base_px_per_row * (dy / gain).exp()).max(1e-6);
            // Put what was under the press back under it, on both axes.
            //
            // Both lines invert the camera's own mapping, so they have
            // to match it exactly. `t_at(x) = t0 + x·upp`, so
            // `t0 = anchor - x·upp`. Vertically the camera reads
            // `slot = centre + (h/2 - y)/ppr`, so the inverse is
            // `centre = anchor - (h/2 - y)/ppr` — note the sign. It used
            // to be written `- (y - h/2)/ppr`, which is the same
            // expression negated: correct at the vertical centre, where
            // the term is zero, and increasingly wrong towards either
            // edge. That is why a zoom begun mid-height behaved and one
            // begun near the top or bottom lurched before it zoomed.
            ed.camera.t0 = *anchor_t - origin.0 * ed.camera.units_per_px;
            ed.camera.vertical.center =
                *anchor_slot - (ed.viewport.h * 0.5 - origin.1) / ed.camera.vertical.px_per_row;
            // `settle_camera`, not `camera.constrain` — the difference
            // is the grid. Constraining alone left the adaptive division
            // frozen at whatever the view was before the drag, so the
            // readout said 1/16 while the notes snapped to something
            // else, and the whole feature looked broken from the one
            // tool most likely to be used to trigger it.
            ed.settle_camera();
        }
        Drag::InsertNote {
            note,
            anchor,
            base,
            sizing,
        } => {
            // A phase change re-takes the anchor and the span, so the
            // note carries on from where it is rather than snapping to
            // wherever the gesture started. Without this, grabbing Shift
            // half way through would teleport the note by however far
            // you had already moved it.
            if mods.shift != *sizing {
                if let Some(n) = ed.doc.note(*note) {
                    *base = (n.start, n.end);
                }
                *anchor = (x, y);
                *sizing = mods.shift;
            }

            let dt = ed.camera.t_at(x) - ed.camera.t_at(anchor.0);
            if *sizing {
                // Pinned where it is; the drag is its length. A note
                // cannot be dragged shorter than nothing, and one that
                // inverted would read as a note starting after it ends.
                let end = ed.snap_time(base.1 + dt);
                let end = end.max(base.0 + ed.grid.step(ed.units_per_beat()) * 0.25);
                ed.apply_live(&Edit::Resize {
                    note: *note,
                    start: base.0,
                    end,
                });
            } else {
                // Carrying it. The length is whatever the last sizing
                // phase left, so a note that has been pulled out stays
                // pulled out while you find its home.
                let len = base.1 - base.0;
                let start = ed.snap_time(base.0 + dt);
                let row = ed.camera.pitch_at(y, ed.viewport).round() as i32;
                ed.apply_live(&Edit::Resize {
                    note: *note,
                    start,
                    end: start + len,
                });
                // Pitch as a transpose from where the note currently is,
                // because that is the edit the document has — there is no
                // "set the row" and inventing one would give the same
                // move two spellings.
                let now = ed.doc.note(*note).map(|n| n.row).unwrap_or(row);
                if row != now {
                    ed.apply_live(&Edit::Transpose {
                        notes: vec![*note],
                        semitones: row.clamp(0, 127) - now,
                    });
                }
            }
        }
        Drag::SelectTouched { .. } => touch_at(ed, drag, x, y),
        Drag::Paint { .. } => paint_at(ed, drag, x, y),
        Drag::DraftAnchor { .. } => {}
        Drag::Separator { sep, law } => {
            // Rebuilt from the separator captured at press, so the
            // stretch is computed from the original layout every frame
            // rather than compounding on the last one.
            let to = if ed.grid.enabled && !mods.shift {
                ed.snap_time(ed.camera.t_at(x))
            } else {
                ed.camera.t_at(x)
            };
            let edits = expression_editor_core::timing::plan(&ed.doc, *sep, to, *law);
            for e in edits {
                ed.apply_live(&e);
            }
        }
        Drag::Handle(hs) => {
            // Shift reverses the pitch snap, as everywhere else here.
            let snap = ed.snap_pitch != mods.shift;
            // Same `y` for every one of them: the gesture is a single
            // delta, and each drag turns it into a change against its
            // own captured base.
            for h in hs.iter_mut() {
                ed.drag_handle(h, y, snap);
            }
        }
        Drag::TempNote {
            note,
            origin_t,
            current_t,
        } => {
            *current_t = ed.camera.t_at(x);
            let (n, t0, t1) = (*note, *origin_t, *current_t);
            // Live, so the shaded range follows the pointer rather than
            // appearing on release.
            ed.set_temp_note(n, t0, t1);
        }
        Drag::CcPen { .. } => cc_draw(ed, drag, x, y),
        Drag::CcLine {
            number,
            start,
            current,
        } => {
            *current = (x, y);
            cc_line(ed, *number, *start, *current, mods);
        }
        Drag::CcErase {
            number,
            start_t,
            current_t,
        } => {
            *current_t = ed.camera.t_at(x);
            let (t0, t1) = (start_t.min(*current_t), start_t.max(*current_t));
            ed.apply_live(&Edit::EraseCc {
                number: *number,
                t0,
                t1,
            });
        }
        Drag::CcScale {
            number,
            t0,
            t1,
            base,
            spanned,
            pivot,
            origin_y,
        } => {
            let t = ed.camera.t_at(x);
            let (lo, hi) = (t0.min(t), t0.max(t));
            // The pivot is captured the moment the range first has
            // width and then held — recomputing it as the sweep grows
            // would move the ground under the vertical drag.
            if (*t1 - *t0).abs() < f64::EPSILON && (hi - lo).abs() > f64::EPSILON {
                *pivot = cc_mean(ed, *number, lo, hi);
            }
            *t1 = t;
            spanned.0 = spanned.0.min(lo);
            spanned.1 = spanned.1.max(hi);

            // A full viewport of travel spans 0x..2x, so the useful
            // range is reachable without a marathon drag and neutral
            // stays where the gesture started.
            let dy = (*origin_y - y) / ed.viewport.h.max(1.0);
            let factor = (1.0 + dy * 2.0).clamp(0.0, 4.0);

            // Rewrite the whole swept-ever span from the captured
            // points: scaled inside the current range, verbatim outside
            // it, so narrowing the sweep puts back what widening it
            // touched.
            let (p, s0, s1) = (*pivot, spanned.0, spanned.1);
            let points: Vec<Point> = base
                .iter()
                .filter(|pt| pt.t >= s0 && pt.t <= s1)
                .map(|pt| Point {
                    t: pt.t,
                    value: if pt.t >= lo && pt.t <= hi {
                        (p + (pt.value - p) * factor).clamp(0.0, 1.0)
                    } else {
                        pt.value
                    },
                    ..Point::default()
                })
                .collect();
            ed.apply_live(&Edit::DrawCc {
                number: *number,
                t0: s0,
                t1: s1,
                points,
            });
        }
        Drag::Velocity {
            notes,
            origin_y,
            fine,
            applied,
        } => {
            // Fine mode is a tenth the travel — the difference between
            // shaping a phrase and nudging one hit.
            let scale = if *fine { 0.0008 } else { 0.008 };
            let delta = (*origin_y - y) * scale;
            let step = delta - *applied;
            if step.abs() > 1e-6 {
                ed.apply_live(&Edit::NudgeVelocity {
                    notes: notes.clone(),
                    delta: step,
                });
                *applied = delta;
            }
        }
    }
}

/// Finish a gesture. Returns the drag to keep live (Curve stays
/// selected so the shape buttons can restyle it).
pub fn pointer_up(ed: &mut Editor, drag: Drag, x: f64, y: f64, mods: Mods) -> Drag {
    match drag {
        Drag::ZoomTool {
            origin,
            marquee,
            current,
            ..
        } => {
            // Only the Alt sweep has anything left to do: the continuous
            // drag already zoomed on the way.
            if marquee {
                let moved = (current.0 - origin.0).abs() + (current.1 - origin.1).abs();
                // A sweep that never moved is a click, and framing a
                // click would zoom to the maximum for what looked like a
                // misclick.
                if moved > 3.0 {
                    let vp = ed.viewport;
                    ed.zoom_to_box(
                        ed.camera.t_at(origin.0),
                        ed.camera.t_at(current.0),
                        ed.camera.pitch_at(origin.1, vp),
                        ed.camera.pitch_at(current.1, vp),
                    );
                }
            }
            Drag::None
        }
        Drag::Marquee {
            origin,
            current,
            additive,
        } => {
            let moved = (current.0 - origin.0).abs() + (current.1 - origin.1).abs();
            if moved > 3.0 {
                let mut sel = ed.selection.clone();
                sel.marquee(&ed.doc, &ed.camera, ed.viewport, origin, current, additive);
                ed.selection = sel;
            } else if !mods.shift {
                ed.selection.clear();
            }
            Drag::None
        }
        // The Curve gesture survives release, and so does its CC twin —
        // both stay live so the shape buttons restyle the stroke that
        // was just drawn.
        live @ (Drag::Curve { .. } | Drag::CcLine { .. }) => live,
        Drag::Handle(hs) => {
            // Fold whole semitones back into the row, restoring the
            // invariant the pitch drag was allowed to break while it ran.
            for h in hs.iter() {
                ed.end_handle_drag(h);
            }
            Drag::None
        }
        Drag::TempNote {
            note,
            origin_t,
            current_t,
        } => {
            // A range too small to see is a click, not a selection, and
            // leaves no scope armed on the note.
            if !ed.set_temp_note(note, origin_t, current_t) {
                ed.clear_temp_note();
            }
            Drag::None
        }
        Drag::RazorCreate {
            origin, pending, ..
        } => {
            // Prefer what was last drawn. Re-resolving here would let
            // the committed area differ from the one on screen whenever
            // release and the last move disagree about the modifiers —
            // let go of Shift a frame early and the razor would snap out
            // from under the rectangle you were looking at.
            if let Some(area) = pending.or_else(|| resolve_razor(ed, origin, (x, y), mods)) {
                ed.razor.add(area);
            }
            Drag::None
        }
        Drag::Pen {
            notes,
            start,
            samples,
        } => {
            let _ = (notes, start, samples, x, y);
            Drag::None
        }
        _ => Drag::None,
    }
}

/// Map one pointer stroke across every targeted note, honouring each
/// note's own whole-note or zone target.
fn write_pen(
    ed: &mut Editor,
    notes: &[NoteId],
    start: (f64, f64),
    samples: &[(f64, f64)],
    mods: Mods,
) {
    let dimension = ed.dimension;
    let start_t = ed.camera.t_at(start.0);
    for &id in notes {
        let Some(n) = ed.doc.note(id) else { continue };
        let (span0, span1) = n.target_span();
        let row = n.row;

        let points: Vec<Point> = samples
            .iter()
            .map(|&(x, y)| {
                let t = ed.camera.t_at(x).clamp(span0, span1);
                let value = match dimension {
                    // Pitch drawing snaps to semitones unless shift is
                    // held for continuous pitch.
                    Dimension::Pitch => {
                        let p = ed.camera.pitch_at(y, ed.viewport);
                        let p = if mods.shift {
                            p
                        } else {
                            ed.tuning.snap(p).pitch
                        };
                        p - row as f64
                    }
                    _ => tools::lane_box_value(&ed.camera, ed.viewport, row, y),
                };
                Point {
                    t,
                    value: dimension.clamp(value),
                    ..Point::default()
                }
            })
            .collect();

        let (t0, t1) = gesture_bounds(&points);
        let (t0, t1) = tools::clamp_gesture((span0, span1), start_t, t0, t1);
        ed.apply_live(&Edit::DrawDimension {
            note: id,
            dimension,
            t0,
            t1,
            points,
        });
    }
}

/// A shaped ramp from the gesture's start to its current position.
fn write_curve(
    ed: &mut Editor,
    notes: &[NoteId],
    start: (f64, f64),
    end: (f64, f64),
    shape: Shape,
) {
    const SAMPLES: usize = 48;
    let dimension = ed.dimension;
    let start_t = ed.camera.t_at(start.0);
    for &id in notes {
        let Some(n) = ed.doc.note(id) else { continue };
        let (span0, span1) = n.target_span();
        let row = n.row;

        let value_at = |y: f64| -> f64 {
            match dimension {
                Dimension::Pitch => ed.camera.pitch_at(y, ed.viewport) - row as f64,
                _ => tools::lane_box_value(&ed.camera, ed.viewport, row, y),
            }
        };
        let (v0, v1) = (value_at(start.1), value_at(end.1));
        let raw0 = ed.camera.t_at(start.0);
        let raw1 = ed.camera.t_at(end.0);
        let (t0, t1) = tools::clamp_gesture((span0, span1), start_t, raw0, raw1);
        if t1 - t0 <= 0.0 {
            continue;
        }
        // Value order follows the drag direction, not time order, so
        // dragging right-to-left still ramps the way it looks.
        let (from, to) = if raw0 <= raw1 { (v0, v1) } else { (v1, v0) };

        let points: Vec<Point> = (0..SAMPLES)
            .map(|i| {
                let f = i as f64 / (SAMPLES - 1) as f64;
                Point {
                    t: t0 + (t1 - t0) * f,
                    value: dimension.clamp(from + (to - from) * shape.amount(f)),
                    ..Point::default()
                }
            })
            .collect();
        ed.apply_live(&Edit::DrawDimension {
            note: id,
            dimension,
            t0,
            t1,
            points,
        });
    }
}

fn gesture_bounds(points: &[Point]) -> (f64, f64) {
    let mut lo = f64::MAX;
    let mut hi = f64::MIN;
    for p in points {
        lo = lo.min(p.t);
        hi = hi.max(p.t);
    }
    if lo > hi { (0.0, 0.0) } else { (lo, hi) }
}

/// Restyle the most recent Curve gesture if there is one; otherwise the
/// active target of every selected note.
pub fn apply_shape(ed: &mut Editor, drag: &Drag, shape: Shape) {
    ed.shape = shape;
    if let Some((notes, x0, x1)) = drag.live_curve() {
        let notes = notes.to_vec();
        let start = (x0, 0.0);
        let end = (x1, 0.0);
        let _ = (start, end);
        // Re-run the gesture with the new shape, keeping its endpoints.
        ed.begin_gesture();
        for &id in &notes {
            let Some(n) = ed.doc.note(id) else { continue };
            let (span0, span1) = n.target_span();
            let (t0, t1) = tools::clamp_gesture(
                (span0, span1),
                ed.camera.t_at(x0),
                ed.camera.t_at(x0),
                ed.camera.t_at(x1),
            );
            ed.apply_live(&Edit::ReshapeDimension {
                note: id,
                dimension: ed.dimension,
                t0,
                t1,
                shape,
                samples: 48,
            });
        }
        return;
    }

    let notes = ed.selection.notes.clone();
    if notes.is_empty() {
        return;
    }
    ed.begin_gesture();
    for id in notes {
        let Some(n) = ed.doc.note(id) else { continue };
        let (t0, t1) = n.target_span();
        ed.apply_live(&Edit::ReshapeDimension {
            note: id,
            dimension: ed.dimension,
            t0,
            t1,
            shape,
            samples: 48,
        });
    }
}

/// Pixels panned per pixel of wheel travel.
///
/// A wheel notch reports a small delta — winit hands Blitz line-ish
/// units, not the screen distance a trackpad would — so panning by the
/// raw delta moved the view a few pixels a notch and read as broken.
/// Tuned by hand at the window rather than derived: the units differ per
/// platform and input device, so there is no figure to compute.
pub(crate) const PAN_GAIN: f64 = 140.0;

/// Wheel travel that doubles the zoom, near enough.
///
/// `e^(travel/D)`: lower is more sensitive. At 300 a notch moved the
/// zoom a percent or two, because winit's delta for one notch is around
/// 1 rather than the ~100 px a browser reports — so the useful divisor
/// is single digits, not hundreds.
pub(crate) const ZOOM_DIVISOR: f64 = 3.0;

/// Wheel/trackpad routing. `(dx, dy)` are the raw deltas.
///
/// Which gesture means what is **not decided here** — it comes from the
/// shared input configuration via [`crate::scroll`], so the editor,
/// the arrange view and REAPER agree. This function only knows how to
/// carry out the actions that config names.
///
/// The binding resolves the gesture; the delta still supplies direction
/// and amount, because the binding table drops it.
pub fn wheel(ed: &mut Editor, x: f64, y: f64, dx: f64, dy: f64, mods: Mods) {
    // `Z` is the zoom key, for the wheel as much as for the drag.
    //
    // Alt used to zoom here, which made Alt mean "create" on a drag and
    // "zoom" on a wheel — the exact overloading the FTS map exists to
    // remove. A modifier meaning two things depending on which *device*
    // you moved is no more derivable than one meaning two things
    // depending on which other key is held.
    //
    // The wheel zooms both axes together and the tool zooms them
    // separately, which is the right division of labour: a wheel is a
    // quick uniform zoom, a drag is the one where you aim.
    if crate::keys::zoom_prefix_held() {
        let travel = if dx.abs() > dy.abs() { dx } else { dy };
        let factor = (travel.abs() / ZOOM_DIVISOR).exp();
        let factor = if travel < 0.0 { factor } else { 1.0 / factor };
        ed.zoom_time_at(x, factor);
        ed.zoom_pitch_at(y, factor);
        return;
    }

    let Some(action) = crate::scroll::action_for(dx, dy, mods) else {
        // Deliberately nothing. An unbound gesture that still moved the
        // view is how the editor drifted away from the DAW's scheme in
        // the first place.
        return;
    };

    // A horizontal gesture carries its travel in `dx`, a modifier-driven
    // one in `dy`. Taking the larger reads both without having to ask
    // which the binding matched on.
    let travel = if dx.abs() > dy.abs() { dx } else { dy };
    let factor = (travel.abs() / ZOOM_DIVISOR).exp();
    let zoom_in = travel < 0.0;

    match action.as_str() {
        "view.vscroll" => ed.pan_px(0.0, -dy * PAN_GAIN),
        "view.hscroll" => ed.pan_px(-travel * PAN_GAIN, 0.0),
        // Vertical is pitch and horizontal is time — the axis names are
        // the DAW's, the meaning is this document's.
        "view.zoom_v" => ed.zoom_pitch_at(y, if zoom_in { factor } else { 1.0 / factor }),
        "view.zoom_h" => ed.zoom_time_at(x, if zoom_in { factor } else { 1.0 / factor }),
        // Both axes at once is the editor's designed zoom, magnets and
        // all — there is no single-axis promise to keep.
        "view.zoom_both" => {
            if zoom_in {
                ed.zoom_in_at(x, y, factor);
            } else {
                ed.zoom_out_at(x, y, factor);
            }
        }
        // The editor's own gesture: nudge selected notes off-grid, the
        // fine-timing move grid snapping would otherwise make impossible.
        "edit.nudge_time" => {
            let notes = ed.selection.notes.clone();
            if !notes.is_empty() {
                let step = match ed.doc.time_base {
                    expression_editor_core::TimeBase::Ppq { .. } => 10.0,
                    expression_editor_core::TimeBase::Frames { .. } => 1.0,
                };
                ed.apply(&Edit::MoveTime {
                    notes,
                    delta: if dy > 0.0 { step } else { -step },
                });
            }
        }
        _ => {}
    }
}

/// Keyboard shortcuts. Returns true if the key was consumed.
///
/// Space is deliberately absent: transport belongs to the host, and a
/// global Space intercept that outlives a crash would leave the DAW's
/// keyboard locked.
/// The razor's own verbs, live only while the razor tool is armed.
///
/// Arming the razor *is* the mode. The reference — MRE, see
/// `spec/midi-editor.md` — is a modal ReaScript you enter and leave with
/// Escape, and its verbs are bare letters. Bare letters are tool
/// shortcuts here, so they can only mean razor verbs while the razor is
/// the tool you are holding.
///
/// That is also what makes the mode safe rather than a trap: it is
/// visible in the toolbar, it is the thing you clicked, and Escape
/// leaves it. Nothing in this table is reachable by accident from
/// Select.
///
/// `None` means "not a razor verb" and falls through to the ordinary
/// keys, so undo, the transport and the view controls all still work
/// while cutting. Only the letters listed here are taken.
///
/// The same verbs have an always-available spelling under the `k`
/// which-key prefix, which lists itself in the overlay. This table is
/// for when your hand is already on the razor.
/// The razor verbs, for the overlay to list.
///
/// The table exists so the keys can be *shown*. A modal surface whose
/// commands are undocumented bare letters is one you have to be told
/// about, and being told about it is not a feature — the whole reason
/// which-key exists is that a keymap should introduce itself.
///
/// Kept beside [`razor_mode_key`], and a test asserts the two agree in
/// both directions: every key listed here is handled, and no letter it
/// handles is missing here. That is the only thing keeping a help panel
/// from becoming a lie the moment a verb is added.
pub const RAZOR_KEYS: &[(&str, &str)] = &[
    ("r", "Retrograde"),
    ("Ctrl+r", "Retrograde pitches only"),
    ("v", "Invert pitches"),
    ("x", "Delete contents"),
    ("d", "Duplicate"),
    ("s", "Select contents"),
    ("u", "Unselect contents"),
    ("f", "Full-lane area"),
    ("i", "Insert mode"),
    ("h", "Lock horizontal"),
    ("l", "Lock vertical"),
    ("←→", "Move by grid"),
    ("↑↓", "Move by row"),
    ("⇧←→", "Resize"),
    ("Esc", "Drop areas, then exit"),
];

/// Whether the razor's own verbs are live right now.
pub fn razor_mode_live(ed: &Editor) -> bool {
    ed.tool == Tool::Razor && !ed.razor.is_empty()
}

fn razor_mode_key(ed: &mut Editor, key: &str, mods: Mods) -> Option<bool> {
    use expression_editor_core::razor::RazorAxis;

    if !razor_mode_live(ed) {
        return None;
    }
    Some(match (key, mods.ctrl) {
        // Retrograde. Ctrl keeps the rhythm and reverses only the
        // pitches, which is MRE's split and a different musical idea:
        // one rewrites the phrase, the other reharmonises the groove
        // that already works.
        ("r", false) => ed.razor_reverse(),
        ("r", true) => ed.razor_reverse_pitches(),
        ("v", _) => ed.razor_invert(),
        ("x", _) => ed.razor_delete_contents(),
        ("d", _) => ed.razor_duplicate(),
        ("s", _) => ed.razor_select_contents(),
        ("u", _) => ed.razor_unselect_contents(),
        ("f", _) => ed.razor_full_lane(),
        // The sticky modes. Pressing the one already on turns it off, so
        // every one of them is its own way out.
        ("i", _) => {
            ed.razor_insert = !ed.razor_insert;
            true
        }
        ("h", _) => {
            ed.razor_axis =
                (ed.razor_axis != Some(RazorAxis::Horizontal)).then_some(RazorAxis::Horizontal);
            true
        }
        ("l", _) => {
            ed.razor_axis =
                (ed.razor_axis != Some(RazorAxis::Vertical)).then_some(RazorAxis::Vertical);
            true
        }
        _ => return None,
    })
}

pub fn key_down(ed: &mut Editor, drag: &Drag, key: &str, mods: Mods) -> bool {
    // Razor mode gets first refusal, because its verbs are bare letters
    // that are tool shortcuts everywhere else.
    if let Some(handled) = razor_mode_key(ed, key, mods) {
        return handled;
    }

    let mouse_t = ed.camera.t_at(ed.viewport.w * 0.5);
    match (key, mods.ctrl, mods.shift) {
        // Escape backs out of the most specific thing first.
        //
        // Razor areas before the note selection, because a razor is the
        // narrower and the more dangerous of the two: it is a standing
        // instruction that the next carve, delete or lane-clear will use,
        // and it survived every other way of changing your mind. There
        // was no way to put it down at all short of drawing over it.
        //
        // Returning `false` when there is nothing to clear matters —
        // Escape has other owners upstream (the draft, the drawer, the
        // multitool) and this must not swallow the key from them.
        ("Escape", _, _) => {
            if !ed.razor.is_empty() {
                ed.razor.clear();
                true
            } else if ed.tool == Tool::Razor {
                // Nothing left to cut, so the second Escape leaves the
                // mode — MRE's "Escape exits the script". Two presses
                // rather than one because dropping the areas and putting
                // the tool down are separate intentions, and the common
                // one is "not that rectangle, let me draw another".
                //
                // The sticky modes go with it. They are scoped to the
                // razor, and leaving them armed for a session you come
                // back to hours later is exactly the mode-you-forgot-you
                // -were-in problem the momentary keys elsewhere avoid.
                ed.razor_insert = false;
                ed.razor_axis = None;
                ed.tool = Tool::Select;
                true
            } else if !ed.selection.is_empty() {
                ed.selection.clear();
                true
            } else {
                false
            }
        }
        ("z", true, false) => ed.undo(),
        ("z", true, true) | ("y", true, _) => ed.redo(),
        ("a", true, _) => {
            ed.selection.notes = ed.doc.notes.iter().map(|n| n.id).collect();
            true
        }
        // Plain `v` used to reset the view. It cannot any more: `v` is
        // the velocity prefix, so the sequence resolver takes it and
        // this arm was never reached — a binding that had quietly
        // stopped existing. Reset moved to `z r`, in the tree that is
        // already about the view.
        // Contextual zoom. One key, and the pointer region picks the
        // behaviour — the MeMagic idea.
        // Contextual zoom. Shift widens the intent: plain F hugs the
        // local passage, Shift+F frames the whole part.
        ("f", false, shift) => {
            let anchor = ed.playhead.unwrap_or(mouse_t);
            let row = ed.camera.vertical.center;
            let modes = if shift {
                ZoomModes::KEYS
            } else {
                ZoomModes::NOTE_AREA
            };
            ed.smart_zoom(modes, anchor, row);
            true
        }
        // Ctrl+X/C/V go through the same command path the context menu
        // uses, so a keyboard cut and a menu cut cannot diverge.
        ("x", true, _) => ed.run_command(&Command::Cut, None),
        ("c", true, _) => ed.run_command(&Command::Copy, None),
        ("v", true, _) => ed.run_command(&Command::Paste, None),
        // Timing mode. The manual's `2`, and free here.
        ("2", false, true) => {
            ed.timing_mode = !ed.timing_mode;
            true
        }
        // Hold to bring the MIDI reference forward, the manual's `M`.
        // Momentary for the same reason `Shift+R` is: it is a thing you
        // check mid-edit, not a mode to be left in.
        ("m", false, _) if ed.reference.is_some() => {
            ed.reference_to_front = true;
            true
        }
        // Arm sibilant editing. Only where there are sibilants: in a
        // MIDI mode this key would toggle an invisible state.
        ("i", false, _) if ed.mode.draws_blobs() => {
            ed.sibilant_scope = !ed.sibilant_scope;
            true
        }
        // Shift+R always brings references forward, so the gesture is
        // reachable from every mode including MPE, where bare `R` is
        // spoken for.
        ("r", false, true) => {
            ed.refs_to_front = true;
            true
        }
        ("s", false, _) => set_tool(ed, Tool::Select),
        // Curve was `c`, which the chord gun needs — `c` for chord is
        // the one strong mnemonic left, and a tool letter is cheaper to
        // move than a whole tree. `w` for the wave it draws.
        ("w", false, _) => set_tool(ed, Tool::Curve),
        ("d", false, _) => set_tool(ed, Tool::NoteDraw),
        ("e", false, _) => set_tool(ed, Tool::NoteErase),
        ("p", false, _) => set_tool(ed, Tool::Pen),
        // `x` for the razor: `r` is the momentary reference key and `k`
        // is the razor which-key prefix, so neither was available.
        ("x", false, _) => set_tool(ed, Tool::Razor),
        ("1", false, _) => {
            ed.grid.coarser();
            true
        }
        ("2", false, _) => {
            ed.grid.finer();
            true
        }
        ("t", false, _) => {
            ed.grid.triplet = !ed.grid.triplet;
            true
        }
        ("q", false, _) => {
            let Some(&id) = ed.selection.notes.first() else {
                return false;
            };
            ed.apply(&Edit::AddZoneSplit {
                note: id,
                t: mouse_t,
            })
        }
        // `g` split the selected note. It cannot any more — `g` is the
        // grid prefix, so the sequence resolver takes the key and this
        // arm stopped being reachable, exactly as plain `v` did when `v`
        // became the velocity prefix.
        //
        // Split is `b` — break. `s` would be the obvious letter and is
        // the Select tool; of what is actually free (a, b, j, n, w, y)
        // this is the only one that means anything.
        ("b", false, _) => {
            let Some(&id) = ed.selection.notes.first() else {
                return false;
            };
            ed.apply(&Edit::SplitNote {
                note: id,
                t: mouse_t,
            })
        }
        // Bare `R` belongs to whichever meaning the current mode can
        // actually use. Channel reassignment is MPE-only — audio and
        // vocal notes have no member channel to assign — so everywhere
        // else the key is free, and goes to Vovious's meaning:
        // bring references forward, held rather than toggled.
        ("r", false, _) if ed.mode != Mode::Mpe => {
            ed.refs_to_front = true;
            true
        }
        ("r", false, _) => {
            let notes = ed.selection.notes.clone();
            !notes.is_empty()
                && ed.apply(&Edit::AssignChannels {
                    notes,
                    seed: 0x5EED,
                })
        }
        // ── nudging a razor ──────────────────────────────────────────
        //
        // The arrows, which is what the Reaper-Tools razor scripts bind:
        // move by grid, move by measure, move by one track. Same shape
        // here, with rows standing in for tracks.
        //
        // Guarded on there *being* a razor rather than on a mode, so
        // there is nothing to enter and nothing to get stuck in. The
        // areas on screen are the mode.
        //
        // Shift resizes instead of moving: it changes what the rectangle
        // covers without touching the material, which is what you want
        // when a sweep came out a grid line short. Ctrl works in
        // measures.
        (arrow @ ("ArrowLeft" | "ArrowRight"), ctrl, shift) if !ed.razor.is_empty() => {
            let step = if ctrl {
                ed.units_per_bar()
            } else {
                ed.grid.step(ed.units_per_beat())
            };
            let dt = if arrow == "ArrowLeft" { -step } else { step };
            if shift {
                ed.razor_resize(dt)
            } else {
                ed.razor_nudge(dt, 0)
            }
        }
        (arrow @ ("ArrowUp" | "ArrowDown"), _, shift) if !ed.razor.is_empty() => {
            let drows = if arrow == "ArrowUp" { 1 } else { -1 };
            if shift {
                ed.razor_resize_rows(drows)
            } else {
                ed.razor_nudge(0.0, drows)
            }
        }

        // A razor outranks the selection, as it does for Escape.
        //
        // The rectangle is the more specific statement of the two, and
        // it is the one you just drew — deleting the notes selected
        // before you reached for the razor is never what was meant. It
        // also carves, so this deletes exactly the rectangle rather than
        // whole notes that happen to poke into it.
        ("Delete", _, _) | ("Backspace", _, _) if !ed.razor.is_empty() => {
            !drag.is_active() && ed.razor_delete_contents()
        }
        ("Delete", _, _) | ("Backspace", _, _) => {
            let notes = ed.selection.notes.clone();
            if notes.is_empty() {
                return false;
            }
            let applied = if drag.is_active() {
                false
            } else {
                ed.apply(&Edit::DeleteNotes(notes))
            };
            ed.selection.clear();
            applied
        }
        _ => false,
    }
}

fn set_tool(ed: &mut Editor, tool: Tool) -> bool {
    ed.tool = tool;
    true
}
