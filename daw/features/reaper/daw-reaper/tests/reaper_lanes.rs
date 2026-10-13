//! Fixed lanes and named comps against a real REAPER.
//!
//! The REAPER half of the round trip in `daw-standalone/tests/lanes.rs`:
//! the same lane layout and the same named comp with two comp areas,
//! written through the facade and read back from what REAPER itself
//! holds — the lane fields via `I_NUMFIXEDLANES` / `C_LANEPLAYS:N` /
//! `P_LANENAME:n`, the comping via the track's state chunk, the item's
//! lane via `I_FIXEDLANE`.
//!
//! Run with: `just reaper daw-test lanes`

use daw::test::reaper_test;
use daw_proto::track::{Comp, CompArea, LaneComping, LaneDisplay};
use daw_proto::{Duration, PositionInSeconds};

fn area(comp_lane: u32, source_lane: u32, start: f64, end: f64) -> CompArea {
    CompArea {
        start: PositionInSeconds::from_seconds(start),
        end: PositionInSeconds::from_seconds(end),
        source_lane,
        comp_lane,
        fade_in: Duration::from_seconds(0.01),
        fade_out: Duration::from_seconds(0.02),
    }
}

#[reaper_test(isolated)]
async fn a_lane_layout_and_a_named_comp_read_back_identical(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Lead Vox", None).await?;
    let item = track
        .items()
        .add(
            PositionInSeconds::from_seconds(0.0),
            Duration::from_seconds(1.0),
        )
        .await?;

    // No lanes yet: the item has no lane and cannot be put on one.
    assert_eq!(track.info().await?.lane_count, 0);
    assert_eq!(item.info().await?.fixed_lane, None);
    assert!(item.set_fixed_lane(0).await.is_err());

    track.set_lane_count(3).await?;
    for (lane, name) in ["Take 1", "Take 2", "Take 3"].iter().enumerate() {
        track.set_lane_name(lane as u32, name).await?;
    }
    let comp = track.create_comp("COMP").await?;
    assert_eq!(comp, 3, "the comp is the lane after the takes");
    let areas = vec![area(comp, 0, 0.0, 0.4), area(comp, 2, 0.4, 1.0)];
    track.set_comp_areas(areas.clone()).await?;
    track.set_lane_play_mask(1 << comp).await?;
    item.set_fixed_lane(2).await?;

    let info = track.info().await?;
    assert_eq!(info.lane_count, 4);
    assert_eq!(info.lane_play_mask, 1 << comp);
    assert_eq!(info.lane_names, vec!["Take 1", "Take 2", "Take 3", "COMP"]);
    // The display mode is REAPER's own (C_LANESETTINGS's big-lanes bit is
    // set by default in this rig) and nothing here sets it; what this
    // asserts is the C_LANESCOLLAPSED reading — uncollapsed lanes are
    // never `One`.
    assert!(
        matches!(info.lane_display, LaneDisplay::Big | LaneDisplay::Small),
        "uncollapsed lanes read as {:?}",
        info.lane_display
    );
    let expected = LaneComping {
        record_lane: None,
        comp_lane: Some(comp),
        last_comp_lane: None,
        areas: areas.clone(),
    };
    assert_eq!(track.comping().await?, expected);
    assert_eq!(
        track.comps().await?,
        vec![Comp {
            lane: comp,
            name: "COMP".into(),
            is_active: true,
            areas,
        }]
    );
    assert_eq!(item.info().await?.fixed_lane, Some(2));
    Ok(())
}

#[reaper_test(isolated)]
async fn renaming_and_switching_the_active_comp(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let project = ctx.project().clone();
    let track = project.tracks().add("Lead Vox", None).await?;
    track.set_lane_count(2).await?;
    let first = track.create_comp("COMP").await?;
    let edit = track.create_comp("EDIT").await?;
    track.set_lane_name(first, "COMP v2").await?;

    let names: Vec<(String, bool)> = track
        .comps()
        .await?
        .into_iter()
        .map(|c| (c.name, c.is_active))
        .collect();
    assert_eq!(
        names,
        vec![("COMP v2".to_string(), false), ("EDIT".to_string(), true)]
    );
    let comping = track.comping().await?;
    assert_eq!(
        (comping.comp_lane, comping.last_comp_lane),
        (Some(edit), Some(first))
    );

    track.set_active_comp(Some(first)).await?;
    let comping = track.comping().await?;
    assert_eq!(
        (comping.comp_lane, comping.last_comp_lane),
        (Some(first), Some(edit))
    );

    // No lane playing is a state REAPER holds, distinct from lane 0.
    track.set_lane_count(3).await?;
    track.set_lane_play_mask(0).await?;
    assert_eq!(track.info().await?.lane_play_mask, 0);
    track.set_lane_play_mask(0b101).await?;
    assert_eq!(track.info().await?.lane_play_mask, 0b101);

    // Lanes off: everything lane-shaped goes with them, the comping too.
    track.set_lane_count(0).await?;
    let info = track.info().await?;
    assert_eq!((info.lane_count, info.lane_play_mask), (0, 0));
    assert!(info.lane_names.is_empty());
    assert_eq!(track.comping().await?, LaneComping::default());
    Ok(())
}
