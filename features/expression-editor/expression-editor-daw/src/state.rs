//! Editor state that travels with the project.
//!
//! The rule that decides what belongs here is one question: *would a
//! collaborator opening this project want this, exactly as I left it?*
//! Yes, it lives here. No, but you want it in every project, it is a
//! preference (`utils::prefs`). No to both, it is ephemeral and is never
//! written at all.
//!
//! Scoped to mode corrections for now; the rest of the per-take bucket
//! is #192 and the lane layout is #201, both of which extend this type
//! rather than adding a second one.
//!
//! ## Only corrections are written
//!
//! Inference is never persisted. That keeps the stored table small,
//! makes every row a decision a human made — so it stays readable inside
//! a `.daw`, which #155 requires — and means improving the inference
//! heuristic later automatically benefits every take except the ones
//! somebody explicitly overrode.
//!
//! One deliberate exception: a correction that happens to match what
//! inference would have picked is still written. "The user affirmed
//! this" is worth pinning against a future heuristic change.
//!
//! ## Versioning
//!
//! A `version` at the root, tolerant parsing underneath, and **no
//! migration chain**. An unrecognised version discards the blob and uses
//! defaults — never a refusal to open, never a half-read. Editor state
//! is derivable and cheap to lose: a dropped lane layout costs a
//! re-layout, a dropped mode override costs one click. That asymmetry is
//! what makes defaults the correct failure rather than a lazy one, and
//! it is why a migration framework would be dead weight across every
//! shape change still ahead of this map.
//!
//! The number exists for the one change tolerant parsing cannot survive:
//! a field that keeps its name and changes its meaning.

use daw::service::ExtState;
use daw::service::ProjectContext;
use daw::service::ext_state::side_store;
use expression_editor_core::{CcDisplay, Dimension, Editor, Mode, StripLane, Tuning, tuning};
use facet::Facet;

/// The namespace the expression editor stores under.
pub const NAMESPACE: &str = "expression-editor";

/// Bumped only when a field keeps its name and changes its meaning.
pub const CURRENT_VERSION: u32 = 1;

/// A mode somebody corrected, against the guid it was corrected on.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct ModeOverride {
    /// A track guid or a take guid. Which one it is follows from the
    /// list it appears in.
    pub guid: String,
    pub mode: Mode,
}

/// A tuning, stored by preset name.
///
/// `Temperament` holds `&'static str` and a table of offsets, so it
/// cannot round-trip by value. Storing the name means a preset whose
/// offsets are corrected in a later build takes effect — which is what
/// you want from a preset — and an unknown name falls back to the
/// default rather than to a guessed tuning.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct TuningRef {
    pub temperament: String,
    pub key_pc: i32,
    pub snap_12tet: bool,
}

impl TuningRef {
    pub fn of(t: &Tuning) -> Self {
        Self {
            temperament: t.temperament.name.to_string(),
            key_pc: t.key_pc,
            snap_12tet: t.snap_12tet,
        }
    }

    pub fn resolve(&self) -> Tuning {
        let mut out = Tuning::default();
        if let Some(t) = tuning::by_name(&self.temperament) {
            out.temperament = t.clone();
        }
        out.key_pc = self.key_pc;
        out.snap_12tet = self.snap_12tet;
        out
    }
}

/// How one take was being *looked at*.
///
/// Everything here answers yes to #192's test — a collaborator opening
/// the project wants it exactly as you left it. Notably absent, and
/// deliberately: the camera and the viewport. Where you were scrolled in
/// this song is yours and resets on open.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct TakeView {
    pub guid: String,
    pub dimension: Dimension,
    pub overlays: Vec<Dimension>,
    pub strip_lane: StripLane,
    /// Project state, not a habit: lane strip height travels with the
    /// song. (The lanes-visible *count* is the preference; this is not.)
    pub lane_strip_h: f64,
    pub cc_display: CcDisplay,
    pub tuning: TuningRef,
}

impl TakeView {
    /// Capture the parts of an editor that travel with the project.
    pub fn capture(guid: impl Into<String>, ed: &Editor) -> Self {
        Self {
            guid: guid.into(),
            dimension: ed.dimension,
            overlays: ed.overlays.clone(),
            strip_lane: ed.strip_lane,
            lane_strip_h: ed.lane_strip_h,
            cc_display: ed.cc_display,
            tuning: TuningRef::of(&ed.tuning),
        }
    }

    /// Restore them, leaving everything ephemeral alone.
    ///
    /// Camera, selection, razor, clipboard, playhead and the momentary
    /// toggles are untouched here *on purpose*: restoring a selection
    /// the user did not make is a hazard, not a convenience.
    pub fn apply(&self, ed: &mut Editor) {
        ed.dimension = self.dimension;
        ed.overlays = self.overlays.clone();
        ed.strip_lane = self.strip_lane;
        ed.lane_strip_h = self.lane_strip_h;
        ed.cc_display = self.cc_display;
        ed.tuning = self.tuning.resolve();
    }
}

/// One lane, as stored.
///
/// Track **guids**, never indices: a layout keyed on position is
/// re-targeted by inserting a track above it, and keyed on name it is
/// broken by a rename. A lane has no id of its own — it *is* its
/// membership, and its position in the list is its order.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StoredLane {
    pub tracks: Vec<String>,
    pub weight: f32,
}

/// The lane layout: **project-scoped, one ordered list**.
///
/// Not per take. A lane spans tracks across the whole song, so keying it
/// by take would give twenty tracks conflicting copies of the same
/// layout. Not per track either: lane order and lane weight have nowhere
/// to live under that, and two tracks could disagree about a lane's
/// identity. (This corrects the per-take filing in #157's original
/// resolution — see the correction comment there.)
///
/// Only a **hand-arranged** layout is stored. An inferred one is
/// recomputed by the matcher on load, which keeps the stored data small
/// and means improving the matcher benefits every song nobody arranged.
#[derive(Debug, Clone, Default, PartialEq, Facet)]
pub struct StoredLayout {
    pub lanes: Vec<StoredLane>,
}

impl StoredLayout {
    /// Capture a layout, empty when it was only inferred.
    pub fn capture(ws: &expression_editor_core::Workspace) -> Self {
        if !ws.layout().is_arranged() {
            return Self::default();
        }
        Self {
            lanes: ws
                .layout()
                .lanes()
                .iter()
                .map(|l| StoredLane {
                    tracks: l.tracks.clone(),
                    weight: l.weight,
                })
                .collect(),
        }
    }

    /// Whether anything was arranged by hand.
    pub fn is_arranged(&self) -> bool {
        !self.lanes.is_empty()
    }

    /// Restore into a workspace, dropping entries whose track is gone
    /// and matching in any track the layout never heard of.
    ///
    /// Silent about both: a deleted track is not an error, and one added
    /// track must never invalidate the whole arrangement.
    pub fn apply(&self, ws: &mut expression_editor_core::Workspace) {
        use expression_editor_core::tracks::{Lane, LaneLayout};

        if !self.is_arranged() {
            return;
        }

        let known: Vec<String> = ws.tracks().iter().map(|t| t.guid.clone()).collect();
        let mut layout = LaneLayout::default();
        for stored in &self.lanes {
            let live: Vec<String> = stored
                .tracks
                .iter()
                .filter(|g| known.contains(g))
                .cloned()
                .collect();
            if live.is_empty() {
                continue;
            }
            let mut lane = Lane::single(live[0].clone(), stored.weight);
            lane.tracks = live;
            lane.weight = stored.weight;
            layout.push(lane);
        }
        layout.mark_arranged();
        *ws.layout_mut() = layout;

        // Anything the stored layout did not mention arrived since it
        // was written; the matcher places it rather than appending
        // blindly, so a new comp lands in its track's lane.
        for guid in known {
            if ws.layout().lane_of(&guid).is_none() {
                ws.place_new_track(&guid);
            }
        }
    }
}

/// A generated envelope, as stored.
///
/// Sparse breakpoints with shapes, in **item-relative time** — seconds
/// from the item's start, not from the project's. That is possible
/// because this data is ours on both backends, and it is what makes
/// moving an item leave every curve aligned to its audio. It is also why
/// the four are not written as FX-parameter envelopes: those are *track*
/// envelopes in project time, so dragging the item would leave its gate
/// curve behind, and keeping them aligned would make item-move a write
/// path for envelope data — which nothing else in this design needs.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StoredEnvelope {
    pub points: Vec<StoredEnvPoint>,
}

#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StoredEnvPoint {
    /// Seconds from the start of the item.
    pub t_s: f64,
    pub db: f64,
    pub hold: bool,
}

#[derive(Debug, Clone, PartialEq, Facet)]
pub struct StoredSpan {
    pub from_s: f64,
    pub to_s: f64,
    pub db: f64,
}

/// The four envelopes and the settings they came from, for one item.
///
/// The **composite is not here**: it is the take volume envelope, which
/// the DAW owns and plays. These four are ours, and they are what you
/// edit.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct ItemEnvelopes {
    pub item_guid: String,
    pub gate: StoredEnvelope,
    pub breath: StoredEnvelope,
    pub sibilance: Vec<StoredSpan>,
    pub ride: StoredEnvelope,
    /// Which are switched off. Bypass is a mix decision, so it travels.
    pub bypass_gate: bool,
    pub bypass_breath: bool,
    pub bypass_sibilance: bool,
    pub bypass_ride: bool,
    /// The config digest these were generated from, so a reopened item
    /// knows whether they are still current without re-analysing.
    pub digest: u64,
}

impl ItemEnvelopes {
    pub fn capture(
        item_guid: impl Into<String>,
        curves: &level_dsp::envelope::Contributions,
        digest: u64,
    ) -> Self {
        let pts = |v: &[level_dsp::envelope::EnvPoint]| StoredEnvelope {
            points: v
                .iter()
                .map(|p| StoredEnvPoint {
                    t_s: p.t_s,
                    db: p.db,
                    hold: p.hold,
                })
                .collect(),
        };
        Self {
            item_guid: item_guid.into(),
            gate: pts(&curves.gate),
            breath: pts(&curves.breath),
            ride: pts(&curves.ride),
            sibilance: curves
                .sibilance
                .iter()
                .map(|s| StoredSpan {
                    from_s: s.from_s,
                    to_s: s.to_s,
                    db: s.db,
                })
                .collect(),
            bypass_gate: curves.bypass.gate,
            bypass_breath: curves.bypass.breath,
            bypass_sibilance: curves.bypass.sibilance,
            bypass_ride: curves.bypass.ride,
            digest,
        }
    }

    pub fn restore(&self) -> level_dsp::envelope::Contributions {
        use level_dsp::envelope::{Bypass, Contributions, EnvPoint, GainSpan};
        let pts = |e: &StoredEnvelope| {
            e.points
                .iter()
                .map(|p| EnvPoint {
                    t_s: p.t_s,
                    db: p.db,
                    hold: p.hold,
                })
                .collect::<Vec<_>>()
        };
        Contributions {
            gate: pts(&self.gate),
            breath: pts(&self.breath),
            ride: pts(&self.ride),
            sibilance: self
                .sibilance
                .iter()
                .map(|s| GainSpan {
                    from_s: s.from_s,
                    to_s: s.to_s,
                    db: s.db,
                })
                .collect(),
            bypass: Bypass {
                gate: self.bypass_gate,
                breath: self.bypass_breath,
                sibilance: self.bypass_sibilance,
                ride: self.bypass_ride,
            },
        }
    }
}

/// Everything about a project that the editor persists.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct EditorState {
    pub version: u32,
    /// Track-level corrections: the default for takes on that track,
    /// including takes recorded later.
    pub track_modes: Vec<ModeOverride>,
    /// Take-level corrections, which beat the track's.
    pub take_modes: Vec<ModeOverride>,
    /// How each take was being looked at.
    pub take_views: Vec<TakeView>,
    /// Generated envelopes, per item.
    pub envelopes: Vec<ItemEnvelopes>,
    /// The hand-arranged lane layout. Empty means nobody arranged one,
    /// so the matcher decides on load.
    ///
    /// A plain field rather than an `Option`: `facet-styx` round-trips
    /// an optional struct out but not back in, and "no lanes" says the
    /// same thing without a shape the parser cannot read.
    pub layout: StoredLayout,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            version: CURRENT_VERSION,
            track_modes: Vec::new(),
            take_modes: Vec::new(),
            take_views: Vec::new(),
            envelopes: Vec::new(),
            layout: StoredLayout::default(),
        }
    }
}

impl EditorState {
    /// Resolve a mode: **take → track → `None`**.
    ///
    /// `None` means nobody has corrected this, so the caller infers. The
    /// order is what makes a track-level correction apply to takes
    /// recorded later while still letting one odd comp disagree.
    pub fn mode_for(&self, track_guid: &str, take_guid: &str) -> Option<Mode> {
        self.take_modes
            .iter()
            .find(|o| o.guid == take_guid)
            .or_else(|| self.track_modes.iter().find(|o| o.guid == track_guid))
            .map(|o| o.mode)
    }

    /// Record a correction on a take.
    pub fn correct_take(&mut self, take_guid: impl Into<String>, mode: Mode) {
        upsert(&mut self.take_modes, take_guid.into(), mode);
    }

    /// Record a correction on a track, which becomes the default for its
    /// takes.
    pub fn correct_track(&mut self, track_guid: impl Into<String>, mode: Mode) {
        upsert(&mut self.track_modes, track_guid.into(), mode);
    }

    /// Forget a take-level correction, falling back to the track's.
    pub fn clear_take(&mut self, take_guid: &str) {
        self.take_modes.retain(|o| o.guid != take_guid);
    }

    /// The stored view for a take, if it has one.
    pub fn view_for(&self, take_guid: &str) -> Option<&TakeView> {
        self.take_views.iter().find(|v| v.guid == take_guid)
    }

    /// Record how a take is being looked at.
    pub fn set_view(&mut self, view: TakeView) {
        match self.take_views.iter_mut().find(|v| v.guid == view.guid) {
            Some(existing) => *existing = view,
            None => self.take_views.push(view),
        }
    }

    /// The stored envelopes for an item.
    pub fn envelopes_for(&self, item_guid: &str) -> Option<&ItemEnvelopes> {
        self.envelopes.iter().find(|e| e.item_guid == item_guid)
    }

    /// Record an item's envelopes.
    pub fn set_envelopes(&mut self, env: ItemEnvelopes) {
        match self
            .envelopes
            .iter_mut()
            .find(|e| e.item_guid == env.item_guid)
        {
            Some(existing) => *existing = env,
            None => self.envelopes.push(env),
        }
    }

    /// Whether anything here is worth writing.
    pub fn is_empty(&self) -> bool {
        self.track_modes.is_empty()
            && self.take_modes.is_empty()
            && self.take_views.is_empty()
            && self.envelopes.is_empty()
            && self.layout.lanes.is_empty()
    }
}

fn upsert(list: &mut Vec<ModeOverride>, guid: String, mode: Mode) {
    match list.iter_mut().find(|o| o.guid == guid) {
        Some(existing) => existing.mode = mode,
        None => list.push(ModeOverride { guid, mode }),
    }
}

/// Read the editor's state for a project.
///
/// Never fails in a way the caller has to handle: unset, unparseable, or
/// written by a version this build does not understand all yield
/// defaults. Losing editor state costs a re-layout and a click; refusing
/// to open a project costs the session.
pub fn load<D: ExtState + ?Sized>(daw: &D, project: ProjectContext) -> EditorState {
    let Some(raw) = side_store::load(daw, project, NAMESPACE) else {
        return EditorState::default();
    };
    match facet_styx::from_str::<EditorState>(&raw) {
        Ok(state) if state.version <= CURRENT_VERSION => state,
        // A newer version is discarded whole rather than half-read: a
        // field whose meaning changed would otherwise be misread as if
        // it had not.
        _ => EditorState::default(),
    }
}

/// Write the editor's state for a project.
///
/// Marks the project dirty, which is the point: a mode correction that
/// is a project's only change must still prompt to save, or it is
/// silently lost on close — the complaint #157 opened with.
pub fn save<D: ExtState + ?Sized>(
    daw: &D,
    project: ProjectContext,
    state: &EditorState,
) -> daw::service::DawResult<()> {
    let text = facet_styx::to_string(state).unwrap_or_default();
    side_store::store(daw, project, NAMESPACE, &text)
}

/// Resolve the mode for a take: **take → track → infer**.
///
/// `infer` is only consulted when nobody has corrected anything, and it
/// is a closure rather than a fixed function so the heuristic can change
/// — and improve — without this layer knowing. That is the whole point
/// of not persisting inference: a better heuristic reaches every take
/// except the ones somebody explicitly overrode.
pub fn resolve_mode<D: ExtState + ?Sized>(
    daw: &D,
    project: ProjectContext,
    track_guid: &str,
    take_guid: &str,
    infer: impl FnOnce() -> Mode,
) -> Mode {
    load(daw, project)
        .mode_for(track_guid, take_guid)
        .unwrap_or_else(infer)
}

/// Record a correction and write it, in one step.
///
/// The write marks the project dirty, which is what makes the
/// correction survive: a correction that is a project's only change
/// must still prompt to save.
pub fn correct_take_mode<D: ExtState + ?Sized>(
    daw: &D,
    project: ProjectContext,
    take_guid: &str,
    mode: Mode,
) -> daw::service::DawResult<()> {
    let mut state = load(daw, project.clone());
    state.correct_take(take_guid, mode);
    save(daw, project, &state)
}

/// Record a track-level correction, the default for its takes —
/// including takes recorded later.
pub fn correct_track_mode<D: ExtState + ?Sized>(
    daw: &D,
    project: ProjectContext,
    track_guid: &str,
    mode: Mode,
) -> daw::service::DawResult<()> {
    let mut state = load(daw, project.clone());
    state.correct_track(track_guid, mode);
    save(daw, project, &state)
}
