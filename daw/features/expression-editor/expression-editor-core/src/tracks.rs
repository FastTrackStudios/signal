//! Multitrack: several documents behind one editing surface.
//!
//! Vovious calls this the TrackSwitcher, and its two load-bearing rules
//! are why this is a layer rather than "just open another editor":
//!
//! - **One undo history per track.** Switching tracks must never let an
//!   undo reach across into another track's edits. That is a property of
//!   where the history *lives*, so it cannot be bolted on afterwards —
//!   hence this landing before the audio surface rather than after.
//! - **Reference tracks.** Any subset of the inactive tracks can be
//!   drawn behind the active one, read-only, so a harmony part can be
//!   tuned against the lead without leaving the window.
//!
//! The camera, tools, grid, tuning and mouse map are deliberately *not*
//! per-track: switching tracks changes what you are editing, not where
//! you are looking. Only the document and its history swap.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::Mode;
use crate::doc::ExpressionDoc;
use crate::edit::History;

/// Undo depth per track.
pub const HISTORY_LIMIT: usize = 10;

/// How a reference track is tinted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RefColor {
    /// The surface's own reference colour.
    #[default]
    Default,
    /// The colour the host gave the track.
    Host,
    /// Outline only — least visual noise, for when several references
    /// are shown at once.
    Shadow,
}

/// One track.
///
/// A track holds no camera: the view is shared across the whole
/// workspace, so switching does not move you.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    /// Stable identity, and the only thing durable data may reference.
    ///
    /// The host supplies it — the DAW adapter passes REAPER's track
    /// GUID, tests pass something readable — and it never changes for
    /// the life of the track. A bare `String` rather than a newtype:
    /// considered and declined, the accepted trade being that a track
    /// guid and a take guid are both bare strings.
    ///
    /// Indices into [`Workspace::tracks`] remain correct for in-memory
    /// addressing and are used throughout. The rule this exists to make
    /// possible is narrower and absolute: **nothing durable may hold an
    /// index**. A stored lane layout keyed on position is re-targeted by
    /// inserting a track above it; keyed on name, it is broken by
    /// [`Workspace::rename`].
    pub guid: String,
    pub name: String,
    /// The host's folder for this track, when it has one.
    ///
    /// Only the matcher reads it, and only to *stop* itself: pairing is
    /// scoped to a folder, so a "Lead Vox" in the choir bus is never
    /// paired with a "Lead Vox" in the band bus. Tracks with no folder
    /// share one root scope.
    pub folder: Option<String>,
    /// Stale while this track is the active one — the live document
    /// lives in `Editor::doc` and is written back on switch. Reads go
    /// through [`Workspace::doc_of`], which refuses the active slot for
    /// exactly this reason.
    doc: ExpressionDoc,
    history: History,
    /// How this track is edited and drawn.
    ///
    /// On the *track*, not on the editor, because a workspace holding a
    /// vocal, its reference MIDI, a guitar and a kit needs all four
    /// surfaces at once — which is the whole point of showing them
    /// stacked. The editor's mode is a view of the active track's.
    ///
    /// Inferred when the track is loaded and the user's from then on: no
    /// threshold gets a whispered vocal and a melodic tom fill both
    /// right, so a wrong guess has to be one click to correct.
    pub mode: Mode,
    /// Height weight in a stacked view, relative to the other tracks.
    ///
    /// Defaults to what the mode needs — a slice strip wants three
    /// bands, a vocal two octaves, a string roll six rows — because
    /// splitting the height evenly gives the guitar the same room as
    /// the kit and neither ends up readable.
    pub weight: f32,
    /// Hidden tracks stay in the workspace and keep their history; they
    /// are simply not drawn. Distinct from removing.
    pub hidden: bool,
    /// Colour the host assigned, for [`RefColor::Host`].
    pub color: Option<String>,
    /// Drawn behind the active track, read-only.
    pub reference: bool,
    pub ref_color: RefColor,
}

/// Source of generated guids for tracks nobody named.
///
/// Guids are project-scoped, so a counter is sufficient: two projects
/// may both contain `t1` and never meet. Generation happens once, at
/// construction — the value is then persisted with the track, so it is
/// stable across save and reload, which is the only stability that
/// matters here.
static NEXT_GUID: AtomicU64 = AtomicU64::new(1);

fn generated_guid() -> String {
    let n = NEXT_GUID.fetch_add(1, Ordering::Relaxed);
    let mut s = String::from("t");
    s.push_str(&n.to_string());
    s
}

impl Track {
    /// A track with a generated guid, for callers with no host identity
    /// to supply — the standalone editor, tests, demo scenes.
    pub fn new(name: impl Into<String>, doc: ExpressionDoc) -> Self {
        Self::with_guid(generated_guid(), name, doc)
    }

    /// A track carrying the host's identity. This is the constructor the
    /// DAW adapter uses, so that persisted data keyed on a guid means
    /// the same track when the project is reopened.
    pub fn with_guid(guid: impl Into<String>, name: impl Into<String>, doc: ExpressionDoc) -> Self {
        Self {
            guid: guid.into(),
            name: name.into(),
            folder: None,
            doc,
            history: History::new(HISTORY_LIMIT),
            mode: Mode::default(),
            weight: Mode::default().stack_weight(),
            hidden: false,
            color: None,
            reference: false,
            ref_color: RefColor::default(),
        }
    }

    /// The same, in a known mode — and taking the mode's natural height
    /// with it, which is the pairing a caller almost always wants.
    pub fn in_mode(name: impl Into<String>, doc: ExpressionDoc, mode: Mode) -> Self {
        let mut t = Self::new(name, doc);
        t.set_mode(mode);
        t
    }

    /// Change mode, and the height with it.
    ///
    /// Only while the weight is still the mode's default: once a user
    /// has sized a track by hand, switching modes must not silently undo
    /// that.
    pub fn set_mode(&mut self, mode: Mode) {
        if (self.weight - self.mode.stack_weight()).abs() < f32::EPSILON {
            self.weight = mode.stack_weight();
        }
        self.mode = mode;
    }
}

/// A lane: the tracks drawn on one shared vertical strip.
///
/// Lanes are a **view** concern, not a container for ownership —
/// everything still layers into one window, and a lane exists only to
/// split the visualisation. That is why a track's document, history and
/// mode stay on the track and none of them appear here.
///
/// A lane holds as many tracks as you want. The motivating case is a
/// vocal and its MIDI guide superimposed, because tuning one against the
/// other means seeing them on the same axis — and, inversely, two
/// instruments doubling a line need *separate* lanes, because layered on
/// top of each other they are invisible.
///
/// **A lane has no identity of its own.** It *is* its set of track
/// guids, and its position in [`LaneLayout`] is its order. Nothing
/// dereferences a lane durably — the per-lane camera is ephemeral,
/// weight lives here, and merge/split rewrite the list wholesale — so an
/// id would be a field that never gets read and quietly drifts.
#[derive(Clone, Debug, PartialEq)]
pub struct Lane {
    /// Track guids, in draw order. Never indices: a lane outlives the
    /// insertion of a track above it.
    pub tracks: Vec<String>,
    /// Height weight relative to the other lanes.
    pub weight: f32,
    /// What `weight` would be if nobody had dragged it.
    ///
    /// Kept so a member's mode change can update the height *only while
    /// it is still the default* — the same bargain [`Track::set_mode`]
    /// strikes, and for the same reason: once you have sized a lane by
    /// hand, switching modes must not silently undo it.
    default_weight: f32,
    /// The role this lane plays in a folded group (`Kick`, `Snare`, …),
    /// when it was made by a group fold rather than by hand or by the
    /// name matcher. A role is a label and a draw rule, not an identity:
    /// merge/split clear it, and nothing durable references it.
    pub role: Option<crate::kit::LaneRole>,
    /// Draw each member in its own sub-row of the lane rather than
    /// summed/overlaid — the `Toms` lane, one row per tom.
    pub split: bool,
    /// Draw only the active member's waveform instead of the members'
    /// sum — "let me see just this mic". A view flag, not an audio
    /// solo: detection and every edit still treat the lane whole.
    pub solo_mic: bool,
}

impl Lane {
    /// A lane holding one track, at that track's natural height.
    pub fn single(guid: impl Into<String>, weight: f32) -> Self {
        Self {
            tracks: vec![guid.into()],
            weight,
            default_weight: weight,
            role: None,
            split: false,
            solo_mic: false,
        }
    }

    /// A role lane over `tracks` (draw order), at `weight`.
    pub fn role(role: crate::kit::LaneRole, tracks: Vec<String>, weight: f32) -> Self {
        Self {
            tracks,
            weight,
            default_weight: weight,
            role: Some(role),
            split: role.splits_members(),
            solo_mic: false,
        }
    }

    pub fn contains(&self, guid: &str) -> bool {
        self.tracks.iter().any(|g| g == guid)
    }

    /// Whether the height has been set by hand.
    pub fn is_hand_sized(&self) -> bool {
        (self.weight - self.default_weight).abs() > f32::EPSILON
    }

    /// Take a new natural height, unless the user has overridden it.
    pub fn set_default_weight(&mut self, weight: f32) {
        if !self.is_hand_sized() {
            self.weight = weight;
        }
        self.default_weight = weight;
    }
}

/// The lanes of a workspace, in top-to-bottom order.
///
/// Persisted as one unit and **project-scoped** — a lane spans tracks
/// across the whole song, so keying it per take would give every track a
/// conflicting copy of the same layout, and keying it per track leaves
/// lane order and lane weight with nowhere to live.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LaneLayout {
    lanes: Vec<Lane>,
    /// Whether a human arranged this.
    ///
    /// Only an arranged layout is written to the project. An inferred
    /// one is recomputed on load, which keeps the stored data small,
    /// makes every stored entry a decision someone made, and means
    /// improving the matcher later benefits every song nobody touched.
    /// Same bargain #191 strikes for mode inference.
    arranged: bool,
}

impl LaneLayout {
    pub fn len(&self) -> usize {
        self.lanes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lanes.is_empty()
    }

    pub fn lanes(&self) -> &[Lane] {
        &self.lanes
    }

    pub fn lane(&self, i: usize) -> Option<&Lane> {
        self.lanes.get(i)
    }

    pub fn lane_mut(&mut self, i: usize) -> Option<&mut Lane> {
        self.lanes.get_mut(i)
    }

    pub fn push(&mut self, lane: Lane) -> usize {
        self.lanes.push(lane);
        self.lanes.len() - 1
    }

    /// Which lane holds this track.
    pub fn lane_of(&self, guid: &str) -> Option<usize> {
        self.lanes.iter().position(|l| l.contains(guid))
    }

    /// Whether a human arranged this layout, and it must be persisted.
    pub fn is_arranged(&self) -> bool {
        self.arranged
    }

    /// Mark the layout as hand-arranged. Called by the gestures that
    /// rearrange it — merge, split, a dragged height — never by the
    /// matcher.
    pub fn mark_arranged(&mut self) {
        self.arranged = true;
    }

    /// Drop a track from whichever lane holds it, removing the lane if
    /// that empties it. A lane with no tracks is not a lane.
    pub fn forget(&mut self, guid: &str) {
        if let Some(i) = self.lane_of(guid) {
            self.lanes[i].tracks.retain(|g| g != guid);
            if self.lanes[i].tracks.is_empty() {
                self.lanes.remove(i);
            }
        }
    }
}

/// Reduce a track name to what the matcher compares.
///
/// Lower-cased, punctuation and whitespace collapsed, and a trailing
/// role token removed — so "Lead Vox", "lead_vox MIDI" and "Lead Vox
/// (Ref)" all reduce to `leadvox` and land in one lane.
///
/// Deliberately conservative. A looser matcher pairs two tracks that
/// merely sound alike, and a wrong pairing hides one track underneath
/// another — the exact failure lanes exist to prevent. Leaving a track
/// unmatched costs one merge gesture; mispairing it costs you the
/// knowledge that you needed to.
pub fn normalize_track_name(name: &str) -> String {
    // Only words that name a track's *role*, never its instrument.
    // "vox" and "gtr" were in this list once and quietly turned
    // "Lead Vox" into "lead", which pairs a vocal with anything else
    // called Lead.
    const ROLE_TOKENS: [&str; 4] = ["midi", "audio", "ref", "reference"];

    let lower = name.to_lowercase();
    let mut words: Vec<String> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect();

    // Only a *trailing* role token is dropped, and only one. "Vox MIDI"
    // is a MIDI guide for a vocal; "MIDI Vox" is a track called MIDI
    // Vox, and guessing otherwise would pair things that are not pairs.
    if words.len() > 1
        && let Some(last) = words.last()
        && ROLE_TOKENS.contains(&last.as_str())
    {
        words.pop();
    }

    words.concat()
}

/// One lane's slot in a stacked layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StackRow {
    /// Index into [`Workspace::layout`].
    pub lane: usize,
    /// Top edge, in the same units as the height passed to
    /// [`Workspace::stack`].
    pub y: f32,
    pub height: f32,
}

/// The set of tracks behind one editor.
///
/// No `Default`: a workspace cannot exist without a document, and a
/// document cannot exist without a time base. Inventing one would mean
/// guessing whether the surface is in ticks or analysis frames.
#[derive(Clone, Debug, PartialEq)]
pub struct Workspace {
    tracks: Vec<Track>,
    active: usize,
    /// How the tracks are grouped into vertical strips.
    ///
    /// Kept beside the tracks rather than derived from them, because it
    /// is the one part of this that a user arranges by hand and expects
    /// back — everything else here can be recomputed.
    layout: LaneLayout,
}

impl Workspace {
    /// A workspace around the document the editor was opened on.
    ///
    /// The slot's copy of `doc` is stale from this moment on — the live
    /// one lives in `Editor::doc`. See [`Workspace::doc_of`].
    pub fn single(name: impl Into<String>, doc: ExpressionDoc) -> Self {
        let track = Track::new(name, doc);
        let mut layout = LaneLayout::default();
        layout.push(Lane::single(track.guid.clone(), track.weight));
        Self {
            tracks: vec![track],
            active: 0,
            layout,
        }
    }

    pub fn layout(&self) -> &LaneLayout {
        &self.layout
    }

    pub fn layout_mut(&mut self) -> &mut LaneLayout {
        &mut self.layout
    }

    /// The track indices a lane draws, in draw order.
    ///
    /// Resolved from guids on every call. Lanes never cache indices —
    /// that is the whole point of holding guids.
    pub fn lane_tracks(&self, lane: usize) -> Vec<usize> {
        self.layout
            .lane(lane)
            .map(|l| {
                l.tracks
                    .iter()
                    .filter_map(|g| self.index_of_guid(g))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The lane holding the active track.
    pub fn active_lane(&self) -> Option<usize> {
        let guid = &self.tracks.get(self.active)?.guid;
        self.layout.lane_of(guid)
    }

    /// Whether a lane has anything to draw.
    ///
    /// A lane whose every track is hidden is not drawn at all — which is
    /// what gives both gestures, hiding a track *inside* a lane and
    /// hiding a whole lane, without a second flag to keep consistent.
    pub fn lane_is_visible(&self, lane: usize) -> bool {
        self.lane_tracks(lane)
            .iter()
            .any(|&i| !self.tracks[i].hidden)
    }

    /// The natural height for a lane: the **max** of its members' modes.
    ///
    /// Max rather than sum or mean, because the tallest member's need is
    /// what makes the lane readable — a kit layered with its MIDI still
    /// needs the kit's rows.
    pub fn natural_weight(&self, lane: usize) -> f32 {
        self.lane_tracks(lane)
            .iter()
            .map(|&i| self.tracks[i].mode.stack_weight())
            .fold(0.0_f32, f32::max)
            .max(0.01)
    }

    /// Group the tracks into lanes by name, discarding any inferred
    /// layout and leaving a hand-arranged one alone.
    ///
    /// One lane per source, except that a track and its own reference
    /// pair into one — tuning a vocal against its MIDI guide means
    /// seeing them superimposed. Matching is exact on
    /// [`normalize_track_name`] and scoped to the folder.
    ///
    /// Runs at load and after a track appears, never on a layout the
    /// user arranged. Lane order follows first appearance, so the result
    /// is stable rather than depending on hash iteration.
    pub fn auto_pair(&mut self) {
        if self.layout.is_arranged() {
            return;
        }

        let mut lanes: Vec<Lane> = Vec::new();
        // Parallel to `lanes`: the (folder, normalized-name) each was
        // opened for. A Vec rather than a map to keep the order stable
        // and the dependency count at zero.
        let mut keys: Vec<(Option<String>, String)> = Vec::new();

        for track in &self.tracks {
            let key = (track.folder.clone(), normalize_track_name(&track.name));
            match keys.iter().position(|k| *k == key) {
                Some(i) => lanes[i].tracks.push(track.guid.clone()),
                None => {
                    keys.push(key);
                    lanes.push(Lane::single(track.guid.clone(), track.mode.stack_weight()));
                }
            }
        }

        self.layout = LaneLayout {
            lanes,
            arranged: false,
        };
        self.refresh_lane_weights();
    }

    /// Place a track that has appeared since the layout was made.
    ///
    /// Not "append a new lane": a take recorded on an existing track
    /// should land in that track's lane, which is what makes the
    /// track-level cascade worth having. Falls back to a lane of its own
    /// when nothing matches — the safe failure.
    pub fn place_new_track(&mut self, guid: &str) {
        let Some(track) = self.track_by_guid(guid) else {
            return;
        };
        let key = (track.folder.clone(), normalize_track_name(&track.name));
        let weight = track.mode.stack_weight();

        // `push` has already given it a lane of its own, so the question
        // is whether a *different* lane wants it. Identify that lane by
        // one of its members rather than by index: removing this track
        // from the lane it arrived in can shift the indices.
        let anchor: Option<String> = self
            .layout
            .lanes()
            .iter()
            .flat_map(|lane| lane.tracks.iter())
            .find(|g| {
                *g != guid
                    && self
                        .track_by_guid(g)
                        .is_some_and(|t| (t.folder.clone(), normalize_track_name(&t.name)) == key)
            })
            .cloned();

        match anchor {
            Some(anchor) => {
                self.layout.forget(guid);
                if let Some(i) = self.layout.lane_of(&anchor)
                    && let Some(lane) = self.layout.lane_mut(i)
                {
                    lane.tracks.push(guid.to_string());
                }
            }
            None if self.layout.lane_of(guid).is_none() => {
                self.layout.push(Lane::single(guid, weight));
            }
            None => {}
        }
        self.refresh_lane_weights();
    }

    /// The next visible track in a lane, wrapping.
    ///
    /// Returns `None` when the lane has nothing else to offer, so a
    /// caller can leave the active track alone rather than pointlessly
    /// re-selecting it.
    pub fn next_in_lane(&self, lane: usize, from: usize) -> Option<usize> {
        let members: Vec<usize> = self
            .lane_tracks(lane)
            .into_iter()
            .filter(|&i| !self.tracks[i].hidden)
            .collect();
        if members.len() < 2 {
            return None;
        }
        let at = members.iter().position(|&i| i == from)?;
        Some(members[(at + 1) % members.len()])
    }

    /// Merge two lanes into one.
    ///
    /// The result takes the **upper** lane's position and the greater of
    /// the two weights. Upper rather than active, because positional is
    /// predictable: taking the active lane's position would make the
    /// same two lanes merge differently depending on which you happened
    /// to click last.
    ///
    /// Marks the layout arranged — a merge is a decision, and decisions
    /// are what get persisted.
    pub fn merge_lanes(&mut self, a: usize, b: usize) -> bool {
        if a == b || a >= self.layout.len() || b >= self.layout.len() {
            return false;
        }
        let (upper, lower) = if a < b { (a, b) } else { (b, a) };
        let Some(moved) = self.layout.lane(lower).map(|l| l.tracks.clone()) else {
            return false;
        };
        let weight = self
            .layout
            .lane(upper)
            .map(|l| l.weight)
            .unwrap_or(1.0)
            .max(self.layout.lane(lower).map(|l| l.weight).unwrap_or(1.0));

        if let Some(lane) = self.layout.lane_mut(upper) {
            lane.tracks.extend(moved);
            lane.weight = weight;
            // A hand-merged lane is no longer the role it was folded as.
            lane.role = None;
            lane.split = false;
        }
        self.layout.lanes.remove(lower);
        self.layout.mark_arranged();
        true
    }

    /// Peel one track out of its lane into a new lane directly below.
    ///
    /// Peel-one rather than explode-into-N, because it is the reversible
    /// gesture: splitting a three-track lane into three is one action
    /// that then needs two merges to undo, and layout undo is a
    /// different mechanism from document undo.
    ///
    /// Splitting the only track in a lane is a no-op — there is nothing
    /// to separate it from.
    pub fn split_track_out(&mut self, guid: &str) -> Option<usize> {
        let from = self.layout.lane_of(guid)?;
        if self.layout.lane(from)?.tracks.len() < 2 {
            return None;
        }
        let weight = self
            .track_by_guid(guid)
            .map(|t| t.mode.stack_weight())
            .unwrap_or(1.0);

        if let Some(lane) = self.layout.lane_mut(from) {
            lane.tracks.retain(|g| g != guid);
        }
        let at = from + 1;
        self.layout.lanes.insert(at, Lane::single(guid, weight));
        self.layout.mark_arranged();
        self.refresh_lane_weights();
        Some(at)
    }

    /// Drop layout entries whose track no longer exists.
    ///
    /// Silent by design: a track someone deleted is not an error worth
    /// reporting, and a mismatch must never invalidate the whole layout.
    pub fn prune_layout(&mut self) {
        let known: Vec<String> = self.tracks.iter().map(|t| t.guid.clone()).collect();
        for i in (0..self.layout.len()).rev() {
            if let Some(lane) = self.layout.lane_mut(i) {
                lane.tracks.retain(|g| known.contains(g));
            }
            if self.layout.lane(i).is_some_and(|l| l.tracks.is_empty()) {
                self.layout.lanes.remove(i);
            }
        }
    }

    /// The row range one document wants, padded.
    ///
    /// Lives here rather than in the renderer because it is a question
    /// about the material, not about pixels — and because a lane's fit
    /// has to span every member, which the renderer should not be
    /// deciding.
    pub fn doc_row_range(doc: &ExpressionDoc, mode: Mode, fit_pad: f64) -> (f64, f64) {
        let (bound_lo, bound_hi) = doc.row_space.bounds();
        // A space with few rows shows all of them: three bands or six
        // strings *are* the axis, and fitting to content would move a
        // kit's rows around as hits come and go.
        if !matches!(doc.row_space, crate::rows::RowSpace::Pitch) {
            return (bound_lo as f64 - 0.5, bound_hi as f64 + 0.5);
        }
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for n in &doc.notes {
            lo = lo.min(n.row as f64);
            hi = hi.max(n.row as f64);
        }
        if !lo.is_finite() || !hi.is_finite() {
            // An octave around middle C is a better blank lane than all
            // 128 rows, which would draw anything loaded later as a
            // smear at the bottom.
            return (54.0, 66.0);
        }
        // A blob needs room above and below for its excursions; a bar
        // does not.
        let pad = if mode.draws_blobs() {
            fit_pad + 1.0
        } else {
            fit_pad
        };
        (lo - pad, hi + pad + 1.0)
    }

    /// The row range a whole lane wants: the union of its members'.
    ///
    /// Union, because fitting to the vocal alone would push its MIDI
    /// guide off the top of the lane — and being able to compare them is
    /// the reason they share a lane.
    ///
    /// `active_doc` is the live document for the active track, whose
    /// parked copy is stale by design.
    pub fn lane_row_range(
        &self,
        lane: usize,
        active_doc: &ExpressionDoc,
        fit_pad: f64,
    ) -> Option<(f64, f64)> {
        self.lane_tracks(lane)
            .into_iter()
            .filter(|&i| !self.tracks[i].hidden)
            .filter_map(|i| {
                let doc = if i == self.active {
                    active_doc
                } else {
                    self.doc_of(i)?
                };
                Some(Self::doc_row_range(doc, self.tracks[i].mode, fit_pad))
            })
            .reduce(|(alo, ahi), (blo, bhi)| (alo.min(blo), ahi.max(bhi)))
    }

    /// Re-apply natural heights after something changed a member's mode.
    /// Hand-sized lanes keep their height.
    pub fn refresh_lane_weights(&mut self) {
        for i in 0..self.layout.len() {
            let natural = self.natural_weight(i);
            if let Some(lane) = self.layout.lane_mut(i) {
                lane.set_default_weight(natural);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    /// Never empty: an editor always has a track, even a blank one.
    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn active(&self) -> usize {
        self.active
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn track(&self, i: usize) -> Option<&Track> {
        self.tracks.get(i)
    }

    pub fn track_mut(&mut self, i: usize) -> Option<&mut Track> {
        self.tracks.get_mut(i)
    }

    pub fn names(&self) -> Vec<&str> {
        self.tracks.iter().map(|t| t.name.as_str()).collect()
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.tracks.iter().position(|t| t.name == name)
    }

    /// Resolve a guid to its current index.
    ///
    /// The index is a *result*, never something to store: it is valid
    /// until the next insert or remove. Durable data holds the guid and
    /// resolves it on load, which is what makes a stored layout survive
    /// a track being inserted above it or renamed.
    pub fn index_of_guid(&self, guid: &str) -> Option<usize> {
        self.tracks.iter().position(|t| t.guid == guid)
    }

    pub fn track_by_guid(&self, guid: &str) -> Option<&Track> {
        self.tracks.iter().find(|t| t.guid == guid)
    }

    pub fn track_by_guid_mut(&mut self, guid: &str) -> Option<&mut Track> {
        self.tracks.iter_mut().find(|t| t.guid == guid)
    }

    /// A parked track's document.
    ///
    /// Returns `None` for the active slot, whose copy here is stale —
    /// the live one is in `Editor::doc`. Making that a `None` rather
    /// than silently handing back the stale copy is what stops a
    /// reference overlay from drawing a track's *previous* state over
    /// the notes you are editing.
    pub fn doc_of(&self, i: usize) -> Option<&ExpressionDoc> {
        if i == self.active {
            return None;
        }
        self.tracks.get(i).map(|t| &t.doc)
    }

    /// Tracks that appear in a stacked view, in order, with indices.
    ///
    /// Unlike [`Workspace::references`] this *includes* the active
    /// track: a stack shows everything, and the active one is simply
    /// the row you are editing.
    pub fn visible(&self) -> impl Iterator<Item = (usize, &Track)> {
        self.tracks.iter().enumerate().filter(|(_, t)| !t.hidden)
    }

    /// The active track's mode.
    pub fn mode(&self) -> Mode {
        self.tracks
            .get(self.active)
            .map(|t| t.mode)
            .unwrap_or_default()
    }

    /// Lay the visible tracks out over `height`, in order.
    ///
    /// Proportional to each track's weight, with a floor: a dimension
    /// squeezed to nothing is a dimension that cannot be clicked to select,
    /// so a user cannot get out of the state they fell into.
    ///
    /// `active_boost` gives the row being edited extra weight — you are
    /// working in it and it should have the room. `1.0` divides purely
    /// by weight.
    pub fn stack(&self, height: f32, active_boost: f32, min_row: f32) -> Vec<StackRow> {
        // One row per *lane*, not per track. The boost goes to the
        // active lane — the active track inside it already carries the
        // highlight, so boosting per-track would double-count.
        let active_lane = self.active_lane();
        let rows: Vec<(usize, f32)> = (0..self.layout.len())
            .filter(|&i| self.lane_is_visible(i))
            .map(|i| {
                let weight = self.layout.lane(i).map(|l| l.weight).unwrap_or(1.0);
                let boost = if Some(i) == active_lane {
                    active_boost
                } else {
                    1.0
                };
                (i, weight.max(0.01) * boost.max(0.01))
            })
            .collect();
        if rows.is_empty() || height <= 0.0 {
            return Vec::new();
        }

        let min_row = min_row.max(0.0);
        let n = rows.len() as f32;

        // When the floor wants more room than there is, lay the lanes
        // out **at the floor and overflow** — the caller scrolls.
        //
        // This used to fall back to an even split, reasoning that if
        // nothing fits well then every lane should be equally bad. That
        // is right for four tracks and wrong for an orchestra: twenty
        // lanes on a 1000px screen is twenty unreadable strips, and a
        // lane you cannot read serves the feature no better than one you
        // cannot see. A floor that is actually honoured is the point.
        if min_row * n > height && min_row <= height {
            let total: f32 = rows.iter().map(|(_, w)| w).sum::<f32>().max(1e-6);
            let mut y = 0.0;
            return rows
                .iter()
                .map(|&(lane, w)| {
                    // Weight still counts above the floor, so a kit stays
                    // taller than a MIDI guide even while scrolling.
                    let h = (min_row * n * (w / total)).max(min_row);
                    let row = StackRow { lane, y, height: h };
                    y += h;
                    row
                })
                .collect();
        }

        // The true last resort: a viewport too small for even one
        // floored lane. Nothing can be readable, so be uniformly wrong
        // rather than arbitrarily so.
        if min_row * n >= height {
            let each = height / n;
            let mut y = 0.0;
            return rows
                .iter()
                .map(|&(track, _)| {
                    let row = StackRow {
                        lane: track,
                        y,
                        height: each,
                    };
                    y += each;
                    row
                })
                .collect();
        }

        // Hand every dimension its floor first, then share what is left by
        // weight. Sharing the whole height by weight and clamping
        // afterwards would overflow by however much the clamping added.
        let spare = height - min_row * n;
        let total: f32 = rows.iter().map(|(_, w)| w).sum();
        let mut y = 0.0;
        rows.iter()
            .map(|&(track, w)| {
                let h = min_row + spare * (w / total);
                let row = StackRow {
                    lane: track,
                    y,
                    height: h,
                };
                y += h;
                row
            })
            .collect()
    }

    /// Which lane a y coordinate falls in.
    pub fn row_at(rows: &[StackRow], y: f32) -> Option<usize> {
        rows.iter()
            .find(|r| y >= r.y && y < r.y + r.height)
            .map(|r| r.lane)
    }

    /// Parked tracks marked as references, with their indices.
    pub fn references(&self) -> impl Iterator<Item = (usize, &Track)> {
        self.tracks
            .iter()
            .enumerate()
            .filter(move |(i, t)| *i != self.active && t.reference)
    }

    /// Add a track and return its index.
    ///
    /// It arrives in a lane of its own. Pairing it with an existing lane
    /// is the matcher's job, not this one's — and a track alone in a
    /// lane is the safe default, because too many lanes is legible and
    /// one merge away, whereas a wrong pairing hides a track underneath
    /// another, which is the failure lanes exist to prevent.
    pub fn push(&mut self, track: Track) -> usize {
        let lane = Lane::single(track.guid.clone(), track.mode.stack_weight());
        self.layout.push(lane);
        self.tracks.push(track);
        self.tracks.len() - 1
    }

    /// Remove a track. The active one cannot be removed — closing what
    /// you are editing is a host decision, not an editor one.
    pub fn remove(&mut self, i: usize) -> bool {
        if i == self.active || i >= self.tracks.len() {
            return false;
        }
        let guid = self.tracks[i].guid.clone();
        self.tracks.remove(i);
        self.layout.forget(&guid);
        if i < self.active {
            self.active -= 1;
        }
        true
    }

    /// Park the live document and history into the active slot, then
    /// take the target's. Returns what the editor should load.
    ///
    /// Both halves happen here so there is no window in which two slots
    /// believe they own the same history.
    pub(crate) fn swap_active(
        &mut self,
        to: usize,
        doc: ExpressionDoc,
        history: History,
    ) -> Option<(ExpressionDoc, History)> {
        if to == self.active || to >= self.tracks.len() {
            return None;
        }
        let cur = &mut self.tracks[self.active];
        cur.doc = doc;
        cur.history = history;

        self.active = to;
        let next = &mut self.tracks[to];
        // The document is cloned rather than moved out: there is no
        // sensible empty `ExpressionDoc` to leave behind, and a track
        // switch is a keystroke, not a hot path. The copy left here is
        // stale, which is precisely what `doc_of` refuses to hand out.
        Some((
            next.doc.clone(),
            core::mem::replace(&mut next.history, History::new(HISTORY_LIMIT)),
        ))
    }

    /// Replace a parked track's document with a fresh analysis.
    ///
    /// For hosts that re-read audio after an edit landed on the daw —
    /// the drawing must follow the writes or the lane shows where the
    /// hits *were*. History is reset: the old undo steps describe a
    /// document that no longer exists underneath them. Refuses the
    /// active slot — that document lives on the editor, and writing the
    /// stale copy here would be exactly what [`Workspace::doc_of`]
    /// exists to prevent; callers go through `Editor::reload_track_doc`.
    pub fn reload_doc(&mut self, guid: &str, doc: ExpressionDoc) -> bool {
        let Some(i) = self.index_of_guid(guid) else {
            return false;
        };
        if i == self.active {
            return false;
        }
        let t = &mut self.tracks[i];
        t.doc = doc;
        t.history = History::new(HISTORY_LIMIT);
        true
    }

    /// Rename without disturbing anything else.
    pub fn rename(&mut self, i: usize, name: impl Into<String>) -> bool {
        match self.tracks.get_mut(i) {
            Some(t) => {
                t.name = name.into();
                true
            }
            None => false,
        }
    }
}
