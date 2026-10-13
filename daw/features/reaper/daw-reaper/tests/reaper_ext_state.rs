//! Integration tests for the ExtState service (#24).
//!
//! Round-trip coverage:
//! - set → get returns the value
//! - set → has reports presence
//! - set → delete → has reports absence
//! - persist=true vs persist=false isolation (in-memory only)
//!
//! Run with: `cargo xtask reaper-test -- reaper_ext_state`

use daw::test::reaper_test;

const TEST_SECTION: &str = "fts.test.ext_state";

#[reaper_test(isolated)]
async fn set_then_get_returns_value(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let ext = ctx.daw.ext_state();

    ext.set(TEST_SECTION, "color", "blue", false).await?;
    let got = ext.get(TEST_SECTION, "color").await?;
    assert_eq!(
        got.as_deref(),
        Some("blue"),
        "ExtState::set followed by get should return the same value"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn has_reflects_presence(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let ext = ctx.daw.ext_state();

    let before = ext.has(TEST_SECTION, "tempo").await?;
    assert!(!before, "key should be absent before set()");

    ext.set(TEST_SECTION, "tempo", "120.0", false).await?;
    let after = ext.has(TEST_SECTION, "tempo").await?;
    assert!(after, "key should exist after set()");

    Ok(())
}

#[reaper_test(isolated)]
async fn delete_removes_value(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let ext = ctx.daw.ext_state();

    ext.set(TEST_SECTION, "transient", "1", false).await?;
    assert!(ext.has(TEST_SECTION, "transient").await?, "set didn't take");

    ext.delete(TEST_SECTION, "transient", false).await?;
    let after = ext.has(TEST_SECTION, "transient").await?;
    assert!(!after, "key should be absent after delete()");

    let got = ext.get(TEST_SECTION, "transient").await?;
    assert_eq!(got, None, "get() after delete() should return None");

    Ok(())
}

#[reaper_test(isolated)]
async fn overwrite_replaces_value(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let ext = ctx.daw.ext_state();

    ext.set(TEST_SECTION, "mode", "manual", false).await?;
    ext.set(TEST_SECTION, "mode", "auto", false).await?;
    let got = ext.get(TEST_SECTION, "mode").await?;
    assert_eq!(
        got.as_deref(),
        Some("auto"),
        "second set() should replace the first"
    );

    Ok(())
}

// ── Project-scoped reads: live, or last-saved? (#189) ────────────────
//
// The REAPER API header describes `GetProjExtState` as returning "the
// value previously associated with this extname and key, **the last time
// the project was saved**."
//
// #190's whole design rests on that describing *persistence* and not
// *read semantics* — that REAPER's project object is the in-memory store,
// so setting a value and reading it back within a session returns the new
// value. If instead reads really do return the last saved value, the
// editor must hold its own read cache in front of ext-state, and #191,
// #192, #201 and #208 all change shape.
//
// It is a one-call question and it gates a lot, so it gets answered here
// rather than assumed.

const PROJ_SECTION: &str = "fts.test.proj_ext_state";

#[reaper_test(isolated)]
async fn project_ext_state_reads_are_live_not_last_saved(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let ext = ctx.daw.ext_state();
    let project = daw_proto::ProjectContext::project(ctx.project().guid());

    ext.set_project(project.clone(), PROJ_SECTION, "mode", "Drums")
        .await?;

    // Deliberately no save between the write and the read.
    let got = ext.get_project(project, PROJ_SECTION, "mode").await?;

    assert_eq!(
        got.as_deref(),
        Some("Drums"),
        "project ext-state reads must be live. If this fails, the header's \
         'last time the project was saved' describes read semantics, and \
         #190 needs an in-editor read cache in front of ext-state."
    );
    Ok(())
}

#[reaper_test(isolated)]
async fn a_project_value_can_be_overwritten_within_a_session(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    let ext = ctx.daw.ext_state();
    let project = daw_proto::ProjectContext::project(ctx.project().guid());

    ext.set_project(project.clone(), PROJ_SECTION, "mode", "Midi")
        .await?;
    ext.set_project(project.clone(), PROJ_SECTION, "mode", "Vocals")
        .await?;

    let got = ext.get_project(project, PROJ_SECTION, "mode").await?;
    assert_eq!(
        got.as_deref(),
        Some("Vocals"),
        "a correction made twice in one session must read back as the second"
    );
    Ok(())
}

#[reaper_test(isolated)]
async fn a_large_project_value_survives_the_read(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    // The editor-state blob is not small. Before `bb824d89f` the read
    // buffer was a fixed 4096 bytes and truncated silently, which under
    // #190's versioning rule discards every correction in the project.
    let ext = ctx.daw.ext_state();
    let project = daw_proto::ProjectContext::project(ctx.project().guid());

    let big = "x".repeat(64 * 1024);
    ext.set_project(project.clone(), PROJ_SECTION, "blob", &big)
        .await?;
    let got = ext.get_project(project, PROJ_SECTION, "blob").await?;

    assert_eq!(
        got.as_deref().map(str::len),
        Some(big.len()),
        "a 64 KiB value must come back whole, not truncated at 4096"
    );
    Ok(())
}
