//! Creating tracks, items, markers and regions with a caller's GUID, moving
//! one track without touching the selection, and every item/take setter
//! announcing itself on the event bus.
//!
//! These are what a peer of a shared session needs from its own engine: an
//! object another engine made is re-created here under the *same* GUID (the
//! shared document keys everything by it), and the bridge re-reads an item
//! whenever the bus says it changed — so a setter that stays silent is an
//! edit that never reaches the other peers.

#![cfg(feature = "bootstrap")]

use daw_proto::event_bus::{BusFilter, DawEvent};
use daw_proto::item::{ItemEvent, TakeEvent};
use daw_proto::marker::MarkerEvent;
use daw_proto::primitives::{BeatAttachMode, Duration, PositionInSeconds};
use daw_proto::region::RegionEvent;
use daw_proto::{
    DawError, FadeShape, ItemRef, ItemSpan, Items, Markers, ProjectContext, ProjectInfo, Regions,
    TakeMarkerCreate, TakeMarkerUpdate, TakeRef, Takes, TimeRange, TrackRef, Tracks,
};
use daw_standalone::bootstrap::build_in_process_daw;
use daw_standalone::sync::Standalone;

const PROJECT: &str = "test-proj";
const TRACK_GUID: &str = "{0C0FFEE0-0000-4000-8000-000000000001}";
const BARE_GUID: &str = "0c0ffee0-0000-4000-8000-000000000002";
const ITEM_GUID: &str = "{1C0FFEE0-0000-4000-8000-000000000001}";
const MARKER_GUID: &str = "{2C0FFEE0-0000-4000-8000-000000000001}";
const REGION_GUID: &str = "{3C0FFEE0-0000-4000-8000-000000000001}";

fn seeded() -> Standalone {
    let s = Standalone::new();
    s.seed_project(ProjectInfo {
        guid: PROJECT.into(),
        name: "test".into(),
        path: String::new(),
    });
    s
}

fn ctx() -> ProjectContext {
    ProjectContext::Project(PROJECT.into())
}

fn span(position: f64, length: f64) -> ItemSpan {
    ItemSpan::new(
        PositionInSeconds::from_seconds(position),
        Duration::from_seconds(length),
    )
}

// ────────────────────────────────────────────────────────────────────
// Tracks
// ────────────────────────────────────────────────────────────────────

#[test]
fn a_track_made_with_a_guid_keeps_it() {
    let daw = seeded();
    Tracks::add(&daw, ctx(), "First", None).unwrap();
    let got = Tracks::add_with_guid(&daw, ctx(), TRACK_GUID, "Peer", Some(0)).unwrap();
    assert_eq!(got, TRACK_GUID);
    let bare = Tracks::add_with_guid(&daw, ctx(), BARE_GUID, "Bare", None).unwrap();
    assert_eq!(bare, BARE_GUID, "any spelling is kept verbatim");

    let track = Tracks::get(&daw, ctx(), TrackRef::Guid(TRACK_GUID.into())).unwrap();
    assert_eq!(track.name, "Peer");
    assert_eq!(track.index, 0, "at_index is honoured");
    let names: Vec<_> = Tracks::all(&daw, ctx())
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, ["Peer", "First", "Bare"]);
}

#[test]
fn a_track_guid_already_in_the_project_is_refused() {
    let daw = seeded();
    Tracks::add_with_guid(&daw, ctx(), TRACK_GUID, "Peer", None).unwrap();
    let err = Tracks::add_with_guid(&daw, ctx(), TRACK_GUID, "Again", None).unwrap_err();
    assert!(matches!(err, DawError::AlreadyExists(_)), "{err:?}");
    assert_eq!(Tracks::count(&daw, ctx()), 1, "no second track");

    let err = Tracks::add_with_guid(&daw, ctx(), "", "Empty", None).unwrap_err();
    assert!(matches!(err, DawError::OperationFailed(_)), "{err:?}");
    let err = Tracks::add_with_guid(&daw, ctx(), "has space", "Spaced", None).unwrap_err();
    assert!(matches!(err, DawError::OperationFailed(_)), "{err:?}");
    assert_eq!(Tracks::count(&daw, ctx()), 1);
}

#[test]
fn move_to_moves_one_track_and_leaves_the_selection_alone() {
    let daw = seeded();
    let a = Tracks::add(&daw, ctx(), "A", None).unwrap();
    let b = Tracks::add(&daw, ctx(), "B", None).unwrap();
    let c = Tracks::add(&daw, ctx(), "C", None).unwrap();
    Tracks::set_folder_depth(&daw, ctx(), TrackRef::Guid(a.clone()), 0).unwrap();
    Tracks::select_exclusive(&daw, ctx(), TrackRef::Guid(b.clone())).unwrap();

    Tracks::move_to(&daw, ctx(), TrackRef::Guid(a.clone()), 2).unwrap();
    let order: Vec<_> = Tracks::all(&daw, ctx())
        .into_iter()
        .map(|t| t.guid)
        .collect();
    assert_eq!(order, [b.clone(), c.clone(), a.clone()]);

    Tracks::move_to(&daw, ctx(), TrackRef::Guid(a.clone()), 0).unwrap();
    let order: Vec<_> = Tracks::all(&daw, ctx())
        .into_iter()
        .map(|t| t.guid)
        .collect();
    assert_eq!(order, [a.clone(), b.clone(), c.clone()]);

    let selected: Vec<_> = Tracks::selected(&daw, ctx())
        .into_iter()
        .map(|t| t.guid)
        .collect();
    assert_eq!(selected, [b], "the selection is not the mover's");

    let err = Tracks::move_to(&daw, ctx(), TrackRef::Guid(a), 3).unwrap_err();
    assert!(matches!(err, DawError::OutOfRange { .. }), "{err:?}");
}

#[test]
fn move_to_keeps_folder_depths() {
    let daw = seeded();
    let folder = Tracks::add(&daw, ctx(), "Folder", None).unwrap();
    let child = Tracks::add(&daw, ctx(), "Child", None).unwrap();
    let other = Tracks::add(&daw, ctx(), "Other", None).unwrap();
    Tracks::set_folder_depth(&daw, ctx(), TrackRef::Guid(folder.clone()), 1).unwrap();
    Tracks::set_folder_depth(&daw, ctx(), TrackRef::Guid(child.clone()), -1).unwrap();

    Tracks::move_to(&daw, ctx(), TrackRef::Guid(other.clone()), 0).unwrap();
    let depths: Vec<_> = Tracks::all(&daw, ctx())
        .into_iter()
        .map(|t| (t.guid, t.folder_depth))
        .collect();
    assert_eq!(depths, [(other, 0), (folder, 1), (child, -1)]);
}

// ────────────────────────────────────────────────────────────────────
// Items
// ────────────────────────────────────────────────────────────────────

#[test]
fn an_item_made_with_a_guid_keeps_it() {
    let daw = seeded();
    let track = Tracks::add(&daw, ctx(), "T", None).unwrap();
    let got = Items::add_item_with_guid(
        &daw,
        ctx(),
        TrackRef::Guid(track.clone()),
        ITEM_GUID,
        span(1.5, 2.0),
    )
    .unwrap();
    assert_eq!(got, ITEM_GUID);

    let item = Items::get_item(&daw, ctx(), ItemRef::Guid(ITEM_GUID.into())).unwrap();
    assert_eq!(item.track_guid, track);
    assert_eq!(item.position.as_seconds(), 1.5);
    assert_eq!(item.length.as_seconds(), 2.0);

    // The same kind of item `add_item` makes: an audio item is then a
    // take plus a source.
    let take = Takes::add_take(&daw, ctx(), ItemRef::Guid(ITEM_GUID.into())).unwrap();
    Takes::set_source_file(
        &daw,
        ctx(),
        ItemRef::Guid(ITEM_GUID.into()),
        TakeRef::Guid(take),
        "/tmp/a.wav".into(),
    )
    .unwrap();
}

#[test]
fn an_item_guid_already_in_the_project_is_refused() {
    let daw = seeded();
    let a = Tracks::add(&daw, ctx(), "A", None).unwrap();
    let b = Tracks::add(&daw, ctx(), "B", None).unwrap();
    Items::add_item_with_guid(&daw, ctx(), TrackRef::Guid(a), ITEM_GUID, span(0.0, 1.0)).unwrap();
    // On another track too: item guids are project-wide.
    let err = Items::add_item_with_guid(
        &daw,
        ctx(),
        TrackRef::Guid(b.clone()),
        ITEM_GUID,
        span(4.0, 1.0),
    )
    .unwrap_err();
    assert!(matches!(err, DawError::AlreadyExists(_)), "{err:?}");
    assert_eq!(Items::get_all_items(&daw, ctx()).len(), 1);

    let err = Items::add_item_with_guid(
        &daw,
        ctx(),
        TrackRef::Guid("no-such-track".into()),
        "{4C0FFEE0-0000-4000-8000-000000000001}",
        span(0.0, 1.0),
    )
    .unwrap_err();
    assert!(matches!(err, DawError::NotFound(_)), "{err:?}");
}

// ────────────────────────────────────────────────────────────────────
// Markers and regions
// ────────────────────────────────────────────────────────────────────

#[test]
fn markers_and_regions_made_with_a_guid_keep_it() {
    let daw = seeded();
    let m = Markers::add_with_guid(&daw, ctx(), MARKER_GUID, 4.0, "Verse").unwrap();
    let marker = Markers::get(&daw, ctx(), m).unwrap();
    assert_eq!(marker.guid.as_deref(), Some(MARKER_GUID));
    assert_eq!(marker.name, "Verse");
    assert_eq!(marker.position_seconds(), 4.0);

    let r = Regions::add_with_guid(
        &daw,
        ctx(),
        REGION_GUID,
        TimeRange::from_seconds(8.0, 16.0),
        "Chorus",
    )
    .unwrap();
    let region = Regions::get(&daw, ctx(), r).unwrap();
    assert_eq!(region.guid.as_deref(), Some(REGION_GUID));
    assert_eq!((region.start_seconds(), region.end_seconds()), (8.0, 16.0));
    assert_eq!(region.name, "Chorus");
}

#[test]
fn a_marker_or_region_guid_already_in_the_project_is_refused() {
    let daw = seeded();
    Markers::add_with_guid(&daw, ctx(), MARKER_GUID, 4.0, "Verse").unwrap();
    Regions::add_with_guid(
        &daw,
        ctx(),
        REGION_GUID,
        TimeRange::from_seconds(0.0, 1.0),
        "R",
    )
    .unwrap();

    let err = Markers::add_with_guid(&daw, ctx(), MARKER_GUID, 5.0, "Again").unwrap_err();
    assert!(matches!(err, DawError::AlreadyExists(_)), "{err:?}");
    // One namespace: markers and regions are one list in the file.
    let err = Markers::add_with_guid(&daw, ctx(), REGION_GUID, 5.0, "Clash").unwrap_err();
    assert!(matches!(err, DawError::AlreadyExists(_)), "{err:?}");
    let err = Regions::add_with_guid(
        &daw,
        ctx(),
        MARKER_GUID,
        TimeRange::from_seconds(2.0, 3.0),
        "Clash",
    )
    .unwrap_err();
    assert!(matches!(err, DawError::AlreadyExists(_)), "{err:?}");
    assert_eq!(Markers::count(&daw, ctx()), 1);
    assert_eq!(Regions::count(&daw, ctx()), 1);
}

/// A marker made the ordinary way has a guid from the start, so a bridge
/// can key it the moment it appears — not only after a save mints one.
#[test]
fn plain_add_gives_markers_and_regions_a_guid() {
    let daw = seeded();
    let m = Markers::add(&daw, ctx(), 1.0, "M").unwrap();
    let r = Regions::add(&daw, ctx(), 2.0, 3.0, "R").unwrap();
    let mg = Markers::get(&daw, ctx(), m).unwrap().guid.unwrap();
    let rg = Regions::get(&daw, ctx(), r).unwrap().guid.unwrap();
    assert!(mg.starts_with('{') && mg.ends_with('}'), "{mg}");
    assert_ne!(mg, rg);
}

// ────────────────────────────────────────────────────────────────────
// Events
// ────────────────────────────────────────────────────────────────────

async fn next_event(rx: &mut daw_control::EventStream<DawEvent>) -> eyre::Result<DawEvent> {
    let event = tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv())
        .await
        .map_err(|_| eyre::eyre!("timed out waiting for daw event"))??
        .ok_or_else(|| eyre::eyre!("daw event stream closed"))?;
    let mut out = None;
    let _ = event.map(|event| out = Some(event));
    Ok(out.expect("vox SelfRef::map runs once"))
}

/// Subscribe to the bus for `filter` and wait until the server holds the
/// sink, so nothing published after this returns can be missed.
async fn subscribe(
    bundle: &daw_standalone::bootstrap::InProcessDaw,
    filter: BusFilter,
) -> eyre::Result<daw_control::EventStream<DawEvent>> {
    let rx = bundle
        .daw
        .events()
        .subscribe(filter.for_project(PROJECT))
        .await?;
    use daw_proto::event_bus::EventBusStreamSource;
    let hub = bundle.standalone.events_hub().clone();
    for _ in 0..200 {
        if hub.subscriber_count() >= 1 {
            return Ok(rx);
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    eyre::bail!("subscription attach never landed")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn creating_with_a_guid_publishes_what_add_publishes() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let project = bundle.daw.current_project().await?;
    let mut rx = subscribe(
        &bundle,
        BusFilter {
            tracks: true,
            items: true,
            markers: true,
            regions: true,
            ..BusFilter::default()
        },
    )
    .await?;

    // Through the daw-control facade, as a bridge would.
    let track = project
        .tracks()
        .add_with_guid(TRACK_GUID, "Peer", None)
        .await?;
    assert_eq!(track.guid(), TRACK_GUID);
    let DawEvent::Track(e) = next_event(&mut rx).await? else {
        panic!("expected a track event");
    };
    assert!(
        matches!(&e.event, daw_proto::TrackEvent::Added(t) if t.guid == TRACK_GUID),
        "{e:?}"
    );

    let item = project
        .items()
        .add_with_guid(
            TRACK_GUID,
            ITEM_GUID,
            PositionInSeconds::from_seconds(1.0),
            Duration::from_seconds(2.0),
        )
        .await?;
    assert_eq!(item.guid(), ITEM_GUID);
    let DawEvent::Item(ItemEvent::Created {
        item, track_guid, ..
    }) = next_event(&mut rx).await?
    else {
        panic!("expected ItemEvent::Created");
    };
    assert_eq!(
        (item.guid.as_str(), track_guid.as_str()),
        (ITEM_GUID, TRACK_GUID)
    );

    let dup = project
        .tracks()
        .add_with_guid(TRACK_GUID, "Again", None)
        .await
        .unwrap_err();
    assert!(dup.to_string().contains("Already exists"), "{dup}");

    let m = project
        .markers()
        .add_with_guid(MARKER_GUID, 3.0, "Verse")
        .await?;
    let DawEvent::Marker(e) = next_event(&mut rx).await? else {
        panic!("expected a marker event");
    };
    assert!(
        matches!(&e.event, MarkerEvent::Added(mk) if mk.id == Some(m) && mk.guid.as_deref() == Some(MARKER_GUID)),
        "{e:?}"
    );

    let r = project
        .regions()
        .add_with_guid(REGION_GUID, 4.0, 8.0, "Chorus")
        .await?;
    let DawEvent::Region(e) = next_event(&mut rx).await? else {
        panic!("expected a region event");
    };
    assert!(
        matches!(&e.event, RegionEvent::Added(rg) if rg.id == Some(r) && rg.guid.as_deref() == Some(REGION_GUID)),
        "{e:?}"
    );

    // move_to through the facade publishes the moves.
    let other = project.tracks().add("Other", None).await?;
    let _ = next_event(&mut rx).await?; // its Added
    other.move_to(0).await?;
    let DawEvent::Track(e) = next_event(&mut rx).await? else {
        panic!("expected a track event");
    };
    assert!(
        matches!(
            &e.event,
            daw_proto::TrackEvent::Moved { new_index: 0, .. }
                | daw_proto::TrackEvent::Moved { new_index: 1, .. }
        ),
        "{e:?}"
    );
    Ok(())
}

/// Every mutating `Items` setter says so on the bus. The bridge re-reads an
/// item on any item event; a silent setter is an edit no peer ever sees.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_item_setter_publishes_an_event() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let daw = bundle.standalone.clone();
    let track = Tracks::add(&daw, ctx(), "T", None).unwrap();
    Tracks::set_lane_count(&daw, ctx(), TrackRef::Guid(track.clone()), 2).unwrap();
    let guid = Items::add_item_with_guid(
        &daw,
        ctx(),
        TrackRef::Guid(track),
        ITEM_GUID,
        span(0.0, 4.0),
    )
    .unwrap();
    let mut rx = subscribe(
        &bundle,
        BusFilter {
            items: true,
            ..BusFilter::default()
        },
    )
    .await?;

    let it = || ItemRef::Guid(guid.clone());
    let d = Duration::from_seconds(0.5);
    let setters: Vec<(&str, Box<dyn Fn() -> daw_proto::DawResult<()>>)> = vec![
        (
            "snap_offset",
            Box::new(|| Items::set_snap_offset(&daw, ctx(), it(), d)),
        ),
        (
            "locked",
            Box::new(|| Items::set_locked(&daw, ctx(), it(), true)),
        ),
        (
            "fade_in",
            Box::new(|| Items::set_fade_in(&daw, ctx(), it(), d, FadeShape::default())),
        ),
        (
            "fade_out",
            Box::new(|| Items::set_fade_out(&daw, ctx(), it(), d, FadeShape::default())),
        ),
        (
            "loop_source",
            Box::new(|| Items::set_loop_source(&daw, ctx(), it(), true)),
        ),
        (
            "beat_attach_mode",
            Box::new(|| Items::set_beat_attach_mode(&daw, ctx(), it(), BeatAttachMode::Beats)),
        ),
        (
            "auto_stretch",
            Box::new(|| Items::set_auto_stretch(&daw, ctx(), it(), true)),
        ),
        (
            "color",
            Box::new(|| Items::set_color(&daw, ctx(), it(), Some(0x112233))),
        ),
        (
            "label",
            Box::new(|| Items::set_label(&daw, ctx(), it(), "Am7")),
        ),
        (
            "group_id",
            Box::new(|| Items::set_group_id(&daw, ctx(), it(), Some(3))),
        ),
        (
            "fixed_lane",
            Box::new(|| Items::set_fixed_lane(&daw, ctx(), it(), 1)),
        ),
        (
            "volume",
            Box::new(|| Items::set_volume(&daw, ctx(), it(), 0.5)),
        ),
        (
            "muted",
            Box::new(|| Items::set_muted(&daw, ctx(), it(), true)),
        ),
        (
            "position",
            Box::new(|| {
                Items::set_position(&daw, ctx(), it(), PositionInSeconds::from_seconds(2.0))
            }),
        ),
        (
            "length",
            Box::new(|| Items::set_length(&daw, ctx(), it(), Duration::from_seconds(3.0))),
        ),
        (
            "selected",
            Box::new(|| Items::set_selected(&daw, ctx(), it(), true)),
        ),
        (
            "select_all_items",
            Box::new(|| Items::select_all_items(&daw, ctx(), false)),
        ),
    ];
    for (name, set) in setters {
        set().unwrap_or_else(|e| panic!("{name}: {e}"));
        let DawEvent::Item(e) = next_event(&mut rx).await? else {
            panic!("{name}: expected an item event");
        };
        assert_eq!(e.item_guid(), guid, "{name}: {e:?}");
        assert_eq!(e.project_guid(), PROJECT, "{name}: {e:?}");
    }

    // The generic variant is the one the unnamed properties use.
    Items::set_label(&daw, ctx(), it(), "C").unwrap();
    assert!(matches!(
        next_event(&mut rx).await?,
        DawEvent::Item(ItemEvent::Changed { .. })
    ));
    // Selecting everything announces only what flipped.
    Items::select_all_items(&daw, ctx(), false).unwrap();
    Items::set_muted(&daw, ctx(), it(), false).unwrap();
    assert!(matches!(
        next_event(&mut rx).await?,
        DawEvent::Item(ItemEvent::MuteChanged { muted: false, .. })
    ));
    Ok(())
}

/// Every mutating `Takes` setter says so on the bus too.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_take_setter_publishes_an_event() -> eyre::Result<()> {
    let bundle = build_in_process_daw(seeded()).await?;
    let daw = bundle.standalone.clone();
    let track = Tracks::add(&daw, ctx(), "T", None).unwrap();
    let item = Items::add_item_with_guid(
        &daw,
        ctx(),
        TrackRef::Guid(track),
        ITEM_GUID,
        span(0.0, 4.0),
    )
    .unwrap();
    let take = Takes::add_take(&daw, ctx(), ItemRef::Guid(item.clone())).unwrap();
    let mut rx = subscribe(
        &bundle,
        BusFilter {
            takes: true,
            ..BusFilter::default()
        },
    )
    .await?;

    let it = || ItemRef::Guid(item.clone());
    let tk = || TakeRef::Guid(take.clone());
    let setters: Vec<(&str, Box<dyn Fn() -> daw_proto::DawResult<()>>)> = vec![
        (
            "color",
            Box::new(|| Takes::set_color(&daw, ctx(), it(), tk(), Some(0x445566))),
        ),
        (
            "preserve_pitch",
            Box::new(|| Takes::set_preserve_pitch(&daw, ctx(), it(), tk(), true)),
        ),
        (
            "start_offset",
            Box::new(|| {
                Takes::set_start_offset(&daw, ctx(), it(), tk(), Duration::from_seconds(0.25))
            }),
        ),
        (
            "name",
            Box::new(|| Takes::set_name(&daw, ctx(), it(), tk(), "Lead".into())),
        ),
        (
            "volume",
            Box::new(|| Takes::set_volume(&daw, ctx(), it(), tk(), 0.8)),
        ),
        (
            "play_rate",
            Box::new(|| Takes::set_play_rate(&daw, ctx(), it(), tk(), 1.5)),
        ),
        (
            "pitch",
            Box::new(|| Takes::set_pitch(&daw, ctx(), it(), tk(), 2.0)),
        ),
        (
            "source_file",
            Box::new(|| Takes::set_source_file(&daw, ctx(), it(), tk(), "/tmp/b.wav".into())),
        ),
        (
            "add_take_marker",
            Box::new(|| {
                Takes::add_take_marker(
                    &daw,
                    ctx(),
                    it(),
                    tk(),
                    TakeMarkerCreate {
                        name: "hit".into(),
                        source_position_seconds: 0.5,
                        color: None,
                        ..Default::default()
                    },
                )
                .map(|_| ())
                .ok_or_else(|| DawError::internal("no marker"))
            }),
        ),
        (
            "set_take_marker",
            Box::new(|| {
                Takes::set_take_marker(
                    &daw,
                    ctx(),
                    it(),
                    tk(),
                    TakeMarkerUpdate {
                        index: 0,
                        name: Some("hat".into()),
                        source_position_seconds: None,
                        color: None,
                        ..Default::default()
                    },
                )
            }),
        ),
        (
            "delete_take_marker",
            Box::new(|| Takes::delete_take_marker(&daw, ctx(), it(), tk(), 0)),
        ),
    ];
    for (name, set) in setters {
        set().unwrap_or_else(|e| panic!("{name}: {e}"));
        let DawEvent::Take(e) = next_event(&mut rx).await? else {
            panic!("{name}: expected a take event");
        };
        assert_eq!(e.item_guid(), item, "{name}: {e:?}");
        assert_eq!(e.project_guid(), PROJECT, "{name}: {e:?}");
    }

    Takes::set_color(&daw, ctx(), it(), tk(), None).unwrap();
    assert!(matches!(
        next_event(&mut rx).await?,
        DawEvent::Take(TakeEvent::Changed { take_guid, .. }) if take_guid == take
    ));
    Ok(())
}
