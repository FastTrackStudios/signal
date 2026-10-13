//! Folding item envelopes into a track lane, where the items are not
//! disjoint (#249).
//!
//! `to_dawproject` gives DAWproject one gain lane per track, built by
//! concatenating each item's composite volume envelope. That is only
//! sound on the arrangement it was written against — one item after
//! another, with nothing between. A comped vocal, which is exactly the
//! material scenario 1 of #149 is built on, breaks both halves of the
//! assumption:
//!
//! - **overlapping** items concatenate into a curve that alternates
//!   between two rides, and
//! - a **gap** inherits the previous item's last value, so silence
//!   between phrases comes back attenuated.
//!
//! The contract pinned here: overlap exports no lane for that track,
//! and each item's ride returns to unity at the item's end.

use daw_proto::automation::{EnvelopePoint, EnvelopeType};
use dawfile_dawproject::types::LaneContent;
use dawfile_standalone::document::DawDocument;
use dawfile_standalone::objects::ObjectStore;
use dawfile_standalone::{DawProject, DocumentEdit, EntityId};

fn point(t: f64, v: f64) -> EnvelopePoint {
    EnvelopePoint {
        time: daw_proto::PositionInSeconds::from_seconds(t),
        value: v,
        ..Default::default()
    }
}

/// One track carrying items at the given `(position, length)`, each with
/// a composite volume envelope that ducks to `value` one second in.
fn track_of_items(items: &[(f64, f64, f64)]) -> DawProject {
    let mut project = DawProject::new(DawDocument::new("Comp"), ObjectStore::new());
    project.edit(|d| {
        let track = d.add_track("Vox");
        for (pos, len, value) in items {
            let item: EntityId = d.add_item(&track, *pos, *len).expect("item");
            let env = d
                .add_item_envelope(&item, EnvelopeType::Volume, "VOLENV")
                .expect("composite");
            // Item-relative times, which is how both `.rpp` and our own
            // format store them.
            d.set_envelope_points(&env, vec![point(0.0, *value), point(1.0, *value)])
                .expect("points");
        }
    });
    project
}

/// Every automation lane's points, in time order.
fn lane_points(project: &DawProject) -> Vec<Vec<(f64, f64)>> {
    let dp = dawfile_standalone::dawproject::to_dawproject(project.document());
    // Through the wire and back: the zip is where a field silently
    // fails to serialize.
    let bytes = dawfile_dawproject::serialize_project_bytes(&dp).expect("write");
    let back = dawfile_dawproject::parse_project_bytes(&bytes).expect("read");

    back.arrangement
        .map(|a| {
            a.lanes
                .iter()
                .filter_map(|l| match &l.content {
                    LaneContent::Automation(p) => {
                        Some(p.points.iter().map(|p| (p.time, p.value)).collect())
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn overlapping_items_do_not_produce_an_interleaved_lane() {
    // Two takes of the same phrase, crossfaded: 0..4 ducked to 0.2, and
    // 3..7 ducked to 0.8. Concatenated and sorted by absolute time they
    // give 0.2, 0.2, 0.8, 0.8 — a lane that jumps between two different
    // performances' rides.
    let project = track_of_items(&[(0.0, 4.0, 0.2), (3.0, 4.0, 0.8)]);
    let lanes = lane_points(&project);

    assert!(
        lanes.is_empty(),
        "overlapping composites cannot be folded into one curve, so the \
         track must export no lane rather than an interleaved one: \
         {lanes:?}"
    );
}

#[test]
fn a_gap_between_items_does_not_inherit_the_previous_value() {
    // Two phrases with two seconds of silence between them. The first
    // ends ducked at 0.2; a track lane holds its last value, so without
    // a return to unity the silence — and the second phrase's attack —
    // come back attenuated.
    let project = track_of_items(&[(0.0, 2.0, 0.2), (4.0, 2.0, 0.9)]);
    let lanes = lane_points(&project);
    assert_eq!(lanes.len(), 1, "disjoint items still fold into one lane");
    let points = &lanes[0];

    // Somewhere in the gap the curve must be back at unity.
    let in_gap = points
        .iter()
        .find(|(t, _)| (2.0..4.0).contains(t))
        .unwrap_or_else(|| panic!("no point in the gap at all: {points:?}"));
    assert_eq!(
        in_gap.1, 1.0,
        "the gap must return to unity, not hold the first item's duck: \
         {points:?}"
    );

    // And the first item's own ride is untouched by that.
    assert_eq!(points[0], (0.0, 0.2), "{points:?}");
}

#[test]
fn the_last_item_returns_to_unity_at_its_end() {
    // Otherwise a single ducked item attenuates the entire rest of the
    // timeline, which is the same bug with nothing after it to notice.
    let project = track_of_items(&[(0.0, 3.0, 0.4)]);
    let lanes = lane_points(&project);
    let points = &lanes[0];

    assert_eq!(
        points.last(),
        Some(&(3.0, 1.0)),
        "the ride should end at the item's end, back at unity: {points:?}"
    );
}

#[test]
fn abutting_items_do_not_collide_at_the_seam() {
    // Item A ends exactly where B begins. A unity point at A's end would
    // land on B's first point — two points at one time, which is the
    // ambiguity this module refuses elsewhere.
    let project = track_of_items(&[(0.0, 2.0, 0.2), (2.0, 2.0, 0.9)]);
    let points = lane_points(&project).remove(0);

    let at_seam: Vec<_> = points.iter().filter(|(t, _)| *t == 2.0).collect();
    assert_eq!(
        at_seam.len(),
        1,
        "exactly one point at the seam: {points:?}"
    );
    assert_eq!(
        at_seam[0].1, 0.9,
        "and it is the second item's, not an injected unity: {points:?}"
    );
}

#[test]
fn an_item_without_a_ride_cannot_make_a_track_overlap() {
    // Only items that actually carry a composite contribute to the lane,
    // so a silent item sitting under a ridden one is not a conflict.
    let mut project = track_of_items(&[(0.0, 2.0, 0.3)]);
    project.edit(|d| {
        let track = d.tracks[0].id.clone();
        // Overlaps the ridden item, but carries no envelope at all.
        d.add_item(&track, 1.0, 2.0).expect("bare item");
    });

    let lanes = lane_points(&project);
    assert_eq!(
        lanes.len(),
        1,
        "a rideless item contributes nothing, so nothing overlaps: {lanes:?}"
    );
    assert_eq!(lanes[0][0], (0.0, 0.3));
}
