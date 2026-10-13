//! Integration tests for daw actions CLI commands against a live REAPER socket.
//!
//! Run with:
//!
//!   cargo xtask reaper-test -- actions_cli

#![cfg(all(feature = "cli", feature = "test-harness"))]

use daw::test::reaper_test;
use serde_json::Value;
use std::process::Command;

fn daw_command() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_daw"));
    if let Ok(socket) = std::env::var("FTS_SOCKET") {
        cmd.arg("--socket").arg(socket);
    }
    cmd
}

fn daw_json(args: &[&str]) -> eyre::Result<Value> {
    let output = daw_command().arg("--json").args(args).output()?;
    if !output.status.success() {
        eyre::bail!(
            "daw {:?} failed\nstdout:\n{}\nstderr:\n{}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[reaper_test(isolated)]
async fn actions_cli_lists_reaper_actions(_ctx: &ReaperTestContext) -> eyre::Result<()> {
    let value = daw_json(&[
        "actions", "list", "--filter", "reaper", "--query", "undo", "--limit", "1",
    ])?;

    assert_eq!(value["filter"], "Reaper");
    assert_eq!(value["count"], 1);
    assert!(
        value["total_count"].as_u64().unwrap_or_default() >= 1,
        "total_count should include all matching undo actions: {value:#?}"
    );
    assert!(
        value["actions"][0]["description"]
            .as_str()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .contains("undo"),
        "expected an undo action row: {value:#?}"
    );

    Ok(())
}

#[reaper_test(isolated)]
async fn actions_cli_lists_registered_actions(ctx: &ReaperTestContext) -> eyre::Result<()> {
    let actions = ctx.daw.action_registry();
    let cmd_id = actions
        .register(
            "FTS_TEST_ACTIONS_CLI_REGISTERED",
            "FTS Test: Actions CLI Registered",
        )
        .await?;

    let value = daw_json(&[
        "actions",
        "registered",
        "--query",
        "Actions CLI Registered",
        "--limit",
        "4",
    ])?;

    assert!(
        value["actions"]
            .as_array()
            .is_some_and(
                |actions| actions.iter().any(|action| action["command_id"] == cmd_id
                    && action["command_name"] == "FTS_TEST_ACTIONS_CLI_REGISTERED"
                    && action["registered_by_fts"] == true)
            ),
        "registered action should be visible through daw actions registered: {value:#?}"
    );

    Ok(())
}
