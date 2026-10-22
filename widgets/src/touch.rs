//! **Touch** — what the framework needs to know to be usable with a finger:
//! whether the rig is being played by touch, and a visible ⋯ where a menu
//! would otherwise hide behind a right-click.
//!
//! The rule every surface follows: **no action may live only behind a
//! right-click, a hover, a double-click, a wheel or a key.** A mouse keeps
//! all of those; a finger gets the same action another way:
//!
//! | mouse | finger |
//! |---|---|
//! | right-click a row / tile | its ⋯, shown always on touch ([`TouchMenuButton`]) |
//! | hover to reveal a button | the button is always shown on touch ([`use_touch`]) |
//! | double-click to reset | double-tap |
//! | ↑ ↓ to audition | tap auditions, with Keep / Undo (the sound browser) |
//!
//! **No press-and-hold.** A finger resting on a list before it scrolls looks
//! exactly like a hold, so nothing in the touch UI acts on one: menus open
//! from their ⋯ ([`right_click`] keeps a right-click to the mouse), rows
//! reorder by a grip, and a scroll or a drag never fires an action. (A
//! footswitch tile's own tap/hold split is the pedal's, not a menu's.)
//!
//! Touch is **learned**: the first pointer event says whether it came from
//! a finger, a pen or a mouse (a laptop with a touchscreen switches as it
//! goes). An iOS build starts in touch; `SIGNAL_TOUCH=1` forces it anywhere,
//! to see the touch layout on a desktop.

use dioxus::prelude::*;

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

    /// Provide it pinned to `on` until a pointer says otherwise — a test's
    /// (or a preview's) way to draw the touch layout.
    pub fn provide_with(on: bool) -> Self {
        use_context_provider(|| Self(Signal::new(on)))
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

/// A right-click handler kept to the mouse. By touch a menu opens only from
/// its ⋯: an iOS web view fires `contextmenu` on a press-and-hold, and a
/// hold that turns into a scroll must never open anything. Wrap an
/// `oncontextmenu` closure in it (in render — it reads the device):
/// `oncontextmenu: right_click(move |e: MouseEvent| open_menu(e))`.
pub fn right_click(mut f: impl FnMut(MouseEvent) + 'static) -> impl FnMut(MouseEvent) + 'static {
    let touch = is_touch();
    move |e: MouseEvent| {
        if touch {
            // No platform menu either.
            e.prevent_default();
            return;
        }
        f(e);
    }
}

/// The ⋯ a touch user opens a tile's or row's menu with — where a mouse
/// user right-clicks. Drawn only by touch, pinned to its (positioned)
/// parent's bottom-right corner (the top corners hold a switch's number and
/// its MIDI badge), and it keeps its press to itself: the tile
/// under it does not see a tap. `inline`: a flex item in its row instead
/// (32px, overhanging its row), for a control too small to pin one over.
#[component]
pub fn TouchMenuButton(
    /// Gets the click, as the tile's right-click handler would.
    onclick: EventHandler<MouseEvent>,
    #[props(default = "Menu".to_string())] title: String,
    #[props(default)] inline: bool,
) -> Element {
    if !use_touch() {
        return rsx! {};
    }
    let place = if inline {
        // Its hit area overhangs the row (negative margins), so a small
        // label row keeps its height.
        "position: relative; flex-shrink: 0; width: 32px; height: 32px; margin: -10px -6px -10px 0;".to_string()
    } else {
        format!("position: absolute; bottom: 2px; right: 2px; z-index: 3; width: {HIT_PX}px; height: {HIT_PX}px;")
    };
    rsx! {
        button {
            "aria-label": "{title}",
            style: "{place} display: flex; align-items: center; justify-content: center; \
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
