//! Folder controls must update both sides of the composed window.
#![cfg(feature = "web")]
use daw_proto::Track;
use daw_ui::components::main_window::MainWindowPreview;
use dioxus::prelude::*;
use dioxus_test::{by_testid, render};

#[component]
fn Surface() -> Element {
    let tracks = [
        ("kit", "Kit", 1),
        ("snare", "Snare", 1),
        ("bottom", "Bottom", -2),
        ("bass", "Bass", 0),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (guid, name, delta))| Track {
        guid: guid.into(),
        name: name.into(),
        index: index as u32,
        folder_depth: delta,
        is_folder: delta > 0,
        volume: 1.0,
        ..Default::default()
    })
    .collect();
    rsx! { MainWindowPreview { tracks, width: 1200.0, height: 850.0 } }
}

#[test]
fn collapse_from_tcp_and_expand_from_mixer_share_visibility() {
    let tester = render(Surface).with_window_size(1220, 870).build();
    tester.drain();
    tester.relayout();
    for id in ["tcp-bottom", "mcp-bottom", "tcp-bass", "mcp-bass"] {
        assert!(tester.query(by_testid(id)).immediately().is_ok(), "{id}");
    }
    let tcp = tester.query(by_testid("tcp-kit")).immediately().unwrap();
    let (x, y) = tcp.document_origin();
    let (_, h) = tcp.size();
    // Disclosure is at the bottom of the TCP's folder column.
    tester.pointer_down(x + 15.0, y + f64::from(h) - 13.0);
    tester.pointer_up(x + 15.0, y + f64::from(h) - 13.0);
    tester.drain();
    tester.relayout();
    for id in ["tcp-snare", "mcp-snare", "tcp-bottom", "mcp-bottom"] {
        assert!(
            tester.query(by_testid(id)).immediately().is_err(),
            "{id} should be hidden"
        );
    }
    for id in ["tcp-kit", "mcp-kit", "tcp-bass", "mcp-bass"] {
        assert!(
            tester.query(by_testid(id)).immediately().is_ok(),
            "{id} stays visible"
        );
    }
    let mcp = tester.query(by_testid("mcp-kit")).immediately().unwrap();
    let (x, y) = mcp.document_origin();
    tester.pointer_down(x + 10.0, y + 10.0);
    tester.pointer_up(x + 10.0, y + 10.0);
    tester.drain();
    assert!(tester.query(by_testid("tcp-bottom")).immediately().is_ok());
    assert!(tester.query(by_testid("mcp-bottom")).immediately().is_ok());
}
