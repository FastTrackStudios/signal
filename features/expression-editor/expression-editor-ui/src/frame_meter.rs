//! Where "frames per second" comes from on a WebView.
//!
//! Nothing in this crate paints there — the engine does — so the only
//! honest source is its own presentation loop. `requestAnimationFrame`
//! fires once per composited frame, which is the number that matches what
//! the eye sees; a count of renders or of DOM mutations does not, and the
//! stress guide is emphatic about never labelling one as the other.
//!
//! Averaged in the page and reported a few times a second rather than
//! ticked per frame. A bridge crossing per frame would be a measurable
//! share of the thing being measured, and the interesting quantity over a
//! drag is the sustained rate anyway.
//!
//! Native needs none of this: `RollWidget::paint` is called by the
//! renderer itself, which is a better vantage point than the page.

use dioxus::prelude::*;

use crate::roll_widget::Frames;

/// Reports the WebView's real frame rate into the shared [`Frames`].
///
/// Renders nothing. Mounted once near the root of a surface that wants
/// the readout; the toolbar picks the numbers up from context.
#[component]
pub fn FrameMeter() -> Element {
    let frames = try_consume_context::<Frames>();
    let mut eval = use_hook(|| {
        document::eval(
            r"
            let windowStart = performance.now();
            let last = windowStart;
            let frames = 0;
            let worst = 0;
            const tick = (now) => {
                const delta = now - last;
                last = now;
                frames += 1;
                // The worst frame in the window, not the mean: a drag
                // that stutters averages well and feels terrible.
                if (delta > worst) { worst = delta; }
                const elapsed = now - windowStart;
                // A rate over the whole window, not the instantaneous
                // gap of whichever frame happened to close it.
                if (elapsed >= 500) {
                    dioxus.send([(frames * 1000) / elapsed, worst]);
                    windowStart = now;
                    frames = 0;
                    worst = 0;
                }
                requestAnimationFrame(tick);
            };
            requestAnimationFrame(tick);
            ",
        )
    });
    use_future(move || {
        let frames = frames.clone();
        async move {
            while let Ok([fps, worst_ms]) = eval.recv::<[f64; 2]>().await {
                // Also to the log, so a rate can be read off a terminal
                // rather than a screenshot of the toolbar. This is the
                // number an A/B on a scroll is decided by, and reading it
                // out of a picture of a window is how a comparison gets
                // lost. `RUST_LOG=expression_editor_ui::frame_meter=info`.
                tracing::info!(
                    ui.fps = fps,
                    ui.worst_frame_ms = worst_ms,
                    "webview frame rate"
                );
                if let Some(frames) = frames.as_ref() {
                    frames.observe_rate(fps, worst_ms);
                }
            }
        }
    });
    rsx! {}
}
