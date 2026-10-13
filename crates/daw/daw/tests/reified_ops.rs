//! End-to-end tests for the agent-facing generic commands (`daw op`,
//! `daw call`, `daw batch`): CLI op helpers → in-process Standalone
//! over a real vox memory link.

#![cfg(feature = "cli")]

use daw::cli::ops::{run_batch, run_call, run_op};
use daw_standalone::bootstrap::build_in_process_daw;
use daw_standalone::sync::Standalone;

#[tokio::test]
async fn op_and_call_round_trip_against_standalone() -> eyre::Result<()> {
    let standalone = Standalone::new();
    let project = daw::service::project::Projects::create(&standalone)
        .expect("create project")
        .guid;
    let in_proc = build_in_process_daw(standalone).await?;
    let daw = &in_proc.daw;

    // Reified op: add a marker.
    let outcome = run_op(
        daw,
        &format!(
            r#"{{"Marker":{{"Add":{{"project":{{"Literal":{{"Project":"{project}"}}}},"position":2.5,"name":"agent"}}}}}}"#
        ),
    )
    .await?;
    assert!(outcome.get("Ok").is_some(), "op should succeed: {outcome}");

    // Human/agent sugar: same surface via service.method.
    let outcome = run_call(
        daw,
        "marker.all",
        Some(&format!(
            r#"{{"project":{{"Literal":{{"Project":"{project}"}}}}}}"#
        )),
    )
    .await?;
    let markers = outcome
        .pointer("/Ok/Marker/All")
        .and_then(|v| v.as_array())
        .expect("marker.all outcome shape");
    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0]["name"], "agent");

    // Unknown service errors with guidance.
    let err = run_call(daw, "nope.thing", None).await.unwrap_err();
    assert!(err.to_string().contains("unknown service"));

    // No-args ergonomics: the current project is injected when a call
    // without a `project` argument fails to parse.
    let outcome = run_call(daw, "transport.is_playing", None).await?;
    assert_eq!(
        outcome.pointer("/Ok/Transport/IsPlaying"),
        Some(&serde_json::Value::Bool(false)),
        "no-args call should default to the current project: {outcome}"
    );

    Ok(())
}

#[tokio::test]
async fn batch_program_chains_steps() -> eyre::Result<()> {
    let standalone = Standalone::new();
    let in_proc = build_in_process_daw(standalone).await?;

    // Step 0 creates a project; steps 1-2 reference it via FromStep.
    let program = r#"{
        "instructions": [
            {"step": 0, "op": {"Project": {"Create": {}}}},
            {"step": 1, "op": {"Marker": {"Add": {"project": {"FromStep": 0}, "position": 1.0, "name": "chained"}}}},
            {"step": 2, "op": {"Marker": {"All": {"project": {"FromStep": 0}}}}}
        ],
        "options": {"undo_label": null, "fail_fast": false}
    }"#;
    let value = run_batch(&in_proc.daw, program).await?;
    let results = value["results"].as_array().expect("results array");
    assert_eq!(results.len(), 3);
    let markers = results[2]
        .pointer("/outcome/Ok/Marker/All")
        .and_then(|v| v.as_array())
        .expect("chained marker list");
    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0]["name"], "chained");
    Ok(())
}
