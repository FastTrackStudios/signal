//! The browser's palette and the three button shapes it uses.
//!
//! Inline strings rather than a stylesheet, and not by preference: Blitz does
//! not load external CSS reliably, and this component tree has to render
//! identically standalone, as a plugin editor, and in the browser remote. A
//! class that only resolves in one of those is a bug that only shows up in
//! the other two.

/// Panel and card ground.
pub const PANEL: &str = "#141418";
/// A row inside a panel — one step up so it reads as a surface.
pub const ROW: &str = "#1b1b21";
/// Hairlines.
pub const BORDER: &str = "#2a2a32";
/// Body text.
pub const TEXT: &str = "#e8e8ec";
/// Secondary text: creators, counts, everything that is context.
pub const MUTED: &str = "#8b8b97";
/// The one accent — used for the action, never for decoration.
pub const ACCENT: &str = "#f97316";
/// Failures only.
pub const DANGER: &str = "#f87171";

/// The action button: download, sign in, use in rig.
#[must_use]
pub fn primary_button() -> String {
    format!(
        "background:{ACCENT};color:#12120f;border:none;border-radius:5px;\
         padding:6px 12px;font-size:12px;font-weight:600;cursor:pointer;\
         white-space:nowrap;"
    )
}

/// The secondary button: close, sign out, open a page.
#[must_use]
pub fn ghost_button() -> String {
    format!(
        "background:transparent;color:{MUTED};border:1px solid {BORDER};\
         border-radius:5px;padding:5px 10px;font-size:12px;cursor:pointer;\
         white-space:nowrap;"
    )
}

/// A metadata pill — gear, make, tag, licence.
#[must_use]
pub fn chip(color: &str) -> String {
    format!(
        "display:inline-block;padding:2px 8px;border-radius:999px;\
         border:1px solid {BORDER};background:{ROW};color:{color};\
         font-size:11px;line-height:1.6;white-space:nowrap;"
    )
}

/// A filter tab, in its selected or unselected state.
#[must_use]
pub fn tab(selected: bool) -> String {
    let (bg, color, border) = if selected {
        (ROW, TEXT, ACCENT)
    } else {
        ("transparent", MUTED, BORDER)
    };
    format!(
        "background:{bg};color:{color};border:1px solid {border};\
         border-radius:5px;padding:5px 10px;font-size:12px;cursor:pointer;\
         white-space:nowrap;"
    )
}

/// The search field.
#[must_use]
pub fn input() -> String {
    format!(
        "flex:1;min-width:120px;background:{ROW};color:{TEXT};\
         border:1px solid {BORDER};border-radius:5px;padding:6px 10px;\
         font-size:13px;outline:none;"
    )
}
