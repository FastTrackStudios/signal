//! Editor workspace behavior.
use super::*;

impl Editor {
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
        self.tracks.set_guid(active, guid);
        if let (Some(track), Some(name)) = (self.tracks.track_mut(active), name) {
            track.name = name;
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

    /// Step to another track, wrapping at both ends.
    pub fn step_track(&mut self, delta: i32) -> bool {
        let n = self.tracks.len() as i32;
        if n <= 1 {
            return false;
        }
        let next = (self.active_track() as i32 + delta).rem_euclid(n) as usize;
        self.switch_track(next)
    }
}
