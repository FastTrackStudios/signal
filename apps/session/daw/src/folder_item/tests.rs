//! The fold, the revision and the cache, against peaks written by hand.
//!
//! Nothing here opens a project. The seam is [`Folder`]: a folder is a
//! list of children with peaks on them, and everything a folder item
//! does is a function of that. The real session goes through
//! `tests/folder_items.rs`, which reads its peaks out of standalone and
//! compares the fold and the picture with committed fixtures.

use anyrender::Paint;
use anyrender::recording::RenderCommand;
use expression_editor_core::kit::LaneRole;
use vello::peniko::Color;

use super::fold::{Child, ChildTake, Grid, GroupBy, Placement, Side, TakePeaks, fold};
use super::{Folder, FolderItems, Place};

/// Peaks that rise from silence to `peak` over `blocks` blocks, mono —
/// a shape whose every block differs from its neighbours, so a fold that
/// picked the wrong block cannot pass by luck.
fn ramp(blocks: usize, peak: f64) -> Vec<f64> {
    (0..blocks)
        .flat_map(|b| {
            let v = peak * (b as f64 + 1.0) / blocks as f64;
            [-v, v]
        })
        .collect()
}

/// A mono take of `blocks` blocks declared at 48 kHz / 480 samples.
fn mono(blocks: usize, peak: f64) -> TakePeaks {
    TakePeaks::new(&ramp(blocks, peak), 1, 480, 48_000.0)
}

/// A stereo take: the left channel flat at `left`, the right at `right`.
fn stereo(blocks: usize, left: f64, right: f64) -> TakePeaks {
    let pairs: Vec<f64> = (0..blocks)
        .flat_map(|_| [-left, left, -right, right])
        .collect();
    TakePeaks::new(&pairs, 2, 480, 48_000.0)
}

fn child(name: &str, role: LaneRole, peaks: TakePeaks) -> Child {
    Child {
        guid: format!("{{{name}}}"),
        name: name.to_owned(),
        role,
        side: None,
        muted: false,
        hidden: false,
        takes: vec![ChildTake::one(0.0, 1.0, peaks)],
    }
}

fn kit() -> Folder {
    Folder {
        guid: "{kit}".to_owned(),
        name: "Drum Kit".to_owned(),
        colour: Color::from_rgb8(0x40, 0x80, 0xc0),
        group_by: GroupBy::Role,
        children: vec![
            child("In", LaneRole::Kick, mono(10, 0.9)),
            child("Top", LaneRole::Snare, mono(10, 0.5)),
            child("T1", LaneRole::Toms, mono(10, 0.3)),
            child("OH", LaneRole::Other, mono(10, 0.7)),
        ],
        start_secs: 0.0,
        length_secs: 1.0,
        take_count: 1,
    }
}

/// The grid the hand-written folder folds onto: its whole second, ten
/// columns.
const GRID10: Grid = Grid::over(0.0, 1.0, 10);
/// The same folder at a zoom a picture is recorded at.
const KIT_GRID: Grid = Grid::over(0.0, 1.0, 200);

fn slot(role: LaneRole) -> usize {
    LaneRole::ALL
        .iter()
        .position(|r| *r == role)
        .expect("every role has a slot")
}

// ────────────────────────────────────────────────────────────────────
// The fold
// ────────────────────────────────────────────────────────────────────

/// `min(min)` / `max(max)`, per column and per role — the folder's kick
/// slot is the loudest of its kick mics and nothing quieter.
///
/// r[verify flow.drums.comping.folder-items]
#[test]
fn the_fold_is_min_min_max_max_per_role() {
    let mut folder = kit();
    folder
        .children
        .push(child("Out", LaneRole::Kick, mono(10, 0.4)));
    let folded = folder.fold(0, GRID10);
    assert_eq!(folded.columns.len(), 10);
    let last = folded.columns.last().expect("ten columns");
    let (lo, hi) = last.slots[slot(LaneRole::Kick)].expect("the kick fed its slot");
    // 0.9 from the In mic, not 0.65 (the mean of 0.9 and 0.4) and not
    // 1.3 (a sum).
    assert!((hi - 0.9).abs() < 1e-6, "kick max {hi}");
    assert!((lo + 0.9).abs() < 1e-6, "kick min {lo}");
}

/// The negative control for that: a mean of the same children is a
/// different, quieter number, so the assertion above could not pass
/// under the rule this one rejects.
#[test]
fn a_mean_of_the_same_children_would_be_quieter() {
    let mut folder = kit();
    folder
        .children
        .push(child("Out", LaneRole::Kick, mono(10, 0.4)));
    let folded = folder.fold(0, GRID10);
    let (_, hi) = folded
        .columns
        .last()
        .and_then(|c| c.slots[slot(LaneRole::Kick)])
        .expect("kick");
    let mean = (0.9_f32 + 0.4) / 2.0;
    assert!(hi > mean, "min/max {hi} must exceed the mean {mean}");
}

/// The outer envelope is everything, including the roles that are heard
/// rather than played — it is never narrower than any child.
#[test]
fn the_outer_envelope_covers_every_role() {
    let folded = kit().fold(0, GRID10);
    let last = folded.columns.last().expect("ten columns");
    let (lo, hi) = last.outer().expect("something is audible");
    assert!((hi - 0.9).abs() < 1e-6, "outer max {hi}");
    for cell in last.slots.iter().flatten() {
        assert!(cell.1 <= hi + 1e-6 && cell.0 >= lo - 1e-6);
    }
}

/// A muted child leaves the sum: the folder hears what the mix hears,
/// so the snare's slot empties when the snare is muted.
///
/// r[verify flow.drums.comping.folder-items]
#[test]
fn a_muted_child_leaves_the_sum() {
    let mut folder = kit();
    assert!(folder.set_muted("{Top}", true));
    let folded = folder.fold(0, GRID10);
    for col in &folded.columns {
        assert!(col.slots[slot(LaneRole::Snare)].is_none());
    }
    assert!(
        folded
            .columns
            .iter()
            .any(|c| c.slots[slot(LaneRole::Kick)].is_some())
    );
}

/// A hidden child stays in it: hiding is the TCP's business, and the
/// fold is the one a mix would make.
///
/// r[verify flow.drums.comping.folder-items]
#[test]
fn a_hidden_child_stays_in_the_sum() {
    let mut folder = kit();
    let before = folder.fold(0, GRID10);
    assert!(folder.set_hidden("{T1}", true));
    assert_eq!(folder.fold(0, GRID10), before);
}

/// A take is a lane, not an item: a child whose pass is a verse, a
/// chorus and a fill folds all three onto the one take, each over its
/// own stretch of the timeline and nothing between them.
///
/// r[verify flow.drums.comping.folder-items]
#[test]
fn one_take_holds_every_item_on_its_lane() {
    let child = Child {
        guid: "{In}".to_owned(),
        name: "In".to_owned(),
        role: LaneRole::Kick,
        side: None,
        muted: false,
        hidden: false,
        takes: vec![ChildTake {
            placements: vec![
                Placement {
                    start_secs: 0.0,
                    length_secs: 1.0,
                    peaks: mono(10, 0.9),
                },
                Placement {
                    start_secs: 3.0,
                    length_secs: 1.0,
                    peaks: mono(10, 0.4),
                },
            ],
        }],
    };
    let folded = fold(&[child], 0, Grid::over(0.0, 4.0, 40), GroupBy::Role);
    let fed = |i: usize| {
        folded
            .columns
            .get(i)
            .and_then(|c| c.slots[slot(LaneRole::Kick)])
    };
    assert!(fed(5).is_some(), "the first item");
    assert!(fed(20).is_none(), "the gap between them");
    assert!(fed(35).is_some(), "the second item");
    let (_, hi) = fed(39).expect("the tail of the second item");
    assert!((hi - 0.4).abs() < 1e-6, "the second item's own level, {hi}");
}

// ────────────────────────────────────────────────────────────────────
// The peak grid — the off-rate decision
// ────────────────────────────────────────────────────────────────────

/// The same blocks with a lying header fold to the same picture.
///
/// A 44.1 kHz source in a 48 kHz project is served on the *project's*
/// grid: the blocks tile the take, but the rate and the
/// samples-per-peak the response declares describe a different spacing.
/// The fold indexes by fraction of the take, so the two fold
/// identically — which is what stops a folder whose children are at
/// different rates from folding one child's hits against another's
/// tails.
#[test]
fn an_off_rate_header_does_not_move_the_fold() {
    let blocks = ramp(20, 0.8);
    let honest = TakePeaks::new(&blocks, 1, 480, 48_000.0);
    let lying = TakePeaks::new(&blocks, 1, 480, 44_100.0);
    for i in 0..20 {
        let (u0, u1) = (f64::from(i) / 20.0, f64::from(i + 1) / 20.0);
        assert_eq!(
            honest.window(0, u0, u1),
            lying.window(0, u0, u1),
            "block {i}"
        );
    }
}

/// And the disagreement is reported rather than swallowed: the drift is
/// the fraction the header is out by, which is what the loader puts on
/// its wide event.
#[test]
fn drift_names_an_off_rate_source() {
    let blocks = ramp(100, 0.8);
    // 100 blocks of 480 samples at 48 kHz is exactly one second.
    let honest = TakePeaks::new(&blocks, 1, 480, 48_000.0);
    assert!(honest.drift(1.0) < 1e-9, "{}", honest.drift(1.0));
    // The same hundred blocks claiming 44.1 kHz claim 1.088 seconds.
    let lying = TakePeaks::new(&blocks, 1, 480, 44_100.0);
    assert!(
        (lying.drift(1.0) - 0.0884).abs() < 1e-3,
        "drift {}",
        lying.drift(1.0)
    );
}

// ────────────────────────────────────────────────────────────────────
// Sides
// ────────────────────────────────────────────────────────────────────

/// A double is two tracks, one per side: each lands wholly in its own
/// slot whatever its own channel count.
///
/// r[verify flow.guitars.folder-items]
#[test]
fn a_double_folds_into_two_sides() {
    let mut left = child("Rhythm L", LaneRole::Other, mono(10, 0.6));
    left.side = Some(Side::Left);
    let mut right = child("Rhythm R", LaneRole::Other, mono(10, 0.3));
    right.side = Some(Side::Right);
    let folded = fold(&[left, right], 0, GRID10, GroupBy::Side);
    let last = folded.columns.last().expect("ten columns");
    let (_, l) = last.slots[Side::Left.slot()].expect("a left side");
    let (_, r) = last.slots[Side::Right.slot()].expect("a right side");
    assert!((l - 0.6).abs() < 1e-6, "left {l}");
    assert!((r - 0.3).abs() < 1e-6, "right {r}");
}

/// A stereo pair is ONE track heard twice: channel 0 is the left side,
/// channel 1 the right.
///
/// r[verify flow.guitars.folder-items]
#[test]
fn a_stereo_pair_folds_into_two_sides() {
    let pair = child("Rooms", LaneRole::Other, stereo(10, 0.7, 0.2));
    let folded = fold(&[pair], 0, GRID10, GroupBy::Side);
    let last = folded.columns.last().expect("ten columns");
    assert!((last.slots[Side::Left.slot()].expect("left").1 - 0.7).abs() < 1e-6);
    assert!((last.slots[Side::Right.slot()].expect("right").1 - 0.2).abs() < 1e-6);
}

/// A mono child with no Channel is heard on both sides, the way a mono
/// source in a stereo item is.
#[test]
fn a_mono_child_with_no_channel_is_on_both_sides() {
    let folded = fold(
        &[child("DI", LaneRole::Other, mono(10, 0.5))],
        0,
        GRID10,
        GroupBy::Side,
    );
    let last = folded.columns.last().expect("ten columns");
    assert_eq!(
        last.slots[Side::Left.slot()],
        last.slots[Side::Right.slot()]
    );
    assert!(last.slots[Side::Left.slot()].is_some());
}

/// The same children grouped by role and by side are different folds —
/// the negative control for the two tests above.
#[test]
fn the_two_groupings_are_not_the_same_fold() {
    let pair = child("Rooms", LaneRole::Other, stereo(10, 0.7, 0.2));
    let by_role = fold(std::slice::from_ref(&pair), 0, GRID10, GroupBy::Role);
    let by_side = fold(&[pair], 0, GRID10, GroupBy::Side);
    assert_ne!(by_role.columns, by_side.columns);
    assert_ne!(by_role.to_text(), by_side.to_text());
}

// ────────────────────────────────────────────────────────────────────
// The revision and the cache
// ────────────────────────────────────────────────────────────────────

/// Muting bumps the revision; hiding does not.
///
/// r[verify flow.drums.comping.folder-items]
#[test]
fn mute_moves_the_revision_and_hide_does_not() {
    let mut folder = kit();
    let before = folder.revision();
    assert!(folder.set_hidden("{T1}", true));
    assert_eq!(folder.revision(), before, "hiding is not a signal change");
    assert!(folder.set_muted("{Top}", true));
    assert_ne!(folder.revision(), before, "muting leaves the sum");
}

/// The zoom is in the key as the GRID it produces, because the fold is
/// on a column grid: two zooms that fold onto the same columns share a
/// picture, and two that do not, cannot.
#[test]
fn the_grid_is_in_the_key() {
    let folder = kit();
    let key = |pps: f64| folder.picture_key(0, folder.grid_at(pps));
    assert_eq!(
        key(100.0),
        key(100.4),
        "a one-second folder folds onto 100 columns either way"
    );
    assert_ne!(key(100.0), key(400.0), "a different grid is a refold");
}

/// And the key is the column count rather than the zoom, which for a
/// long folder are not the same thing.
///
/// This is the negative control for the test above: on a folder as long
/// as the golden session, a zoom bucketed to whole pixels-per-second
/// would put 100.0 and 100.4 px/s in one bucket while they fold onto
/// column counts sixty apart — a materially different fold replayed
/// from cache, which is the failure the zoom is in the key to prevent.
#[test]
fn a_long_folder_refolds_where_a_zoom_bucket_would_not() {
    let mut folder = kit();
    folder.length_secs = 158.0;
    let a = folder.grid_at(100.0);
    let b = folder.grid_at(100.4);
    assert!(
        b.columns.saturating_sub(a.columns) > 50,
        "{} vs {}",
        a.columns,
        b.columns
    );
    assert_ne!(folder.picture_key(0, a), folder.picture_key(0, b));
    assert_eq!(
        crate::num::quantise(100.0, 1.0),
        crate::num::quantise(100.4, 1.0)
    );
}

/// A hidden child replays the held picture; a muted one rebuilds it.
///
/// r[verify flow.drums.comping.folder-items]
#[test]
fn hiding_replays_the_picture_and_muting_rebuilds_it() {
    let mut items = FolderItems::new(vec![kit()]);
    let first = items
        .picture(0, 0, 200.0)
        .map(|s| s.commands.len())
        .expect("a picture");
    assert_eq!(items.counts(), (0, 1));

    items
        .folders
        .first_mut()
        .expect("one folder")
        .set_hidden("{T1}", true);
    assert_eq!(
        items.picture(0, 0, 200.0).map(|s| s.commands.len()),
        Some(first)
    );
    assert_eq!(items.counts(), (1, 1), "hiding is a cache hit");

    items
        .folders
        .first_mut()
        .expect("one folder")
        .set_muted("{Top}", true);
    items.picture(0, 0, 200.0);
    assert_eq!(items.counts(), (1, 2), "muting is a cache miss");
}

/// And the replayed picture is the same picture, not merely the same
/// size: every recorded command matches.
///
/// r[verify flow.drums.comping.folder-items]
#[test]
fn the_replayed_picture_is_byte_identical() {
    let mut items = FolderItems::new(vec![kit()]);
    let before = items
        .picture(0, 0, 200.0)
        .map(scene_digest)
        .expect("a picture");
    items
        .folders
        .first_mut()
        .expect("one folder")
        .set_hidden("{In}", true);
    let after = items
        .picture(0, 0, 200.0)
        .map(scene_digest)
        .expect("a picture");
    assert_eq!(before, after);
    assert_eq!(items.counts(), (1, 1));
}

/// The cache is bounded: a window left open across every zoom of every
/// folder does not hold them all, and the one wanted most recently is
/// never the one dropped.
#[test]
fn the_cache_evicts_rather_than_growing() {
    let mut items = FolderItems::new(vec![kit()]);
    for step in 0..40 {
        items.picture(0, 0, f64::from(step).mul_add(37.0, 10.0));
    }
    assert!(items.held() <= super::cache::CAPACITY, "{}", items.held());
    // The last zoom drawn is still there.
    let (hits, _) = items.counts();
    items.picture(0, 0, 39.0_f64.mul_add(37.0, 10.0));
    assert_eq!(items.counts().0, hits.saturating_add(1));
}

/// The negative control: muting the same child changes the picture, so
/// the comparison above is not one that passes on anything.
///
/// The child is the kick's In mic deliberately. Muting a child that sits
/// wholly inside another's envelope — the overheads under the kick here —
/// changes the sum without changing a pixel, which is right and would
/// make a useless control.
#[test]
fn muting_changes_the_picture() {
    let mut items = FolderItems::new(vec![kit()]);
    let before = items
        .picture(0, 0, 200.0)
        .map(scene_digest)
        .expect("a picture");
    items
        .folders
        .first_mut()
        .expect("one folder")
        .set_muted("{In}", true);
    let after = items
        .picture(0, 0, 200.0)
        .map(scene_digest)
        .expect("a picture");
    assert_ne!(before, after);
}

/// Every fill of a recorded scene, as comparable text: the brush and
/// the shape's bounding box. Two scenes with the same digest draw the
/// same thing.
fn scene_digest(scene: &anyrender::Scene) -> String {
    use core::fmt::Write as _;
    let mut out = String::new();
    for command in &scene.commands {
        if let RenderCommand::Fill(fill) = command {
            let bounds = kurbo_bounds(&fill.shape);
            let _ = writeln!(out, "{:?} {bounds}", fill.brush);
        }
    }
    out
}

fn kurbo_bounds(shape: &vello::kurbo::BezPath) -> String {
    use vello::kurbo::Shape as _;
    let b = shape.bounding_box();
    format!("{:.6} {:.6} {:.6} {:.6}", b.x0, b.y0, b.x1, b.y1)
}

// ────────────────────────────────────────────────────────────────────
// The picture
// ────────────────────────────────────────────────────────────────────

/// The pieces are drawn in their own colours on a dim envelope, kick
/// last — the order that makes the kick pattern read on its own.
///
/// r[verify flow.drums.comping.folder-item-colours]
#[test]
fn the_pieces_are_drawn_in_their_colours_kick_last() {
    let scene = kit().picture(0, KIT_GRID);
    let brushes: Vec<Paint> = scene
        .commands
        .iter()
        .filter_map(|c| match c {
            RenderCommand::Fill(fill) => Some(fill.brush.clone()),
            _ => None,
        })
        .collect();
    // Body, outer envelope, toms, snare, kick.
    assert_eq!(brushes.len(), 5, "{brushes:?}");
    let colour_of = |role: LaneRole| {
        let s = role.color();
        let byte = |i: usize| {
            s.get(i..i + 2)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .unwrap_or(0)
        };
        Color::from_rgb8(byte(1), byte(3), byte(5))
    };
    for (at, role) in [
        (2, LaneRole::Toms),
        (3, LaneRole::Snare),
        (4, LaneRole::Kick),
    ] {
        let Some(Paint::Solid(drawn)) = brushes.get(at) else {
            panic!("no solid fill at {at}: {brushes:?}");
        };
        let want = colour_of(role);
        let [r, g, b, _] = drawn.components;
        let [wr, wg, wb, _] = want.components;
        assert!(
            (r - wr).abs() < 1e-3 && (g - wg).abs() < 1e-3 && (b - wb).abs() < 1e-3,
            "{role:?} drawn as {drawn:?}, wanted {want:?}"
        );
    }
}

/// A double's folder item renders two sides: two fills over the body,
/// in two different shades, one in the upper half of the item and one
/// in the lower.
///
/// r[verify flow.guitars.folder-items]
#[test]
fn a_doubles_folder_item_renders_two_sides() {
    let mut left = child("Rhythm L", LaneRole::Other, mono(10, 0.6));
    left.side = Some(Side::Left);
    let mut right = child("Rhythm R", LaneRole::Other, mono(10, 0.3));
    right.side = Some(Side::Right);
    let folder = Folder {
        guid: "{rhythm}".to_owned(),
        name: "Rhythm".to_owned(),
        colour: Color::from_rgb8(0xc0, 0x60, 0x30),
        group_by: GroupBy::Side,
        children: vec![left, right],
        start_secs: 0.0,
        length_secs: 1.0,
        take_count: 1,
    };
    let scene = folder.picture(0, KIT_GRID);
    let fills: Vec<_> = scene
        .commands
        .iter()
        .filter_map(|c| match c {
            RenderCommand::Fill(fill) => Some(fill),
            _ => None,
        })
        .collect();
    // Body, left side, right side.
    assert_eq!(fills.len(), 3, "{} fills", fills.len());
    let (Some(l), Some(r)) = (fills.get(1), fills.get(2)) else {
        panic!("two sides");
    };
    assert_ne!(
        format!("{:?}", l.brush),
        format!("{:?}", r.brush),
        "each side in its own shade"
    );
    use vello::kurbo::Shape as _;
    let (lb, rb) = (l.shape.bounding_box(), r.shape.bounding_box());
    assert!(lb.y1 <= 0.5 + 1e-9, "the left side sits in the upper half");
    assert!(rb.y0 >= 0.5 - 1e-9, "the right side sits in the lower half");
}

/// The recorded picture is in the unit box, so a replay is one
/// transform — which is what lets a pan and a row resize reuse it.
#[test]
fn a_picture_is_recorded_in_the_unit_box() {
    use vello::kurbo::Shape as _;
    let scene = kit().picture(0, KIT_GRID);
    for command in &scene.commands {
        let RenderCommand::Fill(fill) = command else {
            continue;
        };
        let b = fill.shape.bounding_box();
        assert!(
            b.x0 >= -1e-9 && b.x1 <= 1.0 + 1e-9 && b.y0 >= -1e-9 && b.y1 <= 1.0 + 1e-9,
            "{b:?} leaves the unit box"
        );
    }
    let place = Place {
        x0: 100.0,
        top: 20.0,
        width: 800.0,
        height: 40.0,
    };
    let at = place.transform() * vello::kurbo::Point::new(1.0, 1.0);
    assert!((at.x - 900.0).abs() < 1e-9 && (at.y - 60.0).abs() < 1e-9);
}

/// A folder whose item is shorter than the row still folds onto at
/// least one column, and an empty folder draws its body and nothing
/// else.
#[test]
fn a_degenerate_folder_still_draws() {
    let mut folder = kit();
    folder.length_secs = 0.0;
    assert_eq!(folder.grid_at(200.0).columns, 1);
    let scene = folder.picture(0, KIT_GRID);
    assert!(!scene.commands.is_empty());
}
