//! **Touch** — what the framework needs to know to be usable with a finger:
//! whether the rig is being played by touch, a long-press to stand in for a
//! right-click, and a visible ⋯ where a menu would otherwise hide behind one.
//!
//! The rule every surface follows: **no action may live only behind a
//! right-click, a hover, a double-click, a wheel or a key.** A mouse keeps
//! all of those; a finger gets the same action another way:
//!
//! | mouse | finger |
//! |---|---|
//! | right-click a row / tile | its ⋯, shown always on touch ([`TouchMenuButton`]), or a long-press |
//! | hover to reveal a button | the button is always shown on touch ([`use_touch`]) |
//! | double-click to reset | hold still on the control ([`LongPress`]) |
//! | ↑ ↓ to audition | tap auditions, with Keep / Undo (the sound browser) |
//!
//! Touch is **learned**: the first pointer event says whether it came from
//! a finger, a pen or a mouse (a laptop with a touchscreen switches as it
//! goes). An iOS build starts in touch; `SIGNAL_TOUCH=1` forces it anywhere,
//! to see the touch layout on a desktop.

use std::time::Duration;

use dioxus::dioxus_core::Task;
use dioxus::prelude::*;

/// A still press held this long is a long-press.
pub const LONG_PRESS_MS: u64 = 500;

/// How far a finger may drift before a press becomes a drag (px).
pub const SLOP_PX: f64 = 8.0;

/// The smallest thing a finger should be asked to hit (px) — Apple's 44pt.
pub const HIT_PX: u32 = 44;

/// Whether touch is forced on (`SIGNAL_TOUCH=1`) or the platform is a touch
/// device.
fn forced() -> bool {
    static FORCED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FORCED.get_or_init(|| {
        cfg!(target_os = "ios")
            || std::env::var("SIGNAL_TOUCH").is_ok_and(|v| !v.is_empty() && v != "0")
    })
}

/// The device the rig is being used with, shared from the root.
#[derive(Clone, Copy, PartialEq)]
pub struct Touch(Signal<bool>);

impl Touch {
    /// Provide it for everything below; the root element passes its pointer
    /// events to [`observe`](Self::observe).
    pub fn provide() -> Self {
        use_context_provider(|| Self(Signal::new(forced())))
    }

    /// The provided one, if any.
    #[must_use]
    pub fn try_use() -> Option<Self> {
        try_use_context::<Self>()
    }

    /// Learn from a pointer event which kind of pointer is in use.
    pub fn observe(self, e: &PointerEvent) {
        if forced() {
            return;
        }
        let touch = matches!(e.pointer_type().as_str(), "touch" | "pen");
        let mut s = self.0;
        if *s.peek() != touch {
            s.set(touch);
        }
    }

    /// Whether the last pointer was a finger or a pen.
    #[must_use]
    pub fn on(self) -> bool {
        (self.0)()
    }
}

/// Whether the rig is being used by touch (read in render — it subscribes).
/// Without a provider, only a forced touch counts. Not a hook: it may be
/// called anywhere in a render, conditionally too.
#[must_use]
pub fn is_touch() -> bool {
    match try_consume_context::<Touch>() {
        Some(t) => t.on(),
        None => forced(),
    }
}

/// [`is_touch`], by its hook-shaped name.
#[must_use]
pub fn use_touch() -> bool {
    is_touch()
}

/// A hover-reveal class (`opacity-0 group-hover:opacity-100`): by touch
/// there is no hover, so the thing is simply shown.
#[must_use]
pub fn reveal(class: &'static str) -> &'static str {
    if is_touch() { "" } else { class }
}

/// A target's side on this device: `mouse` px with a mouse, at least
/// [`HIT_PX`] by touch.
#[must_use]
pub fn hit(touch: bool, mouse: u32) -> u32 {
    if touch { mouse.max(HIT_PX) } else { mouse }
}

/// A long-press: a finger (or pen) held still for [`LONG_PRESS_MS`]. The
/// mouse's right-click and double-click stand-in. `Copy` — signals inside.
///
/// Wire it as `onpointerdown: move |e| lp.down(&e, action)`,
/// `onpointermove: move |e| lp.moved(&e)`, `onpointerup` /
/// `onpointercancel` / `onpointerleave: move |_| lp.cancel()`, and skip the
/// element's click when [`fired`](Self::fired) says the press was spent.
#[derive(Clone, Copy, PartialEq)]
pub struct LongPress {
    task: Signal<Option<Task>>,
    at: Signal<(f64, f64)>,
    fired: Signal<bool>,
}

/// A [`LongPress`] for this component.
pub fn use_long_press() -> LongPress {
    LongPress {
        task: use_signal(|| None),
        at: use_signal(|| (0.0, 0.0)),
        fired: use_signal(|| false),
    }
}

impl LongPress {
    /// A press began. Only a finger or a pen arms it (a mouse has its right
    /// button); `action` gets the client point it was held at.
    pub fn down(mut self, e: &PointerEvent, action: impl FnOnce((f64, f64)) + 'static) {
        self.cancel();
        self.fired.set(false);
        if !matches!(e.pointer_type().as_str(), "touch" | "pen") {
            return;
        }
        let p = e.client_coordinates();
        self.at.set((p.x, p.y));
        let mut fired = self.fired;
        let mut task = self.task;
        let at = (p.x, p.y);
        let t = spawn(async move {
            architect::platform::sleep(Duration::from_millis(LONG_PRESS_MS)).await;
            task.set(None);
            fired.set(true);
            action(at);
        });
        self.task.set(Some(t));
    }

    /// The pointer moved: past the slop it is a drag, not a long-press.
    pub fn moved(self, e: &PointerEvent) {
        if self.task.peek().is_none() {
            return;
        }
        let p = e.client_coordinates();
        let (x, y) = *self.at.peek();
        if (p.x - x).hypot(p.y - y) > SLOP_PX {
            self.cancel();
        }
    }

    /// The press ended (or left, or was cancelled) before it was long.
    pub fn cancel(mut self) {
        if let Some(t) = self.task.write().take() {
            t.cancel();
        }
    }

    /// Whether this press already did its long-press action — the click
    /// that follows the release should then do nothing. Reading it resets
    /// it.
    pub fn fired(mut self) -> bool {
        let f = *self.fired.peek();
        if f {
            self.fired.set(false);
        }
        f
    }
}

/// The ⋯ a touch user opens a tile's or row's menu with — where a mouse
/// user right-clicks. Drawn only by touch, pinned to its (positioned)
/// parent's bottom-right corner (the top corners hold a switch's number and
/// its MIDI badge), and it keeps its press to itself: the tile
/// under it does not see a tap.
#[component]
pub fn TouchMenuButton(
    /// Gets the click, as the tile's right-click handler would.
    onclick: EventHandler<MouseEvent>,
    #[props(default = "Menu".to_string())] title: String,
) -> Element {
    if !use_touch() {
        return rsx! {};
    }
    rsx! {
        button {
            style: "position: absolute; bottom: 2px; right: 2px; z-index: 3; width: {HIT_PX}px; \
                    height: {HIT_PX}px; display: flex; align-items: center; justify-content: center; \
                    appearance: none; border: none; background: transparent; padding: 0; cursor: pointer; \
                    color: inherit;",
            title: "{title}",
            onpointerdown: move |e: PointerEvent| e.stop_propagation(),
            onpointerup: move |e: PointerEvent| e.stop_propagation(),
            onclick: move |e: MouseEvent| {
                e.stop_propagation();
                onclick.call(e);
            },
            span {
                style: "display: flex; align-items: center; justify-content: center; width: 26px; height: 26px; \
                        border-radius: 999px; background: rgba(0,0,0,0.35); opacity: 0.85;",
                svg { width: "14", height: "14", view_box: "0 0 24 24", fill: "currentColor",
                    style: "display: block; width: 14px; height: 14px;",
                    circle { cx: "5", cy: "12", r: "2.2" }
                    circle { cx: "12", cy: "12", r: "2.2" }
                    circle { cx: "19", cy: "12", r: "2.2" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_targets_are_at_least_a_fingertip() {
        assert_eq!(hit(true, 22), HIT_PX);
        assert_eq!(hit(true, 64), 64);
        assert_eq!(hit(false, 22), 22);
    }
}
