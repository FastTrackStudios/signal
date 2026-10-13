//! How big the window is, on whichever renderer is hosting it.
//!
//! The workstation lays its panes out from pixel numbers — the arrange
//! pane's share of the height, the left column's width beside a fixed
//! mixer, the editor's cell — so it has to know. None of the three
//! renderers this UI targets agrees on how to say:
//!
//! | target | how |
//! |---|---|
//! | dioxus-native | winit's resize event, via `use_window_event` |
//! | dioxus-desktop | a `resize` listener in the WebView |
//! | dioxus-web | the same listener, same code |
//!
//! The two WebView targets share one implementation, because `eval`'s
//! `dioxus.send` channel is the same on both — which is the whole reason
//! to reach for it rather than each renderer's own window handle.
//!
//! Mounted as a component rather than called as a hook. Both paths need
//! hooks of their own, and hooks cannot be called conditionally — so the
//! decision about which renderer is hosting is made by whether this is
//! mounted at all, and once it is, its own hooks run unconditionally for
//! its whole life. A headless mount (the DOM benchmark, the workstation
//! tests) simply does not mount it and keeps the size it was staged with.

use dioxus::prelude::*;

/// Whether a window exists to be asked.
///
/// Native has to look: the winit context is provided by the renderer and
/// is simply absent in a headless mount. A WebView always has a
/// `window`, and `eval` is how we reach it, so there is nothing to
/// check.
pub fn available() -> bool {
    #[cfg(not(feature = "webview"))]
    {
        dioxus::prelude::try_consume_context::<
            std::sync::Arc<dyn dioxus_native::winit::window::Window>,
        >()
        .is_some()
    }
    #[cfg(feature = "webview")]
    {
        true
    }
}

/// Keeps `size` in step with the window, in CSS pixels.
#[component]
pub fn WindowSize(size: Signal<(f64, f64)>) -> Element {
    inner(size)
}

/// Only a real change should re-lay the window out: a resize drag fires
/// continuously, and a sub-pixel wobble is not a resize.
fn commit(size: &mut Signal<(f64, f64)>, w: f64, h: f64) {
    let (w, h) = (w.max(1.0), h.max(1.0));
    let (was_w, was_h) = *size.peek();
    if (was_w - w).abs() >= 1.0 || (was_h - h).abs() >= 1.0 {
        size.set((w, h));
    }
}

/// Blitz: ask winit, which already knows.
///
/// Not measured off the DOM — dioxus-native never delivers an element
/// resize (`convert_resize_data` is `unimplemented!()`), and awaiting a
/// client rect from a task borrows the document out from under whatever
/// holds it, which is the "RefCell already borrowed" re-entrancy the
/// sizing notes warn about.
#[cfg(not(feature = "webview"))]
fn inner(mut size: Signal<(f64, f64)>) -> Element {
    use dioxus_native::winit::event::WindowEvent;
    use dioxus_native::winit::window::Window;

    let window = dioxus_native::use_window();
    // CSS pixels, which is what every number in the layout is. winit
    // reports physical ones, and on a scaled display the two differ.
    fn logical(window: &dyn Window) -> (f64, f64) {
        let physical = window.surface_size();
        let scale = window.scale_factor().max(f64::EPSILON);
        (
            f64::from(physical.width) / scale,
            f64::from(physical.height) / scale,
        )
    }
    // The size at mount: a window that is never resized still has one,
    // and it is not necessarily the size the runner asked for.
    use_hook({
        let window = window.clone();
        move || {
            let (w, h) = logical(window.as_ref());
            commit(&mut size, w, h);
        }
    });
    dioxus_native::use_window_event(move |event, _| {
        if matches!(
            event,
            WindowEvent::SurfaceResized(_) | WindowEvent::ScaleFactorChanged { .. }
        ) {
            let (w, h) = logical(window.as_ref());
            commit(&mut size, w, h);
        }
    });
    rsx! {}
}

/// WebView (dioxus-desktop) and browser (dioxus-web): listen for the
/// `resize` event and send the viewport back over `eval`'s channel.
///
/// `innerWidth`/`innerHeight` are already CSS pixels, so there is no
/// scale factor to divide out here — the browser has done it.
#[cfg(feature = "webview")]
fn inner(mut size: Signal<(f64, f64)>) -> Element {
    use_future(move || async move {
        let mut eval = document::eval(
            r"
            const send = () => dioxus.send([window.innerWidth, window.innerHeight]);
            // Once now, so a window that is never resized is still measured.
            send();
            window.addEventListener('resize', send);
            ",
        );
        // The listener outlives the script, so this receives until the
        // document goes away.
        while let Ok([w, h]) = eval.recv::<[f64; 2]>().await {
            commit(&mut size, w, h);
        }
    });
    rsx! {}
}
