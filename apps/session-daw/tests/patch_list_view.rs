//! The Patch List view over the fixture album in the fixture room.
//!
//! Two seams: the table the view is built from (rows in, rows out —
//! grouped by performer in the list's order, unresolved roles marked),
//! and the picture, compared against the PNG the bench wrote
//! (`just daw-patch-list`) — the render fixture spec #48's Testing
//! Decisions ask every execution ticket to land. See `CHANNEL_SLACK`
//! for why the comparison has a floor rather than being byte-for-byte,
//! and `the_comparison_still_notices_a_table_that_changed` for the
//! control that keeps that floor honest.
//!
//! r[verify flow.patch-list.plan]
//! r[verify flow.patch-list.studio-profiles]

use session_daw::patch_list::{Mark, Panel, PerformerRows, Row, Table};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

/// The committed picture, at the size the recipe renders.
const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/patch-list.png");
const SIZE: (u32, u32) = (1280, 1280);

fn performer<'a>(table: &'a Table, name: &str) -> Option<&'a PerformerRows> {
    table.performers.iter().find(|p| p.name == name)
}

fn row<'a>(performer: &'a PerformerRows, key: &str) -> Option<&'a Row> {
    performer.rows.iter().find(|r| r.key == key)
}

#[test]
fn the_table_groups_source_kinds_by_performer_in_list_order() -> Result {
    let table = Table::fixture()?;
    let performers: Vec<&str> = table.performers.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        performers,
        [
            "cody", "john", "ron", "belen", "aline", "drummer", "bassist", "engineer", "producer"
        ]
    );

    let cody = performer(&table, "cody").ok_or("cody")?;
    assert_eq!(cody.rows.len(), 6);
    let di = row(cody, "di").ok_or("cody's DI")?;
    assert_eq!(di.kind, "guitar");
    assert_eq!(di.role, "DI 3");
    assert_eq!(di.input, "ch 3");
    assert_eq!(di.mark, Mark::Resolved);

    // A kit reads piece then mic, in the list's order.
    let drummer = performer(&table, "drummer").ok_or("the drummer")?;
    let keys: Vec<&str> = drummer.rows.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(
        &keys[..4],
        ["kick/in", "kick/out", "kick/trig", "snare/top"]
    );
    let snare = row(drummer, "snare/top").ok_or("the snare top")?;
    assert_eq!(snare.input, "ch 28");

    // A MIDI entry says so.
    let john = performer(&table, "john").ok_or("john")?;
    let synth = row(john, "synth").ok_or("john's synth")?;
    assert_eq!(synth.input, "MIDI Prophet-6 ch 1");
    assert_eq!(synth.mark, Mark::Resolved);
    Ok(())
}

#[test]
fn an_unresolved_role_is_marked_not_dropped() -> Result {
    let table = Table::fixture()?;
    let bassist = performer(&table, "bassist").ok_or("the bassist")?;
    let amp = row(bassist, "amp").ok_or("the amp mic")?;
    assert_eq!(amp.role, "Mic 11");
    assert_eq!(amp.mark, Mark::Unresolved);
    assert_eq!(amp.input, "unresolved");
    // The negative control on the same rig.
    let di = row(bassist, "di").ok_or("the DI")?;
    assert_eq!(di.mark, Mark::Resolved);

    let bus = table
        .buses
        .iter()
        .find(|b| b.name == "bassist")
        .ok_or("the bassist's bus")?;
    assert_eq!(bus.mark, Mark::Unresolved);
    assert_eq!(table.unresolved, 2);
    assert_eq!(table.buses.len(), 10);

    let cody = table.buses.first().ok_or("the first bus")?;
    assert_eq!(cody.audience, "cody");
    assert_eq!(cody.input, "out 3/4");
    Ok(())
}

/// How far apart a channel may be before a pixel counts as different.
///
/// Not zero, and the reason is measured rather than assumed: the same
/// scene rasterised on this machine's discrete GPU and on the CI
/// runner's software device disagree on the antialiased edge of a
/// glyph by a few levels. Byte-identical is the standard for what the
/// renderer is HANDED — the row list, the geometry — but the last step
/// belongs to a driver, so the picture is compared as a picture.
const CHANNEL_SLACK: u8 = 24;

/// And how much of the frame may differ at all.
///
/// A quarter of a percent is edge pixels. Anything structural — a row
/// that moved, a performer that vanished, a band that changed colour —
/// is far more than that, because every row of this table is a
/// full-width fill: one row out of place redraws the whole column.
const FRAME_SLACK: f64 = 0.0025;

/// How many pixels of two frames differ beyond the slack, and what
/// share of the frame that is.
fn difference(want: &[u8], got: &[u8]) -> (usize, f64) {
    let differing = want
        .chunks_exact(4)
        .zip(got.chunks_exact(4))
        .filter(|(want, got)| {
            want.iter()
                .zip(got.iter())
                .any(|(w, g)| w.abs_diff(*g) > CHANNEL_SLACK)
        })
        .count();
    #[expect(
        clippy::cast_precision_loss,
        reason = "a ratio of two pixel counts, both under 2^24"
    )]
    let ratio = differing as f64 / (want.len() / 4) as f64;
    (differing, ratio)
}

#[test]
fn the_view_renders_the_committed_fixture() -> Result {
    let Some(rendered) = session_daw::patch_list::shot(&Table::fixture()?, SIZE) else {
        // No GPU on this box: the picture cannot be taken. The table
        // tests above still hold; the picture is checked wherever a
        // device exists.
        tracing::warn!("no wgpu adapter; skipping the patch list picture");
        return Ok(());
    };
    let committed = image::open(FIXTURE)
        .map_err(|e| format!("{FIXTURE} did not open ({e}); run `just daw-patch-list`"))?
        .into_rgba8();
    assert_eq!(committed.dimensions(), SIZE, "the fixture's size");
    assert_eq!(committed.as_raw().len(), rendered.len(), "the frame's size");

    let (differing, ratio) = difference(committed.as_raw(), &rendered);
    assert!(
        ratio <= FRAME_SLACK,
        "the Patch List view no longer matches {FIXTURE}: {differing} pixels \
         ({:.2}%) differ by more than {CHANNEL_SLACK} levels. If the change is \
         intended, regenerate it with `just daw-patch-list` and commit the picture",
        ratio * 100.0
    );
    Ok(())
}

#[test]
fn a_project_with_no_album_file_has_no_plan_to_show() -> Result {
    let tmp = tempfile::tempdir()?;
    let song = tmp.path().join("loose").join("song.rpp");
    std::fs::create_dir_all(song.parent().ok_or("a parent")?)?;
    std::fs::write(&song, "<REAPER_PROJECT>")?;
    assert_eq!(Panel::for_project(&song), Panel::Absent);
    Ok(())
}

#[test]
fn a_double_booked_role_is_shown_in_the_view_not_hidden_by_it() -> Result {
    // The refusal is the thing the engineer has to see before a take
    // (spec #48, story 24), so it is a state of this view rather than
    // an absent view.
    let tmp = tempfile::tempdir()?;
    let album = tmp.path().join("album");
    std::fs::create_dir_all(&album)?;
    std::fs::write(
        album.join("patch-list.styx"),
        "performers {\n    cody {guitar {di \"DI 3\"}}\n    john {guitar {di \"DI 3\"}}\n}",
    )?;
    let song = album.join("song.rpp");
    std::fs::write(&song, "<REAPER_PROJECT>")?;

    let Panel::Problem(why) = Panel::for_project(&song) else {
        return Err("a double-booked DI 3 was not reported".into());
    };
    assert!(why.contains("DI 3"), "{why}");
    Ok(())
}

#[test]
fn the_comparison_still_notices_a_table_that_changed() -> Result {
    // The negative control on the fixture comparison itself: a slack
    // wide enough to absorb two rasterisers must still be narrow enough
    // to catch a performer going missing.
    let mut table = Table::fixture()?;
    table.performers.truncate(table.performers.len() - 1);
    let Some(rendered) = session_daw::patch_list::shot(&table, SIZE) else {
        tracing::warn!("no wgpu adapter; skipping the patch list picture");
        return Ok(());
    };
    let committed = image::open(FIXTURE)
        .map_err(|e| format!("{FIXTURE} did not open ({e})"))?
        .into_rgba8();
    let (_, ratio) = difference(committed.as_raw(), &rendered);
    assert!(
        ratio > FRAME_SLACK,
        "a table with a performer removed still matched the fixture ({:.2}% differing)",
        ratio * 100.0
    );
    Ok(())
}

/// The second render fixture (#57): the fixture album with a session
/// override on Cody's DI and a stale banner. Same threshold as the
/// first — #48's fixture amendment applies to every render fixture
/// this ticket lands, not just the album's own picture.
const FIXTURE_OVERRIDDEN_STALE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/patch-list-overridden-stale.png"
);

#[test]
fn overriding_one_entry_marks_it_and_leaves_the_rest_alone() -> Result {
    // r[verify flow.patch-list.session-override]
    let table = Table::fixture_overridden_stale()?;
    let cody = performer(&table, "cody").ok_or("cody")?;
    let di = row(cody, "di").ok_or("cody's DI")?;
    assert!(di.overridden, "the overridden entry is marked");
    assert_eq!(di.role, "DI 9", "the override's value, not the album's");

    // The negative control: an entry the override never named.
    let pedalboard = row(cody, "pedalboard").ok_or("cody's pedalboard")?;
    assert!(!pedalboard.overridden);
    Ok(())
}

#[test]
fn a_stale_session_carries_its_banner_and_its_unpatched_track() -> Result {
    // r[verify flow.patch-list.apply]
    let table = Table::fixture_overridden_stale()?;
    assert!(table.stale.is_some(), "the session is shown stale");
    assert_eq!(table.unpatched.len(), 1);
    Ok(())
}

#[test]
fn an_entry_with_no_track_is_marked_unused_and_leaves_the_rest_alone() -> Result {
    // r[verify flow.patch-list.apply]
    let table = Table::fixture_overridden_stale()?;
    let producer = performer(&table, "producer").ok_or("producer")?;
    let mic = row(producer, "mic").ok_or("producer's talkback mic")?;
    assert!(mic.unused, "the entry with no track is marked unused");

    // The negative control: an entry the same rig does have a track
    // for is not marked.
    let engineer = performer(&table, "engineer").ok_or("engineer")?;
    let engineer_mic = row(engineer, "mic").ok_or("engineer's talkback mic")?;
    assert!(!engineer_mic.unused);
    Ok(())
}

#[test]
fn the_overridden_stale_view_renders_the_committed_fixture() -> Result {
    // r[verify flow.patch-list.session-override]
    let Some(rendered) = session_daw::patch_list::shot(&Table::fixture_overridden_stale()?, SIZE)
    else {
        tracing::warn!("no wgpu adapter; skipping the patch list picture");
        return Ok(());
    };
    let committed = image::open(FIXTURE_OVERRIDDEN_STALE)
        .map_err(|e| {
            format!(
                "{FIXTURE_OVERRIDDEN_STALE} did not open ({e}); run \
                 `just daw-patch-list-overridden-stale`"
            )
        })?
        .into_rgba8();
    assert_eq!(committed.dimensions(), SIZE, "the fixture's size");
    assert_eq!(committed.as_raw().len(), rendered.len(), "the frame's size");

    let (differing, ratio) = difference(committed.as_raw(), &rendered);
    assert!(
        ratio <= FRAME_SLACK,
        "the overridden/stale Patch List view no longer matches \
         {FIXTURE_OVERRIDDEN_STALE}: {differing} pixels ({:.2}%) differ by more \
         than {CHANNEL_SLACK} levels. If the change is intended, regenerate it \
         with `just daw-patch-list-overridden-stale` and commit the picture",
        ratio * 100.0
    );
    Ok(())
}

#[test]
fn the_overridden_stale_comparison_still_notices_a_table_that_changed() -> Result {
    // The negative control on this fixture's own comparison, same
    // shape as the base fixture's.
    let mut table = Table::fixture_overridden_stale()?;
    table.stale = None;
    let Some(rendered) = session_daw::patch_list::shot(&table, SIZE) else {
        tracing::warn!("no wgpu adapter; skipping the patch list picture");
        return Ok(());
    };
    let committed = image::open(FIXTURE_OVERRIDDEN_STALE)
        .map_err(|e| format!("{FIXTURE_OVERRIDDEN_STALE} did not open ({e})"))?
        .into_rgba8();
    let (_, ratio) = difference(committed.as_raw(), &rendered);
    assert!(
        ratio > FRAME_SLACK,
        "a table with the stale banner removed still matched the fixture ({:.2}% differing)",
        ratio * 100.0
    );
    Ok(())
}
