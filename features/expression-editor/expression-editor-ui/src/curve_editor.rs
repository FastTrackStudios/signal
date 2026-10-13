//! The curve editor, drawn as the velocities it produces.
//!
//! MVelocity draws an abstract Bézier in its own little box, separate
//! from the notes it affects — you shape a line and then find out what
//! it did. Here the box *is* the result: every note in the take gets a
//! bar at its resolved velocity, the full chain applied, and the curve's
//! control handles float on top of it. Dragging a handle reshapes the
//! bars underneath it live.
//!
//! That collapses two widgets into one and removes the guesswork.
//! Selected notes are drawn brighter than unselected ones, so it also
//! answers "what am I about to edit?" without a second control.
//!
//! Rendered with `div`s rather than SVG: Blitz's SVG support is not
//! something this panel should bet its primary readout on, and a bar per
//! note is honest about the fact that velocity is discrete per note
//! rather than continuous.

use dioxus::prelude::*;
use expression_editor_tools::velocity::{Curve, MAX_VELOCITY, Note, Point};

/// Height of the editor box in pixels.
const HEIGHT: f64 = 132.0;
/// Hit/handle size for a control point.
const HANDLE: f64 = 13.0;

/// Bars for `notes` with `curve`'s control points overlaid.
///
/// `curve` is `None` when no curve is engaged — the bars still render
/// (they're the preview of everything else in the chain), just without
/// handles.
#[component]
pub fn CurveEditor(
    /// The fully resolved notes — what the take will look like on commit.
    resolved: Vec<Note>,
    curve: Option<Curve>,
    on_curve: EventHandler<Curve>,
) -> Element {
    // Which control point the current drag owns.
    let mut held = use_signal(|| Option::<usize>::None);
    let mut rect = use_signal(|| (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64));
    // Held so the box can be re-measured at the start of each gesture.
    // Measuring only at mount is why this used to jump: the box sits
    // under a header whose text changes width with the note count, and
    // the panel scrolls, so a mount-time rect goes stale almost at once.
    let mut mounted: Signal<Option<std::rc::Rc<MountedData>>> = use_signal(|| None);

    let points: Vec<Point> = curve
        .as_ref()
        .map(|c| c.points().to_vec())
        .unwrap_or_default();

    // A drag maps pointer position straight to (x, y) in curve space.
    // Both axes move: pinning x would make the interior handles unable to
    // shift where a curve's bend falls, which is most of why you'd reach
    // for a handle rather than a preset.
    let move_point = {
        let points = points.clone();
        move |i: usize, cx: f64, cy: f64| {
            let (rx, ry, rw, rh) = rect();
            if rw <= 0.0 || rh <= 0.0 {
                return;
            }
            let mut next = points.clone();
            if let Some(p) = next.get_mut(i) {
                *p = Point::new(
                    ((cx - rx) / rw).clamp(0.0, 1.0),
                    (1.0 - (cy - ry) / rh).clamp(0.0, 1.0),
                );
            }
            on_curve.call(Curve::new(next));
        }
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

    let count = resolved.len().max(1);
    // Below roughly a pixel per bar there's nothing to see and a gap
    // costs more than the bar; pack them flush instead.
    let gap = if count > 96 { 0.0 } else { 1.0 };

    rsx! {
        div {
            style: "position:relative; height:{HEIGHT}px; padding:4px; border-radius:5px; background:var(--muted, #171717); border:1px solid var(--border, #333); overflow:hidden; touch-action:none;",
            onmounted: move |e| async move {
                mounted.set(Some(e.data()));
                measure().await;
            },
            onresize: move |_| async move { measure().await },
            onmousemove: move |e| {
                if let Some(i) = held() {
                    let c = e.data().client_coordinates();
                    move_point(i, c.x, c.y);
                }
            },
            onmouseup: move |_| held.set(None),
            // Deliberately no `onpointerleave`: dropping the handle the
            // moment the pointer crosses the box edge is what made
            // dragging a control point to the very top or bottom — i.e.
            // to full or zero velocity, the two values you most want —
            // feel like it kept slipping out of your hand.

            // The velocity bars — the actual readout.
            div {
                style: "display:flex; align-items:flex-end; gap:{gap}px; height:100%; width:100%;",
                for note in resolved.iter() {
                    div {
                        key: "{note.index}",
                        style: "flex:1; min-width:1px; height:{f64::from(note.velocity) / f64::from(MAX_VELOCITY) * 100.0}%; border-radius:1px 1px 0 0; background:var(--primary, #d2691e); opacity:{bar_opacity(note.selected)}; pointer-events:none;",
                    }
                }
            }

            // Control handles, floating over the bars.
            for (i, p) in points.iter().copied().enumerate() {
                div {
                    key: "handle-{i}",
                    style: "position:absolute; left:calc({p.x * 100.0}% - {HANDLE / 2.0}px); top:calc({(1.0 - p.y) * 100.0}% - {HANDLE / 2.0}px); width:{HANDLE}px; height:{HANDLE}px; border-radius:50%; background:var(--background, #101010); border:2px solid var(--primary, #d2691e); cursor:grab; box-shadow:0 0 0 1px rgba(0,0,0,0.6); touch-action:none;",
                    onmousedown: move |_| async move {
                        // Re-measure before the gesture can act on a
                        // coordinate, so the first move never snaps the
                        // handle to a position computed from a stale box.
                        measure().await;
                        held.set(Some(i));
                    },
                }
            }
        }
    }
}

/// Selected notes read brighter; unselected ones recede.
///
/// Opacity over one colour, rather than two theme tokens. The obvious
/// pairing — `--primary` for selected, `--secondary` for the rest —
/// renders the whole strip invisible in the fts dark theme, where
/// `--secondary` and `--muted` are both `oklch(0.269 0 0)`: the bars come
/// out exactly the colour of the box they sit in. Fading one colour can't
/// collide with the background in either theme, since `--primary` is
/// defined to contrast with the surface.
fn bar_opacity(selected: bool) -> f64 {
    if selected { 1.0 } else { 0.42 }
}
