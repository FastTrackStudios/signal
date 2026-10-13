//! Sliders and the bar editor.
//!
//! Hand-rolled, deliberately, and on a very specific pattern. Two rounds
//! of getting this wrong are encoded here.
//!
//! **Round one — flicker.** The first version fed its *clamped* value
//! back in as its displayed value, so the thumb oscillated whenever the
//! pointer sat between two steps, and it measured its track only at mount,
//! so it drifted as soon as the panel scrolled.
//!
//! **Round two — a crash.** The fix was to wrap
//! `dioxus_primitives::slider`, which solves both. It also panics: its
//! `onpointerdown` runs `spawn(async { … get_client_rect().await … })`,
//! and under blitz-dom the document is still borrowed by the event
//! dispatch when that task is polled — `RefCell already borrowed`, on a
//! plain click, before any drag. Blitz is the REAPER renderer, so that
//! would have been a panel that crashes the moment you touch a slider.
//! `tests/slider_drag.rs` is the regression guard.
//!
//! So: hand-rolled again, keeping the primitive's two good ideas and
//! avoiding its fatal one.
//!
//! - **Async handler, not `spawn`.** `onmousedown: move |e| async move
//!   { … }` is awaited by dioxus *after* event dispatch releases the
//!   document, so measuring inside it is safe. This is the whole
//!   difference between working and panicking, and it's why the bar
//!   editor was fine while the primitive slider was not.
//! - **Granular state.** A drag tracks its own unclamped value and
//!   reports the clamped one, so feedback can't oscillate.
//! - **Re-measure per gesture**, not once at mount.
//! - **No `pointerleave` cancel** — a gesture ends when the pointer is
//!   released, wherever that happens.
//!
//! Styling is inline throughout: the panel must render identically
//! standalone, as a plugin, and inside REAPER through Blitz, and Blitz
//! does not load external CSS reliably.

use dioxus::prelude::*;

/// Thumb diameter. The slider's row is sized to it so a thumb that
/// overhangs the 6px track doesn't get clipped by the row above.
const THUMB: f64 = 14.0;
const TRACK_H: f64 = 6.0;

/// Fraction of the way along a track, clamped.
fn fraction(origin_x: f64, width: f64, client_x: f64) -> f64 {
    if width <= 0.0 {
        return 0.0;
    }
    ((client_x - origin_x) / width).clamp(0.0, 1.0)
}

/// Snap `v` to `step` within `min..=max`.
fn quantize(v: f64, min: f64, max: f64, step: f64) -> f64 {
    if step <= 0.0 {
        return v.clamp(min, max);
    }
    (((v - min) / step).round() * step + min).clamp(min, max)
}

/// A horizontal slider over an arbitrary numeric range.
///
/// `on_change` fires continuously through the drag — every engine here is
/// cheap and pure, so live feedback costs nothing and a velocity tool
/// that only updates on release is unusable.
#[component]
pub fn Slider(
    value: f64,
    min: f64,
    max: f64,
    #[props(default = 0.01)] step: f64,
    #[props(default = 120.0)] width: f64,
    /// Test hook. Empty in production; a headless test resolves the
    /// wrapper's geometry through it to synthesize a real drag.
    #[props(default)]
    testid: String,
    on_change: EventHandler<f64>,
) -> Element {
    let span = (max - min).max(f64::EPSILON);
    // The live, unclamped value while a gesture is in flight. `None`
    // between gestures, when the prop is the truth.
    let mut granular = use_signal(|| Option::<f64>::None);
    let mut rect = use_signal(|| (0.0_f64, 0.0_f64));
    let mut mounted: Signal<Option<std::rc::Rc<MountedData>>> = use_signal(|| None);

    let measure = move || async move {
        let r = match mounted() {
            Some(el) => el.get_client_rect().await.ok(),
            None => None,
        };
        if let Some(r) = r {
            rect.set((r.origin.x, r.size.width));
        }
    };

    let mut emit = move |client_x: f64| {
        let (x, w) = rect();
        let raw = min + fraction(x, w, client_x) * span;
        granular.set(Some(raw));
        on_change.call(quantize(raw, min, max, step));
    };

    // Drawn from the granular value mid-gesture so the thumb tracks the
    // pointer rather than the quantized feedback.
    let shown = granular().unwrap_or(value);
    let position = ((shown - min) / span).clamp(0.0, 1.0) * 100.0;

    rsx! {
        div {
            "data-testid": "{testid}",
            style: "position:relative; width:{width}px; flex:none; display:flex; align-items:center; height:{THUMB}px; cursor:pointer; touch-action:none; user-select:none;",
            onmounted: move |e| async move {
                mounted.set(Some(e.data()));
                measure().await;
            },
            onresize: move |_| async move { measure().await },
            onmousedown: move |e| async move {
                // Async, not `spawn`: this runs after event dispatch has
                // released the document borrow. See the module docs.
                measure().await;
                emit(e.data().client_coordinates().x);
            },
            onmousemove: move |e| {
                if granular().is_some() {
                    emit(e.data().client_coordinates().x);
                }
            },
            onmouseup: move |_| granular.set(None),

            div { style: "position:absolute; left:0; right:0; height:{TRACK_H}px; border-radius:3px; background:var(--muted, #2a2a2a);" }
            div { style: "position:absolute; left:0; width:{position}%; height:{TRACK_H}px; border-radius:3px; background:var(--primary, #d2691e);" }
            div {
                style: "position:absolute; left:calc({position}% - {THUMB / 2.0}px); width:{THUMB}px; height:{THUMB}px; border-radius:50%; background:var(--foreground, #e8e8e8); border:1px solid var(--border, #444); pointer-events:none;",
            }
        }
    }
}

/// A two-thumb range slider — MVelocity's RANGE control.
///
/// The active thumb is chosen once, on press. Re-picking the nearer thumb
/// per move is what made the first version swap its thumbs mid-drag as
/// soon as they crossed.
#[component]
pub fn RangeSlider(
    low: f64,
    high: f64,
    min: f64,
    max: f64,
    #[props(default = 1.0)] step: f64,
    #[props(default = 150.0)] width: f64,
    #[props(default)] testid: String,
    on_change: EventHandler<(f64, f64)>,
) -> Element {
    let span = (max - min).max(f64::EPSILON);
    let mut held = use_signal(|| Option::<bool>::None);
    let mut rect = use_signal(|| (0.0_f64, 0.0_f64));
    let mut mounted: Signal<Option<std::rc::Rc<MountedData>>> = use_signal(|| None);

    let measure = move || async move {
        let r = match mounted() {
            Some(el) => el.get_client_rect().await.ok(),
            None => None,
        };
        if let Some(r) = r {
            rect.set((r.origin.x, r.size.width));
        }
    };

    let lo_pos = ((low - min) / span).clamp(0.0, 1.0);
    let hi_pos = ((high - min) / span).clamp(0.0, 1.0);

    let apply = move |client_x: f64, low_thumb: bool| {
        let (x, w) = rect();
        let v = quantize(min + fraction(x, w, client_x) * span, min, max, step);
        if low_thumb {
            on_change.call((v, high));
        } else {
            on_change.call((low, v));
        }
    };

    rsx! {
        div {
            "data-testid": "{testid}",
            style: "position:relative; width:{width}px; flex:none; display:flex; align-items:center; height:{THUMB}px; cursor:pointer; touch-action:none; user-select:none;",
            onmounted: move |e| async move {
                mounted.set(Some(e.data()));
                measure().await;
            },
            onresize: move |_| async move { measure().await },
            onmousedown: move |e| async move {
                measure().await;
                let (x, w) = rect();
                let f = fraction(x, w, e.data().client_coordinates().x);
                let low_thumb = (f - lo_pos).abs() <= (f - hi_pos).abs();
                held.set(Some(low_thumb));
                apply(e.data().client_coordinates().x, low_thumb);
            },
            onmousemove: move |e| {
                if let Some(low_thumb) = held() {
                    apply(e.data().client_coordinates().x, low_thumb);
                }
            },
            onmouseup: move |_| held.set(None),

            div { style: "position:absolute; left:0; right:0; height:{TRACK_H}px; border-radius:3px; background:var(--muted, #2a2a2a);" }
            div {
                style: "position:absolute; left:{lo_pos.min(hi_pos) * 100.0}%; width:{(hi_pos - lo_pos).abs() * 100.0}%; height:{TRACK_H}px; border-radius:3px; background:var(--primary, #d2691e);",
            }
            for pos in [lo_pos, hi_pos] {
                div {
                    style: "position:absolute; left:calc({pos * 100.0}% - {THUMB / 2.0}px); width:{THUMB}px; height:{THUMB}px; border-radius:50%; background:var(--foreground, #e8e8e8); border:1px solid var(--border, #444); pointer-events:none;",
                }
            }
        }
    }
}

/// A column of vertical bars you can draw on by dragging across them.
///
/// MVelocity's step-velocity slider bank, and how a pattern should be
/// edited: the bars *are* the pattern, drawn at the same scale as the
/// velocities they set, and dragging across several sets them all in one
/// gesture the way a drum machine's velocity lane does.
///
/// Hand-rolled because no primitive covers "N independent values, drawn
/// across" — but it takes the primitive's two lessons: the box is
/// re-measured at the start of every gesture rather than only at mount,
/// and leaving the box does not cancel the drag.
#[component]
pub fn BarEditor(
    /// Bar heights, in `1..=max`.
    values: Vec<u8>,
    #[props(default = 127)] max: u8,
    #[props(default = 88.0)] height: f64,
    #[props(default)] testid: String,
    on_change: EventHandler<(usize, u8)>,
) -> Element {
    let mut dragging = use_signal(|| false);
    let mut rect = use_signal(|| (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64));
    // Held so the box can be re-measured mid-gesture. Measuring only at
    // mount is what makes a hand-rolled widget drift once anything above
    // it grows or the panel scrolls.
    let mut mounted: Signal<Option<std::rc::Rc<MountedData>>> = use_signal(|| None);
    let count = values.len().max(1);

    // Which bar the pointer is over, and how high up it sits.
    let hit = move |cx: f64, cy: f64| -> Option<(usize, u8)> {
        let (x, y, w, h) = rect();
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let i = (((cx - x) / w) * count as f64)
            .floor()
            .clamp(0.0, (count - 1) as f64) as usize;
        // Inverted: the top of the box is the highest velocity.
        let frac = 1.0 - ((cy - y) / h).clamp(0.0, 1.0);
        Some((i, (frac * f64::from(max)).round().max(1.0) as u8))
    };

    let measure = move || async move {
        let r = match mounted() {
            Some(el) => el.get_client_rect().await.ok(),
            None => None,
        };
        if let Some(r) = r {
            rect.set((r.origin.x, r.origin.y, r.size.width, r.size.height));
        }
    };

    rsx! {
        div {
            "data-testid": "{testid}",
            style: "position:relative; display:flex; align-items:flex-end; gap:2px; height:{height}px; padding:3px; border-radius:4px; background:var(--muted, #1e1e1e); border:1px solid var(--border, #333); cursor:crosshair; touch-action:none; user-select:none;",
            onmounted: move |e| async move {
                mounted.set(Some(e.data()));
                measure().await;
            },
            onresize: move |_| async move { measure().await },
            onmousedown: move |e| async move {
                dragging.set(true);
                // Re-measure before acting on the very first coordinate,
                // so a gesture can never be applied against a stale box.
                measure().await;
                let c = e.data().client_coordinates();
                if let Some((i, v)) = hit(c.x, c.y) {
                    on_change.call((i, v));
                }
            },
            onmousemove: move |e| {
                if dragging() {
                    let c = e.data().client_coordinates();
                    if let Some((i, v)) = hit(c.x, c.y) {
                        on_change.call((i, v));
                    }
                }
            },
            onmouseup: move |_| dragging.set(false),
            // Deliberately no `onpointerleave` handler: cancelling the
            // drag on leave means a fast stroke that strays a pixel above
            // the box drops the rest of the gesture, which is most of what
            // made this widget feel unreliable. The gesture ends when the
            // pointer is released, wherever that happens.

            for (i, v) in values.iter().copied().enumerate() {
                div {
                    key: "{i}",
                    style: "flex:1; min-width:6px; height:{f64::from(v) / f64::from(max) * 100.0}%; border-radius:2px 2px 0 0; background:var(--primary, #d2691e); pointer-events:none;",
                }
            }
        }
    }
}
