//! The project, as one immutable snapshot.
//!
//! Everything the studio draws comes from here, fetched once and then
//! held behind an [`Arc`]. That is the first rule this rebuild is built
//! on: **a component never owns project data it could share**. Dioxus
//! hands a component its props by value on every render, so a
//! `Vec<Item>` prop of a real session's eight hundred takes is a deep
//! copy per render — which is how a UI ends up spending its frame budget
//! memcpy'ing a record it did not change.
//!
//! The snapshot is also *flat and pre-grouped*. Items arrive from the
//! facade as one list for the whole project; a lane wants only its own,
//! and looking that up by scanning is `O(tracks × items)` on every
//! render. Grouping once at fetch time makes drawing a lane an index.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use daw_proto::{Item, Track};

/// A song section — REAPER's regions, the coloured bands over the ruler.
#[derive(Clone, PartialEq, Debug)]
pub struct Section {
    /// REAPER's own region number — what addresses it for an edit.
    ///
    /// Not the index in this list, and not stable across a renumber:
    /// markers and regions are one list in REAPER wearing one set of
    /// numbers, and its renumber action reassigns them. The event
    /// stream reports that as `Renumbered` rather than as a deletion,
    /// so a window holding this can follow it.
    pub id: u32,
    pub start: f64,
    pub end: f64,
    pub name: String,
    /// Already resolved to CSS, so a render is a projection and not a
    /// colour conversion per band per frame.
    pub color: Option<String>,
    /// The ruler lane it sits on (REAPER 7.62+); 0 is the default lane.
    pub lane: u32,
}

/// A tempo or time-signature change, at a time.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TempoChange {
    /// When it takes effect, in seconds.
    pub at: f64,
    pub bpm: f64,
    /// The signature's top number: how many beats to a bar.
    pub beats_per_bar: u32,
    /// Its bottom number: which note gets the beat.
    pub beat_unit: u32,
}

/// A project marker — the numbered flags under the region lane.
#[derive(Clone, PartialEq, Debug)]
pub struct Marker {
    pub at: f64,
    pub name: String,
    pub color: Option<String>,
    /// REAPER's own marker number, which is what the flag is labelled
    /// with. Not the index in this list: markers can be renumbered.
    pub idx: u32,
    /// The ruler lane it sits on (REAPER 7.62+); 0 is the default lane.
    pub lane: u32,
}

/// Everything the window stands on, in one pass.
///
/// Deliberately NOT `Default`-constructed into the live signal and then
/// filled in field by field: a half-populated project is a state every
/// component would have to defend against. The signal holds
/// `Option<Arc<Project>>` instead, so "not open yet" is one branch at
/// the root rather than an empty vector everywhere.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Project {
    /// Project order, folders included — the TCP's own list before any
    /// collapse is applied.
    pub tracks: Vec<Track>,
    /// Items keyed by their track's guid, in time order within a track.
    ///
    /// **Real items only.** A folded row's items live in [`Self::folds`]
    /// and deliberately not here: every edit in the window walks this
    /// map, and a synthetic item in it would be one an edit could delete,
    /// split or address to the backend. Keeping them apart makes that
    /// impossible rather than merely discouraged.
    pub items: HashMap<String, Vec<Item>>,
    /// The rows the view has folded shut, by folder guid.
    ///
    /// Read through [`Self::lane`], which is the one seam every renderer
    /// already goes through — so a folded row draws, labels and
    /// hit-tests like any other without anything being taught about it.
    pub folds: HashMap<String, super::folded::Fold>,
    pub sections: Vec<Section>,
    pub markers: Vec<Marker>,
    /// The project tempo. One number, for now: the ruler's bar lines
    /// assume a constant tempo, which is true of most sessions and
    /// wrong for some. A tempo map belongs here when the ruler grows
    /// one — the shape of this struct is what would change, not the
    /// components that read it.
    pub bpm: f64,
    /// Where the tempo or the time signature changes, in order, with a
    /// point at zero however bare the project is.
    ///
    /// The ruler counts bars, and a bar is only as long as the tempo
    /// and the signature at that moment say it is. Counting the whole
    /// timeline from one nominal tempo puts every bar after the first
    /// change in the wrong place — and a ruler that is wrong about
    /// where bar forty is, is a ruler nobody can edit against.
    pub tempo: Vec<TempoChange>,
    /// End of the last item — how far the timeline has to reach.
    pub length_secs: f64,
    /// How many items the project holds, across every track. Kept
    /// because a count of a grouped map is a walk, and the readout
    /// wants it every render.
    pub item_count: usize,
    /// Each item's title — its active take's name — keyed by item guid.
    /// An item carries no name of its own; the take does, and the
    /// arrangement writes it on the item.
    pub names: HashMap<String, String>,
    /// Which items hold MIDI rather than audio, by item guid.
    ///
    /// A set and not a flag on `Item`, because `Item` is the daw's own
    /// type and MIDI-ness lives on the TAKE — an item can hold several,
    /// and which one is playing is the item's business. This is the
    /// active take's answer, which is the one being drawn.
    pub midi: HashSet<String>,
}

impl Project {
    /// What an item is called: its active take's name, else its label,
    /// else nothing.
    /// Does this item hold MIDI rather than audio?
    ///
    /// The active take's answer, which is the one being drawn.
    #[must_use]
    pub fn is_midi(&self, guid: &str) -> bool {
        self.midi.contains(guid)
    }

    pub fn title<'a>(&'a self, item: &'a Item) -> Option<&'a str> {
        self.names
            .get(&item.guid)
            .map(String::as_str)
            .filter(|n| !n.is_empty())
            .or(item.label.as_deref())
    }

    /// This track's items, or an empty slice. Never allocates: a lane
    /// renders every frame it is on screen.
    ///
    /// A folder the view has shut answers with its children's items,
    /// folded — see [`super::folded`]. This is the only place that
    /// substitution happens, which is what lets every renderer stay
    /// ignorant of it and every editor stay safe from it.
    #[must_use]
    pub fn lane(&self, track_guid: &str) -> &[Item] {
        if let Some(fold) = self.folds.get(track_guid) {
            return &fold.items;
        }
        self.items.get(track_guid).map_or(&[], Vec::as_slice)
    }

    /// `track_guid`'s items on a lane that plays — the row as it sounds.
    /// A track with REAPER 7 fixed lanes keeps its other takes on lanes
    /// that do not ([`on_playing_lane`]); drawn at the row's height they
    /// were six takes piled on one another.
    #[must_use]
    pub fn sounding(&self, track: &daw_proto::Track) -> Vec<&Item> {
        self.lane(&track.guid)
            .iter()
            .filter(|item| on_playing_lane(track, item))
            .collect()
    }

    /// Where an edit aimed at `guid` should actually land.
    ///
    /// `destructive` is whether it changes the items rather than merely
    /// selecting them. See [`super::folded::spread`].
    #[must_use]
    pub fn spread(&self, guid: &str, destructive: bool) -> super::folded::Spread {
        super::folded::spread(&self.folds, guid, destructive)
    }
}

/// Read the whole project from the live facade.
///
/// One pass, no waveforms. Peaks are the slow ninety percent of opening
/// a real session and none of the layout depends on them, so they stream
/// in afterwards ([`super::waveform`]) against a window that is already
/// standing. A spinner in front of an arrangement we could already draw
/// is a self-inflicted wait.
pub async fn fetch() -> Option<Project> {
    let daw = daw_control::Daw::try_get()?;
    fetch_of(daw.current_project().await.ok()?).await
}

/// [`fetch`] for a given project rather than the current one — how a
/// page reads a song it opens in the background, without switching the
/// song on screen away.
pub async fn fetch_of(project: daw_control::Project) -> Option<Project> {
    let tracks = project.tracks().all().await.ok()?;
    let all_items = project.items().all().await.ok()?;

    let mut length = 0.0f64;
    let item_count = all_items.len();
    let mut items: HashMap<String, Vec<Item>> = HashMap::new();
    let mut names: HashMap<String, String> = HashMap::with_capacity(item_count);
    let mut midi: HashSet<String> = HashSet::new();
    for item in all_items {
        length = length.max(item.position.as_seconds() + item.length.as_seconds());
        // The title is the active take's name — two calls per item,
        // in-process, once per open.
        if let Ok(Some(handle)) = project.items().by_guid(&item.guid).await {
            let take = handle.active_take();
            if let Ok(name) = take.name().await
                && !name.is_empty()
            {
                names.insert(item.guid.clone(), name);
            }
            // Whether it is MIDI, from the same take the name came
            // from. The call was already being paid for and the answer
            // thrown away — and without it an item drawn from its
            // content has no way to know WHICH content it has.
            if let Ok(info) = take.info().await
                && info.is_midi
            {
                midi.insert(item.guid.clone());
            }
        }
        items.entry(item.track_guid.clone()).or_default().push(item);
    }
    // Sorted, and sorted TOTALLY — position alone is not enough.
    //
    // Items on a lane can overlap, and a lane draws them in order, so
    // whichever comes last is the one you see where they cross. Ordering
    // by position alone leaves ties to be broken by whatever order the
    // facade happened to return, which is not stable between opens: the
    // same project drew a different picture on the second run, with one
    // take covering another that had been on top a moment before. The
    // item's index within its track is REAPER's own answer to "which of
    // these is on top", so that is the tiebreak, with the guid behind it
    // so the order is total no matter what.
    for lane in items.values_mut() {
        lane.sort_by(|a, b| {
            a.position
                .as_seconds()
                .partial_cmp(&b.position.as_seconds())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.index.cmp(&b.index))
                .then_with(|| a.guid.cmp(&b.guid))
        });
    }

    let sections = project
        .regions()
        .all()
        .await
        .unwrap_or_default()
        .iter()
        .map(|r| Section {
            id: r.id.unwrap_or_default(),
            start: r.time_range.start_seconds(),
            end: r.time_range.end_seconds(),
            name: r.name.clone(),
            color: r.color.map(css_color),
            lane: r.lane.unwrap_or(0),
        })
        .collect();

    // Real markers only. REAPER stores a region as a marker pair and the
    // standalone loader keeps the two apart, but an unnamed marker
    // sitting on a region edge would be ruler noise either way.
    let markers = project
        .markers()
        .all()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|m| !m.name.is_empty())
        .enumerate()
        .map(|(i, m)| Marker {
            at: m.position.seconds().unwrap_or(0.0),
            name: m.name.clone(),
            color: m.color.map(css_color),
            idx: m.id.unwrap_or(i as u32 + 1),
            lane: m.lane.unwrap_or(0),
        })
        .collect();

    let bpm = project.transport().get_tempo().await.unwrap_or(120.0);
    // Every tempo point, so the ruler can count bars through a change
    // rather than through one nominal tempo. A project with none still
    // has a tempo — the seeded point below is what the transport just
    // said — because a ruler with no grid is not a ruler.
    let mut tempo: Vec<TempoChange> = project
        .tempo_map()
        .points()
        .await
        .unwrap_or_default()
        .iter()
        .filter_map(|point| {
            let at = point.position.time.as_ref()?.as_seconds();
            Some(TempoChange {
                at,
                bpm: point.bpm,
                beats_per_bar: point
                    .time_signature
                    .as_ref()
                    .map_or(4, |sig| sig.numerator.max(1)),
                beat_unit: point
                    .time_signature
                    .as_ref()
                    .map_or(4, |sig| sig.denominator.max(1)),
            })
        })
        .collect();
    tempo.sort_by(|a, b| a.at.total_cmp(&b.at));
    if tempo.first().is_none_or(|first| first.at > 0.0) {
        let (num, den) = project
            .tempo_map()
            .time_signature_at(0.0)
            .await
            .unwrap_or((4, 4));
        tempo.insert(
            0,
            TempoChange {
                at: 0.0,
                bpm,
                beats_per_bar: u32::try_from(num.max(1)).unwrap_or(4),
                beat_unit: u32::try_from(den.max(1)).unwrap_or(4),
            },
        );
    }

    Some(Project {
        tracks,
        items,
        // Filled by whoever knows what the view has shut, which is not
        // the fetch: a fold is a view state, not a fact about the file.
        folds: HashMap::new(),
        sections,
        markers,
        bpm,
        tempo,
        // A minute of empty ruler for a project with nothing in it, so
        // the timeline still has somewhere to put its bar numbers.
        length_secs: length.max(60.0),
        item_count,
        names,
        midi,
    })
}

/// Wait for the backend, then read the project.
///
/// The facade is stood up on a worker thread so the window does not have
/// to wait for it, which means it may not exist when the first render
/// runs. A single attempt would leave the panels empty for the life of
/// the window — this is what makes "open the window first" safe.
pub async fn fetch_when_ready() -> Arc<Project> {
    loop {
        if let Some(project) = fetch().await {
            return Arc::new(project);
        }
        futures_timer::Delay::new(std::time::Duration::from_millis(80)).await;
    }
}

/// A service colour (`0xRRGGBB`) as CSS. Masked to 24 bits: a backend
/// that hands back REAPER's "custom colour" flag in the top byte would
/// otherwise format as seven hex digits, which is not a colour, and the
/// band falls back to the accent.
fn css_color(c: u32) -> String {
    format!("#{:06x}", c & 0x00ff_ffff)
}

/// Whether `item` sits on a lane of `track` that plays: always, without
/// fixed lanes; with them, when its lane's bit is in the play mask — the
/// rule the renderer plays by.
#[must_use]
pub fn on_playing_lane(track: &daw_proto::Track, item: &Item) -> bool {
    track.lane_count == 0
        || item
            .fixed_lane
            .is_none_or(|lane| lane < 64 && track.lane_play_mask & (1_u64 << lane) != 0)
}

#[cfg(test)]
mod playing_lane_tests {
    use super::on_playing_lane;
    use daw_proto::{Item, Track};

    /// The renderer's rule: no lanes, everything; with lanes, the lanes in
    /// the play mask — a comp lane playing hides the takes under it.
    #[test]
    fn only_a_playing_lane_sounds() {
        let on = |lane: Option<u32>| Item {
            fixed_lane: lane,
            ..Item::default()
        };
        let plain = Track::default();
        assert!(on_playing_lane(&plain, &on(None)));
        let comped = Track {
            lane_count: 6,
            lane_play_mask: 0b1,
            ..Track::default()
        };
        assert!(
            on_playing_lane(&comped, &on(Some(0))),
            "the comp lane plays"
        );
        assert!(
            !on_playing_lane(&comped, &on(Some(3))),
            "a take under it does not"
        );
        let take_four = Track {
            lane_play_mask: 0b1_0000,
            ..comped
        };
        assert!(on_playing_lane(&take_four, &on(Some(4))));
    }
}
