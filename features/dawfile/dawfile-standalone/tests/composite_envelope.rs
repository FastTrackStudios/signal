//! What happens to scenario 1's five item envelopes on the way out.
//!
//! #149 left this as the one genuine *unknown* rather than merely
//! unspecified: gating, compression, sibilance and de-breathing are
//! four separate envelopes composited into the item's one volume
//! envelope, and whether the four can be reconstructed after a round
//! trip depends on how REAPER and Bitwig each represent item volume
//! automation. Neither was known when the map was written.
//!
//! The answer, pinned here: **neither format has room for the four.**
//! REAPER's item chunk has exactly one volume envelope, and DAWproject
//! has no home for editor-generated automation at all. So the contract
//! is that the *composite* crosses and the sources stay in the `.daw`.
//!
//! That is a limitation, not a bug, but it must be a *stated* one. The
//! failure it is guarding against is silent: exporting to `.rpp`,
//! reopening, and finding the editor offering to recompute a composite
//! from four envelopes that no longer exist.

use std::path::Path;

use daw_proto::automation::{EnvelopePoint, EnvelopeType};
use dawfile_standalone::{DawProject, DocumentEdit, DocumentQuery, EntityId};

/// The four concerns scenario 1 generates, in the order the editor
/// composites them.
const SOURCES: [&str; 4] = ["Gating", "Compression", "Sibilance", "De-Breathing"];

fn point(t: f64, v: f64) -> EnvelopePoint {
    EnvelopePoint {
        time: daw_proto::PositionInSeconds::from_seconds(t),
        value: v,
        ..Default::default()
    }
}

/// A project carrying the four sources and their composite on a real
/// item.
///
/// Built on a fixture rather than from scratch because #156's export is
/// verbatim-source plus minimal patch: a document with no `.rpp`
/// provenance has nothing to patch against and refuses to export. That
/// is the design, so the test has to respect it.
fn scenario_one() -> (DawProject, EntityId) {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../dawfile-reaper/tests/fixtures/setlist/song_a.RPP");
    let (mut project, _) = DawProject::import_rpp_file(&fixture).expect("import fixture");

    let item = project.edit(|d| {
        let item = d
            .tracks
            .iter()
            .find_map(|t| t.items.first())
            .map(|i| i.id.clone())
            .expect("the fixture has an item");

        for name in SOURCES {
            let env = d
                .add_item_envelope(&item, EnvelopeType::Volume, name)
                .expect("source envelope");
            d.set_envelope_points(&env, vec![point(1.0, 0.5), point(2.0, 0.9)])
                .expect("points");
        }

        let composite = d
            .add_item_envelope(&item, EnvelopeType::Volume, "VOLENV")
            .expect("composite");
        d.set_envelope_points(&composite, vec![point(1.0, 0.25), point(2.0, 0.81)])
            .expect("points");
        item
    });

    (project, item)
}

#[test]
fn the_daw_format_keeps_all_five() {
    // The baseline the exports are measured against: natively, nothing
    // is lost, which is why the native format exists.
    let (project, item) = scenario_one();
    assert_eq!(
        project
            .document()
            .item(&item)
            .expect("item")
            .envelopes
            .len(),
        5
    );
}

#[test]
fn only_one_volume_envelope_reaches_the_rpp() {
    // REAPER's item chunk has one volume envelope. Writing five would
    // produce a file REAPER either rejects or silently reads as the
    // last one — a corrupt project either way.
    let (project, _) = scenario_one();
    let text = project.to_rpp_patched().expect("export").0;

    let volenvs = text.matches("<VOLENV").count();
    assert_eq!(
        volenvs, 1,
        "expected exactly one item volume envelope in the .rpp, found \
         {volenvs}:\n{text}"
    );
}

#[test]
fn the_composite_is_the_one_that_crosses() {
    // And it must be the *composite*, not whichever source happened to
    // sort first — the composite is the only one that describes what
    // the item should sound like.
    let (project, _) = scenario_one();
    let text = project.to_rpp_patched().expect("export").0;

    // The composite's second point is 0.81; every source's is 0.9.
    assert!(
        text.contains("0.81"),
        "the composite's points are not in the exported .rpp:\n{text}"
    );
}

#[test]
fn reimporting_reports_the_sources_as_lost() {
    // The silent failure this whole file exists to prevent: a document
    // that came back from a round trip must not look like one that
    // still has four editable sources.
    let (project, _) = scenario_one();
    let text = project.to_rpp_patched().expect("export").0;
    let (back, _) = DawProject::import_rpp(&text, "Composite").expect("reimport");

    let item = back
        .document()
        .tracks
        .iter()
        .find_map(|t| t.items.first())
        .expect("an item")
        .clone();

    let named: Vec<&str> = item
        .envelopes
        .iter()
        .map(|e| e.envelope.name.as_str())
        .collect();
    for source in SOURCES {
        assert!(
            !named.contains(&source),
            "'{source}' came back from a .rpp round trip, but REAPER has \
             nowhere to have stored it — it must be reported lost, not \
             resurrected empty: {named:?}"
        );
    }
    assert_eq!(
        item.envelopes.len(),
        1,
        "the composite alone should survive: {named:?}"
    );
}

#[test]
fn dawproject_carries_the_composite_too() {
    // DAWproject's caveat list already says editor state does not
    // cross. Item *automation* is not editor state, though, and a mix
    // handed to Cubase without its volume ride is wrong in an audible
    // way rather than a cosmetic one.
    use dawfile_dawproject::types::{ExpressionType, LaneContent};

    let (project, _) = scenario_one();
    let dp = dawfile_standalone::dawproject::to_dawproject(project.document());

    // Through the wire and back, not just the in-memory struct: the
    // zip is where a field silently fails to serialize.
    let bytes = dawfile_dawproject::serialize_project_bytes(&dp).expect("write dawproject");
    let back = dawfile_dawproject::parse_project_bytes(&bytes).expect("read dawproject");

    let arrangement = back
        .arrangement
        .expect("an arrangement carrying automation");
    let points: Vec<_> = arrangement
        .lanes
        .iter()
        .filter_map(|l| match &l.content {
            LaneContent::Automation(a) => Some(a),
            _ => None,
        })
        .collect();
    assert_eq!(points.len(), 1, "one composite, one automation lane");

    let a = points[0];
    assert_eq!(a.target.expression, Some(ExpressionType::Gain));
    // The composite's values, not a source's: 0.25 and 0.81 against the
    // sources' 0.5 and 0.9. The trailing 1.0 is #249's return to unity
    // at the item's end — an item's ride is scoped to its item, so it
    // does not attenuate the timeline behind it.
    let values: Vec<f64> = a.points.iter().map(|p| p.value).collect();
    assert_eq!(
        values,
        vec![0.25, 0.81, 1.0],
        "a source envelope crossed instead"
    );
}

#[test]
fn an_ambiguous_item_sends_no_automation_rather_than_a_guess() {
    // Several volume envelopes and none named as the composite: there
    // is no way to know which describes the item, and picking one would
    // hand Cubase a mix that is quietly wrong.
    use dawfile_dawproject::types::LaneContent;

    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../dawfile-reaper/tests/fixtures/setlist/song_a.RPP");
    let (mut project, _) = DawProject::import_rpp_file(&fixture).expect("import fixture");
    project.edit(|d| {
        let item = d
            .tracks
            .iter()
            .find_map(|t| t.items.first())
            .map(|i| i.id.clone())
            .expect("an item");
        for name in ["Gating", "Compression"] {
            let env = d
                .add_item_envelope(&item, EnvelopeType::Volume, name)
                .expect("source");
            d.set_envelope_points(&env, vec![point(1.0, 0.5)])
                .expect("points");
        }
    });

    let dp = dawfile_standalone::dawproject::to_dawproject(project.document());
    let automation = dp
        .arrangement
        .iter()
        .flat_map(|a| &a.lanes)
        .any(|l| matches!(l.content, LaneContent::Automation(_)));
    assert!(!automation, "an arbitrary source envelope was exported");
}

#[test]
fn item_envelope_times_become_absolute_on_the_timeline() {
    // Item envelope times are relative to the item in both `.rpp` and
    // our own format; an arrangement's are absolute. Without the
    // offset every item's volume ride stacks at the top of the
    // timeline, which is silently wrong rather than obviously wrong.
    use dawfile_dawproject::types::LaneContent;

    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../dawfile-reaper/tests/fixtures/setlist/song_a.RPP");
    let (mut project, _) = DawProject::import_rpp_file(&fixture).expect("import fixture");

    let origin = project.edit(|d| {
        // An item that does not start at zero, or the offset proves
        // nothing.
        let (item, origin) = d
            .tracks
            .iter()
            .flat_map(|t| t.items.iter())
            .find(|i| i.item.position.as_seconds() > 0.0)
            .map(|i| (i.id.clone(), i.item.position.as_seconds()))
            .expect("an item starting after zero");
        let env = d
            .add_item_envelope(&item, EnvelopeType::Volume, "VOLENV")
            .expect("composite");
        d.set_envelope_points(&env, vec![point(0.0, 1.0), point(2.0, 0.5)])
            .expect("points");
        origin
    });
    assert!(origin > 0.0, "the fixture item must not start at zero");

    let dp = dawfile_standalone::dawproject::to_dawproject(project.document());
    let a = dp
        .arrangement
        .as_ref()
        .expect("arrangement")
        .lanes
        .iter()
        .find_map(|l| match &l.content {
            LaneContent::Automation(a) => Some(a),
            _ => None,
        })
        .expect("automation");

    let times: Vec<f64> = a.points.iter().map(|p| p.time).collect();
    // The item's own two points, offset onto the timeline. The trailing
    // point is #249's return to unity at the item's end, which is what
    // keeps the ride from colouring the timeline behind it.
    assert_eq!(&times[..2], &[origin, origin + 2.0]);
    assert_eq!(a.points.last().map(|p| p.value), Some(1.0), "{times:?}");
}

#[test]
fn one_lane_per_track_however_many_items_ride() {
    // Two lanes with the same track IDREF and the same Gain expression
    // asks the reader to choose. The composites concatenate instead.
    use dawfile_dawproject::types::LaneContent;

    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../dawfile-reaper/tests/fixtures/setlist/song_a.RPP");
    let (mut project, _) = DawProject::import_rpp_file(&fixture).expect("import fixture");

    let items = project.edit(|d| {
        let track = d
            .tracks
            .iter()
            .position(|t| t.items.len() > 1)
            .or_else(|| d.tracks.iter().position(|t| !t.items.is_empty()))
            .expect("a track with items");
        let ids: Vec<_> = d.tracks[track].items.iter().map(|i| i.id.clone()).collect();
        for id in &ids {
            let env = d
                .add_item_envelope(id, EnvelopeType::Volume, "VOLENV")
                .expect("composite");
            d.set_envelope_points(&env, vec![point(0.0, 0.7)])
                .expect("points");
        }
        ids.len()
    });

    let dp = dawfile_standalone::dawproject::to_dawproject(project.document());
    let arrangement = dp.arrangement.expect("arrangement");
    assert_eq!(
        arrangement.lanes.len(),
        1,
        "{items} items on one track produced {} lanes",
        arrangement.lanes.len()
    );
    let LaneContent::Automation(a) = &arrangement.lanes[0].content else {
        panic!("not automation");
    };
    // Every item's ride is present. Counted by value rather than by
    // total, because #249 also puts a unity point at each item's end —
    // the ride itself is the 0.7.
    let rides = a.points.iter().filter(|p| p.value == 0.7).count();
    assert_eq!(rides, items, "every item's ride should be present");
    // Ascending, because a reader walks them in order.
    assert!(a.points.windows(2).all(|w| w[0].time <= w[1].time));
}
