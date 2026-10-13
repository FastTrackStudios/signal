//! Where a control gets its track from.
//!
//! Two things have to be true at once, and they pull in opposite
//! directions. A control must render from *live* state — mute the track from
//! REAPER's own menu and the button in the app has to light without anybody
//! asking the backend again. And a control must be renderable with **no**
//! backend at all: the theme exporter, the panel tests and the browser
//! playground all draw mixer strips with no DAW attached.
//!
//! So the state and the wire are separated. [`TrackStore`] is a plain map of
//! track GUID to [`Track`], held in a Signal and read from context: a test
//! or a playground seeds it by hand. [`use_daw_tracks`] is the only piece
//! that touches `daw_control`, and all it does is push into the same store —
//! seed once from the project, then apply every [`TrackEvent`] as it
//! arrives.
//!
//! That is why a backend-side mute shows up in the app without a refetch:
//! nothing polls, the event *is* the update.

use std::collections::HashMap;

use daw_proto::{Track, TrackEvent};

use crate::controls::{Drafts, Held};
use crate::prelude::*;

/// Every track the UI knows about, by GUID.
///
/// `Copy`, because a Dioxus `Signal` is: handlers take it by value and
/// closures do not have to clone it.
#[derive(Clone, Copy, PartialEq)]
pub struct TrackStore {
    tracks: Signal<HashMap<String, Track>>,
    /// The values the UI is holding mid-gesture. Consulted on the way in:
    /// an inbound change for a held track is the echo of our own write, and
    /// applying it would fight the finger — see [`crate::controls::Drafts`].
    drafts: Drafts,
    /// Track guids in project order.
    ///
    /// The meter frame is indexed by *position*, not by guid — one frame
    /// carries the whole mixer, which is what makes it cheap — so something
    /// has to know which position is which track. Kept here because this is
    /// the thing that already learns about adds, removes and moves; kept as
    /// a cached vector rather than derived per frame because it is read
    /// thirty times a second and changes about never.
    order: Signal<Vec<String>>,
}

impl Default for TrackStore {
    /// Same scope requirement as [`TrackStore::new`] — it is the same
    /// constructor. Present so the store reads like any other value type.
    fn default() -> Self {
        Self::new()
    }
}

impl TrackStore {
    /// An empty store. Must be called inside a component scope — it owns a
    /// Signal.
    pub fn new() -> Self {
        Self {
            tracks: Signal::new(HashMap::new()),
            drafts: Drafts::new(),
            order: Signal::new(Vec::new()),
        }
    }

    /// Track guids in project order — position `i` is the track the meter
    /// frame's `tracks[i]` describes.
    pub fn order(&self) -> Vec<String> {
        self.order.read().clone()
    }

    /// Move one track to a new position, renumbering everything it passes.
    ///
    /// The backend's own numbering arrives with the next bulk read; until
    /// then this is the honest local answer, and it keeps the meter frame
    /// pointing at the right strips.
    fn move_track(&mut self, guid: &str, to: usize) {
        let mut order = self.order.read().clone();
        let Some(from) = order.iter().position(|g| g == guid) else {
            return;
        };
        let to = to.min(order.len().saturating_sub(1));
        let moved = order.remove(from);
        order.insert(to, moved);

        let mut tracks = self.tracks.write();
        for (i, g) in order.iter().enumerate() {
            if let Some(t) = tracks.get_mut(g) {
                t.index = i as u32;
            }
        }
        drop(tracks);
        self.order.set(order);
    }

    /// Recompute the order from the tracks themselves.
    ///
    /// By `Track::index`, which is the same order `Tracks::all` returns and
    /// therefore the order the meter frame is indexed in. Called whenever
    /// the set of tracks or their indices could have changed — an add, a
    /// remove, a move — because getting this wrong is silent: every meter
    /// still moves, they are just each pointing at the wrong track.
    fn reorder(&mut self) {
        let mut by_index: Vec<(u32, String)> = self
            .tracks
            .read()
            .values()
            .map(|t| (t.index, t.guid.clone()))
            .collect();
        by_index.sort_unstable();
        self.order
            .set(by_index.into_iter().map(|(_, g)| g).collect());
    }

    /// The values the UI is holding. A control writes its drag here and
    /// reads it back the same frame; the sync loop drains it.
    pub fn drafts(&self) -> Drafts {
        self.drafts
    }

    /// What to render for this track's volume: the UI's own in-flight value
    /// while one exists, the engine's otherwise.
    pub fn volume(&self, guid: &str) -> f64 {
        self.drafts
            .volume(guid)
            .or_else(|| self.track(guid).map(|t| t.volume))
            .unwrap_or(0.0)
    }

    /// The same for pan, which a drag owns the same way.
    pub fn pan(&self, guid: &str) -> f64 {
        self.drafts
            .pan(guid)
            .or_else(|| self.track(guid).map(|t| t.pan))
            .unwrap_or(0.0)
    }

    /// Replace everything with the project's current track list.
    ///
    /// The seeding read, and deliberately a *replace*: a track removed while
    /// the app was not listening must not survive the reseed.
    pub fn seed(&mut self, tracks: impl IntoIterator<Item = Track>) {
        self.tracks
            .set(tracks.into_iter().map(|t| (t.guid.clone(), t)).collect());
        self.reorder();
    }

    /// Fold one backend event into the store.
    ///
    /// Only the fields some control renders are handled. The rest are
    /// ignored rather than triggering a refetch — an event this store does
    /// not understand is state nothing on screen is showing yet, and each
    /// control that lands adds its own arm here.
    pub fn apply(&mut self, event: &TrackEvent) {
        let mut tracks = self.tracks.write();
        // The list changing is its own shape; everything else is one field
        // of one track, so those share a single lookup rather than
        // repeating `if let Some(t) = tracks.get_mut(guid)` nine times.
        let (guid, edit): (_, &dyn Fn(&mut Track)) = match event {
            // The three that change *positions*, and so the meter frame's
            // alignment. `drop(tracks)` before `reorder`, which takes the
            // same signal.
            TrackEvent::Added(t) => {
                tracks.insert(t.guid.clone(), t.clone());
                drop(tracks);
                self.reorder();
                return;
            }
            TrackEvent::Removed(guid) => {
                tracks.remove(guid);
                drop(tracks);
                self.reorder();
                return;
            }
            // A move renumbers *every* track it passes, and the event can
            // only name one of them — so the order is edited as a move
            // rather than recomputed from indices that are now stale. A
            // first version set the moved track's index and re-sorted,
            // which left the track it displaced still claiming the same
            // index and the sort deciding between them alphabetically: the
            // meters shifted by one and kept running, which is exactly the
            // silent failure this ordering exists to prevent.
            TrackEvent::Moved {
                guid, new_index, ..
            } => {
                drop(tracks);
                self.move_track(guid, *new_index as usize);
                return;
            }
            TrackEvent::Renamed { guid, name } => (guid, &|t| t.name = name.clone()),
            TrackEvent::MuteChanged { guid, muted } => (guid, &|t| t.muted = *muted),
            TrackEvent::SoloChanged { guid, soloed } => (guid, &|t| t.soloed = *soloed),
            TrackEvent::ArmChanged { guid, armed } => (guid, &|t| t.armed = *armed),
            TrackEvent::SelectionChanged { guid, selected } => (guid, &|t| t.selected = *selected),
            // Dropped while the UI holds this track: it is the echo of a
            // drag still in progress, and obeying it would drag the cap
            // backwards under the pointer.
            TrackEvent::VolumeChanged { guid, volume }
                if !self.drafts.holds(guid, Held::Volume) =>
            {
                (guid, &|t| t.volume = *volume)
            }
            TrackEvent::VolumeChanged { .. } => return,
            TrackEvent::PanChanged { guid, pan } if !self.drafts.holds(guid, Held::Pan) => {
                (guid, &|t| t.pan = *pan)
            }
            TrackEvent::PanChanged { .. } => return,
            TrackEvent::ColorChanged { guid, color } => (guid, &|t| t.color = *color),
            TrackEvent::ParentSendChanged { guid, enabled } => {
                (guid, &|t| t.parent_send = *enabled)
            }
            TrackEvent::PhaseInvertedChanged { guid, inverted } => {
                (guid, &|t| t.phase_inverted = *inverted)
            }
            TrackEvent::InputMonitorChanged { guid, monitor } => {
                (guid, &|t| t.input_monitor = *monitor)
            }
            // The event that did not exist until #142: without it a
            // mixer's FX buttons were right when it opened and wrong from
            // the first plugin the user added.
            TrackEvent::FxCountChanged {
                guid,
                fx_count,
                input_fx_count,
            } => (guid, &|t| {
                t.fx_count = *fx_count;
                t.input_fx_count = *input_fx_count;
            }),
            // Moved and the automation/monitor modes: state no control
            // draws yet. Ignored rather than refetched — the next control
            // that renders one adds its arm here.
            _ => return,
        };
        if let Some(t) = tracks.get_mut(guid) {
            edit(t);
        }
    }

    /// One track, if the store has it.
    pub fn track(&self, guid: &str) -> Option<Track> {
        self.tracks.read().get(guid).cloned()
    }
}

/// The store from context, creating and providing one if this is the first
/// asker.
///
/// Provide-or-consume rather than provide-only: a control has to draw
/// wherever it is put, including a test that mounts one button and nothing
/// else. An empty store renders an unmuted button, which is the right thing
/// to show before the project has answered.
pub fn use_track_store() -> TrackStore {
    use_hook(|| match try_consume_context::<TrackStore>() {
        Some(store) => store,
        None => provide_context(TrackStore::new()),
    })
}

/// One track from the store, re-read whenever it changes.
///
/// A [`Memo`] rather than a plain read so a control only re-renders when
/// *its* track moves — a 40-track project should not redraw every strip
/// because one fader did.
pub fn use_track(guid: String) -> Memo<Option<Track>> {
    let store = use_track_store();
    // `use_reactive!`, not a plain closure: hooks run once, so a closure
    // capturing `guid` would pin the memo to whichever track the component
    // first rendered. A strip that reorders, or a list without keys, then
    // shows one track's name over another's mute.
    use_memo(use_reactive!(|guid| store.track(&guid)))
}

/// The store first, the prop as the seed.
///
/// Both panels render from a `Track` prop but follow the track stream:
/// reading colour and name off the prop left the band and the plate a poll
/// behind the buttons beside them — a recolour in REAPER took up to two
/// seconds to arrive. This is that dance in one place: the memo holds the
/// live track when the store has it, and the caller borrows the seed
/// until the first event lands (or forever, in a test that seeds nothing):
///
/// ```ignore
/// let live = use_live_track(&props.track);
/// let live = live.read();
/// let track = live.as_ref().unwrap_or(&props.track);
/// ```
///
/// Returns the memo rather than an owned `Track` so a strip does not
/// deep-clone its track (guid, name, lane names…) on every render — with
/// meters running that was a clone per strip per frame.
pub fn use_live_track(seed: &Track) -> Memo<Option<Track>> {
    use_track(seed.guid.clone())
}

/// Keep `store` fed from the connected DAW.
///
/// Call once, high up — a strip's worth of controls share the one store.
/// Waits for a connection, subscribes, then seeds: subscribing first means
/// an edit landing during the seeding read shows up as an event rather than
/// falling into the gap between the two calls.
///
pub fn use_daw_tracks(mut store: TrackStore) {
    use_future(move || async move {
        let Some(project) = crate::controls::reach::connected_project().await else {
            return;
        };
        let mut events = match project.tracks().subscribe().await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("track store: cannot subscribe: {e:?}");
                return;
            }
        };
        match project.tracks().all().await {
            Ok(tracks) => store.seed(tracks),
            Err(e) => tracing::warn!("track store: cannot seed: {e:?}"),
        }
        while let Ok(Some(event)) = events.recv().await {
            store.apply(&event.get().event);
        }
    });
}
