//! Mount only the visible mixer strips plus a small overscan margin.
//! Scrolling this pane never rebuilds the arrangement or expression editor.
use std::collections::HashMap;

use daw::service::{Fx, Track};
use daw_theme_art::geometry::mcp::STRIP_W;
use daw_ui::components::folders::FolderState;
use daw_ui::components::mixer::ChannelStripPreview;
use daw_ui::controls::FxSlotStack;
use dioxus::prelude::*;

use super::{FX_BAND_H, MIXER_W};

/// How many strips are mounted past each edge, and how far the pane must
/// scroll before the window is re-cut.
///
/// Mounting a channel strip is not cheap — measured with blitz-dom's
/// phase timer, a frame that mounted one cost 15-17 ms of `construct`
/// against 2 ms of everything else, because a strip is ~186 nodes of
/// traced vector art with its own gradients and text to shape. Re-cutting
/// on every strip of travel meant paying that on half the frames of a
/// scroll. Committing in steps, with enough overscan to cover the drift,
/// pays it on every third frame instead — the same trade the shared
/// TCP/timeline viewport makes, for the same reason.
const OVERSCAN_STRIPS: usize = 6;
/// The commit step, in strips. Must stay comfortably under the overscan.
const COMMIT_STRIPS: usize = 3;

fn strip_range(offset: f64, count: usize) -> std::ops::Range<usize> {
    let first = (offset.max(0.0) / STRIP_W as f64).floor() as usize;
    let visible = (MIXER_W / STRIP_W as f64).ceil() as usize;
    let first = first.min(count.saturating_sub(visible));
    first.saturating_sub(OVERSCAN_STRIPS)
        ..first.saturating_add(visible + OVERSCAN_STRIPS).min(count)
}

#[component]
pub(super) fn WorkstationMixer(
    tracks: Vec<Track>,
    depths: Vec<u32>,
    fx: HashMap<String, Vec<Fx>>,
    mut folders: Signal<FolderState>,
    height: f32,
    rule: String,
    background: String,
) -> Element {
    let mut first_strip = use_signal(|| 0usize);
    let range = strip_range(first_strip() as f64 * STRIP_W as f64, tracks.len());
    let total_w = tracks.len() as f32 * STRIP_W;
    let has_fx = !fx.is_empty();
    rsx! {
        div {
            "data-testid": "workstation-mixer",
            style: "flex:0 0 {MIXER_W}px;width:{MIXER_W}px;min-height:0;overflow-x:scroll;overflow-y:hidden;border-left:1px solid {rule};background:{background};",
            onscroll: move |event: ScrollEvent| {
                let next = (event.scroll_left().max(0.0) / STRIP_W as f64).floor() as usize;
                // Coarse on purpose: blitz scrolls the pane natively
                // whether or not dioxus hears about it, and the only
                // reason to hear about it is to mount what came into
                // range. See `OVERSCAN_STRIPS`.
                if first_strip().abs_diff(next) >= COMMIT_STRIPS {
                    first_strip.set(next);
                }
            },
            div {
                style: "position:relative;width:{total_w}px;height:100%;",
                for i in range {
                    {
                        let track = &tracks[i];
                        let guid = track.guid.clone();
                        rsx! {
                            div {
                                key: "{guid}",
                                style: "position:absolute;left:{i as f32 * STRIP_W}px;top:0;width:{STRIP_W}px;display:flex;flex-direction:column;overflow:hidden;",
                                if has_fx {
                                    div {
                                        style: "height:{FX_BAND_H}px;flex:0 0 auto;display:flex;align-items:flex-end;overflow:hidden;",
                                        FxSlotStack { fx: fx.get(&guid).cloned().unwrap_or_default(), width: STRIP_W }
                                    }
                                }
                                ChannelStripPreview {
                                    track: track.clone(), index: track.index, depth: depths[i],
                                    collapsed: folders.read().is_collapsed(&guid),
                                    onfoldertoggle: { let guid = guid.clone(); move |_| folders.write().toggle(&guid) },
                                    height,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips_are_bounded_at_both_edges_and_after_folding() {
        assert_eq!(strip_range(0.0, 0), 0..0);
        // Five strips fit the column; the rest of the window is overscan.
        const VISIBLE: usize = 5;
        assert_eq!(strip_range(0.0, 1000), 0..(VISIBLE + OVERSCAN_STRIPS));
        let end = strip_range(1e9, 1000);
        assert_eq!(end.end, 1000);
        assert!(end.len() <= VISIBLE + 2 * OVERSCAN_STRIPS);
        // The overscan has to outrun the commit step, or a pane that has
        // scrolled but not yet re-cut its window would reach strips that
        // were never mounted.
        assert!(OVERSCAN_STRIPS >= 2 * COMMIT_STRIPS);
        assert_eq!(strip_range(1e9, 3), 0..3);
    }

    #[component]
    fn LongMixer() -> Element {
        let folders = use_signal(FolderState::default);
        let tracks = (0..65)
            .map(|index| Track {
                guid: format!("track-{index}"),
                name: format!("Track {index}"),
                index,
                volume: 1.0,
                ..Default::default()
            })
            .collect();
        rsx! { div { style: "display:flex;height:900px;",
            WorkstationMixer { tracks, depths:vec![0;65], fx:HashMap::new(), folders,
                height:850.0, rule:"black", background:"black" }
        } }
    }

    #[test]
    fn scrolling_unmounts_and_restores_strips() {
        use blitz_traits::events::{
            BlitzWheelDelta, BlitzWheelEvent, MouseEventButtons, Point, PointerCoords, UiEvent,
        };
        use dioxus_test::{by_testid, render};
        let tester = render(LongMixer).with_window_size(400, 920).build();
        tester.drain();
        tester.relayout();
        let el = tester
            .query(by_testid("workstation-mixer"))
            .immediately()
            .unwrap();
        let (x, y) = el.document_origin();
        let (x, y) = ((x + 200.0) as f32, (y + 10.0) as f32);
        for frame in 0..120 {
            tester.pointer_move(x as f64, y as f64, false);
            tester.send_ui_event(UiEvent::Wheel(BlitzWheelEvent {
                delta: BlitzWheelDelta::Pixels(if frame < 60 { -80.0 } else { 80.0 }, 0.0),
                coords: PointerCoords {
                    page_x: x,
                    page_y: y,
                    screen_x: x,
                    screen_y: y,
                    client_x: x,
                    client_y: y,
                },
                buttons: MouseEventButtons::None,
                mods: Default::default(),
                element: Point::default(),
            }));
            tester.drain();
            tester.relayout();
            if frame == 59 {
                assert!(
                    tester
                        .query(by_testid("mcp-track-0"))
                        .immediately()
                        .is_err()
                );
                assert!(
                    tester
                        .query(by_testid("mcp-track-56"))
                        .immediately()
                        .is_ok()
                );
            }
        }
        assert!(tester.query(by_testid("mcp-track-0")).immediately().is_ok());
    }
}
