//! Opening Signal: its logo — the green wave — drawing itself on, its glow
//! swelling behind it and a glint running along it while the rig comes up,
//! the name tracking in under it; held until the rig's profile and chain are
//! in (and at least long enough to land), then lifted away.
//!
//! CSS keyframes (opacity, transform, width): the window draws document
//! animations at the screen's own rate, 120 Hz on ProMotion. Blitz draws no
//! SVG blur, so the glow is the wave stroked wide and faint, layer on layer.

use dioxus::prelude::*;

/// Whether the player asked for less motion (iOS Settings › Accessibility ›
/// Motion › Reduce Motion). Elsewhere: no.
#[must_use]
pub fn reduce_motion() -> bool {
    #[cfg(target_os = "ios")]
    {
        #[link(name = "UIKit", kind = "framework")]
        unsafe extern "C" {
            fn UIAccessibilityIsReduceMotionEnabled() -> bool;
        }
        // SAFETY: a plain query of a system setting, no arguments.
        unsafe { UIAccessibilityIsReduceMotionEnabled() }
    }
    #[cfg(not(target_os = "ios"))]
    {
        false
    }
}

/// The logo's wave, from the app icon (`apps/desktop/ios/icon.svg`), in its
/// 1024 space: a burst of sine across the middle.
const WAVE: &str = "M200 512 L204 511 L207 508 L210 504 L214 500 L217 494 L221 488 L224 482 L228 474 L231 467 L235 460 L238 452 L242 445 L245 438 L248 431 L252 425 L256 419 L259 414 L262 410 L266 407 L269 404 L273 403 L276 402 L280 403 L283 405 L287 408 L290 412 L294 418 L297 424 L300 432 L304 441 L308 450 L311 461 L314 473 L318 485 L321 498 L325 512 L328 526 L332 541 L335 555 L339 570 L342 585 L346 600 L349 614 L352 628 L356 641 L360 653 L363 665 L366 675 L370 685 L373 693 L377 700 L380 705 L384 710 L387 712 L391 713 L394 712 L398 710 L401 706 L404 701 L408 694 L412 685 L415 675 L418 663 L422 650 L425 636 L429 621 L432 604 L436 587 L439 569 L443 551 L446 531 L450 512 L453 492 L456 473 L460 453 L464 434 L467 416 L470 398 L474 381 L477 364 L481 349 L484 336 L488 323 L491 312 L495 302 L498 294 L502 288 L505 284 L508 281 L512 280 L516 281 L519 284 L522 288 L526 294 L529 302 L533 312 L536 323 L540 336 L543 349 L547 364 L550 381 L554 398 L557 416 L560 434 L564 453 L568 473 L571 492 L574 512 L578 531 L581 551 L585 569 L588 587 L592 604 L595 621 L599 636 L602 650 L606 663 L609 675 L612 685 L616 694 L620 701 L623 706 L626 710 L630 712 L633 713 L637 712 L640 710 L644 705 L647 700 L651 693 L654 685 L658 675 L661 665 L664 653 L668 641 L672 628 L675 614 L678 600 L682 585 L685 570 L689 555 L692 541 L696 526 L699 512 L703 498 L706 485 L710 473 L713 461 L716 450 L720 441 L724 432 L727 424 L730 418 L734 412 L737 408 L741 405 L744 403 L748 402 L751 403 L755 404 L758 407 L762 410 L765 414 L768 419 L772 425 L776 431 L779 438 L782 445 L786 452 L789 460 L793 467 L796 474 L800 482 L803 488 L807 494 L810 500 L814 504 L817 508 L820 511 L824 512";
/// The wave's box in that space (its stroke and glow inside).
const VIEW: (f64, f64, f64, f64) = (130.0, 210.0, 764.0, 574.0);
/// The logo's width on screen, pt.
const LOGO_W: f64 = 340.0;
const GREEN: &str = "#2FD673";
const MINT: &str = "#B6F7CB";
const DEEP: &str = "#0E7C42";
/// Shown at least this long, so the wave lands before it lifts.
const MIN_MS: u64 = 1900;
/// When the wave has drawn on (its 150 ms wait and 1100 ms draw) and the
/// page may be built under it: building takes the main thread a moment,
/// which would hold the drawing mid-stroke.
pub(crate) const DRAWN_MS: u64 = 1300;
/// A glint slice's width, pt (seven make the glint), its sweep, and when
/// it starts: after the page is built under the logo, so an ordinary
/// opening lifts away on the still logo and only a slow one shows it.
const GLINT: f64 = 14.0;
const SCAN_MS: u64 = 1600;
const GLINT_FROM_MS: u64 = 2000;
/// The lift away.
const OUT_MS: u64 = 560;

const CSS: &str = "\
@keyframes sig-draw{from{width:0px}to{width:__W__px}}\
@keyframes sig-glow{0%{opacity:0;transform:scale(0.96)}100%{opacity:1;transform:scale(1)}}\
@keyframes sig-breathe{0%{opacity:0.55}50%{opacity:1}100%{opacity:0.55}}\
@keyframes sig-scan{from{transform:translateX(-98px)}to{transform:translateX(__SCAN__px)}}\
@keyframes sig-unscan{from{transform:translateX(98px)}to{transform:translateX(-__SCAN__px)}}\
@keyframes sig-word{from{opacity:0;letter-spacing:0.9em}to{opacity:1;letter-spacing:0.38em}}\
@keyframes sig-rise{from{opacity:0;transform:translateY(14px) scale(0.94)}to{opacity:1;transform:translateY(0) scale(1)}}\
@keyframes sig-out{from{opacity:1;transform:scale(1)}to{opacity:0;transform:scale(1.08)}}";

/// The wave stroked once, as an SVG `LOGO_W` wide.
#[component]
fn Wave(stroke: String, width: f64, opacity: f64) -> Element {
    let (x, y, w, h) = VIEW;
    let tall = LOGO_W * h / w;
    rsx! {
        svg { width: "{LOGO_W}", height: "{tall}", view_box: "{x} {y} {w} {h}", style: "display: block; flex-shrink: 0;",
            defs {
                linearGradient { id: "sig-body", gradient_units: "userSpaceOnUse", x1: "180", y1: "200", x2: "860", y2: "880",
                    stop { offset: "0", stop_color: MINT }
                    stop { offset: "0.42", stop_color: GREEN }
                    stop { offset: "1", stop_color: DEEP }
                }
            }
            path { d: WAVE, fill: "none", stroke: "{stroke}", stroke_width: "{width}", stroke_linecap: "round", stroke_linejoin: "round", opacity: "{opacity}" }
        }
    }
}

/// Over everything until `ready`, then gone.
#[component]
pub fn Splash(ready: bool) -> Element {
    let mut held = use_signal(|| true);
    let mut leaving = use_signal(|| false);
    let mut gone = use_signal(|| false);
    use_hook(move || {
        spawn(async move {
            architect::platform::sleep(std::time::Duration::from_millis(MIN_MS)).await;
            held.set(false);
        });
    });
    // `ready` is a prop: reactive, so the effect sees it change.
    use_effect(use_reactive!(|ready| {
        if ready && !held() && !*leaving.peek() {
            leaving.set(true);
            spawn(async move {
                architect::platform::sleep(std::time::Duration::from_millis(OUT_MS)).await;
                gone.set(true);
            });
        }
    }));
    if gone() {
        return rsx! {};
    }
    // Less motion asked for: the logo still, whole, and a plain fade away —
    // no drawing on, no glint, no breathing.
    if reduce_motion() {
        let tall = LOGO_W * VIEW.3 / VIEW.2;
        let fade = if leaving() { format!("opacity: 0; transition: opacity {OUT_MS}ms linear; pointer-events: none;") } else { String::new() };
        return rsx! {
            div {
                style: "position: absolute; left: 0; top: 0; right: 0; bottom: 0; z-index: 1000; display: flex; flex-direction: column; align-items: center; justify-content: center; background: #050506; {fade}",
                div { style: "position: relative; width: {LOGO_W}px; height: {tall}px;",
                    Wave { stroke: "url(#sig-body)".to_string(), width: 76.0, opacity: 1.0 }
                }
                div { style: "margin-top: 40px; padding-left: 0.38em; font-size: 30px; font-weight: 800; color: #f4f4f5; letter-spacing: 0.38em;", "SIGNAL" }
            }
        };
    }
    let tall = LOGO_W * VIEW.3 / VIEW.2;
    let out = if leaving() { format!("animation: sig-out {OUT_MS}ms cubic-bezier(0.4, 0, 1, 1) forwards; pointer-events: none;") } else { String::new() };
    let css = CSS.replace("__W__", &format!("{LOGO_W}")).replace("__SCAN__", &format!("{}", LOGO_W));
    rsx! {
        document::Style { {css} }
        div {
            style: "position: absolute; left: 0; top: 0; right: 0; bottom: 0; z-index: 1000; display: flex; flex-direction: column; align-items: center; justify-content: center; background: radial-gradient(circle at 50% 44%, #12161a 0%, #0b0c0e 42%, #050506 100%); {out}",
            div { style: "position: relative; display: flex; flex-direction: column; align-items: center; animation: sig-rise 800ms cubic-bezier(0.16, 1, 0.3, 1) both;",
                div { style: "position: relative; width: {LOGO_W}px; height: {tall}px;",
                    // The green light the wave throws, breathing.
                    div { style: "position: absolute; left: -120px; right: -120px; top: -90px; bottom: -90px; background: radial-gradient(ellipse at center, rgba(47, 214, 115, 0.20) 0%, rgba(47, 214, 115, 0.06) 40%, rgba(47, 214, 115, 0) 70%); animation: sig-glow 1200ms ease-out 500ms both, sig-breathe 2600ms ease-in-out 1700ms infinite;" }
                    // Its glow: the wave wide and faint, swelling in.
                    div { style: "position: absolute; left: 0; top: 0; animation: sig-glow 1000ms ease-out 450ms both;",
                        div { style: "position: absolute; left: 0; top: 0;", Wave { stroke: GREEN.to_string(), width: 190.0, opacity: 0.05 } }
                        div { style: "position: absolute; left: 0; top: 0;", Wave { stroke: GREEN.to_string(), width: 140.0, opacity: 0.08 } }
                        div { style: "position: absolute; left: 0; top: 0;", Wave { stroke: GREEN.to_string(), width: 104.0, opacity: 0.16 } }
                    }
                    // The wave, drawing itself on from the left.
                    div { style: "position: absolute; left: 0; top: 0; height: {tall}px; overflow: hidden; animation: sig-draw 1100ms cubic-bezier(0.65, 0, 0.35, 1) 150ms both;",
                        div { style: "position: absolute; left: 0; top: 0;", Wave { stroke: "url(#sig-body)".to_string(), width: 76.0, opacity: 1.0 } }
                        div { style: "position: absolute; left: 0; top: -6px;", Wave { stroke: "#ffffff".to_string(), width: 26.0, opacity: 0.22 } }
                    }
                    // A glint running along it, while the rig is still coming
                    // up: windows onto a bright copy, moving, the copy held
                    // still under them — seven narrow ones, faint at the
                    // edges, so it has no hard edge.
                    div { style: "position: absolute; left: 0; top: 0; width: {GLINT * 7.0}px; height: {tall}px; animation: sig-scan {SCAN_MS}ms cubic-bezier(0.45, 0, 0.55, 1) {GLINT_FROM_MS}ms infinite both;",
                        for (k, a) in [0.06, 0.16, 0.32, 0.5, 0.32, 0.16, 0.06].into_iter().enumerate() {
                            div { key: "{k}", style: "position: absolute; left: {k as f64 * GLINT}px; top: 0; width: {GLINT}px; height: {tall}px; overflow: hidden;",
                                div { style: "position: absolute; left: {-(k as f64) * GLINT}px; top: 0; animation: sig-unscan {SCAN_MS}ms cubic-bezier(0.45, 0, 0.55, 1) {GLINT_FROM_MS}ms infinite both;",
                                    Wave { stroke: MINT.to_string(), width: 76.0, opacity: a }
                                }
                            }
                        }
                    }
                }
                // The name.
                div { style: "margin-top: 40px; padding-left: 0.38em; font-size: 30px; font-weight: 800; color: #f4f4f5; letter-spacing: 0.38em; animation: sig-word 1200ms cubic-bezier(0.16, 1, 0.3, 1) 650ms both;", "SIGNAL" }
            }
        }
    }
}
