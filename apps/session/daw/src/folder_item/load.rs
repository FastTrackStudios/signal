//! Building folders out of a session: the tree, the roles, the peaks.
//!
//! The adapter on the input side of the fold's seam. Everything past
//! this module works on [`Folder`]s and knows nothing about a facade, a
//! project snapshot or an RPC — which is what lets the fold and the
//! picture be tested with peaks written by hand.

use std::collections::HashMap;

use daw_proto::Track;
use daw_ui::studio::project::Project;
use expression_editor_core::kit::kit_role;
use vello::peniko::Color;

use super::Folder;
use super::fold::{Child, ChildTake, GroupBy, Placement, Side, TakePeaks};

/// Samples per peak asked of the backend.
///
/// The `.reapeaks` finest level's own ratio. Nothing draws finer than
/// this — REAPER does not either — so asking for less buys resolution no
/// picture can show and costs a scan of the PCM.
pub const BLOCK_SIZE: u32 = 160;

/// A declared peak grid this far from the take's real one is off-rate:
/// the source runs at a rate the project does not, and the backend
/// served it on the project's grid (daw#10).
pub const DRIFT_TOLERANCE: f64 = 0.01;

/// What one load found, beside the folders themselves.
///
/// The wide event's fields. Carried out rather than logged from inside
/// so that the load is one event and not one per child.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Loaded {
    pub folders: usize,
    pub children: usize,
    /// The most take lanes any one child had.
    pub takes: usize,
    /// How many items were read, across every child and every take.
    pub items: usize,
    /// Children whose declared peak grid disagrees with the one their
    /// peaks came back on — an off-rate source.
    pub off_rate: usize,
    /// The worst disagreement seen, as a fraction.
    pub worst_drift: f64,
}

/// Every track a folder's item is a view of: the tracks under it, and
/// the track itself when it carries items of its own.
///
/// The second case is not an edge case. `flow.guitars.folder-items` says
/// a channel's folder item sums its sources — and a channel whose
/// sources have not been split out yet is one stereo track, which sums
/// itself. That is what a double is in a session that has not grown one
/// (`flow.scenes.reaper-model`: a pair is ONE stereo track, not a folder
/// over an L and an R).
fn sources<'a>(tracks: &'a [Track], folder: &str) -> Vec<&'a Track> {
    let mut inside: Vec<&str> = vec![folder];
    let mut out = Vec::new();
    for track in tracks {
        if track.guid == folder {
            out.push(track);
            continue;
        }
        let Some(parent) = track.parent_guid.as_deref() else {
            continue;
        };
        if inside.contains(&parent) {
            inside.push(&track.guid);
            out.push(track);
        }
    }
    out
}

/// The enclosing folder names of `track`, nearest first — what
/// [`kit_role`] reads to decide a mic's role from where it sits rather
/// than from what it is called.
fn lineage(by_guid: &HashMap<&str, &Track>, track: &Track) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = track.parent_guid.clone();
    while let Some(guid) = at {
        let Some(parent) = by_guid.get(guid.as_str()) else {
            break;
        };
        out.push(parent.name.clone());
        at = parent.parent_guid.clone();
    }
    out
}

/// The side a track sits on, when its name puts it on one.
///
/// L/R is the **Channel** dimension (`GuitarGrowActions::double`): a
/// Channel shares a chain, a Layer has its own. A track literally called
/// `L` or `R` is one half of a split pair and is read the same way.
fn side_of(name: &str) -> Option<Side> {
    if let Some(side) = Side::parse(name) {
        return Some(side);
    }
    // `Rhythm L`, `GTR-R`, `Amp R 57` — the channel token is a word of
    // the name, not the whole of it.
    name.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .find_map(Side::parse)
}

/// The colour a folder's dim envelope and its sides are drawn in.
fn colour_of(track: &Track, fallback: Color) -> Color {
    track.color.map_or(fallback, |rgb| {
        Color::from_rgba8(
            u8::try_from((rgb >> 16) & 0xff).unwrap_or(0),
            u8::try_from((rgb >> 8) & 0xff).unwrap_or(0),
            u8::try_from(rgb & 0xff).unwrap_or(0),
            0xff,
        )
    })
}

/// Read every folder named in `wanted` out of `snapshot`, fetching each
/// child's peaks through the facade.
///
/// A folder with no audible descendant is skipped rather than returned
/// empty: a Process folder or a bus tree has nothing to fold, and a
/// folder item over silence is a rectangle that says something is there.
///
/// # Errors
///
/// None: an item whose peaks cannot be read contributes an empty take,
/// because a folder row with one unreadable child is still a folder row
/// worth drawing.
///
/// r[impl flow.drums.comping.folder-items]
/// r[impl flow.guitars.folder-items]
pub async fn load(
    daw: &daw_control::Project,
    snapshot: &Project,
    wanted: &[(String, GroupBy)],
    fallback: Color,
) -> (Vec<Folder>, Loaded) {
    load_at(daw, snapshot, wanted, fallback, BLOCK_SIZE).await
}

/// Samples per peak a quantize reads the kit at: where a hit is matters
/// to well under a peak block (160 samples is 3.6 ms at 44.1 kHz, 14.5 ms
/// on an 11 kHz source), and the read is one take, once.
pub const DETECT_BLOCK_SIZE: u32 = 32;

/// [`load`] with `block` samples per peak.
pub async fn load_at(
    daw: &daw_control::Project,
    snapshot: &Project,
    wanted: &[(String, GroupBy)],
    fallback: Color,
    block: u32,
) -> (Vec<Folder>, Loaded) {
    let by_guid: HashMap<&str, &Track> = snapshot
        .tracks
        .iter()
        .map(|t| (t.guid.as_str(), t))
        .collect();
    let mut folders = Vec::new();
    let mut found = Loaded::default();
    for (guid, group_by) in wanted {
        let Some(folder) = by_guid.get(guid.as_str()) else {
            continue;
        };
        let mut children = Vec::new();
        let (mut start, mut end) = (f64::INFINITY, f64::NEG_INFINITY);
        for track in sources(&snapshot.tracks, guid) {
            let lane = snapshot.lane(&track.guid);
            if lane.is_empty() {
                continue;
            }
            let folders_nearest_first = lineage(&by_guid, track);
            let role = kit_role(
                &track.name,
                &folders_nearest_first
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            );
            // A take is a LANE, not an item: a track with REAPER 7 fixed
            // lanes has one take per lane, and a track without them has
            // one take holding every item on it — a verse, a chorus and
            // a fill are one pass, not three takes
            // (`flow.drums.comping.lanes`).
            let mut takes: Vec<ChildTake> = Vec::new();
            let mut ordered: Vec<&daw_proto::Item> = lane.iter().collect();
            ordered.sort_by(|a, b| {
                a.fixed_lane
                    .unwrap_or(0)
                    .cmp(&b.fixed_lane.unwrap_or(0))
                    .then(a.index.cmp(&b.index))
            });
            for item in ordered {
                let at = usize::try_from(item.fixed_lane.unwrap_or(0)).unwrap_or(0);
                if takes.len() <= at {
                    takes.resize_with(at.saturating_add(1), ChildTake::default);
                }
                let Some(into) = takes.get_mut(at) else {
                    continue;
                };
                let start_secs = item.position.as_seconds();
                let length_secs = item.length.as_seconds();
                let peaks = read_peaks(daw, &item.guid, block).await;
                let drift = peaks.drift(length_secs);
                if drift > DRIFT_TOLERANCE {
                    found.off_rate = found.off_rate.saturating_add(1);
                    found.worst_drift = found.worst_drift.max(drift);
                }
                start = start.min(start_secs);
                end = end.max(start_secs + length_secs);
                found.items = found.items.saturating_add(1);
                into.placements.push(Placement {
                    start_secs,
                    length_secs,
                    peaks,
                });
            }
            found.takes = found.takes.max(takes.len());
            children.push(Child {
                guid: track.guid.clone(),
                name: track.name.clone(),
                role,
                side: side_of(&track.name),
                muted: track.muted,
                hidden: !track.visible_in_tcp,
                takes,
            });
        }
        if children.is_empty() || !start.is_finite() || end <= start {
            continue;
        }
        found.children = found.children.saturating_add(children.len());
        let take_count = children.iter().map(|c| c.takes.len()).max().unwrap_or(0);
        folders.push(Folder {
            guid: folder.guid.clone(),
            name: folder.name.clone(),
            colour: colour_of(folder, fallback),
            group_by: *group_by,
            children,
            start_secs: start,
            length_secs: end - start,
            take_count,
        });
    }
    found.folders = folders.len();
    // One wide event for the whole load: how much was folded, and
    // whether any source came back on a grid other than its own. A line
    // per child would say the same thing a hundred times and be
    // unqueryable.
    tracing::info!(
        folder_item.folders = found.folders,
        folder_item.children = found.children,
        folder_item.takes = found.takes,
        folder_item.items = found.items,
        folder_item.off_rate_children = found.off_rate,
        folder_item.worst_grid_drift = found.worst_drift,
        folder_item.block_size = block,
        "folder items loaded"
    );
    (folders, found)
}

/// One item's active take's peaks, or an empty take.
async fn read_peaks(daw: &daw_control::Project, item_guid: &str, block: u32) -> TakePeaks {
    let Ok(Some(handle)) = daw.items().by_guid(item_guid).await else {
        return TakePeaks::empty();
    };
    let Ok(data) = handle.active_take().peaks(block).await else {
        return TakePeaks::empty();
    };
    TakePeaks::new(
        &data.peaks,
        data.num_channels,
        data.samples_per_peak,
        data.sample_rate,
    )
}
