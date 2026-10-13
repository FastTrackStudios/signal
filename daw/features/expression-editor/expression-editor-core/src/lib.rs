//! `expression-editor-core` — the portable engine behind the
//! expression editor.
//!
//! One editor, two products:
//!
//! - **an MPE editor**, where per-note pitch bend, channel pressure,
//!   and CC74 are edited as properties *of a note* instead of in raw
//!   controller lanes;
//! - **a Melodyne competitor**, where an analyzed audio clip's notes
//!   carry a tracked f0 contour that edits by exactly the same
//!   gestures.
//!
//! They unify because a note in either domain is the same thing: an
//! integer pitch row plus a continuous curve measured in semitones
//! relative to it. See [`doc`] for why that framing is the whole trick,
//! and [`blob`] for the center/drift/vibrato decomposition that makes
//! Melodyne's two headline sliders work on hand-drawn MIDI too.
//!
//! This crate holds no UI, no DSP, and no DAW. It is deliberately
//! dependency-free so it compiles for wasm, Blitz-native, and plugin
//! builds alike, and so the interaction model can be tested headlessly
//! — the split MPElodyne got right, kept.
//!
//! The Dioxus surface lives in `expression-editor-ui`; domain adapters
//! (MIDI takes, `tune_dsp::PitchDoc`) live with their domains.

pub mod actions;
pub mod blob;
pub mod camera;
pub mod cc;
pub mod chord;
pub mod clipboard;
pub mod cursor;
pub mod doc;
pub mod draft;
pub mod edit;
pub mod flam;
pub mod handles;
pub mod harmony;
pub mod kit;
pub mod memagic;
pub mod menu;
pub mod mode;
pub mod modulation;
pub mod mouse;
pub mod multitool;
pub mod razor;
pub mod reference;
pub mod rows;
pub mod shape;
pub mod timing;
pub mod tools;
pub mod tracks;
pub mod tuning;
pub mod zoom;

pub use camera::{Bounds, Camera, Content, VerticalCamera, Viewport};
pub use cc::{CcDisplay, CcLane, CcSet};
pub use chord::Chord;
pub use cursor::{Aim, Cursor};
pub use doc::{
    Curve, CurveShape, Dimension, ExpressionDoc, Marker, Note, NoteId, Point, Target, TimeBase,
};
pub use draft::PitchDraft;
pub use edit::{Edit, History};
pub use handles::{Handle, Scope};
pub use mode::{Mode, ModeFamily};
pub use mouse::{Action, MouseMap};
pub use multitool::{Bend, Steepness, Zone};
pub use razor::{RazorArea, RazorSet};
pub use reference::{MidiReference, RefNote, SnapSource};
pub use rows::{Articulation, DrumMap, NoteShape, RowSpace, StringTuning};
pub use shape::Shape;
pub use timing::{Separator, StretchLaw};
pub use tools::{Grid, Hit, Mods, Selection, Tool};
pub use tracks::{RefColor, Track, Workspace};
pub use tuning::{Temperament, Tuning};
pub use zoom::{HorizontalMode, SmartZoom, VerticalMode, ZoomModes};

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

    /// Add a track and return its index. It does not become active.
    ///
    /// The guid is generated. Hosts that have one should use
    /// [`Editor::add_track_with_guid`] instead, so that anything
    /// persisted against this track still resolves next session.
    pub fn add_track(&mut self, name: impl Into<String>, doc: ExpressionDoc) -> usize {
        self.tracks.push(tracks::Track::new(name, doc))
    }

    /// Add a track carrying the host's identity.
    pub fn add_track_with_guid(
        &mut self,
        guid: impl Into<String>,
        name: impl Into<String>,
        doc: ExpressionDoc,
    ) -> usize {
        self.tracks.push(tracks::Track::with_guid(guid, name, doc))
    }

    /// Rows of headroom a lane leaves around its content before fitting.
    ///
    /// Without it, a track whose notes are all on one row fits to zero
    /// height and draws as a hairline, and a melody touching its own
    /// extremes has notes flush against the lane edge where they read as
    /// clipped.
    pub const LANE_FIT_PAD: f64 = 1.0;

    /// Extra weight the active lane gets — you are working in it and it
    /// should have the room.
    ///
    /// In core rather than the renderer so that auto-scroll lays lanes
    /// out exactly as the renderer will; a mismatch scrolls to the wrong
    /// place.
    pub const ACTIVE_BOOST: f32 = 1.8;

    /// The lane-height floor, derived from the viewport and the
    /// `lanes_visible` preference.
    ///
    /// Derived rather than stored, because a pixel floor means something
    /// different on every screen: 200px is five lanes on a laptop and
    /// eleven on a studio monitor.
    pub fn lane_floor(&self) -> f32 {
        (self.viewport.h as f32 / self.lanes_visible.max(1) as f32).max(1.0)
    }

    /// Total height the stack wants, which may exceed the viewport.
    pub fn stack_height(&self, active_boost: f32) -> f32 {
        self.tracks
            .stack(self.viewport.h as f32, active_boost, self.lane_floor())
            .last()
            .map(|r| r.y + r.height)
            .unwrap_or(0.0)
    }

    /// Scroll just enough to bring a lane fully into view.
    ///
    /// Minimal rather than centred: centring makes the whole stack jump
    /// on every switch, while a minimal scroll leaves the surrounding
    /// lanes where your eye left them. This does not contradict the
    /// no-auto-refit rule — the view must not move *mid-gesture*, but
    /// changing which lane is active is the request to work somewhere
    /// else, and a highlight you cannot see is worse than a small
    /// scroll.
    pub fn scroll_lane_into_view(&mut self, lane: usize, active_boost: f32) {
        let rows = self
            .tracks
            .stack(self.viewport.h as f32, active_boost, self.lane_floor());
        let Some(row) = rows.iter().find(|r| r.lane == lane) else {
            return;
        };
        let (top, bottom) = (row.y as f64, (row.y + row.height) as f64);
        let view_h = self.viewport.h;

        if top < self.stack_scroll {
            self.stack_scroll = top;
        } else if bottom > self.stack_scroll + view_h {
            self.stack_scroll = bottom - view_h;
        }

        // Never scroll past the end, and never above the top.
        let total = rows.last().map(|r| (r.y + r.height) as f64).unwrap_or(0.0);
        let max = (total - view_h).max(0.0);
        self.stack_scroll = self.stack_scroll.clamp(0.0, max);
    }

    /// The vertical camera for a lane, if it has been fitted.
    pub fn lane_camera(&self, lane: usize) -> Option<camera::VerticalCamera> {
        self.lane_cameras.get(lane).copied()
    }

    /// Fit every lane to its own content.
    ///
    /// Called on load and from Reset View — **never** in response to an
    /// edit. See [`Editor::lane_cameras`] for why.
    pub fn fit_lanes(&mut self) {
        let count = self.tracks.layout().len();
        let rows = self
            .tracks
            .stack(self.viewport.h as f32, 1.0, 0.0)
            .into_iter()
            .map(|r| (r.lane, r.height as f64))
            .collect::<Vec<_>>();

        self.lane_cameras = (0..count)
            .map(|lane| {
                let height = rows
                    .iter()
                    .find(|(l, _)| *l == lane)
                    .map(|(_, h)| *h)
                    .unwrap_or(self.viewport.h.max(1.0));
                match self
                    .tracks
                    .lane_row_range(lane, &self.doc, Self::LANE_FIT_PAD)
                {
                    Some((lo, hi)) => camera::VerticalCamera::fitted(lo, hi, height),
                    None => camera::VerticalCamera::default(),
                }
            })
            .collect();
    }

    /// Fit one lane, leaving the others where the user put them.
    pub fn fit_lane(&mut self, lane: usize) {
        if self.lane_cameras.len() != self.tracks.layout().len() {
            self.fit_lanes();
            return;
        }
        let height = self
            .tracks
            .stack(self.viewport.h as f32, 1.0, 0.0)
            .into_iter()
            .find(|r| r.lane == lane)
            .map(|r| r.height as f64)
            .unwrap_or(self.viewport.h.max(1.0));
        if let Some((lo, hi)) = self
            .tracks
            .lane_row_range(lane, &self.doc, Self::LANE_FIT_PAD)
            && let Some(slot) = self.lane_cameras.get_mut(lane)
        {
            *slot = camera::VerticalCamera::fitted(lo, hi, height);
        }
    }

    /// Step the selected drum hits through the flam cycle.
    ///
    /// **none → before → after → none.** One key, and the state lives on
    /// the note rather than in a mode: you can look at a hit and know
    /// what the next press will do, which a global before/after flag
    /// cannot give you — there, the same key means different things
    /// depending on what you last did to some other note.
    ///
    /// The third press removing the flam is what makes it a cycle rather
    /// than a trap. Without it, changing your mind means reaching for
    /// undo or hunting the grace note down by hand.
    ///
    /// Silent about pieces that cannot flam — a hi-hat in the selection
    /// is skipped rather than refusing the whole gesture.
    pub fn flam_selection(&mut self) -> usize {
        let RowSpace::Drums(map) = self.row_space.clone() else {
            return 0;
        };
        let ids: Vec<doc::NoteId> = self.selection.notes.clone();

        let mut made = 0;
        for id in ids {
            match flam::flam(&self.doc, &map, id, flam::DEFAULT_FLAM_MS, self.bpm) {
                Ok(edit) => {
                    self.apply(&edit);
                    made += 1;
                }
                Err(_) => continue,
            }
        }

        if made > 0 {
            // Splitting the affected pieces is what makes the flam
            // *visible*: a grace note on a hidden row is a note you
            // cannot see or select.
            self.show_hands_for_selection(&map);
        }
        made
    }

    /// What the next press of the flam key will do to a hit, for a UI
    /// that wants to say so before you press it.
    pub fn flam_step(&self, id: doc::NoteId) -> Option<flam::FlamStep> {
        let RowSpace::Drums(map) = &self.row_space else {
            return None;
        };
        flam::next_step(&self.doc, map, id, flam::DEFAULT_FLAM_MS, self.bpm).ok()
    }

    /// Open both hands for every two-handed piece in the selection.
    fn show_hands_for_selection(&mut self, map: &rows::DrumMap) {
        let rows: Vec<usize> = self
            .selection
            .notes
            .iter()
            .filter_map(|id| self.doc.note(*id))
            .map(|n| n.row.max(0) as usize)
            .collect();
        for row in rows {
            if let Some(other) = map.other_hand_row(row) {
                for r in [row, other] {
                    if !self.split_pieces.contains(&r) {
                        self.split_pieces.push(r);
                    }
                }
            }
        }
        self.split_pieces.sort_unstable();
        self.split_pieces.dedup();
    }

    /// Move the selected hits to a hand.
    ///
    /// The sticking control: a part that says which hand plays what —
    /// notated drum music, or a groove you want to be playable — needs
    /// this to be one action rather than dragging notes between rows and
    /// hoping you picked the right one.
    ///
    /// Opens the piece, because a note that moved to a row you cannot
    /// see has vanished as far as the user is concerned.
    ///
    /// Returns how many moved. Hits on one-handed pieces are skipped
    /// rather than refusing the whole gesture.
    pub fn set_hand_of_selection(&mut self, hand: rows::Hand) -> usize {
        let RowSpace::Drums(map) = self.row_space.clone() else {
            return 0;
        };
        let ids: Vec<doc::NoteId> = self.selection.notes.clone();
        let mut moved = 0;

        for id in ids {
            let Some(note) = self.doc.note(id) else {
                continue;
            };
            let row = note.row.max(0) as usize;
            let Some(target) = map.row_for_hand(row, hand) else {
                continue;
            };
            if target == row {
                continue;
            }
            // Transpose rather than a bare row assignment: on a drum
            // roll a row *is* the pitch, and this is the edit that
            // carries a note's owned expression with it.
            self.apply(&Edit::Transpose {
                notes: vec![id],
                semitones: target as i32 - row as i32,
            });
            moved += 1;
        }

        if moved > 0 {
            self.show_hands_for_selection(&map);
        }
        moved
    }

    /// The header for a drum row, knowing which pieces are open.
    ///
    /// A collapsed piece shows its own name — `T1` — and an open one
    /// shows the hand, `L` or `R`, under it.
    pub fn row_header(&self, row: i32) -> String {
        match &self.row_space {
            RowSpace::Drums(m) => {
                let r = row.max(0) as usize;
                m.display_name(r, self.split_pieces.contains(&r))
            }
            other => other.row_label(row),
        }
    }

    /// The piece name for a split row, so a renderer can bracket the
    /// two hands together.
    pub fn row_group(&self, row: i32) -> Option<String> {
        let RowSpace::Drums(m) = &self.row_space else {
            return None;
        };
        let r = row.max(0) as usize;
        if !self.split_pieces.contains(&r) {
            return None;
        }
        m.group_name(r).map(str::to_string)
    }

    /// Commit a typed syllable and close the field.
    ///
    /// An empty string clears the lyric rather than storing one, so
    /// deleting a syllable is the same gesture as typing one.
    pub fn set_lyric(&mut self, id: doc::NoteId, text: &str) -> bool {
        self.editing_lyric = None;
        self.apply(&Edit::SetText {
            note: id,
            text: (!text.trim().is_empty()).then(|| text.trim().to_string()),
        })
    }

    /// Abandon the lyric field without writing anything.
    pub fn cancel_lyric(&mut self) {
        self.editing_lyric = None;
    }

    /// Put the selection on `fret`, keeping each note's string.
    ///
    /// The fret is derived, so this transposes — it is the "slide the
    /// shape up the neck" gesture, not a relabelling.
    pub fn set_fret_of_selection(&mut self, fret: u8) -> bool {
        let notes: Vec<doc::NoteId> = self.selection.notes.to_vec();
        if notes.is_empty() {
            return false;
        }
        self.apply(&Edit::SetFret { notes, fret })
    }

    /// Move one band split and re-sort every slice into its new band.
    pub fn move_band_split(&mut self, index: usize, hz: f64) -> bool {
        // The document's row space is the authority — every edit reads
        // that one, and `Editor::row_space` is a view of it that is not
        // populated until a mode is applied.
        let RowSpace::Bands(bands) = &self.doc.row_space else {
            return false;
        };
        let mut bands = bands.clone();
        if index >= bands.splits.len() {
            return false;
        }
        // Splits are ascending by contract, and the caller addresses one
        // by index — the inspector's -/+ buttons key on the render-time
        // position. Re-sorting after a write would hand the next click a
        // different split, so a split is instead penned between its
        // neighbours: crossing one has no meaning, since the bands *are*
        // the ascending boundaries.
        let lower = if index == 0 {
            f64::NEG_INFINITY
        } else {
            bands.splits[index - 1]
        };
        let upper = bands
            .splits
            .get(index + 1)
            .copied()
            .unwrap_or(f64::INFINITY);
        bands.splits[index] = hz.clamp(lower, upper);
        let ok = self.apply(&Edit::SetBands {
            bands: bands.clone(),
        });
        if ok {
            self.row_space = RowSpace::Bands(bands);
        }
        ok
    }

    /// Which hand a hit is played with, when its piece has two.
    pub fn hand_of_note(&self, id: doc::NoteId) -> Option<rows::Hand> {
        let RowSpace::Drums(map) = &self.row_space else {
            return None;
        };
        let row = self.doc.note(id)?.row.max(0) as usize;
        map.hand_of(row)
    }

    /// Show or collapse a two-handed piece.
    pub fn toggle_piece_split(&mut self, row: usize) {
        let RowSpace::Drums(map) = self.row_space.clone() else {
            return;
        };
        let Some(other) = map.other_hand_row(row) else {
            return;
        };
        if self.split_pieces.contains(&row) {
            self.split_pieces.retain(|r| *r != row && *r != other);
        } else {
            self.split_pieces.push(row);
            self.split_pieces.push(other);
            self.split_pieces.sort_unstable();
            self.split_pieces.dedup();
        }
        self.refresh_fold();
    }

    /// Recompute which rows the roll folds away.
    ///
    /// A two-handed piece that is not split shows only its right hand,
    /// so the left is folded onto it — one lane called `T1` rather than
    /// two called `T1 L` and `T1 R`.
    pub fn refresh_fold(&mut self) {
        let hidden = match &self.row_space {
            RowSpace::Drums(map) => {
                let visible = map.visible_rows(&self.split_pieces);
                (0..map.lanes.len())
                    .filter(|r| !visible.contains(r))
                    .map(|r| r as i32)
                    .collect()
            }
            _ => Vec::new(),
        };
        self.camera.fold = crate::camera::RowFold::new(hidden);
    }

    /// Give the active track the host's identity.
    ///
    /// `Editor::new` cannot know it — the document arrives before the
    /// adapter has resolved which track it came from — so the adapter
    /// hands it over immediately afterwards. Without this, an editor
    /// opened on a REAPER take would key its persisted state on a
    /// generated guid that means nothing next session.
    pub fn adopt_track_identity(&mut self, guid: impl Into<String>, name: Option<String>) {
        let active = self.tracks.active();
        if let Some(track) = self.tracks.track_mut(active) {
            track.guid = guid.into();
            if let Some(name) = name {
                track.name = name;
            }
        }
    }

    /// Switch to track `i`, parking the current document and history.
    ///
    /// The camera does not move: switching tracks changes what you are
    /// editing, not where you are looking. The selection *is* cleared,
    /// because note ids are per-document and a selection carried across
    /// would point at whatever happened to share those ids.
    /// Move to the next track in the active lane, wrapping.
    ///
    /// The escape hatch for the rule that only the active track takes
    /// gestures: with a vocal and its reference MIDI superimposed, this
    /// is how you reach the other one. Never leaves the lane — moving
    /// between lanes is a click.
    ///
    /// Goes through [`Editor::switch_track`] rather than moving the
    /// active index directly, so the live document and its history are
    /// parked exactly as they are on any other track change.
    pub fn cycle_track_in_lane(&mut self) -> bool {
        let Some(lane) = self.tracks.active_lane() else {
            return false;
        };
        match self.tracks.next_in_lane(lane, self.tracks.active()) {
            Some(next) => self.switch_track(next),
            None => false,
        }
    }

    /// Replace one track's document with a fresh analysis, wherever it
    /// lives — the parked slot, or the live editor when the track is
    /// active. See [`tracks::Workspace::reload_doc`] for why history
    /// resets.
    pub fn reload_track_doc(&mut self, guid: &str, doc: doc::ExpressionDoc) -> bool {
        if self
            .tracks
            .track(self.tracks.active())
            .is_some_and(|t| t.guid == guid)
        {
            self.doc = doc;
            self.history = History::new(tracks::HISTORY_LIMIT);
            self.selection.clear();
            return true;
        }
        self.tracks.reload_doc(guid, doc)
    }

    pub fn switch_track(&mut self, i: usize) -> bool {
        // Validate before moving anything out of the editor, so a
        // rejected switch cannot leave `doc` and `history` stranded.
        if i == self.tracks.active() || i >= self.tracks.len() {
            return false;
        }
        let history = core::mem::replace(&mut self.history, History::new(tracks::HISTORY_LIMIT));
        let Some((doc, history)) = self.tracks.swap_active(i, self.doc.clone(), history) else {
            return false;
        };
        self.doc = doc;
        self.history = history;
        // Changing which lane is active is the request to work somewhere
        // else, so the view follows — minimally.
        if let Some(lane) = self.tracks.active_lane() {
            self.scroll_lane_into_view(lane, Self::ACTIVE_BOOST);
        }
        // Note ids are per-document, so a carried-over selection would
        // point at whatever happens to share those ids on the new track.
        self.selection.clear();
        self.razor = RazorSet::default();
        self.cc_edit = None;
        // The new track brings its own surface with it. Without this a
        // vocal switched to from a kit would be edited on a slice strip
        // — the document changed and nothing else did.
        let mode = self.tracks.mode();
        if mode != self.mode {
            self.set_mode(mode);
        }
        true
    }

    /// The track being edited.
    pub fn active_track(&self) -> usize {
        self.tracks.active()
    }

    /// Show a pitch drawing without recording it.
    pub fn preview_draft(&mut self, draft: &mut draft::PitchDraft) -> bool {
        let mut any = false;
        for e in draft.preview_edits() {
            any |= self.apply_live(&e);
        }
        any
    }

    /// Commit a pitch drawing as **one** step of history.
    ///
    /// Rewinds to the captured curve first, without recording, so the
    /// snapshot the history takes is the state *before* drawing began.
    /// Skipping that would snapshot the live preview instead, and undo
    /// would return to the drawing rather than to what was sung — which
    /// is the whole promise of an explicit apply.
    pub fn apply_draft(&mut self, draft: &draft::PitchDraft) -> bool {
        let Some(commit) = draft.apply_edit() else {
            return false;
        };
        if let Some(rewind) = draft.cancel_edit() {
            self.apply_live(&rewind);
        }
        self.apply(&commit)
    }

    /// Throw a pitch drawing away, restoring exactly what was captured.
    pub fn dismiss_draft(&mut self, draft: &draft::PitchDraft) -> bool {
        match draft.cancel_edit() {
            Some(e) => self.apply_live(&e),
            None => false,
        }
    }

    /// The scope handles on `note` currently address.
    ///
    /// The temporary note if one is open on *this* note, the whole note
    /// otherwise. Resolving it here rather than at each call site is
    /// what makes temporary notes free: every handle already takes a
    /// scope, so nothing else has to know the feature exists.
    /// The tool the toolbar should light up right now.
    ///
    /// The armed tool, unless the modifiers currently held would make
    /// the next drag do something else — hold Ctrl and the razor lights
    /// up, hold Alt and note-draw does, exactly as holding `z` lights up
    /// zoom. The difference is that `z` really does arm the tool and
    /// these do not: the map resolves modifiers by itself, so this only
    /// reports what the map would say.
    ///
    /// Asked of the map rather than of a modifier table, so a rebound
    /// gesture relights the right button with nothing else to change.
    pub fn shown_tool(&self) -> Tool {
        self.mouse
            .resolve(
                mouse::Context::PianoRoll,
                mouse::Gesture::Drag,
                self.held_mods,
            )
            .tool_preview()
            .unwrap_or(self.tool)
    }

    pub fn scope_for(&self, note: NoteId) -> handles::Scope {
        match self.temp_note {
            Some((id, t0, t1)) if id == note => handles::Scope::Range { t0, t1 },
            _ => handles::Scope::Note,
        }
    }

    /// Open a temporary note over `[t0, t1]` of `note`.
    ///
    /// Dragging a new range always replaces the previous one — there is
    /// only ever one, and it is discarded rather than accumulated.
    /// Ranges narrower than a pixel or two are rejected, so a stray
    /// click does not leave an invisible scope armed on the note.
    pub fn set_temp_note(&mut self, note: NoteId, t0: f64, t1: f64) -> bool {
        let Some(n) = self.doc.note(note) else {
            return false;
        };
        let scope = handles::Scope::Range { t0, t1 };
        if !scope.is_valid(n) {
            return false;
        }
        let (lo, hi) = scope.span(n);
        if (hi - lo) < self.camera.units_per_px * 3.0 {
            return false;
        }
        self.temp_note = Some((note, lo, hi));
        true
    }

    pub fn clear_temp_note(&mut self) {
        self.temp_note = None;
    }

    /// Write a handle drag at pointer height `y`.
    ///
    /// Always rebuilt from the drag's captured lanes, never from what
    /// is currently on screen — see [`handles::HandleDrag`]. Call inside
    /// a gesture opened with [`Editor::begin_gesture`]; this uses
    /// `apply_live` so the whole drag is one undo step.
    /// `snap` applies only to the coarse pitch handle. The UI resolves
    /// it as `ed.snap_pitch != mods.shift`, the same shift-reverses
    /// rule every other snap on this surface follows.
    pub fn drag_handle(&mut self, drag: &mut handles::HandleDrag, y: f64, snap: bool) -> bool {
        use handles::Handle as H;
        let amount = drag.amount(y, self.viewport.h);
        drag.applied = amount;

        let Some(note) = self.doc.note(drag.note) else {
            return false;
        };
        let (t0, t1) = drag.scope.span(note);
        if t1 <= t0 {
            return false;
        }
        let id = drag.note;

        // Restore the captured dimension first, so the edit below always
        // sees the same input it saw on the previous frame.
        let restore = |ed: &mut Self, dimension: Dimension| {
            let points = drag.base_of(dimension).points().to_vec();
            ed.apply_live(&Edit::RestoreDimension {
                note: id,
                dimension,
                t0,
                t1,
                points,
            });
        };

        match drag.handle {
            H::Pitch | H::FinePitch => {
                restore(self, Dimension::Pitch);
                // Coarse pitch snaps, fine pitch never does — that is
                // the whole distinction between the two handles. The
                // row is left alone during the drag and normalized on
                // release.
                //
                // Snapping goes through the temperament rather than
                // rounding to a semitone, so in a microtonal tuning the
                // handle lands on the tuning's degrees and not on 12-TET
                // ones that are not in the scale.
                let delta = if drag.handle == H::Pitch && snap {
                    // The note's pitch is its contour's *centre*, not
                    // its value at the midpoint — that reading carries
                    // whatever drift and vibrato are passing through,
                    // and snapping against it lands the note wherever
                    // the wobble happened to be.
                    let base = drag.base_row as f64
                        + blob::decompose(
                            drag.base_of(Dimension::Pitch),
                            t0,
                            t1,
                            edit::DEFAULT_SAMPLES,
                            self.doc.time_base.units_per_second(self.bpm),
                            Dimension::Pitch.default_value(),
                        )
                        .center;
                    self.tuning.snap(base + amount).pitch - base
                } else {
                    amount
                };
                self.apply_live(&Edit::ShiftDimension {
                    note: id,
                    dimension: Dimension::Pitch,
                    t0,
                    t1,
                    delta,
                })
            }
            H::LeftSlope | H::RightSlope => {
                restore(self, Dimension::Pitch);
                self.apply_live(&Edit::TiltDimension {
                    note: id,
                    dimension: Dimension::Pitch,
                    t0,
                    t1,
                    amount,
                    from_start: drag.handle == H::LeftSlope,
                })
            }
            H::Formant | H::Amplitude => {
                let dimension = if drag.handle == H::Formant {
                    Dimension::Timbre
                } else {
                    Dimension::Pressure
                };
                // A level, so it reads off the captured value at the
                // scope's midpoint rather than restoring and shifting.
                let mid = (t0 + t1) * 0.5;
                let base = drag
                    .base_of(dimension)
                    .sample(mid, dimension.default_value());

                // Sibilant scope: the amplitude handle addresses only
                // the unvoiced spans inside the scope. Each is written
                // separately rather than as one range, because the
                // voiced singing between them must not move.
                if drag.handle == H::Amplitude && drag.sibilants {
                    let spans: Vec<(f64, f64)> = self
                        .doc
                        .unvoiced
                        .iter()
                        .filter(|(a, b)| *b >= t0 && *a <= t1)
                        .map(|(a, b)| (a.max(t0), b.min(t1)))
                        .filter(|(a, b)| b > a)
                        .collect();
                    // A hairline either side of each span, holding the
                    // level the note already had.
                    //
                    // Without these the edit leaks: a curve holds its
                    // endpoint value outside the authored range, so on
                    // a note whose Pressure was never authored, writing
                    // only the consonant would raise the *whole* note
                    // to that level — the singing included, which is
                    // precisely what this scope exists to avoid.
                    let eps = (t1 - t0) * 1e-3;
                    let mut any = false;
                    for (a, b) in spans {
                        // Restore first: the level is absolute, so a
                        // span already written this frame has to go
                        // back before it is written again.
                        let points = drag.base_of(dimension).points().to_vec();
                        self.apply_live(&Edit::RestoreDimension {
                            note: id,
                            dimension,
                            t0: a,
                            t1: b,
                            points,
                        });
                        for (g0, g1) in [(a - eps * 2.0, a - eps), (b + eps, b + eps * 2.0)] {
                            if g0 > t0 && g1 < t1 {
                                let held = drag
                                    .base_of(dimension)
                                    .sample(g0, dimension.default_value());
                                self.apply_live(&Edit::SetDimensionLevel {
                                    note: id,
                                    dimension,
                                    t0: g0,
                                    t1: g1,
                                    value: held,
                                });
                            }
                        }
                        any |= self.apply_live(&Edit::SetDimensionLevel {
                            note: id,
                            dimension,
                            t0: a,
                            t1: b,
                            value: base + amount,
                        });
                    }
                    return any;
                }

                self.apply_live(&Edit::SetDimensionLevel {
                    note: id,
                    dimension,
                    t0,
                    t1,
                    value: base + amount,
                })
            }
            H::Vibrato => {
                restore(self, Dimension::Pitch);
                // 1.0 is as sung; 0 is robotic; above 1 exaggerates.
                // Drift is held at full so the vibrato handle changes
                // only the vibrato, which is what it says it does.
                self.apply_live(&Edit::ReblendPitch {
                    note: id,
                    t0,
                    t1,
                    drift_amount: 1.0,
                    modulation_amount: (1.0 + amount).max(0.0),
                })
            }
        }
    }

    /// Finish a handle drag.
    ///
    /// Folds whole semitones back into the row, restoring the invariant
    /// the surface depends on. Only the pitch handles can break it, so
    /// only they pay for it.
    pub fn end_handle_drag(&mut self, drag: &handles::HandleDrag) -> bool {
        use handles::Handle as H;
        if !matches!(drag.handle, H::Pitch | H::FinePitch) {
            return false;
        }
        self.apply_live(&Edit::NormalizeRow {
            notes: vec![drag.note],
        })
    }

    /// Apply an edit through the undo stack.
    pub fn apply(&mut self, edit: &Edit) -> bool {
        self.history.apply(&mut self.doc, edit)
    }

    /// Snapshot before a drag that will stream many edits, so the whole
    /// gesture collapses into one undo step.
    pub fn begin_gesture(&mut self) {
        self.history.begin_gesture(&self.doc);
    }

    /// Apply without recording — for the streaming edits inside a
    /// gesture already opened with [`Editor::begin_gesture`].
    pub fn apply_live(&mut self, edit: &Edit) -> bool {
        edit.apply(&mut self.doc)
    }

    // ── razor operations ─────────────────────────────────────────────
    //
    // The razor's *operations* have existed in `razor::` since it was
    // written; what was missing was any way to reach them. They took a
    // single area and a document, which is the right shape for the
    // algorithm and the wrong one for a command: every caller then had
    // to loop the set, open a gesture, and remember that carving
    // invalidates ids.
    //
    // These are that missing layer. One undo step each, all areas, and
    // no argument the caller has to get right.

    /// Run `op` over every area, as one undo step.
    ///
    /// Areas are snapshotted first because the operations carve, which
    /// rewrites `doc.notes` underneath an iteration over `self.razor`.
    /// The whole set, not just one: a razor set is a *single* selection
    /// that happens to be discontiguous, and an operation that quietly
    /// applied to one rectangle of four would be the surprise.
    fn razor_op(&mut self, op: impl Fn(&mut ExpressionDoc, RazorArea) -> bool) -> bool {
        if self.razor.is_empty() {
            return false;
        }
        let areas = self.razor.areas.clone();
        self.begin_gesture();
        let mut ok = false;
        for area in areas {
            ok |= op(&mut self.doc, area);
        }
        ok
    }

    /// Delete everything inside the areas, keeping the areas.
    ///
    /// The areas stay so a delete can be followed by another operation
    /// on the same region — clearing them as well would make the
    /// commonest pair (delete, then paste something else there) take two
    /// gestures to set up.
    pub fn razor_delete_contents(&mut self) -> bool {
        self.razor_op(razor::delete_contents)
    }

    /// Mirror each area's contents in time about its own centre.
    pub fn razor_reverse(&mut self) -> bool {
        self.razor_op(razor::reverse_contents)
    }

    /// Split notes at the area edges and leave everything in place.
    ///
    /// Every other razor operation carves as a side effect of doing
    /// something else. This is the carve on its own — the way to turn
    /// "this rectangle" into real note boundaries you can then work with
    /// by ordinary means.
    pub fn razor_split(&mut self) -> bool {
        self.razor_op(|doc, area| !razor::carve(doc, area).is_empty())
    }

    /// Scale each area's contents in time about the area's start.
    ///
    /// `2.0` turns a bar of eighths into a bar of quarters spilling past
    /// the area; `0.5` halves it. The area itself is scaled to match, so
    /// the rectangle keeps describing what it holds.
    pub fn razor_scale(&mut self, factor: f64) -> bool {
        if factor <= 0.0 {
            return false;
        }
        let ok = self.razor_op(|doc, area| {
            razor::stretch_contents(doc, area, area.t0, area.t0 + area.width() * factor)
        });
        if ok {
            for a in &mut self.razor.areas {
                a.t1 = a.t0 + a.width() * factor;
            }
        }
        ok
    }

    /// Reverse the pitches, keeping the rhythm.
    pub fn razor_reverse_pitches(&mut self) -> bool {
        self.razor_op(razor::reverse_pitches)
    }

    /// Mirror the pitches about their own centre.
    pub fn razor_invert(&mut self) -> bool {
        self.razor_op(razor::invert_pitches)
    }

    /// Copy the contents forward by the area's own width, and take the
    /// areas with them.
    ///
    /// The areas move rather than staying put, so pressing it twice
    /// gives you three copies rather than two in the same place — the
    /// gesture is "again", and again has to land somewhere new.
    pub fn razor_duplicate(&mut self) -> bool {
        if self.razor.is_empty() {
            return false;
        }
        let areas = self.razor.areas.clone();
        let insert = self.razor_insert;
        self.begin_gesture();
        let mut ok = false;
        for area in &areas {
            let width = area.width();
            ok |= razor::move_contents(&mut self.doc, *area, width, 0, true, insert);
        }
        for a in &mut self.razor.areas {
            let width = a.width();
            *a = a.translated(width, 0);
        }
        ok
    }

    /// Take the notes inside the areas out of the selection.
    pub fn razor_unselect_contents(&mut self) -> bool {
        if self.razor.is_empty() || self.selection.is_empty() {
            return false;
        }
        let inside: Vec<NoteId> = self
            .razor
            .areas
            .iter()
            .flat_map(|a| razor::peek(&self.doc, *a))
            .collect();
        let before = self.selection.notes.len();
        self.selection.notes.retain(|id| !inside.contains(id));
        before != self.selection.notes.len()
    }

    /// Grow the areas to cover every row.
    ///
    /// MRE's "full-lane area". The rectangle stops being about *which*
    /// pitches and becomes purely about a span of time, which is what
    /// you want when the edit is rhythmic and the pitch range was only
    /// ever an accident of how far you dragged.
    pub fn razor_full_lane(&mut self) -> bool {
        if self.razor.is_empty() {
            return false;
        }
        for a in &mut self.razor.areas {
            a.row_lo = 0;
            a.row_hi = 127;
        }
        true
    }

    /// Erase the active expression lane across the areas.
    pub fn razor_clear_lane(&mut self) -> bool {
        let dimension = self.dimension;
        self.razor_op(move |doc, area| razor::clear_lane(doc, area, dimension))
    }

    /// Select the notes inside the areas, keeping the areas.
    ///
    /// The bridge between the two kinds of selection. A razor says
    /// "this rectangle" and a selection says "these notes"; carving
    /// first is what makes the second answer exact, because afterwards
    /// there are no partial overlaps left to round one way or the other.
    ///
    /// The areas stay, following MRE: selecting is usually a step
    /// towards another razor operation on the same region, not a way of
    /// finishing with it.
    pub fn razor_select_contents(&mut self) -> bool {
        if self.razor.is_empty() {
            return false;
        }
        let areas = self.razor.areas.clone();
        self.begin_gesture();
        let mut ids = Vec::new();
        for area in areas {
            ids.extend(razor::carve(&mut self.doc, area));
        }
        ids.sort_unstable();
        ids.dedup();
        let found = !ids.is_empty();
        self.selection.notes = ids;
        found
    }

    /// Move the areas by `(dt, drows)`, carrying their contents.
    pub fn razor_nudge(&mut self, dt: f64, drows: i32) -> bool {
        if self.razor.is_empty() {
            return false;
        }
        let areas = self.razor.areas.clone();
        let insert = self.razor_insert;
        self.begin_gesture();
        let mut ok = false;
        for area in &areas {
            ok |= razor::move_contents(&mut self.doc, *area, dt, drows, false, insert);
        }
        for a in &mut self.razor.areas {
            *a = a.translated(dt, drows);
        }
        ok || dt != 0.0 || drows != 0
    }

    /// Move the areas' right edge by `dt`, leaving the contents alone.
    ///
    /// Resize rather than stretch: this changes what the rectangle
    /// *covers*, which is the thing you adjust when the sweep came out a
    /// grid line short. Scaling the material is [`Editor::razor_scale`].
    pub fn razor_resize(&mut self, dt: f64) -> bool {
        if self.razor.is_empty() {
            return false;
        }
        let mut ok = false;
        for a in &mut self.razor.areas {
            // Never past its own start: an inverted area is not a
            // smaller one, it is a broken one.
            let t1 = (a.t1 + dt).max(a.t0 + f64::EPSILON);
            ok |= t1 != a.t1;
            a.t1 = t1;
        }
        ok
    }

    /// Grow or shrink the areas by whole rows, from the top.
    pub fn razor_resize_rows(&mut self, drows: i32) -> bool {
        if self.razor.is_empty() {
            return false;
        }
        for a in &mut self.razor.areas {
            a.row_hi = (a.row_hi + drows).max(a.row_lo);
        }
        true
    }

    // ── the edit cursor, and what follows it ─────────────────────────
    //
    // `hjkl` as the FTS REAPER profile defines it
    // (`reaper-input/config/.../navigation.styx` and `midi.styx`): `h`
    // and `l` walk the edit cursor, `j` and `k` change track. The
    // modifiers stack meanings on the horizontal pair — Ctrl for a grid
    // step instead of a measure, Ctrl+Shift to drag a time selection
    // along behind it.
    //
    // Note that `h`/`l` are *not* note movement in that profile, and
    // deliberately: the arrows are. Moving the cursor is what you do
    // constantly and moving notes is what you do on purpose.

    /// Where the edit cursor is, or the left edge of the view.
    ///
    /// The cursor is `Option` because a document does not necessarily
    /// have one; every command that walks it has to start somewhere, and
    /// what is on screen is the least surprising place.
    pub fn cursor(&self) -> f64 {
        self.playhead.unwrap_or_else(|| self.camera.t_at(0.0))
    }

    /// Move the edit cursor by `delta`, clamped to the document.
    pub fn move_cursor(&mut self, delta: f64) -> bool {
        let to = (self.cursor() + delta).clamp(self.doc.start, self.doc.end);
        let moved = self.playhead != Some(to);
        self.playhead = Some(to);
        moved
    }

    /// Move the cursor and drag the time selection with it.
    ///
    /// Anchored on where the selection already starts, so pressing the
    /// key repeatedly grows one range rather than making a new one each
    /// time — and reversing direction shrinks it back rather than
    /// flipping to a fresh range on the other side.
    pub fn move_cursor_extending(&mut self, delta: f64) -> bool {
        let from = self.time_selection.map(|(a, _)| a).unwrap_or(self.cursor());
        if !self.move_cursor(delta) {
            return false;
        }
        let to = self.cursor();
        self.time_selection = Some(if from <= to { (from, to) } else { (to, from) });
        true
    }

    /// One measure, in document units — what `h` and `l` step by.
    pub fn measure(&self) -> f64 {
        self.units_per_bar()
    }

    /// The grid step, or a beat when the grid is free.
    pub fn grid_step(&self) -> f64 {
        let step = self.grid.step(self.units_per_beat());
        if step > 0.0 {
            step
        } else {
            self.units_per_beat()
        }
    }

    pub fn clear_time_selection(&mut self) -> bool {
        self.time_selection.take().is_some()
    }

    /// Lengthen or shorten the selected notes by `delta`.
    ///
    /// Never past nothing: a note dragged shorter than zero would read
    /// as one that starts after it ends, and the shortening key is held
    /// down as readily as any other.
    pub fn nudge_note_lengths(&mut self, delta: f64) -> bool {
        let notes = self.selection.notes.clone();
        if notes.is_empty() {
            return false;
        }
        let floor = self.grid_step() * 0.25;
        self.begin_gesture();
        let mut ok = false;
        for id in notes {
            let Some(n) = self.doc.note(id) else { continue };
            let (start, end) = (n.start, n.end);
            let next = (end + delta).max(start + floor);
            if (next - end).abs() > f64::EPSILON {
                ok |= self.apply_live(&Edit::Resize {
                    note: id,
                    start,
                    end: next,
                });
            }
        }
        ok
    }

    /// Step to another track, wrapping at both ends.
    pub fn step_track(&mut self, delta: i32) -> bool {
        let n = self.tracks.len() as i32;
        if n <= 1 {
            return false;
        }
        let next = (self.active_track() as i32 + delta).rem_euclid(n) as usize;
        self.switch_track(next)
    }

    // ── the chord gun ────────────────────────────────────────────────

    /// Fire the chord on `degree` (1..=7) into the document.
    ///
    /// At the play cursor when there is one, else at the left edge of
    /// the view — the same rule paste follows, and for the same reason:
    /// dropping a chord at zero puts it off screen and looks like
    /// nothing happened.
    ///
    /// One grid division long, because that is the length every other
    /// insert on this surface uses and a chord you have to resize is a
    /// chord you did not want fired.
    ///
    /// The new notes become the selection, so the gesture composes: fire
    /// a chord, then `v u` to ramp it, or drag it somewhere else. And it
    /// is one undo step, however many notes the depth produced.
    pub fn insert_chord(&mut self, degree: usize) -> bool {
        let pitches = self.chord_gun.pitches(degree);
        if pitches.is_empty() {
            return false;
        }
        let start = self
            .playhead
            .unwrap_or_else(|| self.camera.t_at(0.0))
            .max(self.doc.start);
        let start = self.snap_time(start);
        let len = self.grid.step(self.units_per_beat());
        // A free grid has no step to take a length from; a beat is the
        // honest default rather than a zero-length note.
        let len = if len > 0.0 {
            len
        } else {
            self.units_per_beat()
        };

        self.begin_gesture();
        let mut ids = Vec::with_capacity(pitches.len());
        for pitch in pitches {
            let id = self.doc.mint_id();
            let mut note = doc::Note::new(id, start, start + len, pitch);
            note.velocity = 100.0 / 127.0;
            self.apply_live(&Edit::AddNote(Box::new(note)));
            ids.push(id);
        }
        self.selection.notes = ids;
        true
    }

    /// Put the document back to how the open gesture found it.
    ///
    /// For destructive drags, which cannot be expressed as a delta.
    /// Moving a razor's contents *carves* — it splits notes at the area
    /// boundaries and clears the ground it lands on — so re-running it
    /// against a document it has already carved does not redo the move,
    /// it cuts a second time in a place the material has since left.
    /// Frame by frame that shreds the take: the notes that moved first
    /// stop moving, notes that were never in the area get dragged along,
    /// and everything ends up in pieces.
    ///
    /// A delta-based drag has no such problem, which is why nothing else
    /// here needs this. The answer for the ones that do is to recompute
    /// from the gesture's own starting point every frame — which costs
    /// nothing to remember, because [`Editor::begin_gesture`] already
    /// snapshotted it for undo.
    ///
    /// Undo is untouched: the snapshot is cloned, not consumed.
    pub fn revert_gesture(&mut self) -> bool {
        let Some(base) = self.history.gesture_base() else {
            return false;
        };
        self.doc = base.clone();
        true
    }

    pub fn undo(&mut self) -> bool {
        let ok = self.history.undo(&mut self.doc);
        self.resync_row_space(ok);
        ok
    }

    pub fn redo(&mut self) -> bool {
        let ok = self.history.redo(&mut self.doc);
        self.resync_row_space(ok);
        ok
    }

    /// Bring the editor's row-space view back in line with the
    /// document after history moves underneath it.
    ///
    /// Some edits change the row space itself — `SetBands` moves the
    /// band splits — and the document is the authority. Without this an
    /// undo restored the old splits in the document while the roll and
    /// the inspector kept drawing the new ones, and every later edit
    /// resorted against a row space nobody could see.
    fn resync_row_space(&mut self, changed: bool) {
        if changed && self.row_space != self.doc.row_space {
            self.row_space = self.doc.row_space.clone();
            self.refresh_fold();
        }
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn content(&self) -> Content {
        content_of(&self.doc)
    }

    /// The Reset View camera for the current content.
    pub fn reset_camera(&self) -> Camera {
        camera::reset_view(
            self.content(),
            self.viewport,
            CUSHION,
            PAD,
            self.camera.fold,
        )
    }

    /// `V` — snap directly to Reset View, no interpolation, no magnets.
    pub fn reset_view(&mut self) {
        self.camera = self.reset_camera();
        // Reset View is the one gesture that re-fits lanes. Everything
        // else leaves them exactly where they are, including edits that
        // push content out of view.
        self.fit_lanes();
    }

    /// Contextual zoom and scroll — MeMagic, applied to this document.
    ///
    /// One entry point for every region, so a host binds a single action
    /// and the region decides what it means. See [`memagic`] for the
    /// design and where it comes from.
    ///
    /// Returns whether anything moved, so a caller can fall through to
    /// another binding when the gesture had nothing to say (a
    /// `ScrollToAnchor` with no row under the pointer, say).
    pub fn memagic(&mut self, region: memagic::Region, anchor: memagic::Anchor) -> bool {
        self.memagic_with(region.modes(), anchor, &memagic::Config::default())
    }

    /// The same, with the mode pair and tuning spelled out.
    pub fn memagic_with(
        &mut self,
        modes: memagic::Modes,
        anchor: memagic::Anchor,
        cfg: &memagic::Config,
    ) -> bool {
        let content = self.content();
        let upb = self.units_per_bar();
        let view = self.camera.time_span(self.viewport);
        let mut moved = false;

        // Horizontal first: the vertical `InView` scope reads the time
        // window, so it has to see the one the gesture is producing
        // rather than the one it is replacing.
        if let Some((t0, len)) =
            memagic::horizontal_span(&self.doc, content, modes.horizontal, anchor, upb, view, cfg)
        {
            self.camera.t0 = t0;
            self.camera.units_per_px = (len / self.viewport.w.max(1.0)).max(1e-9);
            moved = true;
        }

        let view = self.camera.time_span(self.viewport);
        if let Some((lo, hi)) =
            memagic::vertical_range(&self.doc, content, modes.vertical, anchor, view, cfg)
        {
            let rows = (hi - lo + 1.0).max(1.0);
            self.camera.vertical.center = (lo + hi) / 2.0;
            // The row-height ceiling is what stops a two-note passage
            // filling the lane with two enormous rows.
            self.camera.vertical.px_per_row =
                (self.viewport.h / rows).min(cfg.max_px_per_row).max(1e-6);
            moved = true;
        }

        if moved {
            self.settle_camera();
        }
        moved
    }

    pub fn bounds(&self) -> Bounds {
        let c = self.content();
        let span = (c.t_end - c.t_start).max(1.0);
        // The row range comes from the mode's row space, not from a
        // constant: a drum map is twenty lanes and a pitch roll is 128,
        // and fitting the roll to "128" in drums mode would leave a
        // screen of empty rows under the kit.
        let (row_min, row_max) = self.doc.row_space.bounds();
        Bounds {
            t_min: c.t_start - span * CUSHION,
            t_max: c.t_end + span * CUSHION,
            row_min: row_min as f64,
            row_max: row_max as f64,
            ..Bounds::default()
        }
    }

    /// Zoom in around the pointer, with the edge magnet applied in the
    /// same pass — never as a second mutation.
    pub fn zoom_in_at(&mut self, mouse_x: f64, mouse_y: f64, factor: f64) {
        let content = self.content();
        let anchor_t = self.camera.t_at(mouse_x);
        let anchor_pitch = self.camera.pitch_at(mouse_y, self.viewport);

        let mut base = self.camera;
        base.zoom_time_about(anchor_t, factor);
        base.zoom_pitch_about(anchor_pitch, factor, self.viewport);

        let mut influences = Vec::new();
        if let Some(edge) = camera::edge_magnet(
            base,
            anchor_t,
            content,
            self.viewport,
            EDGE_DEAD_ZONE,
            EDGE_WHITESPACE,
        ) {
            influences.push(edge);
        }
        influences.extend(camera::pitch_focus(
            base,
            self.local_pitch(anchor_t),
            anchor_pitch,
            LOCAL_PITCH_WEIGHT,
            MOUSE_PITCH_WEIGHT,
        ));
        if let Some(deep) = camera::deep_zoom_center(
            base,
            content,
            DEEP_ZOOM_ONSET,
            Bounds::default().max_px_per_semitone,
        ) {
            influences.push(deep);
        }

        self.camera = camera::blend(base, &influences);
        self.settle_camera();
    }

    /// Zoom out around the pointer. The reset magnet engages only in
    /// the final stretch — early engagement is what makes a zoom-out
    /// feel like it is being taken away from you.
    pub fn zoom_out_at(&mut self, mouse_x: f64, mouse_y: f64, factor: f64) {
        let anchor_t = self.camera.t_at(mouse_x);
        let anchor_pitch = self.camera.pitch_at(mouse_y, self.viewport);

        let mut base = self.camera;
        base.zoom_time_about(anchor_t, 1.0 / factor.max(1e-6));
        base.zoom_pitch_about(anchor_pitch, 1.0 / factor.max(1e-6), self.viewport);

        let reset = self.reset_camera();
        let mut influences = Vec::new();
        influences.extend(camera::pitch_focus(
            base,
            self.local_pitch(anchor_t),
            anchor_pitch,
            LOCAL_PITCH_WEIGHT,
            MOUSE_PITCH_WEIGHT,
        ));
        if let Some(tail) = camera::reset_tail(base, reset, RESET_TAIL_START) {
            influences.push(tail);
        }

        self.camera = camera::blend(base, &influences);
        self.settle_camera();
    }

    /// Zoom the time axis alone, about the pointer.
    ///
    /// Deliberately without the magnets [`zoom_in_at`](Self::zoom_in_at)
    /// blends in. Those aim the camera on *both* axes — edge, local pitch,
    /// deep-zoom centre — and a gesture the user asked to move one axis
    /// must not quietly move the other. The magnets belong to the
    /// both-axes zoom, where there is no such promise to keep.
    pub fn zoom_time_at(&mut self, mouse_x: f64, factor: f64) {
        let anchor_t = self.camera.t_at(mouse_x);
        self.camera.zoom_time_about(anchor_t, factor);
        self.settle_camera();
    }

    /// Zoom the pitch axis alone, about the pointer. See
    /// [`zoom_time_at`](Self::zoom_time_at) for why the magnets are absent.
    pub fn zoom_pitch_at(&mut self, mouse_y: f64, factor: f64) {
        let anchor_pitch = self.camera.pitch_at(mouse_y, self.viewport);
        self.camera
            .zoom_pitch_about(anchor_pitch, factor, self.viewport);
        self.settle_camera();
    }

    /// Settle the camera after a move, and let the grid follow it.
    ///
    /// Every camera change ends here — there were eight `constrain`
    /// calls and now there is one place they all go, which is what makes
    /// "the grid follows the zoom" true rather than true in the seven
    /// paths somebody remembered.
    ///
    /// The grid part is a no-op unless the user has asked for an
    /// adaptive density, and usually a no-op even then: a division only
    /// moves when the zoom crosses a power of two.
    ///
    /// **Public, and the required ending for any direct camera write.**
    /// It was private, which made the promise above impossible to keep:
    /// the interactive zooms live in `expression-editor-ui`, where they
    /// set `camera.units_per_px` by hand, and the one function that would
    /// have refitted the grid afterwards was not reachable from there. So
    /// the grid followed the wheel and not the zoom *tool* — which, since
    /// `z` became the tool, is most zooming. Anything that assigns to
    /// `camera` ends here.
    pub fn settle_camera(&mut self) {
        self.camera.constrain(self.bounds(), self.viewport);
        let bar_px = self.units_per_bar() / self.camera.units_per_px;
        self.grid.refit(bar_px);
    }

    /// Move the grid's ceiling, and refit to the view at once.
    ///
    /// Through the editor rather than on `Grid` directly, because
    /// refitting needs the camera and the tempo map and `Grid` has
    /// neither. A caller that reached past these would leave the readout
    /// showing a division that is not the one being snapped to.
    pub fn grid_coarser(&mut self) {
        self.grid.coarser();
        self.settle_camera();
    }

    /// Set the division outright, and refit to the view at once.
    ///
    /// The *ceiling*, like every other grid control: an adaptive grid
    /// may still show something coarser, which is what the readout says.
    pub fn set_grid_division(&mut self, division: f64) {
        self.grid.set_division(division);
        self.settle_camera();
    }

    /// Triplet and dotted, which clear each other.
    pub fn set_grid_triplet(&mut self, on: bool) {
        self.grid.set_triplet(on);
        self.settle_camera();
    }

    pub fn set_grid_dotted(&mut self, on: bool) {
        self.grid.set_dotted(on);
        self.settle_camera();
    }

    pub fn grid_finer(&mut self) {
        self.grid.finer();
        self.settle_camera();
    }

    /// How tightly the grid packs its lines, or [`Density::Fixed`] to
    /// stop it following the zoom at all.
    pub fn set_grid_density(&mut self, density: adaptive_grid::Density) {
        self.grid.adaptive.density = density;
        self.settle_camera();
    }

    /// Frame a box of the document: `t0..t1` across, `row_lo..row_hi` down.
    ///
    /// What the zoom tool's Alt-sweep lands on, and the honest primitive
    /// behind "zoom to this". Written as a single assignment of the
    /// camera rather than a sequence of zoom steps, because a sequence
    /// has to decide an order and either order leaves the other axis
    /// anchored on the wrong thing.
    ///
    /// Degenerate boxes are refused rather than clamped: a zero-width
    /// sweep is a click that moved a pixel, and framing it would zoom to
    /// the maximum and lose the user's place for what looked like a
    /// misclick.
    pub fn zoom_to_box(&mut self, t0: f64, t1: f64, row_lo: f64, row_hi: f64) {
        let (t0, t1) = (t0.min(t1), t0.max(t1));
        let (lo, hi) = (row_lo.min(row_hi), row_lo.max(row_hi));
        let span_t = t1 - t0;
        let span_rows = hi - lo;
        if !(span_t.is_finite() && span_t > 0.0) || !(span_rows.is_finite() && span_rows > 0.0) {
            return;
        }
        self.camera.units_per_px = (span_t / self.viewport.w.max(1.0)).max(1e-9);
        self.camera.t0 = t0;
        self.camera.vertical.px_per_row = (self.viewport.h.max(1.0) / span_rows).max(1e-6);
        self.camera.vertical.center = (lo + hi) * 0.5;
        self.settle_camera();
    }

    pub fn pan_px(&mut self, dx: f64, dy: f64) {
        self.camera.pan_px(dx, dy);
        self.settle_camera();
    }

    pub fn resize(&mut self, viewport: Viewport) {
        self.viewport = viewport;
        self.settle_camera();
    }

    /// Weighted pitch center of notes near `t` — what the vertical
    /// magnet aims at.
    pub fn local_pitch(&self, t: f64) -> Option<f64> {
        let window = self.viewport.w * self.camera.units_per_px * 0.25;
        let mut sum = 0.0;
        let mut weight = 0.0;
        for n in &self.doc.notes {
            let distance = if t < n.start {
                n.start - t
            } else if t > n.end {
                t - n.end
            } else {
                0.0
            };
            if distance > window {
                continue;
            }
            let w = (1.0 - distance / window.max(1e-9)) * n.weight.max(0.05);
            sum += n.row as f64 * w;
            weight += w;
        }
        (weight > 1e-9).then(|| sum / weight)
    }

    /// Document units per beat at the current tempo.
    pub fn units_per_beat(&self) -> f64 {
        self.doc.time_base.units_per_beat(self.bpm)
    }

    /// Document units per bar.
    pub fn units_per_bar(&self) -> f64 {
        self.units_per_beat() * self.beats_per_bar.max(1.0)
    }

    /// `(bar, beat)` at `t`, both 1-based — what the ruler prints.
    pub fn bar_beat(&self, t: f64) -> (i64, i64) {
        let beats = (t - self.doc.start) / self.units_per_beat();
        let bpb = self.beats_per_bar.max(1.0);
        let bar = (beats / bpb).floor();
        (bar as i64 + 1, (beats - bar * bpb).floor() as i64 + 1)
    }

    /// Snap a time to the local grid.
    pub fn snap_time(&self, t: f64) -> f64 {
        self.grid.snap(t, self.doc.start, self.units_per_beat())
    }

    /// Notes a discrete command acts on.
    ///
    /// A right-click on an unselected note targets *that* note. Menus
    /// that quietly act on a selection somewhere else on screen are how
    /// the wrong bar gets deleted.
    pub fn command_targets(&self, under: Option<NoteId>) -> Vec<NoteId> {
        match under {
            Some(id) if !self.selection.notes.contains(&id) => vec![id],
            Some(id) => {
                if self.selection.notes.is_empty() {
                    vec![id]
                } else {
                    self.selection.notes.clone()
                }
            }
            None => self.selection.notes.clone(),
        }
    }

    /// Note ids inside the bar containing `t`.
    pub fn notes_in_measure(&self, t: f64) -> Vec<NoteId> {
        let bar = self.units_per_bar();
        let i = ((t - self.doc.start) / bar).floor();
        let (lo, hi) = (self.doc.start + i * bar, self.doc.start + (i + 1.0) * bar);
        self.doc
            .notes
            .iter()
            .filter(|n| n.start < hi && n.end > lo)
            .map(|n| n.id)
            .collect()
    }

    /// Run a context-menu command.
    ///
    /// Commands the core cannot complete on its own — the ones that
    /// need a text field or a submenu — return `false` so the UI knows
    /// to open something rather than assuming the edit happened.
    pub fn run_command(&mut self, cmd: &menu::Command, under: Option<NoteId>) -> bool {
        use menu::Command as C;

        // The razor verbs, which act on the areas rather than on a
        // selection — so they are answered before `command_targets`
        // works out what a note command would apply to.
        //
        // These call the same methods the keyboard does. A menu that
        // reimplemented them would be a second definition of "reverse",
        // and the two would differ the first time either changed.
        match cmd {
            C::RazorReverse => return self.razor_reverse(),
            C::RazorReversePitches => return self.razor_reverse_pitches(),
            C::RazorInvert => return self.razor_invert(),
            C::RazorDeleteContents => return self.razor_delete_contents(),
            C::RazorDuplicate => return self.razor_duplicate(),
            C::RazorSelectContents => return self.razor_select_contents(),
            C::RazorSplit => return self.razor_split(),
            C::RazorClearLane => return self.razor_clear_lane(),
            C::RazorFullLane => return self.razor_full_lane(),
            // The factor is carried as a small integer because `Command`
            // derives `PartialEq` and a menu built twice has to compare
            // equal — two `f64`s that went through different arithmetic
            // do not reliably do that.
            C::RazorScale(n) => {
                let factor = if *n <= 1 { 0.5 } else { *n as f64 };
                return self.razor_scale(factor);
            }
            C::RazorClear => {
                let had = !self.razor.is_empty();
                self.razor.clear();
                return had;
            }
            _ => {}
        }

        let targets = self.command_targets(under);
        match cmd {
            C::Copy => self.clipboard.copy_from(&self.doc, &targets),
            C::Cut => {
                if !self.clipboard.copy_from(&self.doc, &targets) {
                    return false;
                }
                self.apply(&Edit::DeleteNotes(targets))
            }
            C::Paste => {
                // Paste lands at the playhead when there is one, and
                // back where it came from otherwise — never silently at
                // zero, which puts the phrase off-screen.
                let t = self.playhead.unwrap_or(self.doc.start);
                let notes = self.clipboard.placed(t, self.clipboard.origin_row());
                self.apply(&Edit::PasteNotes(notes))
            }
            C::Delete => self.apply(&Edit::DeleteNotes(targets)),
            C::SelectAll => {
                self.selection.notes = self.doc.notes.iter().map(|n| n.id).collect();
                true
            }
            C::SelectMeasure => {
                let t = self
                    .playhead
                    .or_else(|| {
                        targets
                            .first()
                            .and_then(|id| self.doc.note(*id))
                            .map(|n| n.start)
                    })
                    .unwrap_or(self.doc.start);
                self.selection.notes = self.notes_in_measure(t);
                !self.selection.notes.is_empty()
            }
            C::CopyMeasure => {
                let t = self
                    .playhead
                    .or_else(|| {
                        targets
                            .first()
                            .and_then(|id| self.doc.note(*id))
                            .map(|n| n.start)
                    })
                    .unwrap_or(self.doc.start);
                let ids = self.notes_in_measure(t);
                self.clipboard.copy_from(&self.doc, &ids)
            }
            C::ClearExpression => {
                let dimension = self.dimension;
                let spans: Vec<(NoteId, f64, f64)> = targets
                    .iter()
                    .filter_map(|id| self.doc.note(*id).map(|n| (*id, n.start, n.end)))
                    .collect();
                let mut any = false;
                for (id, t0, t1) in spans {
                    any |= self.apply(&Edit::EraseDimension {
                        note: id,
                        dimension,
                        t0,
                        t1,
                    });
                }
                any
            }
            C::ToggleMute => self.apply(&Edit::ToggleMuted { notes: targets }),
            C::AssignChannels => self.apply(&Edit::AssignChannels {
                notes: targets,
                seed: 0,
            }),
            C::CycleString(id) => {
                let Some(n) = self.doc.note(*id) else {
                    return false;
                };
                let RowSpace::Strings(tuning) = self.doc.row_space.clone() else {
                    return false;
                };
                // The string is on the note; the row is the pitch. Using
                // the row here cycled to a "string" that was a MIDI
                // pitch number.
                //
                // Cycling walks the strings that can actually *play* this
                // pitch, and wraps. A plain `+ 1` clamped at the top into
                // a no-op that still reported success, and dead-ended
                // mid-neck the moment the next string could not reach the
                // note (the A string's 2nd fret is fret -3 on the D).
                let count = tuning.strings();
                if count == 0 {
                    return false;
                }
                let current = n.string;
                let pitch = n.row;
                let reachable = |s: usize| {
                    let fret = pitch - tuning.open(s);
                    (0..=tuning.frets as i32).contains(&fret)
                };
                // Start past the current string, or at the lowest for a
                // note that carries none yet, then wrap once.
                let start = current.map(|s| s as usize + 1).unwrap_or(0);
                let Some(next) = (0..count)
                    .map(|i| (start + i) % count)
                    .find(|&s| Some(s as u8) != current && reachable(s))
                else {
                    // No other string reaches this pitch — the note is
                    // playable in one place, so there is nothing to
                    // cycle to and nothing to undo.
                    return false;
                };
                self.apply(&Edit::SetString {
                    note: *id,
                    string: next as i32,
                })
            }
            C::ToggleLegato(id) => {
                let Some(n) = self.doc.note(*id) else {
                    return false;
                };
                let gap = if n.legato { 0.0 } else { 1.0 };
                self.apply(&Edit::Legato {
                    notes: vec![*id],
                    gap,
                })
            }
            C::SplitNote(id, t) => self.apply(&Edit::SplitNote { note: *id, t: *t }),
            C::MergeNotes(id) => self.merge_with_next(*id),
            // `EditLyric` opens the field rather than applying: the
            // text arrives later, through `set_lyric`.
            C::EditLyric(id) => {
                self.editing_lyric = Some(*id);
                true
            }
            // These still need UI: a submenu, a panel.
            C::SetArticulation(_) | C::Properties => false,
            // Answered above, before the targets are worked out. Listed
            // rather than swept up by a `_`, so a *new* note command
            // still fails to compile until it has an arm.
            C::RazorReverse
            | C::RazorReversePitches
            | C::RazorInvert
            | C::RazorDeleteContents
            | C::RazorDuplicate
            | C::RazorSelectContents
            | C::RazorSplit
            | C::RazorClearLane
            | C::RazorFullLane
            | C::RazorScale(_)
            | C::RazorClear => false,
        }
    }

    /// Absorb the next note on the same row into `id`.
    ///
    /// The audio editor's note-assignment merge, and the same operation
    /// a MIDI editor wants for a note split by mistake. The survivor
    /// keeps its own expression and simply extends — re-deriving a
    /// merged curve from two would discard whichever was edited.
    fn merge_with_next(&mut self, id: NoteId) -> bool {
        let Some(n) = self.doc.note(id) else {
            return false;
        };
        let (row, end) = (n.row, n.end);
        let next = self
            .doc
            .notes
            .iter()
            .filter(|o| o.row == row && o.start >= end && o.id != id)
            .min_by(|a, b| a.start.total_cmp(&b.start))
            .map(|o| (o.id, o.end));
        let Some((next_id, next_end)) = next else {
            return false;
        };
        let Some(n) = self.doc.note(id) else {
            return false;
        };
        let start = n.start;
        self.apply(&Edit::Resize {
            note: id,
            start,
            end: next_end,
        }) && self.apply(&Edit::DeleteNotes(vec![next_id]))
    }

    /// Switch mode, re-applying its preset.
    ///
    /// A preset, not a lock: everything it sets can be changed
    /// afterwards. Switching back re-applies.
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        // The track owns the mode; `Editor::mode` is the active one's.
        // Writing both keeps a stack drawn from the workspace agreeing
        // with the surface the user is looking at.
        if let Some(track) = self.tracks.track_mut(self.tracks.active()) {
            track.set_mode(mode);
        }
        // A mode preset supplies a row space, but never overwrites one
        // of the same kind: the document's own may have been tuned —
        // band splits moved, a guitar retuned — and re-applying a preset
        // (which switching tracks does) must not silently undo that.
        if self.doc.row_space.same_kind(&mode.default_row_space()) {
            self.row_space = self.doc.row_space.clone();
        } else {
            self.row_space = mode.default_row_space();
            self.doc.row_space = self.row_space.clone();
        }
        // A new row space means a new fold; drums fold, nothing else
        // does, and switching between them must not leave the old one
        // applied to the new rows.
        self.split_pieces.clear();
        self.refresh_fold();
        self.mouse = mode.default_mouse();
        self.overlays = mode.default_overlays();
        self.strip_lane = mode.default_strip();
        if !mode.has_expression_lanes() {
            // Leaving the active dimension on Pressure in plain MIDI would
            // point every gesture at something the format cannot carry.
            self.dimension = Dimension::Pitch;
        }
        self.reset_view();
    }

    /// Whether CC edit mode is on.
    pub fn cc_editing(&self) -> bool {
        self.cc_edit.is_some()
    }

    /// Enter CC edit mode on a controller, pinning it if it was not
    /// already visible — editing something invisible is a trap.
    pub fn edit_cc(&mut self, number: u8) {
        self.doc.cc.ensure(number);
        if let Some(l) = self.doc.cc.get_mut(number) {
            l.pinned = true;
        }
        self.cc_edit = Some(number);
    }

    pub fn exit_cc_edit(&mut self) {
        self.cc_edit = None;
    }

    /// Sounding pitches of the selected notes — what the chord box
    /// reads.
    ///
    /// Falls back to the notes under the playhead when nothing is
    /// selected, so the box says something useful while you navigate
    /// rather than going blank.
    pub fn chord_pitches(&self) -> Vec<i32> {
        let space = &self.row_space;
        if !self.selection.notes.is_empty() {
            let mut v: Vec<i32> = self
                .selection
                .notes
                .iter()
                .filter_map(|id| self.doc.note(*id))
                .map(|n| space.pitch_of(n))
                .collect();
            v.sort_unstable();
            v.dedup();
            return v;
        }
        let Some(t) = self.playhead else {
            return Vec::new();
        };
        let mut v: Vec<i32> = self
            .doc
            .notes
            .iter()
            .filter(|n| n.start <= t && n.end > t && !n.muted)
            .map(|n| space.pitch_of(n))
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// The chord the box shows, if the current pitches form one.
    pub fn current_chord(&self) -> Option<Chord> {
        chord::identify(&self.chord_pitches())
    }

    /// Notes reduced to what zoom cares about.
    pub fn zoom_spans(&self) -> Vec<zoom::Span> {
        self.doc
            .notes
            .iter()
            .map(|n| zoom::Span {
                start: n.start,
                end: n.end,
                row: n.row,
            })
            .collect()
    }

    /// Contextual zoom: one gesture, and where the pointer is decides
    /// what "zoom" means. See [`zoom`].
    pub fn smart_zoom(&mut self, modes: ZoomModes, anchor_t: f64, anchor_row: f64) {
        let spans = self.zoom_spans();
        let content = self.content();
        let bar = self.units_per_bar();
        self.camera = zoom::apply_horizontal(
            self.camera,
            modes.horizontal,
            &spans,
            anchor_t,
            content,
            self.viewport,
            bar,
            self.smart_zoom,
        );
        // Vertical runs against the *new* horizontal span, so
        // "notes in view" means notes in the view we just produced —
        // not the one we started from.
        let view = self.camera.time_span(self.viewport);
        self.camera = zoom::apply_vertical(
            self.camera,
            modes.vertical,
            &spans,
            anchor_row,
            self.viewport,
            view,
            self.smart_zoom,
        );
        self.settle_camera();
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
