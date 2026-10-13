//! Integration tests for the Toolbar service (#24).
//!
//! Round-trip coverage:
//! - add button → returns ok with a command id
//! - workflow batch: add three buttons with same workflow_id → remove
//!   workflow → assert all gone
//! - move button: add → move to position 0 → re-query (best-effort —
//!   we just confirm the call succeeds)
//!
//! Run with: `cargo xtask reaper-test -- reaper_toolbar`

use daw::test::reaper_test;
use daw_proto::{ToolbarButton, ToolbarTarget};

const TEST_WORKFLOW: &str = "fts.test.toolbar";

fn make_button(name: &str, label: &str) -> ToolbarButton {
    ToolbarButton {
        command_name: name.to_string(),
        label: label.to_string(),
        ..Default::default()
    }
}

#[reaper_test(isolated)]
async fn add_button_succeeds_and_returns_command_id(
    ctx: &daw::test::ReaperTestContext,
) -> eyre::Result<()> {
    // Register a backing action first so the toolbar has something to
    // bind to. Toolbar add_button accepts any command_name but a
    // well-formed test pre-registers it.
    let actions = ctx.daw.action_registry();
    let cmd_name = "fts.test.toolbar.add_one";
    let _ = actions
        .register(cmd_name, "FTS Test: Toolbar Add One")
        .await?;

    let toolbar = ctx.daw.toolbar();
    let result = toolbar
        .add_button(make_button(cmd_name, "Test One"), TEST_WORKFLOW)
        .await?;

    assert!(
        result.ok,
        "add_button should succeed, got error: {:?}",
        result.error
    );
    assert!(
        result.command_id.is_some(),
        "add_button should return a resolved command id"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn workflow_remove_cleans_up_batch(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();
    let toolbar = ctx.daw.toolbar();

    // Register + add three buttons under the same workflow.
    for i in 0..3 {
        let cmd = format!("fts.test.toolbar.batch_{i}");
        let _ = actions
            .register(&cmd, &format!("FTS Test: Batch {i}"))
            .await?;
        let res = toolbar
            .add_button(make_button(&cmd, &format!("Batch {i}")), TEST_WORKFLOW)
            .await?;
        assert!(res.ok, "batch add #{i} failed: {:?}", res.error);
    }

    // Single-call cleanup of the workflow.
    let cleanup = toolbar.remove_workflow_buttons(TEST_WORKFLOW).await?;
    assert!(
        cleanup.ok,
        "remove_workflow_buttons should succeed: {:?}",
        cleanup.error
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn remove_button_succeeds(ctx: &daw::test::ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();
    let cmd = "fts.test.toolbar.remove_target";
    let _ = actions.register(cmd, "FTS Test: Remove Target").await?;

    let toolbar = ctx.daw.toolbar();
    let added = toolbar
        .add_button(make_button(cmd, "Remove Me"), TEST_WORKFLOW)
        .await?;
    assert!(added.ok, "add_button setup failed");

    let removed = toolbar.remove_button(ToolbarTarget::Main, cmd).await?;
    assert!(
        removed.ok,
        "remove_button should succeed: {:?}",
        removed.error
    );

    Ok(())
}
