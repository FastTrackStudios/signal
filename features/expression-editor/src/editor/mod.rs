//! Editor state and initialization. Behavior is organized by responsibility below.
use crate::{
    blob, camera, cc, chord, clipboard, doc, draft, edit, flam, handles, harmony, memagic, menu,
    mode, mouse, razor, reference, rows, shape, tools, tracks, tuning, zoom,
};
use camera::{Bounds, Camera, Content, Viewport};
use cc::CcDisplay;
use chord::Chord;
use doc::{Dimension, ExpressionDoc, NoteId};
use edit::{Edit, History};
use mode::Mode;
use mouse::MouseMap;
use razor::{RazorArea, RazorSet};
use reference::MidiReference;
use rows::RowSpace;
use shape::Shape;
use tools::{Grid, Hit, Mods, Selection, Tool};
use tracks::Workspace;
use tuning::Tuning;
use zoom::{SmartZoom, ZoomModes};

mod commands;
mod gestures;
mod history;
mod instruments;
mod navigation;
mod razor_ops;
mod workspace;

/// Everything the canvas needs to render and interact, in one place.
///
/// The UI owns one of these and reads it; it never mutates the document
/// except through [`Editor::apply`], which keeps undo honest.
///
/// `PartialEq` so it can ride in a Dioxus prop without a wrapper.
#[derive(Clone, Debug, PartialEq)]
pub struct Editor {
    pub doc: ExpressionDoc,
    /// The shared camera. Its horizontal half is authoritative for every
    /// lane; its vertical half serves the single-track roll.
    pub camera: Camera,
    /// One vertical camera per lane, parallel to the workspace layout.
    ///
    /// Ephemeral — never persisted, re-fitted on load. Time stays on
    /// `camera` because two instruments doubling a line are only
    /// comparable on a common time axis; vertical is per lane because
    /// that is what makes twenty lanes readable at once.
    ///
    /// Fitting happens on load and on an explicit Reset View, and
    /// **never automatically**. Re-fitting when content changes would
    /// rescale a lane under the cursor mid-gesture — drag one note up an
    /// octave and the whole lane moves. The accepted cost is that an
    /// edit can push content out of view, with Reset View one key away.
    pub lane_cameras: Vec<camera::VerticalCamera>,
    /// How far the stack is scrolled, in pixels. Ephemeral.
    pub stack_scroll: f64,
    /// How many lanes fill the viewport before the stack scrolls.
    ///
    /// A user preference, read by the host and handed down — core has
    /// one dependency by design and does not reach for a config file.
    /// Default 5.
    pub lanes_visible: u8,
    pub viewport: Viewport,
    pub tool: Tool,
    /// The dimension being edited. Others may still be drawn as overlays.
    pub dimension: Dimension,
    /// Lanes drawn behind the active one.
    pub overlays: Vec<Dimension>,
    pub selection: Selection,
    /// Which product this editor is being: MIDI, MPE, Drums, Guitar,
    /// Vocals or Audio. Decides which controls are on screen.
    pub mode: Mode,
    /// What the vertical axis means — pitch, drum lanes, or strings.
    pub row_space: RowSpace,
    /// Active razor areas.
    pub razor: RazorSet,
    /// How pinned controller lanes are drawn. The lanes themselves are
    /// document data (`doc.cc`); this is view policy.
    pub cc_display: CcDisplay,
    /// The controller being edited, when CC edit mode is on. The roll
    /// becomes that dimension's editing surface and the notes recede.
    pub cc_edit: Option<u8>,
    /// Velocity / CC lane strip height in pixels, 0 when hidden.
    pub lane_strip_h: f64,
    /// Which dimension the strip shows.
    pub strip_lane: StripLane,
    /// Colour guitar notes by string rather than by pitch class.
    ///
    /// On by default: on a string roll the row *is* the string, so
    /// colouring by it makes the shape of a part legible at a glance —
    /// which run is on the B, where it crosses to the G. Turn it off to
    /// read harmony instead, where pitch-class colour is the more useful
    /// signal.
    pub color_by_string: bool,
    /// Which drum pieces are shown as two rows instead of one.
    ///
    /// Row indices into the drum map. Empty is the ordinary case: a
    /// piece splits only when a part needs the sticking, so the roll
    /// does not carry twice the rows for material that never asks.
    pub split_pieces: Vec<usize>,
    /// The note whose lyric is being typed, if the field is open.
    pub editing_lyric: Option<doc::NoteId>,
    /// Context × modifier → action. Editable, so a host can ship a
    /// REAPER-matching profile or its own.
    pub mouse: MouseMap,
    pub tuning: Tuning,
    pub grid: Grid,
    pub shape: Shape,
    /// Tempo used to place the grid; a real tempo map lives in the
    /// host adapter.
    pub bpm: f64,
    /// Contextual-zoom tuning.
    pub smart_zoom: SmartZoom,
    /// Beats per bar, for the ruler's bar numbering.
    pub beats_per_bar: f64,
    /// Transport position, when the host supplies one.
    pub playhead: Option<f64>,
    /// Cut/copy/paste buffer. Editor-local rather than system-wide —
    /// see [`clipboard::Clipboard`].
    pub clipboard: clipboard::Clipboard,
    /// The other tracks. `doc` above is always the *active* one; the
    /// workspace holds the rest, each with its own undo stack.
    pub tracks: Workspace,
    /// `R` held: reference tracks come forward. A momentary key rather
    /// than a toggle, because it is used to *check* something mid-edit
    /// and a mode you can forget you are in is worse than a held key.
    pub refs_to_front: bool,
    /// Razor drags leave what they land on alone.
    ///
    /// MRE's `I`, "insert mode — don't delete target area". Off, a razor
    /// dropped on occupied ground clears it first, which is what makes
    /// comping work: the take you drag in *replaces* the one underneath.
    /// On, both survive, which is what you want when the razor is a way
    /// of layering rather than of choosing.
    ///
    /// A sticky mode rather than a modifier, because the modifier
    /// spelling of it in MRE is a four-key chord and nobody reaches for
    /// one of those mid-edit.
    pub razor_insert: bool,
    /// Razor drags are locked to one axis. `None` is free movement.
    ///
    /// MRE's `H` and `L`, which are mutually exclusive there and are one
    /// field here for the same reason — two booleans would have a fourth
    /// state that means nothing.
    pub razor_axis: Option<razor::RazorAxis>,
    /// The modifiers currently held down.
    ///
    /// Here rather than in a component signal because the *toolbar*
    /// needs them: holding Ctrl means the next drag razors, and the
    /// surface should say so before you commit to it — the same promise
    /// the painted cursor already makes, which the tool buttons were not
    /// keeping. Only the surface writes this; everything else reads it.
    ///
    /// It changes what the toolbar *shows*, never what a gesture does.
    /// The map already resolves modifiers on its own, and a second
    /// authority for the same question is how the two start disagreeing.
    pub held_mods: Mods,
    /// The time selection: a span with no pitch, unlike a razor.
    ///
    /// The two are different tools and both are worth having. A razor is
    /// a rectangle — these rows, this span — and carves. A time
    /// selection is the whole part between two points, and is what
    /// loops, renders and "select notes in the range" act on. REAPER
    /// keeps both for the same reason.
    ///
    /// Extended by the cursor keys, which is the point: `Ctrl+Shift+h`
    /// and `Ctrl+Shift+l` walk the edit cursor and drag the selection
    /// behind it, so a range gets built by pressing a key rather than by
    /// aiming a drag.
    pub time_selection: Option<(f64, f64)>,
    /// The chord gun's key, scale, depth and inversion.
    ///
    /// Editor state rather than document state: it is how you are
    /// *entering* notes, like the grid or the armed tool, and two people
    /// opening the same part should not inherit each other's idea of
    /// what `3` means.
    pub chord_gun: harmony::ChordGun,
    /// Whether the coarse pitch handle snaps to the tuning. Shift
    /// reverses it per-gesture, as everywhere else on this surface.
    pub snap_pitch: bool,
    /// Whether the amplitude handle edits only the *unvoiced* spans
    /// inside a note rather than the whole thing.
    ///
    /// A consonant carries no pitch, so it is invisible on the pitch
    /// track and untouched by any edit that works on notes — yet a
    /// harsh "s" is exactly the thing that needs riding down. Shift
    /// reverses it per-gesture, so the other scope is always one key
    /// away. When armed, the sibilant spans shade in the waveform and
    /// the amplitude handle draws hollow.
    pub sibilant_scope: bool,
    /// Timing mode: separators draw and take the pointer.
    ///
    /// A mode rather than always-on, because a full-height line at
    /// every note join is a picket fence across a screen that is
    /// otherwise about pitch.
    pub timing_mode: bool,
    /// Show every track stacked on one timeline instead of the single
    /// roll.
    ///
    /// A view flag rather than a mode: the document, selection and
    /// history are untouched, and switching back leaves the roll exactly
    /// where it was. The stack is how you find *which* track needs work
    /// — the roll is where you do it.
    pub stacked: bool,
    /// A MIDI part loaded as a tuning target, drawn behind the notes.
    pub reference: Option<MidiReference>,
    /// `M` held: the reference comes forward, as with `Shift+R` for
    /// reference tracks. Momentary for the same reason.
    pub reference_to_front: bool,
    /// The temporary note: a range inside a note that the handles
    /// address instead of the whole thing. `None` is the ordinary case.
    ///
    /// Not document data — it is a view onto a note, discarded the
    /// moment another range is drawn.
    pub temp_note: Option<(NoteId, f64, f64)>,
    history: History,
}

impl Editor {
    pub fn new(doc: ExpressionDoc, viewport: Viewport) -> Self {
        let content = content_of(&doc);
        let camera =
            camera::reset_view(content, viewport, CUSHION, PAD, camera::RowFold::default());
        let tracks = Workspace::single("Track 1", doc.clone());
        let mut editor = Self {
            doc,
            tracks,
            camera,
            lane_cameras: Vec::new(),
            stack_scroll: 0.0,
            lanes_visible: 5,
            viewport,
            // `Tool::default()`, not a literal: this said `Tool::Curve`
            // and silently outranked the enum's own default, so changing
            // that default changed nothing anyone could see.
            tool: Tool::default(),
            held_mods: Mods::default(),
            time_selection: None,
            chord_gun: harmony::ChordGun::default(),
            razor_insert: false,
            razor_axis: None,
            dimension: Dimension::Pitch,
            overlays: Vec::new(),
            selection: Selection::default(),
            mode: Mode::default(),
            row_space: RowSpace::Pitch,
            razor: RazorSet::default(),
            cc_display: CcDisplay::default(),
            cc_edit: None,
            lane_strip_h: 96.0,
            strip_lane: StripLane::Velocity,
            color_by_string: true,
            split_pieces: Vec::new(),
            editing_lyric: None,
            mouse: MouseMap::default(),
            tuning: Tuning::default(),
            grid: Grid::default(),
            shape: Shape::Linear,
            bpm: 120.0,
            smart_zoom: SmartZoom::default(),
            beats_per_bar: 4.0,
            playhead: None,
            clipboard: clipboard::Clipboard::default(),
            refs_to_front: false,
            snap_pitch: true,
            sibilant_scope: false,
            timing_mode: false,
            stacked: false,
            reference: None,
            reference_to_front: false,
            temp_note: None,
            history: History::new(tracks::HISTORY_LIMIT),
        };
        // Fit once, up front. Every later fit is an explicit Reset View.
        editor.fit_lanes();
        editor
    }

    pub fn hit_test(&self, x: f64, y: f64) -> Hit {
        tools::hit_test(
            &self.doc,
            &self.camera,
            self.viewport,
            self.dimension,
            x,
            y,
            tools::HitConfig::default(),
        )
    }

    /// Lanes to draw, back to front: overlays first, active dimension last.
    pub fn draw_order(&self) -> Vec<Dimension> {
        let mut lanes: Vec<Dimension> = self
            .overlays
            .iter()
            .copied()
            .filter(|&l| l != self.dimension)
            .collect();
        lanes.push(self.dimension);
        lanes
    }
}

/// Horizontal cushion around the item, as a fraction of its span.
pub const CUSHION: f64 = 0.03;
/// Vertical headroom above and below the content in Reset View.
pub const PAD: f64 = 0.35;
/// Edge magnet stays out of the inner 35% of the item's half-span.
const EDGE_DEAD_ZONE: f64 = 0.35;
/// Whitespace the edge magnet leaves past a framed edge.
const EDGE_WHITESPACE: f64 = 0.2;
/// The reset magnet is inert until 80% of the way to Reset View.
const RESET_TAIL_START: f64 = 0.8;
const LOCAL_PITCH_WEIGHT: f64 = 0.45;
const MOUSE_PITCH_WEIGHT: f64 = 0.22;
/// Deep-zoom center pull begins at 72% of the vertical range.
const DEEP_ZOOM_ONSET: f64 = 0.72;

/// The content box a camera should frame.
pub fn content_of(doc: &ExpressionDoc) -> Content {
    let (pitch_lo, pitch_hi) = doc.pitch_extent().unwrap_or((48.0, 72.0));
    Content {
        t_start: doc.start,
        t_end: doc.end.max(doc.start + 1.0),
        pitch_lo,
        pitch_hi: pitch_hi.max(pitch_lo + 1.0),
    }
}

/// What the bottom lane strip displays.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, facet::Facet)]
pub enum StripLane {
    /// Per-note velocity, drawn as stems.
    Velocity,
    /// Per-note release velocity.
    OffVelocity,
    /// A continuous expression dimension, drawn as a curve.
    Expression(Dimension),
}

impl StripLane {
    pub const ALL: [StripLane; 5] = [
        StripLane::Velocity,
        StripLane::OffVelocity,
        StripLane::Expression(Dimension::Pitch),
        StripLane::Expression(Dimension::Pressure),
        StripLane::Expression(Dimension::Timbre),
    ];

    pub fn label(&self) -> &'static str {
        match self {
            StripLane::Velocity => "Velocity",
            StripLane::OffVelocity => "Release",
            StripLane::Expression(Dimension::Pitch) => "Pitch",
            StripLane::Expression(Dimension::Pressure) => "Pressure",
            StripLane::Expression(Dimension::Timbre) => "Timbre",
        }
    }

    /// Per-note lanes draw as stems; continuous ones draw as a curve.
    pub fn is_per_note(&self) -> bool {
        matches!(self, StripLane::Velocity | StripLane::OffVelocity)
    }
}
