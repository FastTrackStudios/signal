//! Hit testing must survive a mutation that has not been laid out yet.
//!
//! The workstation crashed in a real window with `Option::unwrap()` on a
//! `None` inside blitz-dom's `hit_inner`, reached from `set_hover_to` on
//! an ordinary pointer move. `paint_children` still held the id of a node
//! that had been removed: `drain()` applies dioxus's mutations, but it is
//! `relayout()` that rebuilds the paint order, and a real event loop is
//! free to deliver a pointer move in between. The stress harness never
//! saw it because it always resolves before sending the next event.
//!
//! Culling makes that window matter: every scroll that re-cuts a mounted
//! range removes nodes. This drives the same sequence deliberately.
use dioxus_test::{DocumentTester, by_testid};
use expression_editor_standalone::{
    Args, Runner,
    workstation::{WorkstationApp, bootstrap_daw_blocking, stage_workstation},
};
use std::time::{Duration, Instant};

#[test]
#[ignore = "needs a real project; run with --ignored"]
fn pointer_move_between_mutation_and_layout() -> eyre::Result<()> {
    // The project comes from the environment, not from argv: a test
    // binary's arguments belong to the test harness.
    let Ok(project) = std::env::var("FTS_STRESS_PROJECT") else {
        eprintln!("set FTS_STRESS_PROJECT to a practice .RPP");
        return Ok(());
    };
    let args = Args {
        source: expression_editor_standalone::Source::Rpp(project.into()),
        target: Default::default(),
        mode: None,
        width: 1600,
        height: 900,
        out: None,
    };
    let runner = Runner::open(&args.source, &args.target, args.viewport(), args.mode)?;
    let standalone = runner
        .daw
        .clone()
        .ok_or_else(|| eyre::eyre!("needs a project"))?;
    bootstrap_daw_blocking(&standalone)?;
    stage_workstation(
        runner.loaded.into_editor(),
        runner.host,
        (args.width as f64, args.height as f64),
    );
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(drive())
}

/// Mount, wait for the project, then hover between mutation and layout.
async fn drive() -> eyre::Result<()> {
    let tester = DocumentTester::from_element(WorkstationApp)
        .with_window_size(1600, 900)
        .build();
    let started = Instant::now();
    loop {
        tester.drain();
        tester.relayout();
        if tester
            .query(by_testid("workstation-ready"))
            .immediately()
            .is_ok()
        {
            break;
        }
        eyre::ensure!(
            started.elapsed() < Duration::from_secs(600),
            "readiness timeout"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let pane = tester
        .query(by_testid("workstation-arrange"))
        .immediately()
        .unwrap();
    let (ox, oy) = pane.document_origin();
    let (w, h) = pane.size();
    let (x, y) = (ox + f64::from(w) * 0.5, oy + f64::from(h) * 0.5);

    // Scroll, apply the mutations, then hover BEFORE laying out again.
    for step in 0..40 {
        let dy = if step % 2 == 0 { -120.0 } else { 120.0 };
        tester.pointer_move(x, y, false);
        tester.send_ui_event(blitz_traits::events::UiEvent::Wheel(
            blitz_traits::events::BlitzWheelEvent {
                delta: blitz_traits::events::BlitzWheelDelta::Pixels(0.0, dy),
                coords: blitz_traits::events::PointerCoords {
                    page_x: x as f32,
                    page_y: y as f32,
                    screen_x: x as f32,
                    screen_y: y as f32,
                    client_x: x as f32,
                    client_y: y as f32,
                },
                buttons: blitz_traits::events::MouseEventButtons::None,
                mods: Default::default(),
                element: blitz_traits::events::Point::default(),
            },
        ));
        // Mutations land here; the paint order is NOT rebuilt until
        // `relayout`. The pointer move on the next line is the one that
        // used to walk a removed node.
        tester.drain();
        tester.pointer_move(x, y - 1.0, false);
        tester.pointer_move(x + 1.0, y, false);
        tester.relayout();
    }
    Ok(())
}
