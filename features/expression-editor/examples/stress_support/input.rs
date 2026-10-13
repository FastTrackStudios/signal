//! Deterministic input through Blitz's DOM dispatcher; no OS input or window.
use blitz_traits::events::{
    BlitzWheelDelta, BlitzWheelEvent, MouseEventButtons, Point, PointerCoords, UiEvent,
};
use dioxus_test::{DocumentTester, Key, Modifiers, by_testid};

pub const PHASES: &[&str] = &[
    "tcp_vertical",
    "arrange_horizontal",
    "arrange_zoom",
    "mixer_horizontal",
    "drum_pan",
    "drum_vertical",
    "drum_zoom",
    "all_panels",
];

fn point(tester: &DocumentTester, id: &str) -> (f64, f64) {
    let el = tester
        .query(by_testid(id))
        .immediately()
        .expect("workstation pane mounted");
    let (x, y) = el.document_origin();
    let (w, h) = el.size();
    // Top of mixer avoids controls with their own wheel handlers.
    (
        x + f64::from(w) * 0.6,
        y + if id == "stack-cell" {
            f64::from(h) * 0.5
        } else {
            f64::from(h).min(12.0)
        },
    )
}

fn wheel(tester: &DocumentTester, points: &Points, id: &str, dx: f64, dy: f64, mods: Modifiers) {
    let (x, y) = points[id];
    tester.pointer_move_mods(x, y, false, mods);
    tester.send_ui_event(UiEvent::Wheel(BlitzWheelEvent {
        delta: BlitzWheelDelta::Pixels(dx, dy),
        coords: PointerCoords {
            page_x: x as f32,
            page_y: y as f32,
            screen_x: x as f32,
            screen_y: y as f32,
            client_x: x as f32,
            client_y: y as f32,
        },
        buttons: MouseEventButtons::None,
        mods,
        element: Point::default(),
    }));
}

pub type Points = std::collections::HashMap<&'static str, (f64, f64)>;

pub fn points(tester: &DocumentTester) -> Points {
    [
        "workstation-arrange",
        "workstation-timeline",
        "workstation-mixer",
        "stack-cell",
    ]
    .into_iter()
    .map(|id| (id, point(tester, id)))
    .collect()
}

pub fn observe(tester: &DocumentTester, phase: &str) -> String {
    // The combined phase drives all seven inputs, so watching one pane
    // would call it dead the moment the drum camera stopped — while the
    // arrangement and mixer were still moving under it.
    if phase == "all_panels" {
        return PHASES[..7]
            .iter()
            .map(|phase| observe(tester, phase))
            .collect::<Vec<_>>()
            .join("|");
    }
    let id = match phase {
        "tcp_vertical" => "workstation-arrange",
        "arrange_horizontal" | "arrange_zoom" => "workstation-timeline",
        "mixer_horizontal" => "workstation-mixer",
        _ => "stack-cell",
    };
    let el = tester.query(by_testid(id)).immediately().unwrap();
    if matches!(
        phase,
        "tcp_vertical" | "arrange_horizontal" | "mixer_horizontal"
    ) {
        format!("{:?}", el.document_origin())
    } else {
        el.outer_html()
    }
}

pub fn frame(tester: &DocumentTester, points: &Points, phase: &str, frame: usize) {
    let direction = if (frame / 30) % 2 == 0 { 1.0 } else { -1.0 };
    let none = Modifiers::empty();
    match phase {
        "tcp_vertical" => wheel(
            tester,
            points,
            "workstation-arrange",
            0.0,
            -direction * 80.0,
            none,
        ),
        "arrange_horizontal" => wheel(
            tester,
            points,
            "workstation-timeline",
            -direction * 80.0,
            0.0,
            none,
        ),
        "arrange_zoom" => wheel(
            tester,
            points,
            "workstation-timeline",
            0.0,
            -direction * 2.0,
            Modifiers::CONTROL,
        ),
        "mixer_horizontal" => wheel(
            tester,
            points,
            "workstation-mixer",
            -direction * 40.0,
            0.0,
            none,
        ),
        "drum_pan" => wheel(tester, points, "stack-cell", direction * 4.0, 0.0, none),
        "drum_vertical" => wheel(tester, points, "stack-cell", 0.0, direction * 4.0, none),
        "drum_zoom" => {
            let (x, y) = points["stack-cell"];
            tester.key_down(Key::Character("z".into()), none);
            tester.pointer_down(x, y);
            tester.pointer_move(x + direction * 2.0, y, true);
            tester.pointer_up(x + direction * 2.0, y);
            tester.key_up(Key::Character("z".into()), none);
        }
        "all_panels" => {
            for phase in &PHASES[..7] {
                self::frame(tester, points, phase, frame);
            }
        }
        _ => unreachable!(),
    }
}
