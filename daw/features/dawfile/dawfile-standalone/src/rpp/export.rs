//! `.session` → `.rpp`.
//!
//! Export is a **patch**, not a regeneration. The original project text is
//! re-parsed and only the values the document actually changed are written
//! over; every construct the schema never modelled is still in the tree and
//! goes out untouched. This is the whole reason the round trip can be
//! proven rather than asserted, and it is why the corpus test can assert
//! that exporting an unedited document is a no-op down to the byte.

mod sources;

use super::*;
use crate::document::{DawDocument, EnvelopeNode, ItemNode, TakeNode, TrackNode};
use crate::error::{DawError, DawResult};
use crate::objects::ObjectStore;
use daw_proto::automation::EnvelopeShape;
use daw_proto::item::{FadeShape, Item};
use daw_proto::track::{LaneComping, LaneDisplay, Track};
use dawfile_reaper::rpp_tree::RToken;
use dawfile_reaper::types::track::{
    FixedLaneFields, comping_from_lines, comping_lines, lane_settings,
};

/// What an export changed.
///
/// Empty for an unedited document — which is the assertion the corpus test
/// leans on.
#[derive(Clone, Debug, Default)]
pub struct ExportReport {
    /// One line per rewritten token, in `<entity> <KEY>[n]: old → new`
    /// form. Useful in a failure message, and it is what a future
    /// `fts daw diff` would print.
    pub changes: Vec<String>,
}

impl ExportReport {
    /// Whether the export rewrote anything at all.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

/// Export a document back to REAPER project text.
///
/// Requires the document's verbatim source object, because that is what is
/// being patched. A document with no provenance cannot be exported this way
/// — creating an `.rpp` from nothing is a different job (project scaffolding
/// already lives in `dawfile_reaper::scaffold`).
pub fn to_rpp(document: &DawDocument, store: &ObjectStore) -> DawResult<String> {
    Ok(to_rpp_patched(document, store)?.0)
}

/// Export, and report what was rewritten.
pub fn to_rpp_patched(
    document: &DawDocument,
    store: &ObjectStore,
) -> DawResult<(String, ExportReport)> {
    let provenance = document
        .provenance
        .as_ref()
        .ok_or_else(|| DawError::Rpp("document has no .rpp provenance to export against".into()))?;

    let source = store.get(&provenance.source)?;
    let text = String::from_utf8_lossy(source).into_owned();
    let mut root = dawfile_reaper::rpp_tree::read_rpp_chunk(&text)
        .map_err(|error| DawError::Rpp(error.to_string()))?;

    let mut report = ExportReport::default();
    patch_project(&mut root, document, &mut report);
    // Everything below is project-scoped and has to see the final track
    // list: `AUXRECV` names its source by position, so a send cannot be
    // numbered until the adds and removes above have settled.
    patch_receives(&mut root, document, &mut report);
    patch_tempo_map(&mut root, document, &mut report);
    patch_markers(&mut root, document, &mut report);
    sources::write_missing_sources(&mut root, document, store, &mut report)?;

    let mut rendered =
        dawfile_reaper::rpp_tree::stringify_rpp_node(&RNodeTree::Chunk(root.clone()));
    if !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    Ok((rendered, report))
}

/// Walk the tree beside the document, matching **by id** at every level.
///
/// Nothing here indexes into a list to find its counterpart: a track is
/// found by its GUID, an item by its `IGUID`, an envelope by its chunk name
/// or `EGUID`. Reordering tracks in the editor therefore patches the right
/// chunks rather than scrambling them.
fn patch_project(root: &mut RChunk, document: &DawDocument, report: &mut ExportReport) {
    // A track chunk with no counterpart in the document was deleted in the
    // editor. Dropping the chunk is the *only* structural removal that is
    // safe to infer, and it has to happen — leaving it behind would mean an
    // export that quietly resurrects a deleted track.
    let mut removed = Vec::new();
    root.children.retain(|child| {
        let RNodeTree::Chunk(chunk) = child else {
            return true;
        };
        if chunk.name().as_deref() != Some("TRACK") {
            return true;
        }
        match track_guid(chunk) {
            Some(guid) if document.track_by_guid(&guid).is_none() => {
                removed.push(guid);
                false
            }
            _ => true,
        }
    });
    for guid in removed {
        report.changes.push(format!("track {guid}: removed"));
    }

    let mut present = Vec::new();
    let mut last_track_at = None;
    for (position, child) in root.children.iter().enumerate() {
        if let RNodeTree::Chunk(chunk) = child
            && chunk.name().as_deref() == Some("TRACK")
        {
            last_track_at = Some(position);
            if let Some(guid) = track_guid(chunk) {
                present.push(guid);
            }
        }
    }

    for position in 0..root.children.len() {
        let RNodeTree::Chunk(chunk) = &mut root.children[position] else {
            continue;
        };
        if chunk.name().as_deref() != Some("TRACK") {
            continue;
        }
        let Some(guid) = track_guid(chunk) else {
            continue;
        };
        let Some(track_node) = document.track_by_guid(&guid) else {
            continue;
        };
        patch_track(chunk, track_node, report);
    }

    // Tracks the editor added have no chunk yet. They go in after the last
    // existing track, in document order, so the arrangement reads the same
    // way in REAPER as it does in the editor.
    let mut insert_at = last_track_at
        .map(|position| position + 1)
        .unwrap_or(root.children.len());
    for track_node in &document.tracks {
        if present.iter().any(|guid| guid == track_node.id.as_str()) {
            continue;
        }
        report
            .changes
            .push(format!("track {}: added", track_node.id));
        root.children
            .insert(insert_at, RNodeTree::Chunk(build_track(track_node)));
        insert_at += 1;
    }

    reorder_tracks(root, document, report);
}

/// Put the `<TRACK>` chunks into the document's arrange order.
///
/// Arrange order is data — it is what the arrange view shows, and on a
/// `.rpp` it is also what the folder encoding means, since `ISBUS` says
/// "how much deeper is the next row". A track moved in the editor that
/// stayed put in the file would export a hierarchy that does not match
/// the one the document describes.
///
/// Only the track chunks move, and only into each other's slots: every
/// other project-level line stays exactly where it was. A no-op when the
/// orders already agree, so an unedited export stays byte-identical.
fn reorder_tracks(root: &mut RChunk, document: &DawDocument, report: &mut ExportReport) {
    let wanted: Vec<&str> = document
        .tracks
        .iter()
        .map(|node| node.id.as_str())
        .collect();
    let rank = |guid: &str| wanted.iter().position(|id| *id == guid);

    // Only chunks the document knows about take part. One it does not
    // recognise — a track with no readable GUID — keeps its slot rather
    // than being sorted to an arbitrary end.
    let slots: Vec<usize> = root
        .children
        .iter()
        .enumerate()
        .filter(|(_, child)| match child {
            RNodeTree::Chunk(chunk) if chunk.name().as_deref() == Some("TRACK") => {
                track_guid(chunk).and_then(|guid| rank(&guid)).is_some()
            }
            _ => false,
        })
        .map(|(position, _)| position)
        .collect();

    let current: Vec<usize> = slots
        .iter()
        .filter_map(|&position| match &root.children[position] {
            RNodeTree::Chunk(chunk) => track_guid(chunk).and_then(|guid| rank(&guid)),
            _ => None,
        })
        .collect();
    if current.windows(2).all(|pair| pair[0] < pair[1]) {
        return;
    }

    report.changes.push("track order: rearranged".to_string());
    let mut taken: Vec<(usize, RNodeTree)> = current
        .iter()
        .copied()
        .zip(
            slots
                .iter()
                .map(|&position| root.children[position].clone()),
        )
        .collect();
    taken.sort_by_key(|(rank, _)| *rank);
    for (&position, (_, chunk)) in slots.iter().zip(taken) {
        root.children[position] = chunk;
    }
}

// ── routing ────────────────────────────────────────────────────────────

/// Bring every track's `AUXRECV` lines in line with the document.
///
/// This is the one place the format's "never reference by position" rule
/// has to be un-done, because `.rpp` gives no alternative: `AUXRECV` names
/// its source by **track index**. Resolving ids to indices here — against
/// the track order actually being written, after adds and removes — is
/// what keeps a send pointing at the track it was drawn to rather than at
/// whatever now sits in that row.
fn patch_receives(root: &mut RChunk, document: &DawDocument, report: &mut ExportReport) {
    const RECEIVE_KEY: &str = "AUXRECV";
    // The track order of the file being written, which is what REAPER will
    // count when it reads the indices back.
    let order: Vec<String> = root
        .children
        .iter()
        .filter_map(|child| match child {
            RNodeTree::Chunk(chunk) if chunk.name().as_deref() == Some("TRACK") => {
                track_guid(chunk)
            }
            _ => None,
        })
        .collect();
    let index_of = |guid: &str| order.iter().position(|candidate| candidate == guid);

    for child in &mut root.children {
        let RNodeTree::Chunk(chunk) = child else {
            continue;
        };
        if chunk.name().as_deref() != Some("TRACK") {
            continue;
        }
        let Some(node) = track_guid(chunk).and_then(|guid| document.track_by_guid(&guid)) else {
            continue;
        };

        let wanted: Vec<Vec<String>> = node
            .receives
            .iter()
            .filter_map(|receive| {
                // A send whose source is not being written has no index to
                // point at. `check_invariants` calls that out; here the
                // honest thing is to drop the line rather than aim it at
                // an unrelated track.
                let source = index_of(receive.source.as_str())?;
                Some(vec![
                    RECEIVE_KEY.to_string(),
                    source.to_string(),
                    receive.send_mode.to_string(),
                    format_f64(receive.volume),
                    format_f64(receive.pan),
                    if receive.muted { "1" } else { "0" }.to_string(),
                    if receive.mono { "1" } else { "0" }.to_string(),
                    if receive.phase_inverted { "1" } else { "0" }.to_string(),
                    receive.source_channels.to_string(),
                    receive.dest_channels.to_string(),
                    format_f64(receive.pan_law),
                    receive.midi_channels.to_string(),
                    receive.automation_mode.to_string(),
                ])
            })
            .collect();

        let is_receive =
            |child: &RNodeTree| matches!(child, RNodeTree::Node(l) if key(l) == RECEIVE_KEY);
        let existing: Vec<Vec<String>> = chunk
            .children
            .iter()
            .filter_map(|child| match child {
                RNodeTree::Node(line) if is_receive(child) => Some(tokens(line)),
                _ => None,
            })
            .collect();

        // Compare by meaning, not by text. REAPER writes fields this
        // schema does not model — a pan law of `-1:U`, a trailing `''` —
        // and re-emitting a line that already says the right thing would
        // silently normalise them away on every save.
        let order_ids: Vec<crate::id::EntityId> =
            order.iter().map(crate::id::EntityId::adopt).collect();
        let decoded: Vec<crate::document::ReceiveNode> = existing
            .iter()
            .filter_map(|line| super::import::read_receive(line, &order_ids))
            .collect();
        if decoded == node.receives {
            continue;
        }
        if existing == wanted {
            continue;
        }
        report.changes.push(format!(
            "track {} receives: {} line(s) \u{2192} {} line(s)",
            node.id,
            existing.len(),
            wanted.len()
        ));

        // REAPER writes the receives last in the track's line run, after
        // `MAINSEND`, and always before the nested chunks.
        let insert_at = chunk
            .children
            .iter()
            .position(is_receive)
            .or_else(|| {
                chunk
                    .children
                    .iter()
                    .rposition(|child| matches!(child, RNodeTree::Node(l) if key(l) == "MAINSEND"))
                    .map(|position| position + 1)
            })
            .or_else(|| {
                chunk
                    .children
                    .iter()
                    .position(|child| matches!(child, RNodeTree::Chunk(_)))
            })
            .unwrap_or(chunk.children.len());
        chunk.children.retain(|child| !is_receive(child));
        for (offset, line) in wanted.iter().enumerate() {
            let refs: Vec<&str> = line.iter().map(String::as_str).collect();
            chunk.children.insert(insert_at + offset, node_line(&refs));
        }
    }
}

/// A track chunk's stable id: the header GUID, or `TRACKID` on projects old
/// enough not to carry one in the header.
fn track_guid(chunk: &RChunk) -> Option<String> {
    header_param(chunk, 1)
        .filter(|token| token.starts_with('{'))
        .or_else(|| child_node(chunk, "TRACKID").and_then(|node| param(node, 1)))
}

fn patch_track(chunk: &mut RChunk, node: &TrackNode, report: &mut ExportReport) {
    let label = format!("track {}", node.id);
    let track = &node.track;

    reconcile(
        chunk,
        "ITEM",
        |inner| child_node(inner, "IGUID").and_then(|line| param(line, 1)),
        &node.items,
        |item| item.id.as_str(),
        build_item,
        &label,
        "item",
        report,
    );
    reconcile_envelopes(chunk, &node.envelopes, &label, report);

    // Items and envelopes are patched by walking the children once and
    // dispatching, so a track with both keeps its original ordering.
    for child in &mut chunk.children {
        match child {
            RNodeTree::Node(line) => match key(line).as_str() {
                "NAME" => set_string(line, 1, &track.name, &label, "NAME", report),
                "VOLPAN" => {
                    set_number(line, 1, track.volume, &label, "VOLPAN", report);
                    set_number(line, 2, track.pan, &label, "VOLPAN", report);
                }
                "MUTESOLO" => {
                    set_bool(line, 1, track.muted, &label, "MUTESOLO", report);
                    set_bool(line, 2, track.soloed, &label, "MUTESOLO", report);
                }
                "IPHASE" => set_bool(line, 1, track.phase_inverted, &label, "IPHASE", report),
                "SEL" => set_bool(line, 1, track.selected, &label, "SEL", report),
                // The folder tree. `parent` is the document's truth and
                // `ISBUS` is its `.rpp` spelling — without this a track
                // re-parented in the editor exports back flat, which is
                // most of what a session's organisation pass does.
                //
                // Compared by meaning rather than by token: field 1 is
                // not a bool. REAPER also writes `2` there (the last
                // track inside a folder), and rewriting that as `0`
                // because the document says "not a folder" would be a
                // silent loss on a file nobody edited.
                "ISBUS" => {
                    let folder = param_i64(line, 1) == Some(1);
                    let depth = param_i64(line, 2).unwrap_or(0) as i32;
                    if (folder, depth) != (track.is_folder, track.folder_depth) {
                        set_bool(line, 1, track.is_folder, &label, "ISBUS", report);
                        set_number(
                            line,
                            2,
                            f64::from(track.folder_depth),
                            &label,
                            "ISBUS",
                            report,
                        );
                    }
                }
                "PEAKCOL" => {
                    if let Some(color) = track.color {
                        // REAPER writes PEAKCOL as a *signed* 32-bit
                        // value; the document stores the same bits
                        // unsigned (see the importer's `as u32`).
                        // Writing the unsigned reading back would flag
                        // every coloured track as changed and flip the
                        // token's text form.
                        set_number(line, 1, color as i32 as f64, &label, "PEAKCOL", report);
                    }
                }
                "REC" => set_bool(line, 1, track.armed, &label, "REC", report),
                _ => {}
            },
            RNodeTree::Chunk(inner) => {
                let inner_name = inner.name().unwrap_or_default();
                if inner_name == "ITEM" {
                    let iguid = child_node(inner, "IGUID").and_then(|line| param(line, 1));
                    if let Some(iguid) = iguid
                        && let Some(item_node) = node
                            .items
                            .iter()
                            .find(|candidate| candidate.id.as_str() == iguid)
                    {
                        patch_item(inner, item_node, report);
                        set_item_lane(inner, &item_node.item, track.lane_count, report);
                    }
                } else if is_envelope_chunk(&inner_name)
                    && let Some(envelope_node) = find_envelope(&node.envelopes, inner, &inner_name)
                {
                    patch_envelope(inner, envelope_node, &label, report);
                }
            }
        }
    }
    patch_lanes(chunk, node, &label, report);
    patch_group_flags(chunk, node, &label, report);
    patch_colour(chunk, node, &label, report);
}

/// Give a track a `PEAKCOL` line when the document coloured a track the
/// file never coloured.
///
/// The in-place case is handled by the `PEAKCOL` arm of [`patch_track`];
/// this is only the upsert, because a colour set on a track REAPER wrote
/// without one would otherwise have nowhere to go. REAPER writes the line
/// straight after `NAME`.
fn patch_colour(chunk: &mut RChunk, node: &TrackNode, label: &str, report: &mut ExportReport) {
    let Some(colour) = node.track.color else {
        return;
    };
    if chunk
        .children
        .iter()
        .any(|child| matches!(child, RNodeTree::Node(line) if key(line) == "PEAKCOL"))
    {
        return;
    }
    report.changes.push(format!("{label} PEAKCOL: added"));
    let at = chunk
        .children
        .iter()
        .position(|child| matches!(child, RNodeTree::Node(line) if key(line) == "NAME"))
        .map(|position| position + 1)
        .unwrap_or(0);
    chunk
        .children
        .insert(at, node_line(&["PEAKCOL", &(colour as i32).to_string()]));
}

// ── track groups ───────────────────────────────────────────────────────

/// Bring a track chunk's `GROUP_FLAGS` / `GROUP_FLAGS_HIGH` in line with
/// the document.
///
/// A no-op when the lines already say what the document says, so an
/// unedited export stays byte-identical, and the same shape as
/// [`patch_lanes`] next door: the old lines are dropped and the new set
/// inserted where the first of them was.
///
/// Placement for a track that has newly joined a group follows what
/// REAPER writes — `GROUP_FLAGS` sits after `REC`/`VU` and before
/// `TRACKHEIGHT`, i.e. after the lane lines [`patch_lanes`] has just
/// placed, which is why this runs second.
fn patch_group_flags(chunk: &mut RChunk, node: &TrackNode, label: &str, report: &mut ExportReport) {
    const GROUP_KEYS: [&str; 2] = ["GROUP_FLAGS", "GROUP_FLAGS_HIGH"];
    let is_group_line = |child: &RNodeTree| matches!(child, RNodeTree::Node(l) if GROUP_KEYS.contains(&key(l).as_str()));

    let (low, high) = node.track.grouping.to_rpp_fields();
    let wanted: Vec<Vec<String>> = [("GROUP_FLAGS", low), ("GROUP_FLAGS_HIGH", high)]
        .into_iter()
        .filter(|(_, fields)| !fields.is_empty())
        .map(|(key, fields)| {
            std::iter::once(key.to_string())
                .chain(fields.iter().map(u32::to_string))
                .collect()
        })
        .collect();

    let existing: Vec<Vec<String>> = chunk
        .children
        .iter()
        .filter_map(|child| match child {
            RNodeTree::Node(line) if is_group_line(child) => Some(tokens(line)),
            _ => None,
        })
        .collect();
    if existing == wanted {
        return;
    }
    report.changes.push(format!(
        "{label} groups: {} lines \u{2192} {} lines",
        existing.len(),
        wanted.len()
    ));

    // Where REAPER would have put them: over the old lines if there were
    // any, else just before TRACKHEIGHT, else before the first nested
    // chunk — never after the items.
    let insert_at = chunk
        .children
        .iter()
        .position(is_group_line)
        .or_else(|| {
            chunk
                .children
                .iter()
                .position(|child| matches!(child, RNodeTree::Node(l) if key(l) == "TRACKHEIGHT"))
        })
        .or_else(|| {
            chunk
                .children
                .iter()
                .position(|child| matches!(child, RNodeTree::Chunk(_)))
        })
        .unwrap_or(chunk.children.len());
    chunk.children.retain(|child| !is_group_line(child));
    for (offset, line) in wanted.iter().enumerate() {
        let refs: Vec<&str> = line.iter().map(String::as_str).collect();
        chunk.children.insert(insert_at + offset, node_line(&refs));
    }
}

// ── fixed lanes ────────────────────────────────────────────────────────
//
// The lane lines REAPER writes under <TRACK>, in the order it writes
// them: FREEMODE 2 (fixed lanes on), FIXEDLANES, LANESOLO, LANEREC,
// LANENAME, then ITEMLANES and the LINKEDLANE comp areas. A track whose
// lanes were switched off loses all of them and gets FREEMODE 0.

/// The lane lines a track should carry, keyed for upsert. `settings` is
/// the `FIXEDLANES` bitfield to preserve from the file (only the big-lanes
/// bit is ours to change).
fn lane_lines(node: &TrackNode, settings: i64) -> Vec<Vec<String>> {
    let track = &node.track;
    let big = track.lane_display == LaneDisplay::Big;
    let one = track.lane_display == LaneDisplay::One;
    let settings = if big {
        settings | i64::from(lane_settings::BIG_LANES)
    } else {
        settings & !i64::from(lane_settings::BIG_LANES)
    };
    let mut lines = vec![
        vec!["FREEMODE".into(), "2".into()],
        vec![
            "FIXEDLANES".into(),
            settings.to_string(),
            "0".into(),
            if one { "1" } else { "0" }.into(),
            "0".into(),
            "0".into(),
        ],
        vec![
            "LANESOLO".into(),
            (track.lane_play_mask as u32).to_string(),
            ((track.lane_play_mask >> 32) as u32).to_string(),
            "0".into(),
            "0".into(),
            "0".into(),
            "0".into(),
            "0".into(),
            "0".into(),
        ],
    ];
    // LANEREC goes before LANENAME, as REAPER writes them; ITEMLANES and
    // the comp areas after. The codec is `dawfile-reaper`'s, shared with
    // the live backend's chunk patcher.
    let mut comping = comping_lines(&node.comping, track.lane_count);
    if let Some(at) = comping.iter().position(|l| l[0] == "LANEREC") {
        lines.push(comping.remove(at));
    }
    if !track.lane_names.is_empty() {
        let mut line = vec!["LANENAME".to_string()];
        line.extend(track.lane_names.iter().cloned());
        lines.push(line);
    }
    lines.extend(comping);
    lines
}

const LANE_KEYS: [&str; 7] = [
    "FIXEDLANES",
    "LANESOLO",
    "LANEREC",
    "LANENAME",
    "ITEMLANES",
    "LINKEDLANE",
    "FREEMODE",
];

fn is_lane_line(child: &RNodeTree) -> bool {
    matches!(child, RNodeTree::Node(line) if LANE_KEYS.contains(&key(line).as_str()))
}

/// Bring a track chunk's lane lines in line with the document.
///
/// A no-op when the lines already say what the document says, so an
/// unedited export stays byte-identical. Otherwise the old lane lines are
/// dropped and the new set inserted where the first of them was (or before
/// the first item), which keeps the diff local.
fn patch_lanes(chunk: &mut RChunk, node: &TrackNode, label: &str, report: &mut ExportReport) {
    let existing: Vec<Vec<String>> = chunk
        .children
        .iter()
        .filter(|child| is_lane_line(child))
        .filter_map(|child| match child {
            RNodeTree::Node(line) => Some(tokens(line)),
            RNodeTree::Chunk(_) => None,
        })
        .collect();
    let free_mode = existing
        .iter()
        .find(|t| t[0] == "FREEMODE")
        .and_then(|t| t.get(1))
        .and_then(|v| v.parse::<i64>().ok());
    let settings = existing
        .iter()
        .find(|t| t[0] == "FIXEDLANES")
        .and_then(|t| t.get(1))
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(1);

    // Compare by meaning, not by text: a REAPER-written lanes track has
    // no LANESOLO when lane 0 plays and no LANEREC until it comps, and
    // rewriting those lines on an unedited export would be churn.
    if decoded_lanes(&existing, node) == (node.track.clone(), node.comping.clone()) {
        return;
    }
    let wanted: Vec<Vec<String>> = if node.track.lane_count > 0 {
        lane_lines(node, settings)
    } else if free_mode == Some(2) {
        // Lanes switched off: the geometry mode goes back to normal and
        // every lane line goes with it.
        vec![vec!["FREEMODE".into(), "0".into()]]
    } else {
        // Never had lanes: leave a free-item-positioning track's FREEMODE
        // (and anything else) exactly as it was.
        return;
    };
    report.changes.push(format!(
        "{label} lanes: {} lines → {} lines",
        existing.len(),
        wanted.len()
    ));
    let insert_at = chunk
        .children
        .iter()
        .position(is_lane_line)
        .or_else(|| {
            chunk
                .children
                .iter()
                .position(|child| matches!(child, RNodeTree::Chunk(_)))
        })
        .unwrap_or(chunk.children.len());
    chunk.children.retain(|child| !is_lane_line(child));
    for (offset, line) in wanted.iter().enumerate() {
        let refs: Vec<&str> = line.iter().map(String::as_str).collect();
        chunk.children.insert(insert_at + offset, node_line(&refs));
    }
}

/// What the importer would read from these lane lines, laid over the
/// document's track so only the lane fields differ.
///
/// Both halves run the codecs `dawfile-reaper` exports, which is what
/// keeps "unchanged" here meaning the same thing it means on import.
fn decoded_lanes(lines: &[Vec<String>], node: &TrackNode) -> (Track, LaneComping) {
    let find = |k: &str| lines.iter().find(|t| t[0] == k);
    let int = |t: &[String], i: usize| {
        t.get(i)
            .and_then(|v| v.parse::<f64>().ok())
            .map(|v| v as i64)
    };
    let fields = FixedLaneFields {
        has_fixed_lanes: find("FIXEDLANES").is_some(),
        settings: find("FIXEDLANES").and_then(|t| int(t, 1)).unwrap_or(0) as i32,
        show_play_only_lane: find("FIXEDLANES").and_then(|t| int(t, 3)).unwrap_or(0) != 0,
        lane_solo: find("LANESOLO")
            .map(|t| (int(t, 1).unwrap_or(0) as u32, int(t, 2).unwrap_or(0) as u32)),
        lane_names: find("LANENAME")
            .map(|t| t[1..].to_vec())
            .unwrap_or_default(),
        item_lanes: find("ITEMLANES")
            .and_then(|t| int(t, 1))
            .filter(|&n| n >= 0)
            .map(|n| n as u32),
        max_item_lane: node.items.iter().filter_map(|i| i.item.fixed_lane).max(),
    };
    let state = fields.decode();
    let mut track = node.track.clone();
    track.lane_count = state.lane_count;
    track.lane_play_mask = state.lane_play_mask;
    track.lane_names = state.lane_names;
    track.lane_display = state.lane_display;
    let comping = comping_from_lines(lines.iter().map(Vec::as_slice));
    (track, comping)
}

/// Put an item on its fixed lane: `YPOS <y> <height> 2`, each lane
/// `1/lane_count` of the track tall. Left alone on a track without lanes,
/// where `YPOS` is free-item-positioning geometry the document does not
/// model.
fn set_item_lane(chunk: &mut RChunk, item: &Item, lane_count: u32, report: &mut ExportReport) {
    if lane_count == 0 {
        return;
    }
    let Some(lane) = item.fixed_lane else {
        return;
    };
    let height = 1.0 / f64::from(lane_count);
    let y = f64::from(lane) * height;
    let current = child_node(chunk, "YPOS").and_then(|line| {
        let (y, h) = (param_f64(line, 1)?, param_f64(line, 2)?);
        (h > 1e-9).then(|| (y / h).round() as u32)
    });
    if current == Some(lane) {
        return;
    }
    report.changes.push(format!(
        "item {} YPOS: lane {} → {lane}",
        item.guid,
        current.map(|l| l.to_string()).unwrap_or_default()
    ));
    let line = node_line(&["YPOS", &format_f64(y), &format_f64(height), "2"]);
    let at = chunk
        .children
        .iter()
        .position(|child| matches!(child, RNodeTree::Node(l) if key(l) == "YPOS"));
    match at {
        Some(at) => chunk.children[at] = line,
        None => {
            let at = chunk
                .children
                .iter()
                .position(|child| matches!(child, RNodeTree::Chunk(_)))
                .unwrap_or(chunk.children.len());
            chunk.children.insert(at, line);
        }
    }
}

fn patch_item(chunk: &mut RChunk, node: &ItemNode, report: &mut ExportReport) {
    let label = format!("item {}", node.id);
    let item = &node.item;
    reconcile_envelopes(chunk, &node.envelopes, &label, report);
    reconcile_takes(chunk, node, &label, report);
    // Takes are matched by GUID, so the walk collects them in file order and
    // pairs each with the document's take of the same id.
    // Resolve the whole run before writing NAME/SOFFS: GUID appears after
    // those fields, and empty/null take slots do not reliably match indices.
    let runs = take_runs(chunk);
    let mut current_take: Option<&TakeNode> = None;

    for (position, child) in chunk.children.iter_mut().enumerate() {
        if let Some((index, (_, _, guid))) = runs
            .iter()
            .enumerate()
            .find(|(_, (start, _, _))| *start == position)
        {
            current_take = match guid {
                Some(guid) => node.takes.iter().find(|take| take.id.as_str() == guid),
                None => node.takes.get(index),
            };
        }
        match child {
            RNodeTree::Node(line) => match key(line).as_str() {
                "POSITION" => set_number(
                    line,
                    1,
                    item.position.as_seconds(),
                    &label,
                    "POSITION",
                    report,
                ),
                "LENGTH" => set_number(line, 1, item.length.as_seconds(), &label, "LENGTH", report),
                "SNAPOFFS" => set_number(
                    line,
                    1,
                    item.snap_offset.as_seconds(),
                    &label,
                    "SNAPOFFS",
                    report,
                ),
                "MUTE" => set_bool(line, 1, item.muted, &label, "MUTE", report),
                "SEL" => set_bool(line, 1, item.selected, &label, "SEL", report),
                "LOCK" => set_bool(line, 1, item.locked, &label, "LOCK", report),
                "LOOP" => set_bool(line, 1, item.loop_source, &label, "LOOP", report),
                "FADEIN" => {
                    set_number(
                        line,
                        1,
                        fade_shape_code(item.fade_in_shape),
                        &label,
                        "FADEIN",
                        report,
                    );
                    set_number(
                        line,
                        2,
                        item.fade_in_length.as_seconds(),
                        &label,
                        "FADEIN",
                        report,
                    );
                }
                "FADEOUT" => {
                    set_number(
                        line,
                        1,
                        fade_shape_code(item.fade_out_shape),
                        &label,
                        "FADEOUT",
                        report,
                    );
                    set_number(
                        line,
                        2,
                        item.fade_out_length.as_seconds(),
                        &label,
                        "FADEOUT",
                        report,
                    );
                }
                "COLOR" => {
                    if let Some(color) = item.color {
                        set_number(line, 1, color as f64, &label, "COLOR", report);
                    }
                }

                // ── take-scoped keys ────────────────────────────────
                "TAKE" | "GUID" => {}
                "NAME" => {
                    if let Some(take) = current_take {
                        set_string(line, 1, &take.take.name, &label, "take NAME", report);
                    }
                }
                // Field 1 is the *item's* trim and field 3 the take's volume —
                // see the matching note in the importer. One line, two owners.
                "VOLPAN" => {
                    set_number(line, 1, item.volume, &label, "VOLPAN", report);
                    if let Some(take) = current_take {
                        set_number(line, 3, take.take.volume, &label, "take VOLPAN", report);
                    }
                }
                "SOFFS" => {
                    if let Some(take) = current_take {
                        set_number(
                            line,
                            1,
                            take.take.start_offset.as_seconds(),
                            &label,
                            "take SOFFS",
                            report,
                        );
                    }
                }
                "PLAYRATE" => {
                    if let Some(take) = current_take {
                        set_number(
                            line,
                            1,
                            take.take.play_rate,
                            &label,
                            "take PLAYRATE",
                            report,
                        );
                        set_bool(
                            line,
                            2,
                            take.take.preserve_pitch,
                            &label,
                            "take PLAYRATE",
                            report,
                        );
                        set_number(line, 3, take.take.pitch, &label, "take PLAYRATE", report);
                    }
                }
                "CHANMODE" => {
                    if let Some(take) = current_take {
                        set_number(
                            line,
                            1,
                            take.take.channel_mode as f64,
                            &label,
                            "take CHANMODE",
                            report,
                        );
                    }
                }
                _ => {}
            },
            RNodeTree::Chunk(inner) => {
                let inner_name = inner.name().unwrap_or_default();
                if is_envelope_chunk(&inner_name)
                    && let Some(envelope_node) = find_envelope(&node.envelopes, inner, &inner_name)
                {
                    patch_envelope(inner, envelope_node, &label, report);
                }
            }
        }
    }

    reconcile_stretch_markers(chunk, node, &label, report);
}

/// Rewrite each take's `SM` lines to match the document.
///
/// Stretch markers are a *list the edit owns*, like envelope points:
/// there is no honest way to patch one marker against another, so the
/// take's whole set is replaced whenever it differs — deleted when the
/// document has none, inserted (before the take's `<SOURCE`, where
/// REAPER writes them) when the file had none. A take whose markers
/// already match is left byte-for-byte alone.
fn reconcile_stretch_markers(
    chunk: &mut RChunk,
    node: &ItemNode,
    label: &str,
    report: &mut ExportReport,
) {
    let runs = take_runs(chunk);
    // Reverse order, so earlier runs' indices survive later edits.
    for (run_index, (start, end, guid)) in runs.iter().enumerate().rev() {
        let take_node = guid
            .as_ref()
            .and_then(|g| node.takes.iter().find(|t| t.id.as_str() == g))
            .or_else(|| node.takes.get(run_index));
        let want: Vec<[String; 3]> = take_node
            .map(|t| {
                t.stretch_markers
                    .iter()
                    .map(|m| {
                        [
                            format_f64(m.position),
                            format_f64(m.source_position),
                            format_f64(m.slope),
                        ]
                    })
                    .collect()
            })
            .unwrap_or_default();

        let existing: Vec<usize> = (*start..*end)
            .filter(|&i| matches!(&chunk.children[i], RNodeTree::Node(line) if key(line) == "SM"))
            .collect();
        let have: Vec<Vec<String>> = existing
            .iter()
            .map(|&i| match &chunk.children[i] {
                RNodeTree::Node(line) => (1..=3).filter_map(|n| param(line, n)).collect(),
                _ => unreachable!(),
            })
            .collect();
        let unchanged = have.len() == want.len()
            && have
                .iter()
                .zip(&want)
                .all(|(h, w)| h.len() == 3 && h.iter().zip(w.iter()).all(|(a, b)| a == b));
        if unchanged {
            continue;
        }

        // Where the new set goes: where the old one was, else just
        // before the run's first chunk child (the `<SOURCE`), else the
        // run's end.
        let insert_at = existing.first().copied().unwrap_or_else(|| {
            (*start..*end)
                .find(|&i| matches!(&chunk.children[i], RNodeTree::Chunk(_)))
                .unwrap_or(*end)
        });
        for &i in existing.iter().rev() {
            chunk.children.remove(i);
        }
        let insert_at = insert_at - existing.iter().filter(|&&i| i < insert_at).count();
        for (offset, m) in want.iter().enumerate() {
            chunk
                .children
                .insert(insert_at + offset, node_line(&["SM", &m[0], &m[1], &m[2]]));
        }
        report.changes.push(format!(
            "{label} take {}: SM {} → {}",
            guid.as_deref().unwrap_or("?"),
            have.len(),
            want.len()
        ));
    }
}

/// Find the document's envelope for a chunk — by `EGUID` if the file has
/// one, otherwise by the chunk name, which is unique within its owner.
fn find_envelope<'a>(
    envelopes: &'a [EnvelopeNode],
    chunk: &RChunk,
    chunk_name: &str,
) -> Option<&'a EnvelopeNode> {
    if let Some(guid) = child_node(chunk, "EGUID").and_then(|line| param(line, 1))
        && let Some(found) = envelopes
            .iter()
            .find(|candidate| candidate.id.as_str() == guid)
    {
        return Some(found);
    }
    envelopes
        .iter()
        .find(|candidate| candidate.envelope.name == chunk_name)
}

/// Patch an envelope's points.
///
/// Points are a list, not a set of addressable entities, so this is the one
/// place export rewrites structurally: if the point list differs from the
/// file's, the `PT` lines are replaced wholesale, keeping every other line
/// (`ACT`, `VIS`, `DEFSHAPE`, `<EXT>` blocks) exactly where it was.
fn patch_envelope(
    chunk: &mut RChunk,
    node: &EnvelopeNode,
    owner_label: &str,
    report: &mut ExportReport,
) {
    let existing: Vec<(f64, f64, i64)> = chunk
        .children
        .iter()
        .filter_map(|child| match child {
            RNodeTree::Node(line) if key(line) == "PT" => Some((
                param_f64(line, 1).unwrap_or(0.0),
                param_f64(line, 2).unwrap_or(0.0),
                param_i64(line, 3).unwrap_or(0),
            )),
            _ => None,
        })
        .collect();

    let wanted: Vec<(f64, f64, i64)> = node
        .points
        .iter()
        .map(|point| {
            (
                point.time.as_seconds(),
                point.value,
                envelope_shape_code(point.shape),
            )
        })
        .collect();

    if existing == wanted {
        return;
    }

    report.changes.push(format!(
        "{owner_label} envelope {} PT: {} points → {} points",
        node.id,
        existing.len(),
        wanted.len()
    ));

    // Rebuild in place: drop the old `PT` lines, then insert the new ones
    // where the first one was, so the envelope's other lines keep their
    // order and a diff stays local.
    let insert_at = chunk
        .children
        .iter()
        .position(|child| matches!(child, RNodeTree::Node(line) if key(line) == "PT"))
        .unwrap_or(chunk.children.len());
    chunk
        .children
        .retain(|child| !matches!(child, RNodeTree::Node(line) if key(line) == "PT"));

    for (offset, point) in node.points.iter().enumerate() {
        let line = RNode::from_tokens(vec![
            RToken::new("PT"),
            RToken::new(format_f64(point.time.as_seconds())),
            RToken::new(format_f64(point.value)),
            RToken::new(envelope_shape_code(point.shape).to_string()),
            RToken::new(if point.selected { "1" } else { "0" }),
            RToken::new("0"),
            RToken::new(format_f64(point.tension)),
        ]);
        chunk
            .children
            .insert(insert_at + offset, RNodeTree::Node(line));
    }
}

// ── structural reconciliation ──────────────────────────────────────────
//
// Patching alone would silently lose anything the editor *added* and
// silently resurrect anything it *deleted*. These functions close that gap:
// the tree's chunks and the document's nodes are matched by id, chunks with
// no node are dropped, and nodes with no chunk are built.

/// Drop chunks whose entity is gone; append chunks for entities that are new.
#[allow(clippy::too_many_arguments)]
fn reconcile<T>(
    parent: &mut RChunk,
    chunk_name: &str,
    id_of_chunk: impl Fn(&RChunk) -> Option<String>,
    nodes: &[T],
    id_of_node: impl Fn(&T) -> &str,
    build: impl Fn(&T) -> RChunk,
    owner_label: &str,
    kind: &str,
    report: &mut ExportReport,
) {
    let mut removed = Vec::new();
    parent.children.retain(|child| {
        let RNodeTree::Chunk(chunk) = child else {
            return true;
        };
        if chunk.name().as_deref() != Some(chunk_name) {
            return true;
        }
        match id_of_chunk(chunk) {
            Some(id) if !nodes.iter().any(|node| id_of_node(node) == id) => {
                removed.push(id);
                false
            }
            _ => true,
        }
    });
    for id in removed {
        report
            .changes
            .push(format!("{owner_label} {kind} {id}: removed"));
    }

    let present: Vec<String> = parent
        .children
        .iter()
        .filter_map(|child| match child {
            RNodeTree::Chunk(chunk) if chunk.name().as_deref() == Some(chunk_name) => {
                id_of_chunk(chunk)
            }
            _ => None,
        })
        .collect();

    for node in nodes {
        let id = id_of_node(node);
        if present.iter().any(|existing| existing == id) {
            continue;
        }
        report
            .changes
            .push(format!("{owner_label} {kind} {id}: added"));
        parent.children.push(RNodeTree::Chunk(build(node)));
    }
}

/// Envelopes are matched by `EGUID` when present and by chunk name otherwise
/// — the same rule [`find_envelope`] uses, so add/remove and patch never
/// disagree about which chunk is which.
fn reconcile_envelopes(
    parent: &mut RChunk,
    nodes: &[EnvelopeNode],
    owner_label: &str,
    report: &mut ExportReport,
) {
    let mut removed = Vec::new();
    parent.children.retain(|child| {
        let RNodeTree::Chunk(chunk) = child else {
            return true;
        };
        let name = chunk.name().unwrap_or_default();
        if !is_envelope_chunk(&name) {
            return true;
        }
        if find_envelope(nodes, chunk, &name).is_some() {
            return true;
        }
        removed.push(name);
        false
    });
    for name in removed {
        report
            .changes
            .push(format!("{owner_label} envelope {name}: removed"));
    }

    for node in nodes {
        let already = parent.children.iter().any(|child| match child {
            RNodeTree::Chunk(chunk) => {
                let name = chunk.name().unwrap_or_default();
                is_envelope_chunk(&name)
                    && find_envelope(std::slice::from_ref(node), chunk, &name).is_some()
            }
            _ => false,
        });
        if already {
            continue;
        }
        report
            .changes
            .push(format!("{owner_label} envelope {}: added", node.id));
        parent.children.push(RNodeTree::Chunk(build_envelope(node)));
    }
}

/// Takes are a flat run inside `<ITEM>` rather than chunks, so they cannot go
/// through [`reconcile`].
///
/// Pairing is by take GUID where the file has one. A run with no GUID — an
/// item written by an older REAPER, or a bare empty item — cannot be
/// identified, so it is paired positionally with the next unclaimed take
/// instead. Guessing there is safe in a way that guessing a *deletion* would
/// not be: the worst case is patching an unnamed take's values, not
/// destroying it.
fn reconcile_takes(chunk: &mut RChunk, node: &ItemNode, label: &str, report: &mut ExportReport) {
    let runs = take_runs(chunk);
    if runs.is_empty() {
        return;
    }

    let mut claimed: Vec<&str> = Vec::new();
    // `None` means "this run has no counterpart and should go".
    let mut pairing: Vec<Option<&str>> = Vec::with_capacity(runs.len());
    for (_, _, guid) in &runs {
        match guid {
            Some(guid) => {
                let found = node
                    .takes
                    .iter()
                    .find(|take| take.id.as_str() == guid)
                    .map(|take| take.id.as_str());
                if let Some(id) = found {
                    claimed.push(id);
                }
                pairing.push(found);
            }
            None => {
                let next = node
                    .takes
                    .iter()
                    .map(|take| take.id.as_str())
                    .find(|id| !claimed.contains(id));
                if let Some(id) = next {
                    claimed.push(id);
                }
                pairing.push(next);
            }
        }
    }

    // Removals go back-to-front so earlier ranges stay valid.
    let mut leading_run_removed = false;
    for (position, ((start, end, guid), paired)) in runs.iter().zip(&pairing).enumerate().rev() {
        if paired.is_some() {
            continue;
        }
        report.changes.push(format!(
            "{label} take {}: removed",
            guid.clone().unwrap_or_else(|| "<unnamed>".to_string())
        ));
        chunk.children.drain(*start..*end);
        if position == 0 {
            leading_run_removed = true;
        }
    }

    for take in &node.takes {
        if claimed.contains(&take.id.as_str()) {
            continue;
        }
        report
            .changes
            .push(format!("{label} take {}: added", take.id));
        chunk.children.push(if take.take.is_active {
            node_line(&["TAKE", "SEL"])
        } else {
            node_line(&["TAKE"])
        });
        for line in take_lines(take) {
            chunk.children.push(RNodeTree::Node(line));
        }
    }

    // REAPER writes the first take with no `TAKE` marker. If the run that
    // held that position was deleted, the marker that now leads has to go or
    // REAPER reads an empty take ahead of everything.
    if leading_run_removed
        && let Some(position) = chunk
            .children
            .iter()
            .position(|child| matches!(child, RNodeTree::Node(line) if key(line) == "TAKE"))
    {
        chunk.children.remove(position);
    }
}

/// The `[start, end)` child ranges of each take inside an `<ITEM>`, with the
/// take's GUID where it has one.
fn take_runs(chunk: &RChunk) -> Vec<(usize, usize, Option<String>)> {
    let mut boundaries = vec![0usize];
    for (position, child) in chunk.children.iter().enumerate() {
        if let RNodeTree::Node(line) = child
            && key(line) == "TAKE"
        {
            boundaries.push(position);
        }
    }
    boundaries.push(chunk.children.len());

    let mut runs = Vec::new();
    for window in boundaries.windows(2) {
        let (start, end) = (window[0], window[1]);
        if start >= end {
            continue;
        }
        let guid = chunk.children[start..end]
            .iter()
            .find_map(|child| match child {
                RNodeTree::Node(line) if key(line) == "GUID" => param(line, 1),
                _ => None,
            });
        runs.push((start, end, guid));
    }
    runs
}

// ── tempo map ──────────────────────────────────────────────────────────

/// Bring the project's `<TEMPOENVEX>` chunk in line with the document.
///
/// The map is a list of points, not a set of addressable entities, so this
/// follows [`patch_envelope`]'s rule: if the decoded map differs from the
/// document's, every `PT` line is replaced and everything else in the
/// chunk (`EGUID`, `ACT`, `VIS`, `DEFSHAPE`) is left exactly where it was.
/// "Differs" is decided by running the *importer's* decoder over the
/// chunk, so an unedited export can never churn.
fn patch_tempo_map(root: &mut RChunk, document: &DawDocument, report: &mut ExportReport) {
    let signature = super::import::project_time_signature(root);
    let at = root.children.iter().position(
        |child| matches!(child, RNodeTree::Chunk(chunk) if chunk.name().as_deref() == Some("TEMPOENVEX")),
    );

    if let Some(at) = at
        && let RNodeTree::Chunk(chunk) = &root.children[at]
        && super::import::read_tempo_map(chunk, signature) == document.tempo_map
    {
        return;
    }
    if at.is_none() && document.tempo_map.is_empty() {
        return;
    }

    report
        .changes
        .push(format!("tempo map: {} point(s)", document.tempo_map.len()));

    let points: Vec<RNodeTree> = document
        .tempo_map
        .iter()
        .map(|point| {
            // Field 4 packs a signature change as `numerator | denominator
            // << 16`, and 0 means "no change here" — which is how REAPER
            // tells a tempo-only point from one that also changes the bar.
            let packed = point
                .time_signature
                .map(|signature| {
                    i64::from(signature.numerator) | (i64::from(signature.denominator) << 16)
                })
                .unwrap_or(0);
            node_line(&[
                "PT",
                &format_f64(point.position_seconds()),
                &format_f64(point.bpm),
                &point.shape.unwrap_or(0).to_string(),
                &packed.to_string(),
                if point.selected.unwrap_or(false) {
                    "1"
                } else {
                    "0"
                },
                "0",
                &format_f64(point.bezier_tension.unwrap_or(0.0)),
            ])
        })
        .collect();

    match at {
        Some(at) => {
            let RNodeTree::Chunk(chunk) = &mut root.children[at] else {
                return;
            };
            let insert_at = chunk
                .children
                .iter()
                .position(|child| matches!(child, RNodeTree::Node(l) if key(l) == "PT"))
                .unwrap_or(chunk.children.len());
            chunk
                .children
                .retain(|child| !matches!(child, RNodeTree::Node(l) if key(l) == "PT"));
            for (offset, line) in points.into_iter().enumerate() {
                chunk.children.insert(insert_at + offset, line);
            }
        }
        None => {
            // REAPER writes `<TEMPOENVEX>` after the master-track lines and
            // before the markers and tracks, which is what this finds.
            let mut chunk = RChunk::new(vec![RToken::new("TEMPOENVEX")]);
            chunk.children.push(node_line(&["ACT", "1", "-1"]));
            chunk.children.push(node_line(&["VIS", "1", "0", "1"]));
            chunk.children.push(node_line(&["LANEHEIGHT", "0", "0"]));
            chunk.children.push(node_line(&["ARM", "0"]));
            chunk
                .children
                .push(node_line(&["DEFSHAPE", "1", "-1", "-1"]));
            chunk.children.extend(points);
            let insert_at = first_project_block(root);
            root.children.insert(insert_at, RNodeTree::Chunk(chunk));
        }
    }

    // The `TEMPO` line carries the tempo and signature in force at the
    // start of the timeline. Left stale, REAPER shows the old tempo in the
    // transport until the playhead crosses the first point.
    if let Some(first) = document.tempo_map.first() {
        let signature = first.time_signature.unwrap_or(signature);
        for child in &mut root.children {
            let RNodeTree::Node(line) = child else {
                continue;
            };
            if key(line) != "TEMPO" {
                continue;
            }
            set_number(line, 1, first.bpm, "project", "TEMPO", report);
            set_number(
                line,
                2,
                f64::from(signature.numerator),
                "project",
                "TEMPO",
                report,
            );
            set_number(
                line,
                3,
                f64::from(signature.denominator),
                "project",
                "TEMPO",
                report,
            );
        }
    }
}

/// Where a project-level block belongs: before the first `MARKER` line or
/// `<TRACK>` chunk, which is where REAPER writes `<TEMPOENVEX>`.
fn first_project_block(root: &RChunk) -> usize {
    root.children
        .iter()
        .position(|child| match child {
            RNodeTree::Node(line) => key(line) == "MARKER",
            RNodeTree::Chunk(chunk) => {
                matches!(
                    chunk.name().as_deref(),
                    Some("TRACK" | "PROJBAY" | "EXTENSIONS")
                )
            }
        })
        .unwrap_or(root.children.len())
}

// ── markers and regions ────────────────────────────────────────────────

/// Bring the project's `MARKER` lines in line with the document.
///
/// Markers are entities with ids, so this patches like the track walk does
/// rather than regenerating: an existing line keeps every field the schema
/// does not model, a marker the editor deleted loses its line, and a
/// marker the editor added gets a fresh one. A region is two lines sharing
/// one numeric id, so growing or losing a `region_end_seconds` adds or
/// drops the second line.
fn patch_markers(root: &mut RChunk, document: &DawDocument, report: &mut ExportReport) {
    let is_marker = |child: &RNodeTree| matches!(child, RNodeTree::Node(l) if key(l) == "MARKER");
    if document.markers.is_empty() && !root.children.iter().any(is_marker) {
        return;
    }

    // The numeric id is REAPER's, not ours: it is how the two halves of a
    // region find each other on the way back in.
    let mut next_id = root
        .children
        .iter()
        .filter_map(|child| match child {
            RNodeTree::Node(line) if is_marker(child) => param_i64(line, 1),
            _ => None,
        })
        .chain(
            document
                .markers
                .iter()
                .filter_map(|m| m.marker.id.map(i64::from)),
        )
        .max()
        .unwrap_or(0)
        + 1;

    let existing: Vec<Vec<String>> = root
        .children
        .iter()
        .filter_map(|child| match child {
            RNodeTree::Node(line) if is_marker(child) => Some(tokens(line)),
            _ => None,
        })
        .collect();

    // Compare by meaning, through the importer's own reader, so an
    // unedited export never rewrites a marker line.
    let decoded = super::import::read_markers(&existing);
    if decoded.len() == document.markers.len()
        && decoded
            .iter()
            .zip(&document.markers)
            .all(|(have, want)| marker_facts(have) == marker_facts(want))
    {
        return;
    }

    // Each marker keeps the line it already had, with only the modelled
    // fields written over. REAPER puts things there this schema does not
    // model — the `R` in field 7, a full-form closing line on some
    // regions — and regenerating would quietly flatten them.
    let mut opening: std::collections::HashMap<i64, Vec<String>> = std::collections::HashMap::new();
    let mut closing: std::collections::HashMap<i64, Vec<String>> = std::collections::HashMap::new();
    for line in &existing {
        let Some(numeric) = line.get(1).and_then(|token| token.parse::<i64>().ok()) else {
            continue;
        };
        match opening.entry(numeric) {
            std::collections::hash_map::Entry::Vacant(slot) => {
                slot.insert(line.clone());
            }
            // A second line with an id already seen is a region's closing
            // half, which is the only thing `MARKER` repeats an id for.
            std::collections::hash_map::Entry::Occupied(_) => {
                closing.entry(numeric).or_insert_with(|| line.clone());
            }
        }
    }

    let mut wanted: Vec<Vec<String>> = Vec::new();
    for node in &document.markers {
        let numeric = match node.marker.id {
            Some(id) => i64::from(id),
            None => {
                let id = next_id;
                next_id += 1;
                id
            }
        };
        let region = node.region_end_seconds.is_some();

        let mut line = opening.remove(&numeric).unwrap_or_else(|| {
            vec![
                "MARKER".to_string(),
                numeric.to_string(),
                "0".to_string(),
                String::new(),
                "0".to_string(),
                "0".to_string(),
                "1".to_string(),
                "B".to_string(),
                String::new(),
                "0".to_string(),
            ]
        });
        line.resize(10.max(line.len()), String::new());
        line[1] = numeric.to_string();
        line[2] = format_f64(node.marker.position_seconds());
        line[3] = node.marker.name.clone();
        line[4] = if region { "1" } else { "0" }.to_string();
        line[5] = node.marker.color.unwrap_or(0).to_string();
        line[8] = node
            .marker
            .guid
            .clone()
            .unwrap_or_else(|| node.id.to_string());
        line[9] = node.marker.lane.unwrap_or(0).to_string();
        wanted.push(line);

        if let Some(end) = node.region_end_seconds {
            let mut line = closing.remove(&numeric).unwrap_or_else(|| {
                vec![
                    "MARKER".to_string(),
                    numeric.to_string(),
                    "0".to_string(),
                    String::new(),
                    "1".to_string(),
                ]
            });
            line[1] = numeric.to_string();
            line[2] = format_f64(end);
            line[4] = "1".to_string();
            wanted.push(line);
        }
    }

    report.changes.push(format!(
        "markers: {} line(s) \u{2192} {} line(s)",
        existing.len(),
        wanted.len()
    ));

    let insert_at = root
        .children
        .iter()
        .position(is_marker)
        .unwrap_or_else(|| first_project_block(root));
    root.children.retain(|child| !is_marker(child));
    for (offset, line) in wanted.iter().enumerate() {
        let refs: Vec<&str> = line.iter().map(String::as_str).collect();
        root.children.insert(insert_at + offset, node_line(&refs));
    }
}

/// Everything about a marker the document models, as a comparable tuple.
///
/// `MarkerNode` is not `PartialEq` (its payload comes from the facade), so
/// the comparison is spelled out — and spelling it out is what makes it
/// obvious which facts a rewrite is allowed to be triggered by.
type MarkerFacts = (
    String,
    Option<u32>,
    String,
    String,
    Option<u32>,
    Option<u32>,
    String,
);

fn marker_facts(node: &crate::document::MarkerNode) -> MarkerFacts {
    (
        node.id.to_string(),
        node.marker.id,
        format_f64(node.marker.position_seconds()),
        node.marker.name.clone(),
        node.marker.color,
        node.marker.lane,
        node.region_end_seconds.map(format_f64).unwrap_or_default(),
    )
}

// ── chunk builders ─────────────────────────────────────────────────────
//
// Minimal but well-formed: enough for REAPER to open the project and see the
// entity, and nothing invented beyond what the document actually says.

/// `GROUP_FLAGS 1 0 1 …` — the bitmask fields as one line.
fn group_flag_line(key: &str, fields: &[u32]) -> RNodeTree {
    let mut line = vec![RToken::new(key)];
    line.extend(fields.iter().map(|f| RToken::new(f.to_string())));
    RNodeTree::Node(RNode::from_tokens(line))
}

fn node_line(tokens: &[&str]) -> RNodeTree {
    RNodeTree::Node(RNode::from_tokens(
        tokens.iter().map(|token| RToken::new(*token)).collect(),
    ))
}

fn build_track(node: &TrackNode) -> RChunk {
    let track = &node.track;
    let mut chunk = RChunk::new(vec![RToken::new("TRACK"), RToken::new(node.id.as_str())]);
    chunk.children.push(node_line(&["NAME", &track.name]));
    if let Some(color) = track.color {
        chunk
            .children
            .push(node_line(&["PEAKCOL", &color.to_string()]));
    }
    chunk.children.push(node_line(&[
        "VOLPAN",
        &format_f64(track.volume),
        &format_f64(track.pan),
        "-1",
        "-1",
        "1",
    ]));
    chunk.children.push(node_line(&[
        "MUTESOLO",
        if track.muted { "1" } else { "0" },
        if track.soloed { "1" } else { "0" },
        "0",
    ]));
    chunk.children.push(node_line(&[
        "IPHASE",
        if track.phase_inverted { "1" } else { "0" },
    ]));
    chunk.children.push(node_line(&[
        "ISBUS",
        if track.is_folder { "1" } else { "0" },
        &track.folder_depth.to_string(),
    ]));
    chunk
        .children
        .push(node_line(&["SEL", if track.selected { "1" } else { "0" }]));
    // REAPER writes the group lines between REC/VU and TRACKHEIGHT —
    // of the lines this builder emits, that is just before NCHAN.
    let (group_low, group_high) = track.grouping.to_rpp_fields();
    for (key, fields) in [
        ("GROUP_FLAGS", &group_low),
        ("GROUP_FLAGS_HIGH", &group_high),
    ] {
        if !fields.is_empty() {
            chunk.children.push(group_flag_line(key, fields));
        }
    }
    chunk.children.push(node_line(&["NCHAN", "2"]));
    chunk
        .children
        .push(node_line(&["TRACKID", node.id.as_str()]));

    if track.lane_count > 0 {
        // A fresh lanes track carries no `C_LANESETTINGS` bits: in
        // particular NOT auto-remove-empty-lanes (&1), which is REAPER's
        // own default and would silently drop the empty take lanes this
        // document is explicitly asking for the moment REAPER opened it.
        for line in lane_lines(node, 0) {
            let refs: Vec<&str> = line.iter().map(String::as_str).collect();
            chunk.children.push(node_line(&refs));
        }
    }

    for envelope in &node.envelopes {
        chunk
            .children
            .push(RNodeTree::Chunk(build_envelope(envelope)));
    }
    for item in &node.items {
        let mut built = build_item(item);
        set_item_lane(
            &mut built,
            &item.item,
            track.lane_count,
            &mut ExportReport::default(),
        );
        chunk.children.push(RNodeTree::Chunk(built));
    }
    chunk
}

fn build_item(node: &ItemNode) -> RChunk {
    let item = &node.item;
    let mut chunk = RChunk::new(vec![RToken::new("ITEM")]);
    chunk.children.push(node_line(&[
        "POSITION",
        &format_f64(item.position.as_seconds()),
    ]));
    chunk.children.push(node_line(&[
        "SNAPOFFS",
        &format_f64(item.snap_offset.as_seconds()),
    ]));
    chunk.children.push(node_line(&[
        "LENGTH",
        &format_f64(item.length.as_seconds()),
    ]));
    chunk.children.push(node_line(&[
        "LOOP",
        if item.loop_source { "1" } else { "0" },
    ]));
    chunk.children.push(node_line(&[
        "FADEIN",
        &format_f64(fade_shape_code(item.fade_in_shape)),
        &format_f64(item.fade_in_length.as_seconds()),
        "0",
    ]));
    chunk.children.push(node_line(&[
        "FADEOUT",
        &format_f64(fade_shape_code(item.fade_out_shape)),
        &format_f64(item.fade_out_length.as_seconds()),
        "0",
    ]));
    chunk.children.push(node_line(&[
        "MUTE",
        if item.muted { "1" } else { "0" },
        "0",
    ]));
    chunk
        .children
        .push(node_line(&["SEL", if item.selected { "1" } else { "0" }]));
    chunk.children.push(node_line(&["IGUID", node.id.as_str()]));

    for envelope in &node.envelopes {
        chunk
            .children
            .push(RNodeTree::Chunk(build_envelope(envelope)));
    }
    for (position, take) in node.takes.iter().enumerate() {
        if position > 0 {
            chunk.children.push(if take.take.is_active {
                node_line(&["TAKE", "SEL"])
            } else {
                node_line(&["TAKE"])
            });
        }
        for line in take_lines(take) {
            chunk.children.push(RNodeTree::Node(line));
        }
    }
    chunk
}

/// The flat run of lines one take contributes to an `<ITEM>`.
fn take_lines(node: &TakeNode) -> Vec<RNode> {
    let take = &node.take;
    let mut lines = vec![
        RNode::from_tokens(vec![RToken::new("NAME"), RToken::new(&take.name)]),
        // `VOLPAN <item trim> <take pan> <take volume> <take pan law>`. The
        // item's trim is written by the caller's item, so a freshly built
        // take leaves it at unity and puts its own volume in field 3.
        RNode::from_tokens(vec![
            RToken::new("VOLPAN"),
            RToken::new("1"),
            RToken::new("0"),
            RToken::new(format_f64(take.volume)),
            RToken::new("-1"),
        ]),
        RNode::from_tokens(vec![
            RToken::new("SOFFS"),
            RToken::new(format_f64(take.start_offset.as_seconds())),
        ]),
        RNode::from_tokens(vec![
            RToken::new("PLAYRATE"),
            RToken::new(format_f64(take.play_rate)),
            RToken::new(if take.preserve_pitch { "1" } else { "0" }),
            RToken::new(format_f64(take.pitch)),
            RToken::new("-1"),
            RToken::new("0"),
            RToken::new("0.0025"),
        ]),
        RNode::from_tokens(vec![
            RToken::new("CHANMODE"),
            RToken::new(take.channel_mode.to_string()),
        ]),
        RNode::from_tokens(vec![RToken::new("GUID"), RToken::new(node.id.as_str())]),
    ];
    for marker in &node.stretch_markers {
        lines.push(RNode::from_tokens(vec![
            RToken::new("SM"),
            RToken::new(format_f64(marker.position)),
            RToken::new(format_f64(marker.source_position)),
            RToken::new(format_f64(marker.slope)),
        ]));
    }
    lines
}

fn build_envelope(node: &EnvelopeNode) -> RChunk {
    // The envelope's REAPER chunk name was kept verbatim on import, so a
    // round-tripped envelope goes back under the name it came in with.
    let chunk_name = if node.envelope.name.is_empty() {
        "VOLENV2"
    } else {
        node.envelope.name.as_str()
    };
    let mut chunk = RChunk::new(vec![RToken::new(chunk_name)]);
    chunk.children.push(node_line(&["EGUID", node.id.as_str()]));
    chunk.children.push(node_line(&["ACT", "1", "-1"]));
    // `VIS`'s second field is the lane flag, not a constant: exporting
    // "0" here sent every laned envelope back overlaid on the waveform,
    // which is a silent loss on round trip.
    chunk.children.push(node_line(&[
        "VIS",
        if node.envelope.visible { "1" } else { "0" },
        if node.envelope.in_own_lane { "1" } else { "0" },
        "1",
    ]));
    chunk.children.push(node_line(&[
        "LANEHEIGHT",
        &node.envelope.lane_height.to_string(),
        "0",
    ]));
    chunk.children.push(node_line(&[
        "ARM",
        if node.envelope.armed { "1" } else { "0" },
    ]));
    chunk
        .children
        .push(node_line(&["DEFSHAPE", "0", "-1", "-1"]));
    for point in &node.points {
        chunk.children.push(node_line(&[
            "PT",
            &format_f64(point.time.as_seconds()),
            &format_f64(point.value),
            &envelope_shape_code(point.shape).to_string(),
            if point.selected { "1" } else { "0" },
            "0",
            &format_f64(point.tension),
        ]));
    }
    chunk
}

// ── token-level writers ────────────────────────────────────────────────
//
// Each one is a no-op when the value already matches. That is what keeps an
// unedited export byte-identical and an edited one a minimal diff.

fn set_number(
    line: &mut RNode,
    index: usize,
    value: f64,
    label: &str,
    key_name: &str,
    report: &mut ExportReport,
) {
    let current = param_f64(line, index);
    // Compare numerically, not textually: `1` and `1.0` are the same value
    // and rewriting one as the other would churn every file.
    if current.is_some_and(|current| current == value) {
        return;
    }
    let rendered = format_f64(value);
    report.changes.push(format!(
        "{label} {key_name}[{index}]: {} → {rendered}",
        param(line, index).unwrap_or_default()
    ));
    write_token(line, index, rendered);
}

fn set_bool(
    line: &mut RNode,
    index: usize,
    value: bool,
    label: &str,
    key_name: &str,
    report: &mut ExportReport,
) {
    if param_bool(line, index) == Some(value) {
        return;
    }
    let rendered = if value { "1" } else { "0" }.to_string();
    report.changes.push(format!(
        "{label} {key_name}[{index}]: {} → {rendered}",
        param(line, index).unwrap_or_default()
    ));
    write_token(line, index, rendered);
}

fn set_string(
    line: &mut RNode,
    index: usize,
    value: &str,
    label: &str,
    key_name: &str,
    report: &mut ExportReport,
) {
    if param(line, index).as_deref() == Some(value) {
        return;
    }
    report.changes.push(format!(
        "{label} {key_name}[{index}]: {} → {value}",
        param(line, index).unwrap_or_default()
    ));
    write_token(line, index, value.to_string());
}

/// Replace one token, leaving every other token on the line untouched.
///
/// Dropping `line` is deliberate: the tree's stringifier prefers `tokens`
/// when both are present, so leaving a stale raw line behind would be
/// harmless but confusing to anyone debugging the tree.
fn write_token(line: &mut RNode, index: usize, value: String) {
    let mut current: Vec<RToken> = tokens(line).into_iter().map(RToken::new).collect();
    if current.len() <= index {
        current.resize(index + 1, RToken::new("0"));
    }
    current[index] = RToken::new(value);
    line.tokens = Some(current);
    line.line = None;
}

fn fade_shape_code(shape: FadeShape) -> f64 {
    match shape {
        FadeShape::Linear => 0.0,
        FadeShape::FastStart => 1.0,
        FadeShape::FastEnd => 2.0,
        FadeShape::FastStartSteep => 3.0,
        FadeShape::FastEndSteep => 4.0,
        FadeShape::SlowStartEnd => 5.0,
        FadeShape::SlowStartEndSteep => 6.0,
    }
}

fn envelope_shape_code(shape: EnvelopeShape) -> i64 {
    match shape {
        EnvelopeShape::Linear => 0,
        EnvelopeShape::Square => 1,
        EnvelopeShape::SlowStartEnd => 2,
        EnvelopeShape::FastStart => 3,
        EnvelopeShape::FastEnd => 4,
        EnvelopeShape::Bezier => 5,
    }
}
