//! A hosted arrangement must leave scrolling to the shared TCP/timeline viewport.
use blitz_traits::events::{
    BlitzWheelDelta, BlitzWheelEvent, MouseEventButtons, Point, PointerCoords, UiEvent,
};
use daw::service::Track;
use daw_ui::components::arrangement_view::ArrangePreview;
use dioxus::prelude::*;
use dioxus_test::{by_testid, render};

#[component]
fn Hosted() -> Element {
    let position = use_signal(|| 0.0f32);
    let tracks = (0..12)
        .map(|index| Track {
            guid: format!("t{index}"),
            index,
            ..Default::default()
        })
        .collect();
    rsx! {
        div { "data-testid": "viewport", style: "width:400px;height:150px;overflow:scroll;",
            ArrangePreview { tracks, width:1000.0, height:1000.0, scrollable:false, play_position:position, edit_position:position }
        }
    }
}

#[test]
fn wheel_over_lanes_moves_the_parent_viewport_on_both_axes() {
    let tester = render(Hosted).with_window_size(450, 200).build();
    tester.drain();
    tester.relayout();
    let el = tester.query(by_testid("viewport")).immediately().unwrap();
    let before = el.document_origin();
    let (x, y) = (before.0 + 200.0, before.1 + 80.0);
    tester.pointer_move(x, y, false);
    tester.send_ui_event(UiEvent::Wheel(BlitzWheelEvent {
        delta: BlitzWheelDelta::Pixels(-40.0, -60.0),
        coords: PointerCoords {
            page_x: x as f32,
            page_y: y as f32,
            screen_x: x as f32,
            screen_y: y as f32,
            client_x: x as f32,
            client_y: y as f32,
        },
        buttons: MouseEventButtons::None,
        mods: Default::default(),
        element: Point::default(),
    }));
    tester.drain();
    tester.relayout();
    let after = el.document_origin();
    assert_eq!(after, (before.0 - 40.0, before.1 - 60.0));
}
