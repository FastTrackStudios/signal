//! Writing a quantize to a real host — the standalone backend, through
//! the same `daw` facade the REAPER build uses.
//!
//! The claim under test is phase coherency: a group of mics on one
//! source must be cut in exactly the same places, or the source smears
//! at every join. That is the whole reason drum editing is done by
//! splitting rather than warping, and it is the thing a unit test on the
//! planner cannot check — it is a property of what reaches the host.

#![cfg(feature = "daw")]

use daw::service::item::Items;
use daw::service::midi::Midi;
use daw::service::{ItemRef, ProjectContext, TakeRef, Takes, TrackRef, Tracks};
use daw::standalone::sync::Standalone;
use expression_editor_audio::apply_quantize::{GroupError, apply_split, group_start};
use expression_editor_audio::detect::Transient;
use expression_editor_audio::gate::Hit;
use expression_editor_audio::quantize::{QuantizeConfig, SplitConfig, plan};

const SR: f64 = 44100.0;
const GRID: f64 = 0.25;
const TAKE_LEN: f64 = 2.0;

fn project() -> Standalone {
    let daw = Standalone::new();
    daw.seed_project(daw::service::ProjectInfo {
        guid: "quantize".into(),
        name: "Quantize".into(),
        path: String::new(),
    });
    daw
}

/// One item per track, all starting together — a close mic and two
/// overheads on the same performance.
fn mics(daw: &Standalone, names: &[&str], start: f64) -> Vec<ItemRef> {
    let ctx = ProjectContext::Current;
    names
        .iter()
        .map(|name| {
            let track = Tracks::add(daw, ctx.clone(), name, None).unwrap();
            let loc =
                Midi::create_midi_item(daw, ctx.clone(), TrackRef::Guid(track), start, TAKE_LEN)
                    .expect("item");
            loc.item
        })
        .collect()
}

fn transients(times: &[f64]) -> Vec<Transient> {
    times
        .iter()
        .map(|&at| Transient {
            at,
            loudness: 0.9,
            crest_db: 12.0,
            hit: Hit {
                sample: (at * SR) as usize,
                peak: 0.9,
                rms: 0.6,
                crest_db: 12.0,
            },
        })
        .collect()
}

/// Every item on `track_name`'s track, as (position, length, offset).
fn pieces_on(daw: &Standalone, item: &ItemRef) -> Vec<(f64, f64, f64)> {
    let ctx = ProjectContext::Current;
    let info = daw.get_item(ctx.clone(), item.clone()).expect("item");
    let mut out: Vec<(f64, f64, f64)> = daw
        .get_items(ctx.clone(), TrackRef::Guid(info.track_guid.clone()))
        .into_iter()
        .map(|i| {
            let offset = daw
                .get_take(ctx.clone(), ItemRef::Guid(i.guid.clone()), TakeRef::Active)
                .map(|t| t.start_offset.as_seconds())
                .unwrap_or(0.0);
            (i.position.as_seconds(), i.length.as_seconds(), offset)
        })
        .collect();
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    out
}

/// A plan for hits that are all 20 ms late.
fn late_plan() -> expression_editor_audio::quantize::Plan {
    let late: Vec<f64> = (1..5).map(|i| i as f64 * GRID + 0.02).collect();
    plan(
        &transients(&late),
        QuantizeConfig {
            grid_secs: GRID,
            ..QuantizeConfig::default()
        },
    )
}

#[test]
fn every_mic_is_cut_in_exactly_the_same_places() {
    // The property the whole group path exists for. Two mics cut a few
    // samples apart smear the source at every join, and no amount of
    // crossfading hides it.
    let daw = project();
    let items = mics(&daw, &["Kick In", "Kick Out", "OH L"], 0.0);
    let p = late_plan();

    let applied = apply_split(
        &daw,
        ProjectContext::Current,
        &items,
        &p.splits(TAKE_LEN, SplitConfig::default()),
        SplitConfig::default(),
    )
    .expect("applied");
    assert_eq!(applied.items, 3);

    let first = pieces_on(&daw, &items[0]);
    assert!(first.len() > 1, "the item was not split: {first:?}");
    for item in &items[1..] {
        assert_eq!(
            pieces_on(&daw, item),
            first,
            "this mic was cut differently from the first"
        );
    }
}

#[test]
fn each_piece_lands_so_its_transient_is_on_the_grid() {
    let daw = project();
    let items = mics(&daw, &["Snare"], 0.0);
    let p = late_plan();
    let split = SplitConfig {
        leading_pad_secs: 0.008,
        crossfade_secs: 0.006,
    };
    apply_split(
        &daw,
        ProjectContext::Current,
        &items,
        &p.splits(TAKE_LEN, split),
        split,
    )
    .expect("applied");

    // A piece's transient sits `pad` into it, so the transient's new
    // position is the piece's position plus the pad. Every one of them
    // must land on a division.
    let placed = pieces_on(&daw, &items[0]);
    for m in &p.moves {
        let want = m.division;
        assert!(
            placed
                .iter()
                .any(|(pos, _, _)| (pos + split.leading_pad_secs - want).abs() < 1e-6),
            "no piece puts a transient on {want}: {placed:?}"
        );
    }
}

#[test]
fn the_leading_pad_moves_the_cut_and_not_the_snap() {
    // Conflating the two makes the audio arrive `pad` early and flams
    // every hit against the rest of the kit. The check: changing the pad
    // must move where pieces *read from*, and not where their transients
    // land.
    let p = late_plan();

    // Only pieces that *carry* a transient are subject to this: the
    // lead-in has none, so its length changes with the pad and nothing
    // about it snaps anywhere.
    let landed = |pad: f64| {
        let daw = project();
        let items = mics(&daw, &["Tom"], 0.0);
        let cfg = SplitConfig {
            leading_pad_secs: pad,
            crossfade_secs: 0.0,
        };
        let split = p.splits(TAKE_LEN, cfg);
        apply_split(&daw, ProjectContext::Current, &items, &split, cfg).expect("applied");
        let host = pieces_on(&daw, &items[0]);
        assert_eq!(host.len(), split.len(), "one host item per planned piece");
        host.into_iter()
            .zip(split)
            .filter(|(_, piece)| piece.transient.is_some())
            .collect::<Vec<_>>()
    };

    let small = landed(0.002);
    let large = landed(0.020);
    assert_eq!(small.len(), large.len());
    assert!(!small.is_empty());
    for ((a, _), (b, _)) in small.iter().zip(&large) {
        // A transient sits `pad` into its piece, so its landing point is
        // the piece position plus the pad — and that must be identical
        // whatever the pad is.
        assert!(
            ((a.0 + 0.002) - (b.0 + 0.020)).abs() < 1e-6,
            "the snap point moved with the pad: {a:?} vs {b:?}"
        );
        // And the source offset really did change: a bigger pad reads
        // from earlier in the file.
        assert!(
            a.2 > b.2,
            "a bigger pad must read from earlier: {} vs {}",
            a.2,
            b.2
        );
    }
}

#[test]
fn pieces_overlap_by_a_crossfade_so_there_is_something_to_fade_between() {
    let daw = project();
    let items = mics(&daw, &["Snare"], 0.0);
    let p = late_plan();
    let cfg = SplitConfig {
        leading_pad_secs: 0.005,
        crossfade_secs: 0.010,
    };
    apply_split(
        &daw,
        ProjectContext::Current,
        &items,
        &p.splits(TAKE_LEN, cfg),
        cfg,
    )
    .expect("applied");

    let placed = pieces_on(&daw, &items[0]);
    for pair in placed.windows(2) {
        let (pos_a, len_a, _) = pair[0];
        let (pos_b, _, _) = pair[1];
        let overlap = pos_a + len_a - pos_b;
        assert!(
            overlap > 0.0,
            "a butt join is a click — pieces must overlap: {pair:?}"
        );
    }
}

#[test]
fn an_item_that_starts_late_is_still_cut_relative_to_itself() {
    // Positions written to the host are absolute; everything the planner
    // produces is relative to the take. Getting this wrong puts every
    // piece of a mid-song item at the top of the timeline.
    let start = 12.5;
    let daw = project();
    let items = mics(&daw, &["Snare"], start);
    let p = late_plan();
    apply_split(
        &daw,
        ProjectContext::Current,
        &items,
        &p.splits(TAKE_LEN, SplitConfig::default()),
        SplitConfig::default(),
    )
    .expect("applied");

    let placed = pieces_on(&daw, &items[0]);
    assert!(
        placed.iter().all(|(pos, _, _)| *pos >= start - 0.1),
        "pieces landed before the item did: {placed:?}"
    );
}

#[test]
fn a_ragged_group_is_refused_rather_than_guessed_at() {
    // Items that do not share a start are not the same performance, so
    // "the same cut" means different audio on each. Refusing beats
    // producing a smeared result that looks like the algorithm is bad.
    let daw = project();
    let mut items = mics(&daw, &["Kick"], 0.0);
    items.extend(mics(&daw, &["OH"], 0.5));

    let err = group_start(&daw, ProjectContext::Current, &items).unwrap_err();
    match err {
        GroupError::Ragged { spread_secs } => {
            assert!((spread_secs - 0.5).abs() < 1e-6, "reported {spread_secs}")
        }
        other => panic!("expected Ragged, got {other:?}"),
    }
}

#[test]
fn a_hair_of_disagreement_is_not_ragged() {
    // Items nudged by a fraction of a millisecond are still the same
    // take, and refusing them would make the feature unusable on any
    // project a human has touched.
    let daw = project();
    let mut items = mics(&daw, &["Kick"], 0.0);
    items.extend(mics(&daw, &["OH"], 0.0002));
    assert!(group_start(&daw, ProjectContext::Current, &items).is_ok());
}

#[test]
fn an_empty_group_is_refused() {
    let daw = project();
    assert_eq!(
        group_start(&daw, ProjectContext::Current, &[]).unwrap_err(),
        GroupError::Empty
    );
}

#[test]
fn a_plan_with_nothing_in_it_leaves_the_items_alone() {
    // Quantizing a take with no detected hits must not rebuild it as one
    // piece — that is a no-op edit that still costs an undo step and
    // still marks the project dirty.
    let daw = project();
    let items = mics(&daw, &["Snare"], 0.0);
    let before = pieces_on(&daw, &items[0]);

    let empty = plan(&[], QuantizeConfig::default());
    let applied = apply_split(
        &daw,
        ProjectContext::Current,
        &items,
        &empty.splits(TAKE_LEN, SplitConfig::default()),
        SplitConfig::default(),
    )
    .expect("applied");

    assert_eq!(applied.pieces, 0);
    assert_eq!(pieces_on(&daw, &items[0]), before);
}

// r[verify drums.quantize.apply]
#[test]
fn warp_writes_one_marker_map_to_every_mic() {
    use daw::service::StretchMarkers;
    use expression_editor_audio::apply_quantize::apply_warp;

    let daw = project();
    let items = mics(&daw, &["Kick In", "Kick Out", "OH L"], 0.0);
    let p = late_plan();
    let frames = (TAKE_LEN * 100.0) as usize;
    let alignment = p.alignment(frames, 100.0).expect("alignment");

    let applied =
        apply_warp(&daw, ProjectContext::Current, &items, &alignment).expect("warp applied");
    assert_eq!(applied.items, 3);
    assert!(applied.pieces > 0, "markers were written");

    let markers_of = |item: &ItemRef| {
        daw.get_stretch_markers(ProjectContext::Current, item.clone(), TakeRef::Active)
    };
    let first = markers_of(&items[0]);
    assert!(!first.is_empty());
    for item in &items[1..] {
        assert_eq!(markers_of(item), first, "identical maps on every mic");
    }
}
