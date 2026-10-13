//! What the pointer looks like, derived from what it would do.
//!
//! REAPER's single best affordance is that the cursor answers "what
//! happens if I press here?" *before* you press — the `[` and `]`
//! brackets at the ends of an item say you are about to grab an edge,
//! the hand says you are about to pan, the pencil says you are about to
//! draw. None of it is a mode readout; it is a preview of the gesture.
//!
//! ## The one rule: the cursor is a function of the action
//!
//! This module never hit-tests and never reads modifiers. It maps
//! [`Action`] — the value [`crate::mouse::MouseMap`] already resolved
//! for the pointer's context, gesture and modifiers — onto a glyph. That
//! is deliberate and it is the whole design:
//!
//! - Rebind `Alt+drag` on a note from Copy to Erase and the cursor
//!   follows on its own. A second table keyed on context and modifiers
//!   would have to be edited in step, and the day it was not, the
//!   surface would start lying about itself.
//! - The host overlay (`reaper-mouse.ini`) and every mode preset get
//!   correct cursors for free, having never heard of this module.
//! - A new [`Action`] is a compile error here, not a silent [`Arrow`].
//!
//! The two things a resolved action cannot say — *which* end of a note
//! is under the pointer, and *which* handle — arrive as [`Aim`].
//!
//! [`Arrow`]: Cursor::Arrow

use crate::handles::Handle;
use crate::mouse::{Action, Context};

/// What the hit test knows that the action does not.
///
/// A `MoveNoteEdge` says nothing about which edge, and both slope
/// handles resolve to the same action; the glyph has to differ or it
/// tells you nothing about which one you grabbed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Aim {
    /// `Some(true)` on a note's start edge, `Some(false)` on its end.
    pub start_edge: Option<bool>,
    /// The note handle under the pointer, when there is one.
    pub handle: Option<Handle>,
}

impl Aim {
    pub const NONE: Aim = Aim {
        start_edge: None,
        handle: None,
    };

    pub fn edge(start: bool) -> Self {
        Self {
            start_edge: Some(start),
            handle: None,
        }
    }

    pub fn handle(handle: Handle) -> Self {
        Self {
            start_edge: None,
            handle: Some(handle),
        }
    }
}

/// A pointer glyph.
///
/// Semantic, not pictorial: the variants name what the gesture *is*, and
/// the surface decides how to draw it. That split is what lets the same
/// resolution run in three places that cannot share a drawing —
/// `expression-editor-ui` paints these into the Vello scene, a DOM host
/// falls back to [`Cursor::css`], and a test asserts on the variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Cursor {
    /// Nothing to say — chrome, and anything unbound.
    #[default]
    Arrow,
    /// Empty roll, where a press starts a rectangle.
    Crosshair,
    /// Marquee that adds to the selection rather than replacing it.
    MarqueeAdd,
    /// Marquee that flips what it sweeps.
    MarqueeToggle,

    /// `[` — the start edge of a note.
    EdgeLeft,
    /// `]` — the end edge.
    EdgeRight,
    /// A bracket with an arrow through it: the drag scales every
    /// selected note's length, not just this one's.
    StretchLeft,
    StretchRight,

    /// Four-way: the note follows the pointer on both axes.
    Move,
    /// Locked to time.
    MoveH,
    /// Locked to pitch.
    MoveV,
    /// Move that leaves the original behind.
    Copy,

    /// Freehand expression.
    Pencil,
    /// A shaped ramp between two points.
    Curve,
    /// Sweeping notes onto the grid.
    Brush,
    /// Wiping expression back to default.
    Eraser,
    /// Deleting notes.
    NoteEraser,

    /// Panning the view.
    Hand,
    /// The same hand, closed — a pan already under way.
    HandClosed,
    /// Anchored zoom.
    Zoom,
    /// Placing the play cursor.
    Playhead,
    /// Auditioning a row or a note.
    Audition,

    /// Vertical drag sets velocity or level.
    Velocity,
    /// Vertical drag scales a range about its own mean.
    Scale,
    /// Muting.
    Mute,
    /// Typing a syllable onto a note.
    Text,
    /// Splitting at a zone boundary or a timing separator.
    Split,

    /// Cutting out a razor rectangle.
    Razor,
    /// The edge of one.
    RazorEdge,

    /// One of the seven note handles, which each get their own glyph.
    Handle(Handle),

    /// The gesture is blocked — the drawer has the target locked.
    Forbidden,
}

impl Cursor {
    /// The nearest standard CSS keyword.
    ///
    /// A fallback, not the plan: the whole point of the painted glyphs is
    /// that CSS has no `[`, no pencil and no razor, and Blitz supports no
    /// `cursor: url(…)` to supply them. This exists so a DOM host that
    /// cannot paint into the scene still gets *something* directional,
    /// and so the roll element can name a sensible cursor for the frames
    /// before the first paint.
    pub fn css(&self) -> &'static str {
        match self {
            Cursor::Arrow => "default",
            Cursor::Crosshair | Cursor::MarqueeAdd | Cursor::MarqueeToggle => "crosshair",
            Cursor::EdgeLeft | Cursor::EdgeRight => "ew-resize",
            Cursor::StretchLeft | Cursor::StretchRight => "col-resize",
            Cursor::Move => "move",
            Cursor::MoveH => "ew-resize",
            Cursor::MoveV => "ns-resize",
            Cursor::Copy => "copy",
            Cursor::Pencil | Cursor::Curve | Cursor::Brush => "crosshair",
            Cursor::Eraser | Cursor::NoteEraser => "cell",
            Cursor::Hand => "grab",
            Cursor::HandClosed => "grabbing",
            Cursor::Zoom => "zoom-in",
            Cursor::Playhead | Cursor::Split => "col-resize",
            Cursor::Audition => "pointer",
            Cursor::Velocity | Cursor::Scale => "ns-resize",
            Cursor::Mute => "pointer",
            Cursor::Text => "text",
            Cursor::Razor => "crosshair",
            Cursor::RazorEdge => "ew-resize",
            Cursor::Handle(_) => "ns-resize",
            Cursor::Forbidden => "not-allowed",
        }
    }

    /// A short name, for a status readout and for tests to assert on.
    pub fn label(&self) -> &'static str {
        match self {
            Cursor::Arrow => "Arrow",
            Cursor::Crosshair => "Select",
            Cursor::MarqueeAdd => "Add to selection",
            Cursor::MarqueeToggle => "Toggle selection",
            Cursor::EdgeLeft => "Note start",
            Cursor::EdgeRight => "Note end",
            Cursor::StretchLeft => "Stretch from start",
            Cursor::StretchRight => "Stretch from end",
            Cursor::Move => "Move",
            Cursor::MoveH => "Move in time",
            Cursor::MoveV => "Move in pitch",
            Cursor::Copy => "Copy",
            Cursor::Pencil => "Draw",
            Cursor::Curve => "Ramp",
            Cursor::Brush => "Paint",
            Cursor::Eraser => "Erase expression",
            Cursor::NoteEraser => "Erase notes",
            Cursor::Hand | Cursor::HandClosed => "Pan",
            Cursor::Zoom => "Zoom",
            Cursor::Playhead => "Play cursor",
            Cursor::Audition => "Audition",
            Cursor::Velocity => "Velocity",
            Cursor::Scale => "Scale",
            Cursor::Mute => "Mute",
            Cursor::Text => "Lyric",
            Cursor::Split => "Split",
            Cursor::Razor => "Razor",
            Cursor::RazorEdge => "Razor edge",
            Cursor::Handle(h) => h.label(),
            Cursor::Forbidden => "Locked",
        }
    }

    /// The glyph for a resolved action.
    ///
    /// `context` breaks the handful of ties an action alone cannot: the
    /// same [`Action::SelectNote`] is a crosshair over the roll and an
    /// arrow over a note, and [`Action::ActiveTool`] means whatever the
    /// armed tool means, which only the caller knows — so it is left to
    /// [`Cursor::for_tool`].
    pub fn for_action(action: Action, context: Context, aim: Aim) -> Cursor {
        // A handle is drawn in front of everything and pressed before
        // everything (`handle_press` runs ahead of the map), so it also
        // outranks whatever the map would have said here.
        if let Some(handle) = aim.handle {
            return Cursor::Handle(handle);
        }

        // Which bracket, for every action that grabs an end.
        let left = aim.start_edge.unwrap_or(true);

        match action {
            Action::None => match context {
                Context::PianoRoll => Cursor::Crosshair,
                _ => Cursor::Arrow,
            },

            // ── selection ────────────────────────────────────────────
            Action::MarqueeSelect | Action::SelectTouched => Cursor::Crosshair,
            Action::MarqueeAdd
            | Action::AddNoteToSelection
            | Action::SelectNoteAndLater
            | Action::SelectNoteAndLaterSameRow
            | Action::SelectAllInMeasure => Cursor::MarqueeAdd,
            Action::MarqueeToggle | Action::ToggleSelectTouched | Action::ToggleNoteSelection => {
                Cursor::MarqueeToggle
            }
            Action::SelectNote | Action::DeselectAll => match context {
                Context::PianoRoll => Cursor::Crosshair,
                _ => Cursor::Arrow,
            },
            Action::SelectRow => Cursor::Audition,

            // ── note creation ────────────────────────────────────────
            Action::InsertNote
            | Action::InsertNoteNoSnap
            | Action::InsertNoteDragToExtend
            | Action::InsertNoteDragToExtendNoSnap
            | Action::InsertNoteDragToMove
            | Action::InsertNoteDragToEditVelocity => Cursor::Pencil,
            Action::PaintNotes | Action::PaintNotesNoSnap | Action::PaintRowOfNotes => {
                Cursor::Brush
            }

            // ── note editing ─────────────────────────────────────────
            Action::MoveNote
            | Action::MoveNoteNoSnap
            | Action::MoveNoteOneAxis
            | Action::MoveNoteIgnoringSelection => Cursor::Move,
            Action::MoveNoteHorizontally => Cursor::MoveH,
            Action::MoveNoteVertically | Action::TransposeSnapped => Cursor::MoveV,
            Action::CopyNote | Action::CopyNoteNoSnap => Cursor::Copy,
            Action::MoveNoteEdge | Action::MoveNoteEdgeNoSnap => {
                if left {
                    Cursor::EdgeLeft
                } else {
                    Cursor::EdgeRight
                }
            }
            // Length and position stretching both grab an end and pull
            // the whole selection with it, so both take the arrowed
            // bracket — the extra arrow is exactly the "this reaches
            // past the note you grabbed" warning.
            Action::StretchNotes | Action::StretchNotePositions => {
                if left {
                    Cursor::StretchLeft
                } else {
                    Cursor::StretchRight
                }
            }
            Action::DoubleNoteLength | Action::HalveNoteLength => {
                if left {
                    Cursor::EdgeLeft
                } else {
                    Cursor::EdgeRight
                }
            }
            Action::EditNoteVelocity | Action::EditNoteVelocityFine => Cursor::Velocity,
            Action::EraseNote | Action::EraseNotes => Cursor::NoteEraser,
            Action::ToggleNoteMute => Cursor::Mute,
            Action::SetNoteChannelHigher | Action::SetNoteChannelLower => Cursor::MoveV,

            // ── expression ───────────────────────────────────────────
            // `ActiveTool` is deliberately the armed tool's glyph and
            // not a generic one: see `for_tool`, which the caller
            // reaches for instead.
            Action::ActiveTool => Cursor::Arrow,
            Action::PenOverride | Action::EditCcEvents => Cursor::Pencil,
            Action::DrawCcLine => Cursor::Curve,
            Action::EraseCcEvents => Cursor::Eraser,
            Action::ScaleExpression | Action::ScaleCcEvents => Cursor::Scale,

            // ── domain-specific ──────────────────────────────────────
            Action::EditLyric => Cursor::Text,
            Action::CycleArticulation | Action::CycleString => Cursor::Audition,

            // ── razor ────────────────────────────────────────────────
            Action::RazorCreate | Action::RazorAddArea => Cursor::Razor,
            Action::RazorMoveContents
            | Action::RazorMoveContentsNoSnap
            | Action::RazorMoveAreaOnly => Cursor::Move,
            Action::RazorCopyContents => Cursor::Copy,
            Action::RazorMoveVertically => Cursor::MoveV,
            Action::RazorMoveHorizontally => Cursor::MoveH,
            Action::RazorStretchContents | Action::RazorResizeArea => Cursor::RazorEdge,
            Action::RazorDeleteContents | Action::RazorRemoveArea | Action::RazorClearAll => {
                Cursor::NoteEraser
            }

            // ── navigation ───────────────────────────────────────────
            Action::Pan => Cursor::Hand,
            Action::ZoomAnchored => Cursor::Zoom,
            Action::MovePlayhead | Action::MovePlayheadNoSnap => Cursor::Playhead,
            Action::ContextMenu => Cursor::Arrow,
            Action::Audition => Cursor::Audition,
        }
    }

    /// The glyph an armed tool claims a gesture with.
    ///
    /// Reached only for [`Action::ActiveTool`], which is the map saying
    /// "the tool owns this one" — see [`crate::mouse::MouseMap::resolve_for`].
    pub fn for_tool(tool: crate::Tool, context: Context) -> Cursor {
        match tool {
            crate::Tool::Select => match context {
                Context::PianoRoll => Cursor::Crosshair,
                _ => Cursor::Arrow,
            },
            crate::Tool::Pen => Cursor::Pencil,
            crate::Tool::Curve => Cursor::Curve,
            crate::Tool::Eraser => Cursor::Eraser,
            crate::Tool::NoteDraw => Cursor::Pencil,
            crate::Tool::NoteErase => Cursor::NoteEraser,
            crate::Tool::Razor => Cursor::Razor,
            crate::Tool::Velocity => Cursor::Velocity,
            crate::Tool::Zoom => Cursor::Zoom,
        }
    }

    /// The glyph while a gesture is already running.
    ///
    /// Distinct from the hover glyph for one case that matters: a hand
    /// that has closed on the canvas reads as *held*, and every surface
    /// that pans draws it that way. Everything else keeps whatever it
    /// was pressed with, because the cursor changing mid-drag is a
    /// flinch, not information.
    pub fn while_dragging(self) -> Cursor {
        match self {
            Cursor::Hand => Cursor::HandClosed,
            other => other,
        }
    }
}
