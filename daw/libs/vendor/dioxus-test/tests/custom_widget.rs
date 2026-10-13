//! A custom widget lays out and paints under the headless harness.
//!
//! This is the property the expression editor's roll is about to depend
//! on, and it is worth a test of its own because the failure mode is
//! silent: without the `custom-widget` feature on blitz-dom and
//! blitz-paint, the widget still mounts and still gets a box, and the
//! renderer simply draws nothing where it is. That reads as a bug in the
//! widget rather than a missing feature in the build.
//!
//! It also pins the reason the roll is moving to this API at all: the
//! widget is *handed* its width and height. There is nothing to measure,
//! nothing to report from the host, and no ratio between a declared size
//! and a used one for a renderer to scale by.

use std::cell::RefCell;
use std::rc::Rc;

use dioxus::prelude::*;
use dioxus_native_dom::CustomWidgetAttr;
use dioxus_test::{by_testid, render};

/// What the widget was told to paint into, recorded so the test can
/// assert on it.
#[derive(Default, Clone, Copy, Debug, PartialEq)]
struct Painted {
    calls: usize,
    width: u32,
    height: u32,
}

struct Probe {
    seen: Rc<RefCell<Painted>>,
}

impl blitz_dom::Widget for Probe {
    fn paint(
        &mut self,
        _ctx: &mut dyn anyrender::RenderContext,
        _styles: &blitz_dom::node::ComputedStyles,
        width: u32,
        height: u32,
        _scale: f64,
    ) -> anyrender::Scene {
        use anyrender::PaintScene as _;
        {
            let mut seen = self.seen.borrow_mut();
            seen.calls += 1;
            seen.width = width;
            seen.height = height;
        }
        let mut scene = anyrender::Scene::new();
        // Fill the whole box, so a screenshot would show it.
        scene.fill(
            peniko::Fill::NonZero,
            Default::default(),
            peniko::color::palette::css::RED,
            None,
            &peniko::kurbo::Rect::new(0.0, 0.0, width as f64, height as f64),
        );
        scene
    }
}

thread_local! {
    static SEEN: Rc<RefCell<Painted>> = Rc::new(RefCell::new(Painted::default()));
}

fn app() -> Element {
    let widget = use_hook(|| {
        let seen = SEEN.with(|s| s.clone());
        CustomWidgetAttr::new(Probe { seen })
    });
    rsx! {
        style { "html, body {{ margin: 0; padding: 0; width: 100%; height: 100%; }}" }
        div {
            style: "width: 400px; height: 220px;",
            // The size goes on the widget's own element. It is a
            // replaced element, so a parent with a size is not enough —
            // blitz-paint skips a widget whose box is zero, silently.
            object {
                "data-testid": "host",
                "data": widget,
                style: "display: block; width: 100%; height: 100%;",
            }
        }
    }
}

#[test]
fn a_custom_widget_is_painted_and_is_handed_its_box() {
    let doc = render(app).with_window_size(800, 600).build();
    doc.drain();
    doc.relayout();

    // Painting is what the `custom-widget` feature gates, so it has to be
    // triggered rather than assumed: `render_png` runs the same
    // `blitz_paint::paint_scene` the window does.
    let out = std::env::temp_dir().join("dioxus-test-custom-widget.png");
    doc.render_png(&out);

    let seen = SEEN.with(|s| *s.borrow());
    assert!(
        seen.calls > 0,
        "the widget was never painted — `custom-widget` is probably off \
         for blitz-dom/blitz-paint in this build graph"
    );

    // The box it was handed is its own element's, not the window's.
    let host = size_of(&doc, "host");
    assert_eq!(
        (seen.width, seen.height),
        (host.0 as u32, host.1 as u32),
        "the widget was painted at a size that is not its element's box"
    );
}

fn size_of(doc: &dioxus_test::DocumentTester, testid: &str) -> (f32, f32) {
    doc.query(by_testid(testid))
        .immediately()
        .unwrap_or_else(|e| panic!("no element {testid}: {e:?}"))
        .size()
}
