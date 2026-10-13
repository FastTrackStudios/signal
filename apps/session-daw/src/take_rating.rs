//! Rating vocal takes while recording: a ★ and a ✕, dropped on the active
//! take of the vocal track as REAPER's own rank markers (`:)`, `:))`,
//! `:)))`, `:(` — [`daw_proto::TakeRating`]). They ARE the take's review:
//! REAPER shows and comps by them, and the session reads its review back
//! from the take markers.
//!
//! One ★ is one star; ★ again at the same spot makes it two, then three —
//! the last star's marker replaced by the next ([`Stars`]). The spot is a
//! moment ([`Scope::Moment`]: the playhead, while he sings or scrubbed back
//! to) or the whole take ([`Scope::Take`]: its start, from the prompt at the
//! end of a take).
//!
//! The active take is the vocal track's: the top-level track whose name
//! says vocals, or the tracks under it when it is a folder — an armed one
//! first (the one just recorded to).

use std::sync::Mutex;

use daw_proto::TakeRating;

/// Where a mark goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scope {
    /// The moment at this time (project seconds), on the take under it.
    Moment(f64),
    /// The whole take covering this time: marked at its start.
    Take(f64),
}

impl Scope {
    const fn at(self) -> f64 {
        match self {
            Self::Moment(at) | Self::Take(at) => at,
        }
    }
}

/// How close (in song seconds) another ★ has to be to raise the last one
/// rather than start a new mark.
pub const SAME_SPOT: f64 = 4.0;
/// The most stars a mark has.
pub const MAX_STARS: u8 = 3;

/// Counting stars at a spot: what the next ★ makes it — the rule the pad
/// shows, and the markers follow.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stars {
    /// The last star's spot (song seconds) and how many it has.
    last: Option<(f64, u8)>,
}

impl Stars {
    /// A ★ at `at`: how many stars the mark there now has.
    pub fn press(&mut self, at: f64) -> u8 {
        let (spot, level) = match self.last {
            Some((spot, level)) if (at - spot).abs() < SAME_SPOT => {
                (spot, (level + 1).min(MAX_STARS))
            }
            _ => (at, 1),
        };
        self.last = Some((spot, level));
        level
    }

    /// A ✕, or anything else that ends the counting.
    pub fn reset(&mut self) {
        self.last = None;
    }

    /// The stars at the last spot, if any.
    #[must_use]
    pub fn level(&self) -> Option<u8> {
        self.last.map(|(_, level)| level)
    }
}

/// The last star marker placed: which item, its marker, where, how many —
/// what the next ★ at the same spot replaces.
struct Placed {
    item: String,
    index: u32,
    spot: f64,
    level: u8,
}

static LAST: Mutex<Option<Placed>> = Mutex::new(None);

/// A ★ in `scope`: one star, or — at the spot of the last — one more.
pub fn star(scope: Scope) {
    run(async move {
        let Some(target) = vocal_take(scope.at()).await else {
            tracing::warn!(rating.at = scope.at(), "take-rating: no vocal take there");
            return;
        };
        let spot = match scope {
            Scope::Moment(at) => at,
            Scope::Take(_) => target.start,
        };
        // Raise the last star, if this is its spot: its marker goes, and
        // the next level takes its place.
        let raise = LAST.lock().ok().and_then(|mut last| {
            let placed = last.take()?;
            (placed.item == target.item && (spot - placed.spot).abs() < SAME_SPOT).then_some(placed)
        });
        let (spot, level) = match raise {
            Some(placed) => {
                if let Err(e) = target.take.delete_marker(placed.index).await {
                    tracing::warn!(error = %e, "take-rating: the last star could not be raised");
                }
                (placed.spot, (placed.level + 1).min(MAX_STARS))
            }
            None => (spot, 1),
        };
        if let Some(index) = mark(&target.take, TakeRating::UpRank(level), spot).await
            && let Ok(mut last) = LAST.lock()
        {
            *last = Some(Placed {
                item: target.item,
                index,
                spot,
                level,
            });
        }
    });
}

/// A ✕ in `scope`: something went wrong there.
pub fn miss(scope: Scope) {
    if let Ok(mut last) = LAST.lock() {
        *last = None;
    }
    run(async move {
        let Some(target) = vocal_take(scope.at()).await else {
            tracing::warn!(rating.at = scope.at(), "take-rating: no vocal take there");
            return;
        };
        let spot = match scope {
            Scope::Moment(at) => at,
            Scope::Take(_) => target.start,
        };
        mark(&target.take, TakeRating::DownRank, spot).await;
    });
}

/// Drop `rank` on `take` at `at`: its marker's index, if it landed.
async fn mark(take: &daw::rpc::TakeHandle, rank: TakeRating, at: f64) -> Option<u32> {
    let position = daw_proto::Position::from_time(daw_proto::PositionInSeconds::from_seconds(at));
    match take
        .add_marker_at(position, &rank.to_marker_name(), None)
        .await
    {
        Ok(Some(index)) => {
            tracing::info!(rating.rank = %rank.to_marker_name(), rating.at = at, "take-rating: marked");
            Some(index)
        }
        Ok(None) => {
            tracing::warn!(rating.at = at, "take-rating: outside the take");
            None
        }
        Err(e) => {
            tracing::warn!(error = %e, "take-rating: the marker was refused");
            None
        }
    }
}

/// The vocal take at a time: its active take, its item, where it starts.
struct Target {
    take: daw::rpc::TakeHandle,
    item: String,
    start: f64,
}

/// The active take of the vocal track at `at`.
async fn vocal_take(at: f64) -> Option<Target> {
    let daw = daw::rpc::Daw::try_get()?;
    let project = daw.current_project().await.ok()?;
    let tracks = project.tracks().all().await.ok()?;
    let vocal = vocal_tracks(&tracks);
    if vocal.is_empty() {
        return None;
    }
    let items = project.items().all().await.ok()?;
    // Under `at`, on a vocal track; an armed one first, then the latest.
    let item = items
        .iter()
        .filter_map(|item| {
            let rank = vocal
                .iter()
                .position(|(guid, _)| *guid == item.track_guid)?;
            let from = item.position.as_seconds();
            let to = from + item.length.as_seconds();
            (from <= at && at < to).then_some((vocal[rank].1, from, item))
        })
        .max_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)))?;
    let handle = project.items().by_guid(&item.2.guid).await.ok()??;
    Some(Target {
        take: handle.active_take(),
        item: item.2.guid.clone(),
        start: item.1,
    })
}

/// The vocal tracks, by guid, and whether each is armed: the tracks under
/// the top-level track whose name says vocals — a folder's children, or
/// that track itself when it is a plain track.
fn vocal_tracks(tracks: &[daw_proto::Track]) -> Vec<(String, bool)> {
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let top = |track: &daw_proto::Track| {
        let mut at = track;
        while let Some(parent) = at.parent_guid.as_deref().and_then(|g| by_guid.get(g)) {
            at = parent;
        }
        at.name.to_lowercase()
    };
    let is_vocal = |name: &str| name.contains("vocal") || name.contains("vox");
    let mut out: Vec<(String, bool)> = tracks
        .iter()
        .filter(|t| !t.is_folder && is_vocal(&top(t)))
        .map(|t| (t.guid.clone(), t.armed))
        .collect();
    if out.is_empty() {
        out = tracks
            .iter()
            .filter(|t| is_vocal(&t.name.to_lowercase()))
            .map(|t| (t.guid.clone(), t.armed))
            .collect();
    }
    out
}

/// Run `work` against the facade, off the event loop.
fn run(work: impl std::future::Future<Output = ()> + 'static + MaybeSend) {
    #[cfg(not(feature = "native"))]
    wasm_bindgen_futures::spawn_local(work);
    #[cfg(feature = "native")]
    {
        let Some(runtime) = crate::open::runtime() else {
            return;
        };
        runtime.spawn(work);
    }
}

#[cfg(feature = "native")]
trait MaybeSend: Send {}
#[cfg(feature = "native")]
impl<T: Send> MaybeSend for T {}
#[cfg(not(feature = "native"))]
trait MaybeSend {}
#[cfg(not(feature = "native"))]
impl<T> MaybeSend for T {}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(
        guid: &str,
        name: &str,
        parent: Option<&str>,
        folder: bool,
        armed: bool,
    ) -> daw_proto::Track {
        daw_proto::Track {
            guid: guid.into(),
            name: name.into(),
            parent_guid: parent.map(Into::into),
            is_folder: folder,
            armed,
            ..daw_proto::Track::default()
        }
    }

    #[test]
    fn the_vocal_tracks_are_the_ones_under_the_vocals_folder() {
        let tracks = [
            track("d", "Drums", None, true, false),
            track("k", "Kick", Some("d"), false, false),
            track("v", "VOCALS", None, true, false),
            track("l", "Lead", Some("v"), false, true),
            track("b", "BGV", Some("v"), false, false),
        ];
        assert_eq!(
            vocal_tracks(&tracks),
            vec![("l".to_owned(), true), ("b".to_owned(), false)]
        );
    }

    #[test]
    fn a_star_again_at_the_same_spot_is_one_more_up_to_three() {
        let mut stars = Stars::default();
        assert_eq!(stars.press(10.0), 1);
        assert_eq!(stars.press(11.5), 2);
        assert_eq!(stars.press(12.0), 3);
        assert_eq!(stars.press(12.5), 3, "three is the most");
        assert_eq!(stars.press(40.0), 1, "somewhere else is a new mark");
        stars.reset();
        assert_eq!(stars.press(40.5), 1, "after a miss, counting starts again");
    }

    #[test]
    fn the_ranks_are_reapers_marker_names() {
        let names: Vec<String> = [
            TakeRating::UpRank(1),
            TakeRating::UpRank(3),
            TakeRating::DownRank,
        ]
        .iter()
        .map(|r| r.to_marker_name())
        .collect();
        assert_eq!(names, [":)", ":)))", ":("]);
    }
}
