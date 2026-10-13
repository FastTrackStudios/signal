//! A collapsed folder's own items, folded from its children's.
//!
//! # What this is for
//!
//! A drum kit is a folder tree because that is how the audio has to be
//! routed: a kick is a Sum with three mics under it, a tom is its mic
//! and its trigger. But the routing is a mix concern, and a mix concern
//! is not what an arrangement is for. While you are editing, a kick is
//! **one thing** — the mics were recorded together, comped together and
//! trimmed together, so what you want on screen is one row per drum with
//! the take on it, and the mics available when you go looking for them.
//!
//! REAPER draws nothing on a closed folder, so folding the kit away
//! today means folding the audio away with it. This is what fills that
//! row: the folder keeps its own track control panel — its name, its
//! fader, its colour, because the mix happens there — and the items on
//! it are its children's.
//!
//! # The rule
//!
//! Every boundary of every child, in order; the row is the spans between
//! them that at least one child is sounding across.
//!
//! One rule, and the two cases the desk cares about fall out of it
//! rather than being special-cased:
//!
//! - **The mics agree**, which is the normal case, because they are one
//!   performance. Their boundaries are the same boundaries, so the spans
//!   are the items, and the row looks exactly like the take.
//! - **They do not**, because one mic was punched in, or a trigger lane
//!   was trimmed on its own. Then the extra boundaries appear as extra
//!   spans, and the row says so. It is not a lie about a clean take and
//!   it is not a refusal to draw; it is the edit that is actually there.
//!
//! [`Span::whole`] is what tells the two apart, for anything downstream
//! that wants to draw the second differently.
//!
//! # Why items and not a picture
//!
//! [`crate::studio`]'s sibling `folder_item` module folds children's
//! PEAKS onto a column grid and draws one continuous picture across the
//! folder's whole extent. That is the right answer for an overview and
//! the wrong one here: a picture has no edges to grab, no title, and no
//! gaps between takes. These are real items, so the row hit-tests,
//! labels and selects like every other row, and an edit has somewhere to
//! land.
//!
//! # Editing, which is the point and is not built yet
//!
//! **An edit on a folded item is an edit on every child item under it.**
//! Drag the kick's take and its three mics move together; trim its edge
//! and they all trim to the same boundary; delete it and they all go.
//! That is the whole reason this row is worth having — a view you can
//! read but not work in is a view you stop using — and it is decided,
//! not speculative. It is simply not written yet.
//!
//! Everything it needs is here. A folded item's guid carries the folder
//! and the span ([`guid_of`], [`parse_guid`]), and the span carries the
//! real items ([`Span::from`]). So the path is: the hit test lands on a
//! folded guid, [`parse_guid`] says which fold, [`Span::from`] says which
//! items, and the edit is applied to all of them as ONE undo step.
//!
//! What is genuinely undecided is the ragged case — what trimming the
//! edge of a span where [`Span::whole`] is false should do, when only
//! some of the children start there. That, the undo grouping, and what
//! take lanes mean on a row that has no takes of its own are written up
//! on issue #114.
//!
//! Until then, one invariant holds absolutely: a `folded:` guid names
//! nothing in the session and must never reach the backend. It is a view
//! coordinate, and writing one would address an item that does not
//! exist.

use std::collections::HashMap;

use daw_proto::Item;

/// One stretch of a folded row: a span with children sounding across it.
#[derive(Clone, PartialEq, Debug)]
pub struct Span {
    pub start: f64,
    pub end: f64,
    /// The child items sounding across it.
    ///
    /// Kept rather than counted because this is the only record of what
    /// a folded item actually IS. It is also the list an edit works
    /// through: moving, trimming or deleting the folded item means doing
    /// the same to every guid in here, in one undo step — see the module
    /// doc. In project order, so the list is stable between renders and
    /// two equal folds compare equal.
    pub from: Vec<String>,
    /// Whether every child sounding here starts and ends exactly here.
    ///
    /// True is the ordinary case and means the span is a take: one item,
    /// no edges of its own. False means the children disagreed and this
    /// is a piece of the disagreement — worth drawing as a fragment
    /// rather than as a take, because that is what it is.
    pub whole: bool,
}

impl Span {
    /// How long it lasts.
    #[must_use]
    pub fn length(&self) -> f64 {
        (self.end - self.start).max(0.0)
    }
}

/// The shortest span worth drawing, in seconds.
///
/// Two children whose boundaries differ by a microsecond are two
/// children that agree; the difference is a rounding in whatever wrote
/// the file, not an edit anybody made. Without a floor here every such
/// pair would produce a third span a micron wide between them, and a
/// clean take would render as three items.
const GRAIN: f64 = 0.001;

/// Fold a folder's children's items into the items its own row shows.
///
/// `children` is one slice per child track, each in position order.
#[must_use]
pub fn spans(children: &[&[Item]]) -> Vec<Span> {
    // Every edge anybody has, in order and without duplicates. The
    // spans are what lies between consecutive edges, so this is the
    // whole algorithm — the rest is asking who is sounding where.
    let mut edges: Vec<f64> = Vec::new();
    for lane in children {
        for item in *lane {
            edges.push(item.position.as_seconds());
            edges.push(item.position.as_seconds() + item.length.as_seconds());
        }
    }
    edges.sort_by(f64::total_cmp);
    edges.dedup_by(|a, b| (*a - *b).abs() < GRAIN);

    let mut out: Vec<Span> = Vec::new();
    for pair in edges.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if end - start < GRAIN {
            continue;
        }
        // Who covers the MIDDLE, not who touches an edge: an item that
        // ends exactly where this span starts is not sounding in it.
        let middle = f64::midpoint(start, end);
        let mut from = Vec::new();
        let mut whole = true;
        for lane in children {
            for item in *lane {
                let at = item.position.as_seconds();
                let to = at + item.length.as_seconds();
                if at < middle && middle < to {
                    whole &= (at - start).abs() < GRAIN && (to - end).abs() < GRAIN;
                    from.push(item.guid.clone());
                }
            }
        }
        if from.is_empty() {
            continue;
        }
        out.push(Span {
            start,
            end,
            from,
            whole,
        });
    }
    out
}

/// The folded spans as items on the folder's own row.
///
/// Real [`Item`]s, and deliberately: everything that draws, labels,
/// hit-tests or selects a lane already works on items, and a second kind
/// of thing on a row would have to be taught to all of it.
///
/// The guid is derived from the folder and the span's place in it, which
/// makes it stable across a re-fold — a shape cache and a selection both
/// key on it — and recoverable, which is how an edit gets from the row
/// back to the mics under it. Nothing in the session has this guid, so
/// it can never be mistaken for a real item and written to REAPER.
#[must_use]
pub fn lane(folder: &daw_proto::Track, spans: &[Span]) -> Vec<Item> {
    spans
        .iter()
        .enumerate()
        .map(|(index, span)| Item {
            guid: guid_of(&folder.guid, index),
            track_guid: folder.guid.clone(),
            index: u32::try_from(index).unwrap_or(u32::MAX),
            position: daw_proto::PositionInSeconds::from_seconds(span.start),
            length: daw_proto::Duration::from_seconds(span.length()),
            // The folder's own colour and the folder's own name, because
            // the row IS the folder: its fader is what moves and its
            // name is what is written beside it. Taking a child's would
            // say the row belonged to one of the mics — a kick whose
            // takes are labelled `In 1` is a kick that looks like its
            // inside mic.
            color: folder.color,
            label: Some(format!("{} {}", folder.name, index.saturating_add(1))),
            ..Item::default()
        })
        .collect()
}

/// The guid a folded item gets.
#[must_use]
pub fn guid_of(folder_guid: &str, index: usize) -> String {
    format!("folded:{folder_guid}:{index}")
}

/// The folder a folded guid came from, and which of its spans.
///
/// The way back. An edit lands on a row, the row hands over a guid, and
/// this says which fold it belongs to — from there [`Span::from`] says
/// which real items have to move.
#[must_use]
pub fn parse_guid(guid: &str) -> Option<(&str, usize)> {
    let rest = guid.strip_prefix("folded:")?;
    let (folder, index) = rest.rsplit_once(':')?;
    Some((folder, index.parse().ok()?))
}

/// Work out every shut folder's fold and put it on the project.
///
/// The one place a window does this, so the painted arrangement and the
/// component tree cannot disagree about what a shut folder shows.
///
/// `shown` is the row list the scene left — a folder is shut when it is
/// on screen and nothing under it is. `on` is
/// `Settings::folded_takes`: off, the folds are cleared and a shut
/// folder is an empty row, which is what REAPER draws.
///
/// Replaces rather than merges: a fold is derived from the session and
/// the view, and keeping a stale one would draw a take where the session
/// no longer has one.
pub fn refold(project: &mut super::project::Project, shown: &[daw_proto::Track], on: bool) {
    refold_lanes(project, shown, on, &|_, _| false);
}

/// [`refold`], with the shut folders `laned` names (asked with every
/// track, and the folder) folded lane by lane: each lane of the tracks
/// under it that have lanes folded on its own, its items on that lane
/// (`fixed_lane`) — every take beside the comp, to look at, hear and comp
/// from, rather than only what plays.
pub fn refold_lanes(
    project: &mut super::project::Project,
    shown: &[daw_proto::Track],
    on: bool,
    laned: &dyn Fn(&[daw_proto::Track], &daw_proto::Track) -> bool,
) {
    project.folds.clear();
    if !on {
        return;
    }
    let tracks = std::mem::take(&mut project.tracks);
    for folder in shut(&tracks, shown) {
        if laned(&tracks, folder) {
            if let Some(fold) = fold_lanes(project, &tracks, folder) {
                project.folds.insert(folder.guid.clone(), fold);
            }
            continue;
        }
        // What each child plays: a take on a lane that does not play is
        // an alternative, not part of the folder's sound.
        let playing: Vec<Vec<Item>> = under(&tracks, &folder.guid)
            .iter()
            .map(|child| {
                project
                    .lane(&child.guid)
                    .iter()
                    .filter(|item| super::project::on_playing_lane(child, item))
                    .cloned()
                    .collect()
            })
            .collect();
        let lanes: Vec<&[Item]> = playing.iter().map(Vec::as_slice).collect();
        let spans = spans(&lanes);
        if spans.is_empty() {
            continue;
        }
        let items = lane(folder, &spans);
        project
            .folds
            .insert(folder.guid.clone(), Fold { items, spans });
    }
    project.tracks = tracks;
}

/// A shut folder folded lane by lane: lane `n` of every track under it
/// with lanes (more than one) folded into the spans of the folder's lane
/// `n`. `None` when no lane holds anything.
fn fold_lanes(
    project: &super::project::Project,
    tracks: &[daw_proto::Track],
    folder: &daw_proto::Track,
) -> Option<Fold> {
    let children: Vec<&daw_proto::Track> = under(tracks, &folder.guid)
        .into_iter()
        .filter(|t| !t.is_folder && t.lane_count > 1)
        .collect();
    let count = children.iter().map(|t| t.lane_count).max()?.min(64);
    let mut laned: Vec<(u32, Span)> = Vec::new();
    for at in 0..count {
        let on_lane: Vec<Vec<Item>> = children
            .iter()
            .map(|child| {
                project
                    .lane(&child.guid)
                    .iter()
                    .filter(|item| item.fixed_lane.unwrap_or(0) == at)
                    .cloned()
                    .collect()
            })
            .collect();
        let slices: Vec<&[Item]> = on_lane.iter().map(Vec::as_slice).collect();
        laned.extend(spans(&slices).into_iter().map(|span| (at, span)));
    }
    (!laned.is_empty()).then(|| laned_fold(folder, laned))
}

/// Spans, each on its lane, as a fold: [`lane`]'s items with their lane
/// set, so a row showing lanes puts each in its lane's strip.
fn laned_fold(folder: &daw_proto::Track, laned: Vec<(u32, Span)>) -> Fold {
    let (lanes, spans): (Vec<u32>, Vec<Span>) = laned.into_iter().unzip();
    let mut items = lane(folder, &spans);
    for (item, at) in items.iter_mut().zip(lanes) {
        item.fixed_lane = Some(at);
    }
    Fold { items, spans }
}

/// `spans` with the seams joined.
///
/// Spans that touch or overlap — a cut, a crossfade — are one, so a take
/// edited into slices reads as the take it still is. A gap between them,
/// where nothing sounds, still parts them.
///
/// What a folder row shows when the edits are not what is being looked
/// at (mixing, playing live). A joined span is whole only when it was
/// one span already: its children's edges are not its own.
#[must_use]
pub fn join(spans: &[Span]) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    for span in spans {
        match out.last_mut() {
            Some(last) if span.start <= last.end + GRAIN => {
                last.end = last.end.max(span.end);
                for guid in &span.from {
                    if !last.from.contains(guid) {
                        last.from.push(guid.clone());
                    }
                }
                last.whole = false;
            }
            _ => out.push(span.clone()),
        }
    }
    out
}

/// Every fold on `project` with its seams joined ([`join`]): after
/// [`refold`], for a window not showing the cuts.
pub fn join_seams(project: &mut super::project::Project) {
    let folders: Vec<daw_proto::Track> = project
        .tracks
        .iter()
        .filter(|t| project.folds.contains_key(&t.guid))
        .cloned()
        .collect();
    for folder in folders {
        if let Some(fold) = project.folds.get_mut(&folder.guid) {
            // Folded lane by lane: each lane's seams joined on their own,
            // a take never run into the comp under it.
            if fold.items.iter().any(|item| item.fixed_lane.is_some()) {
                let mut by_lane: std::collections::BTreeMap<u32, Vec<Span>> =
                    std::collections::BTreeMap::new();
                for (item, span) in fold.items.iter().zip(&fold.spans) {
                    by_lane
                        .entry(item.fixed_lane.unwrap_or(0))
                        .or_default()
                        .push(span.clone());
                }
                let laned = by_lane
                    .into_iter()
                    .flat_map(|(at, spans)| join(&spans).into_iter().map(move |span| (at, span)))
                    .collect();
                *fold = laned_fold(&folder, laned);
                continue;
            }
            let spans = join(&fold.spans);
            fold.items = lane(&folder, &spans);
            fold.spans = spans;
        }
    }
}

/// Every collapsed folder's folded lane, by folder guid.
///
/// `descendants` answers which tracks are under a folder — the caller's
/// job, because it is the caller that knows the tree and the collapse
/// state, and this module is meant to be testable with items written by
/// hand.
#[must_use]
pub fn lanes<'a>(
    folders: impl IntoIterator<Item = &'a daw_proto::Track>,
    descendants: &dyn Fn(&str) -> Vec<&'a [Item]>,
) -> HashMap<String, Vec<Item>> {
    folders
        .into_iter()
        .filter_map(|folder| {
            let children = descendants(&folder.guid);
            let spans = spans(&children);
            (!spans.is_empty()).then(|| (folder.guid.clone(), lane(folder, &spans)))
        })
        .collect()
}

/// One folded row: the boxes it shows, and what they were folded from.
///
/// Held together because they are two views of one answer and a window
/// that had one without the other would either draw items it could not
/// resolve or resolve items it did not draw.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Fold {
    /// What the row shows — real [`Item`]s, so everything that draws a
    /// lane draws these the same way.
    pub items: Vec<Item>,
    /// What each of them was folded from, in the same order.
    pub spans: Vec<Span>,
}

/// What an edit aimed at an item on a folded row really is.
#[derive(Clone, PartialEq, Debug)]
pub enum Spread {
    /// Not a folded item at all. The edit means what it says.
    Direct,
    /// A folded item. The edit belongs to these real items instead —
    /// every one of them, as one step, because on screen they are one
    /// thing and a half-applied edit would be a lie about the take.
    Children(Vec<String>),
    /// A folded item the edit cannot honestly be applied to, and why.
    Refused(&'static str),
}

/// Where an edit aimed at `guid` should actually land.
///
/// `destructive` is whether the edit CHANGES the items — a move, a trim,
/// a split, a delete — as against merely selecting them. The distinction
/// is the whole of the ragged case: selecting every mic under a fragment
/// is unambiguous and harmless, and moving them is neither.
///
/// # Why a fragment is refused
///
/// A span with [`Span::whole`] false exists because the children
/// disagree — one mic was punched in, or a trigger lane was trimmed on
/// its own. Its edges are not any child's edges. Dragging it would have
/// to mean one of three things and there is no way to tell which:
/// move every child (which moves audio outside the fragment the hand
/// took hold of), move only the children that start there (which tears
/// the take apart), or move nothing and pretend.
///
/// So it is refused, with a sentence saying so. A fragment is EVIDENCE
/// of an edit somebody already made, and the honest answer is to open
/// the folder and work on the mic that disagrees.
#[must_use]
pub fn spread(
    folds: &std::collections::HashMap<String, Fold>,
    guid: &str,
    destructive: bool,
) -> Spread {
    let Some((folder, index)) = parse_guid(guid) else {
        return Spread::Direct;
    };
    let Some(span) = folds.get(folder).and_then(|fold| fold.spans.get(index)) else {
        // A folded guid whose fold is gone: the view changed under the
        // gesture. Refusing beats guessing — the alternative is writing
        // an edit addressed to nothing.
        return Spread::Refused("that row was re-folded while you were working on it");
    };
    if destructive && !span.whole {
        return Spread::Refused(
            "the takes under this row do not line up here, so there is no one edit to make — \
             open the folder and work on the track that differs",
        );
    }
    Spread::Children(span.from.clone())
}

/// Every track under `folder`, however deep.
#[must_use]
pub fn under<'a>(tracks: &'a [daw_proto::Track], folder: &str) -> Vec<&'a daw_proto::Track> {
    // By parent links rather than by the depth deltas beside them: a
    // session that has been dragged about has correct parents and
    // occasionally a stale depth, and a wrong answer here puts another
    // drum's take on this drum's row.
    let mut inside: std::collections::HashSet<&str> = std::collections::HashSet::new();
    inside.insert(folder);
    let mut out = Vec::new();
    // One pass in project order works because a child never precedes
    // its parent in a session file.
    for track in tracks {
        let parent = track.parent_guid.as_deref();
        if parent.is_some_and(|p| inside.contains(p)) {
            inside.insert(&track.guid);
            out.push(track);
        }
    }
    out
}

/// The folders on screen whose contents are not.
///
/// A scene that collapses a folder drops its rows and leaves the folder
/// itself, which is how REAPER ends up drawing an empty lane where the
/// kit used to be. These are the rows that want filling.
#[must_use]
pub fn shut<'a>(
    tracks: &'a [daw_proto::Track],
    shown: &[daw_proto::Track],
) -> Vec<&'a daw_proto::Track> {
    let on_screen: std::collections::HashSet<&str> =
        shown.iter().map(|t| t.guid.as_str()).collect();
    shown
        .iter()
        .filter(|track| track.is_folder)
        .filter_map(|track| tracks.iter().find(|t| t.guid == track.guid))
        .filter(|folder| {
            let inside = under(tracks, &folder.guid);
            // A folder whose every child is hidden (a guitar's `Main` with
            // its DI nested under it and hidden) is not shut: it is a track,
            // its own items drawn, not a fold of what nobody is shown.
            inside.iter().any(|t| t.visible_in_tcp)
                && !inside.iter().any(|t| on_screen.contains(t.guid.as_str()))
        })
        .collect()
}

/// Fold several children's peaks into the one the folder's row shows.
///
/// The loudest of them at each point, not the mean. A mean of three
/// mics of one drum is quieter than any of them, which draws a kit as
/// though it had been recorded further away the more microphones were
/// on it; the maximum is never narrower than any child and is what the
/// arrangement's own folder items settled on for the same reason.
///
/// Resampled onto the longest child's grid, because the children are one
/// performance and their peak lists are the same performance at whatever
/// resolution each was read at.
#[must_use]
pub fn wave(children: &[&[f32]]) -> Vec<f32> {
    let width = children.iter().map(|c| c.len()).max().unwrap_or(0);
    if width == 0 {
        return Vec::new();
    }
    (0..width)
        .map(|i| {
            children
                .iter()
                .filter(|child| !child.is_empty())
                .map(|child| {
                    // Where this bin falls in a child of a different
                    // length: the same fraction of the performance.
                    let at = i
                        .saturating_mul(child.len())
                        .checked_div(width)
                        .unwrap_or(0)
                        .min(child.len().saturating_sub(1));
                    child.get(at).copied().unwrap_or(0.0)
                })
                .fold(0.0_f32, f32::max)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Span, lane, parse_guid, spans};
    use daw_proto::{Duration, Item, PositionInSeconds};

    fn item(guid: &str, at: f64, len: f64) -> Item {
        Item {
            guid: guid.to_owned(),
            position: PositionInSeconds::from_seconds(at),
            length: Duration::from_seconds(len),
            ..Item::default()
        }
    }

    /// A take slip-edited into slices, crossfaded at each seam, is one
    /// take again with the seams joined — and a real gap still parts it.
    #[test]
    fn joined_seams_read_as_the_take() {
        let slices = [
            item("a", 0.0, 1.005),
            item("b", 1.0, 1.0),
            item("c", 2.0, 0.5),
            item("d", 3.0, 1.0),
        ];
        let cut = spans(&[&slices]);
        assert!(cut.len() >= 4, "every slice shows: {cut:?}");
        let joined = super::join(&cut);
        let edges: Vec<(f64, f64)> = joined.iter().map(|s| (s.start, s.end)).collect();
        assert_eq!(edges, [(0.0, 2.5), (3.0, 4.0)]);
        assert_eq!(joined[0].from, ["a", "b", "c"]);
        assert!(!joined[0].whole, "its children's edges are not its own");
        assert!(joined[1].whole, "a span alone is as whole as it was");
    }

    /// A guitar's `Main` with its DI nested under it and hidden is a track,
    /// not a shut folder; a folder with a child that could show is shut
    /// when the child is off screen.
    #[test]
    fn a_folder_of_only_hidden_tracks_is_not_shut() {
        let track =
            |guid: &str, parent: Option<&str>, folder: bool, visible: bool| daw_proto::Track {
                guid: guid.to_owned(),
                parent_guid: parent.map(str::to_owned),
                is_folder: folder,
                visible_in_tcp: visible,
                ..daw_proto::Track::default()
            };
        let tracks = vec![
            track("main", None, true, true),
            track("di", Some("main"), false, false),
            track("kick", None, true, true),
            track("in", Some("kick"), false, true),
        ];
        let shown = vec![tracks[0].clone(), tracks[2].clone()];
        let shut: Vec<&str> = super::shut(&tracks, &shown)
            .iter()
            .map(|t| t.guid.as_str())
            .collect();
        assert_eq!(shut, ["kick"]);
    }

    /// The case the feature exists for: three mics of one drum, recorded
    /// and comped together, so their boundaries are the same boundaries.
    /// The row is the take — two items, not six, and no edges invented
    /// between them.
    #[test]
    fn mics_that_agree_fold_to_the_take() {
        let top = [item("top1", 0.0, 4.0), item("top2", 8.0, 4.0)];
        let bottom = [item("bot1", 0.0, 4.0), item("bot2", 8.0, 4.0)];
        let trig = [item("trg1", 0.0, 4.0), item("trg2", 8.0, 4.0)];
        let folded = spans(&[&top[..], &bottom[..], &trig[..]]);
        assert_eq!(folded.len(), 2, "{folded:?}");
        assert!((folded[0].start - 0.0).abs() < 1e-9);
        assert!((folded[0].end - 4.0).abs() < 1e-9);
        assert!((folded[1].start - 8.0).abs() < 1e-9);
        assert!(folded.iter().all(|s| s.whole), "a clean take reads ragged");
        assert_eq!(folded[0].from.len(), 3, "the row lost a mic");
    }

    /// The gap between two takes stays a gap. Nothing is sounding there,
    /// so nothing is drawn there — a folded row that bridged its takes
    /// would claim audio the session does not have.
    #[test]
    fn a_gap_between_takes_is_not_filled_in() {
        let one = [item("a", 0.0, 4.0), item("b", 8.0, 4.0)];
        let folded = spans(&[&one[..]]);
        assert_eq!(folded.len(), 2);
        assert!((folded[0].end - 4.0).abs() < 1e-9);
        assert!((folded[1].start - 8.0).abs() < 1e-9);
    }

    /// A mic punched in on its own splits the row where it starts, and
    /// says which part of the span is the clean take and which is the
    /// punch. Three spans out of two items, because that is the edit
    /// that is actually there.
    #[test]
    fn a_punch_on_one_mic_shows_as_its_own_span() {
        let top = [item("top", 0.0, 8.0)];
        let punch = [item("punch", 2.0, 4.0)];
        let folded = spans(&[&top[..], &punch[..]]);
        assert_eq!(folded.len(), 3, "{folded:?}");
        assert!(!folded[0].whole, "a fragment claimed to be a take");
        assert_eq!(folded[0].from, ["top"]);
        assert_eq!(folded[1].from, ["top", "punch"]);
        assert_eq!(folded[2].from, ["top"]);
        assert!((folded[1].start - 2.0).abs() < 1e-9);
        assert!((folded[1].end - 6.0).abs() < 1e-9);
    }

    /// Boundaries a micron apart are boundaries that agree. Whatever
    /// wrote the file rounded; nobody made that edit, and a span a
    /// micron wide between two mics is not a thing to draw.
    #[test]
    fn a_rounding_is_not_an_edit() {
        let a = [item("a", 0.0, 4.0)];
        let b = [item("b", 0.000_02, 3.999_97)];
        let folded = spans(&[&a[..], &b[..]]);
        assert_eq!(folded.len(), 1, "{folded:?}");
        assert!(folded[0].whole);
    }

    /// A folder with nothing under it folds to nothing rather than to
    /// one empty item.
    #[test]
    fn an_empty_folder_folds_to_nothing() {
        assert!(spans(&[]).is_empty());
        assert!(spans(&[&[][..], &[][..]]).is_empty());
    }

    /// A zero-length item is not a span. It has no middle to be inside
    /// of, and a row of them would be a row of invisible items that
    /// still hit-test.
    #[test]
    fn a_zero_length_item_is_not_a_span() {
        let none = [item("z", 4.0, 0.0)];
        assert!(spans(&[&none[..]]).is_empty());
    }

    /// The loudest mic, not the average of them. Three mics of one drum
    /// averaged are quieter than any of them, which would draw a kit as
    /// though more microphones had moved it further away.
    #[test]
    fn a_folded_wave_takes_the_loudest_mic() {
        let quiet = [0.1_f32, 0.2, 0.1];
        let loud = [0.9_f32, 0.1, 0.4];
        assert_eq!(super::wave(&[&quiet[..], &loud[..]]), [0.9, 0.2, 0.4]);
    }

    /// Children read at different resolutions are the same performance,
    /// so the shorter one is stretched onto the longer one's grid rather
    /// than truncating it.
    #[test]
    fn a_shorter_peak_list_is_stretched_not_cut() {
        let coarse = [1.0_f32, 0.0];
        let fine = [0.0_f32, 0.0, 0.0, 0.5];
        let folded = super::wave(&[&coarse[..], &fine[..]]);
        assert_eq!(folded.len(), 4, "{folded:?}");
        assert!((folded[0] - 1.0).abs() < f32::EPSILON);
        assert!((folded[3] - 0.5).abs() < f32::EPSILON);
    }

    /// Nothing to fold folds to nothing, rather than to a row of silence
    /// that still draws a flat line.
    #[test]
    fn nothing_folds_to_nothing() {
        assert!(super::wave(&[]).is_empty());
        assert!(super::wave(&[&[][..]]).is_empty());
    }

    fn folds() -> std::collections::HashMap<String, super::Fold> {
        let spans = vec![
            Span {
                start: 0.0,
                end: 4.0,
                from: vec!["in".to_owned(), "out".to_owned()],
                whole: true,
            },
            Span {
                start: 4.0,
                end: 6.0,
                from: vec!["out".to_owned()],
                whole: false,
            },
        ];
        let folder = daw_proto::Track {
            guid: "kick".to_owned(),
            name: "Kick".to_owned(),
            ..daw_proto::Track::default()
        };
        let mut out = std::collections::HashMap::new();
        out.insert(
            "kick".to_owned(),
            super::Fold {
                items: lane(&folder, &spans),
                spans,
            },
        );
        out
    }

    /// The ordinary case: an edit on the row is an edit on every mic
    /// under it, which is the whole reason the row is worth having.
    #[test]
    fn an_edit_on_a_clean_take_reaches_every_mic() {
        assert_eq!(
            super::spread(&folds(), &super::guid_of("kick", 0), true),
            super::Spread::Children(vec!["in".to_owned(), "out".to_owned()])
        );
    }

    /// An item that is not folded is an item, and nothing happens to it.
    #[test]
    fn a_real_item_passes_straight_through() {
        assert_eq!(
            super::spread(&folds(), "some-real-guid", true),
            super::Spread::Direct
        );
    }

    /// A fragment has no edges of its own, so there is no one edit to
    /// make on it. Refused rather than guessed at.
    #[test]
    fn a_fragment_refuses_an_edit_that_would_change_it() {
        let at = super::guid_of("kick", 1);
        assert!(matches!(
            super::spread(&folds(), &at, true),
            super::Spread::Refused(_)
        ));
    }

    /// But selecting one is unambiguous — every mic sounding there —
    /// so it is allowed. Refusing a click would make the row feel
    /// broken rather than careful.
    #[test]
    fn a_fragment_can_still_be_selected() {
        assert_eq!(
            super::spread(&folds(), &super::guid_of("kick", 1), false),
            super::Spread::Children(vec!["out".to_owned()])
        );
    }

    /// A fold that went away under the gesture is refused rather than
    /// applied to whatever is at that index now.
    #[test]
    fn a_stale_fold_is_refused_not_guessed() {
        assert!(matches!(
            super::spread(&folds(), &super::guid_of("kick", 9), false),
            super::Spread::Refused(_)
        ));
        assert!(matches!(
            super::spread(&folds(), &super::guid_of("gone", 0), false),
            super::Spread::Refused(_)
        ));
    }

    /// The guid a folded item gets leads back to the fold it came from,
    /// which is how an edit on the row reaches the mics under it.
    #[test]
    fn a_folded_guid_says_where_it_came_from() {
        let folder = daw_proto::Track {
            guid: "kick".to_owned(),
            name: "Kick".to_owned(),
            ..daw_proto::Track::default()
        };
        let made = lane(
            &folder,
            &[Span {
                start: 1.0,
                end: 3.0,
                from: vec!["top".to_owned()],
                whole: true,
            }],
        );
        assert_eq!(made.len(), 1);
        assert_eq!(made[0].track_guid, "kick");
        assert_eq!(
            made[0].label.as_deref(),
            Some("Kick 1"),
            "the row lost its name"
        );
        assert!((made[0].length.as_seconds() - 2.0).abs() < 1e-9);
        assert_eq!(parse_guid(&made[0].guid), Some(("kick", 0)));
        assert_eq!(parse_guid("not-folded"), None);
    }
}

#[cfg(test)]
mod lane_fold_tests {
    use daw_proto::{Duration, Item, PositionInSeconds, Track};

    fn item(guid: &str, track: &str, lane: u32, at: f64, length: f64) -> Item {
        Item {
            guid: guid.to_owned(),
            track_guid: track.to_owned(),
            position: PositionInSeconds::from_seconds(at),
            length: Duration::from_seconds(length),
            fixed_lane: Some(lane),
            ..Item::default()
        }
    }

    /// A shut kit whose lanes are open is folded lane by lane — the comp on
    /// lane 0, the take on lane 1 — and joining its seams keeps the lanes
    /// apart; asked for none, it folds what plays, as it always did.
    #[test]
    fn a_shut_folder_shows_each_lane_folded_on_its_own() {
        let mic = |guid: &str| Track {
            guid: guid.to_owned(),
            parent_guid: Some("kit".to_owned()),
            visible_in_tcp: true,
            lane_count: 2,
            lane_play_mask: 0b1,
            ..Track::default()
        };
        let folder = Track {
            guid: "kit".to_owned(),
            name: "Kit".to_owned(),
            is_folder: true,
            ..Track::default()
        };
        let mut project = super::super::project::Project {
            tracks: vec![folder.clone(), mic("in"), mic("out")],
            ..Default::default()
        };
        for mic in ["in", "out"] {
            project.items.insert(
                mic.to_owned(),
                vec![
                    item(&format!("{mic}-c1"), mic, 0, 0.0, 2.0),
                    item(&format!("{mic}-c2"), mic, 0, 2.0, 3.0),
                    item(&format!("{mic}-t"), mic, 1, 0.0, 5.0),
                ],
            );
        }
        let shown = vec![folder];
        super::refold_lanes(&mut project, &shown, true, &|_, f| f.guid == "kit");
        let fold = project.folds.get("kit").expect("folded");
        let on = |lane: u32| {
            fold.items
                .iter()
                .filter(|i| i.fixed_lane == Some(lane))
                .count()
        };
        assert_eq!((on(0), on(1)), (2, 1), "the comp's two pieces, the take whole");
        assert_eq!(fold.items.len(), fold.spans.len());
        let take = fold
            .items
            .iter()
            .zip(&fold.spans)
            .find(|(i, _)| i.fixed_lane == Some(1))
            .map(|(_, span)| span.from.clone());
        assert_eq!(take, Some(vec!["in-t".to_owned(), "out-t".to_owned()]));

        super::join_seams(&mut project);
        let fold = project.folds.get("kit").expect("still folded");
        let on = |lane: u32| {
            fold.items
                .iter()
                .filter(|i| i.fixed_lane == Some(lane))
                .count()
        };
        assert_eq!((on(0), on(1)), (1, 1), "the comp's seam joined, the take its own");

        super::refold(&mut project, &shown, true);
        let fold = project.folds.get("kit").expect("folded again");
        assert!(
            fold.items.iter().all(|i| i.fixed_lane.is_none()),
            "only what plays, as one row"
        );
        assert_eq!(fold.items.len(), 2);
    }
}
