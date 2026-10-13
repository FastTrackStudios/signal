//! The native panels mount and draw.
//!
//! A Dioxus component that type-checks may still render nothing — an
//! attribute the renderer cannot parse, a group that swallows its own
//! contents. Both have happened in this tree. These render the panel to a
//! string and look for the shapes.

use dioxus::prelude::*;

fn render(app: fn() -> Element) -> String {
    let mut dom = VirtualDom::new(app);
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

#[test]
fn the_transport_bar_draws_its_buttons() {
    fn app() -> Element {
        rsx! {
            daw_ui::panels::NativeTransportBar {
                playing: Signal::new(true),
                bpm: 120.0,
            }
        }
    }
    let html = render(app);

    // Seven buttons, each an <svg> of its own.
    assert_eq!(
        html.matches("<svg").count(),
        7,
        "one svg per button:\n{html}"
    );
    // Glyphs are drawn, not typed and not blitted.
    assert!(html.contains("<path"), "no vector glyph:\n{html}");
    // Lit, because `playing` is true: the plate carries the blue bevel.
    assert!(
        html.contains("#4dbdfb"),
        "play should be lit and is not:\n{html}"
    );
    assert!(!html.contains("<img"), "a panel is blitting:\n{html}");
    assert!(!html.contains("url(data:"), "a panel is blitting:\n{html}");
}
