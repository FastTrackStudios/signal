//! The tablet layout's tokens — the touch prototype's `styles.css`
//! (`prototypes/touch`), one value per name. Inline-style fragments, since
//! Blitz takes no stylesheet (CLAUDE.md); a colour is changed here or
//! nowhere.

// The whole set, ported at once: later views use what Perform does not.
#![allow(dead_code)]

// ── Grounds ────────────────────────────────────────────────────────────────

pub const DESK: &str = "#0c0c0f";
pub const SHEET: &str = "#0e0e11";
pub const SHEET_2: &str = "#141418";
/// The browser's and the main area's ground.
pub const MAIN: &str = "#0f0f12";
/// The well a meter or fader sits in.
pub const WELL: &str = "#08080a";

// ── Lines ──────────────────────────────────────────────────────────────────

pub const RULE: &str = "#222228";
pub const RULE_STRONG: &str = "#2b2b31";

// ── Ink ────────────────────────────────────────────────────────────────────

pub const INK: &str = "#e4e4e7";
pub const INK_2: &str = "#a1a1aa";
/// 4.6:1 on a lifted row, 6:1 on the desk.
pub const INK_3: &str = "#8e8e98";
/// A disabled glyph or an idle dot — never text.
pub const DIM: &str = "#3f3f46";

// ── States ─────────────────────────────────────────────────────────────────

pub const LIVE: &str = "#22c55e";
pub const LIVE_BG: &str = "rgba(34,197,94,0.12)";
/// Text on the playing green.
pub const ON_LIVE: &str = "#04210f";
/// Unsaved edits.
pub const MODIFIED: &str = "#f59e0b";
pub const VOID: &str = "#f87171";
/// A picked row, lifted (not blue).
pub const UP: &str = "#26262b";
pub const FOCUS_FG: &str = "#d4d4d8";
/// A control's plain fill, and its fill when set.
pub const FILL: &str = "rgba(255,255,255,0.06)";
pub const FILL_ON: &str = "rgba(255,255,255,0.1)";

// ── Shape ──────────────────────────────────────────────────────────────────

pub const R: &str = "6px";
pub const R_MD: &str = "8px";
/// Apple's minimum touch target, pt.
pub const HIT: u32 = 44;

/// The top bar's height.
pub const TOP_H: u32 = 48;
/// The foot bar's height.
pub const FOOT_H: u32 = 56;
/// The setlist header's height — the macro bar's, so the lines run across.
pub const HEADER_H: u32 = super::macros::MACRO_BAR_H;
/// The sidebar's width — the prototype's (an iPhone's, 402pt).
pub const SIDEBAR_W: u32 = 402;

/// The font every surface uses.
pub const FONT: &str = "-apple-system, 'SF Pro Text', system-ui, 'Segoe UI', Roboto, 'Helvetica Neue', sans-serif";

pub const CLEAR: &str = "transparent";
pub const NO_SHADOW: &str = "none";
/// A row that is on, lifted.
pub const ROW_ON: &str = "rgba(255,255,255,0.05)";
/// A played song's name, struck.
pub const STRIKE: &str = "text-decoration: line-through;";
pub const NOTHING: &str = "";
pub const DANGER_INK: &str = "#1a0505";
/// A dock toggle's margin, and a view tab's.
pub const PIN_MARGIN: &str = "6px 2px";
pub const TAB_MARGIN: &str = "0 2px";

/// `a` when `cond`, else `b` — for a value inside an rsx style string,
/// which takes no `if` block.
#[must_use]
pub fn pick<T>(cond: bool, a: T, b: T) -> T {
    if cond { a } else { b }
}

/// A percentage of 0..1, whole.
#[must_use]
pub fn pct(level: f64) -> u32 {
    (level.clamp(0.0, 1.0) * 100.0) as u32
}

/// An effect's colour as text: lifted toward white, so the darker hues
/// still read at 4.5:1 on the dark grounds.
#[must_use]
pub fn lift(colour: &str) -> String {
    format!("color-mix(in oklab, {colour} 78%, white)")
}

/// A muted tint of a colour over the sheet: how anything that belongs to a
/// song is picked out.
#[must_use]
pub fn tint(colour: &str, pct: u32) -> String {
    format!("color-mix(in oklab, {colour} {pct}%, {SHEET})")
}

/// A choice's border: ringed `w` pt in the focus ink when on, a hairline
/// when not.
#[must_use]
pub fn ring(on: bool, w: f32) -> String {
    if on { format!("{w}px solid {FOCUS_FG}") } else { format!("1px solid {RULE_STRONG}") }
}

/// A row's state dot: filled green when on, an open ring of `edge` when not.
#[must_use]
pub fn state_dot(on: bool, edge: &str) -> String {
    if on { format!("background: {LIVE};") } else { format!("border: 1.5px solid {edge};") }
}

/// The line a dragged row will land above.
#[must_use]
pub fn drop_line(on: bool) -> String {
    if on { format!("border-top: 2px solid {INK_2};") } else { String::new() }
}

/// A focus outline, when on.
#[must_use]
pub fn focus_outline(on: bool) -> String {
    if on { format!("outline: 2px solid {FOCUS_FG};") } else { String::new() }
}

/// A hairline under a row, unless it stands apart.
#[must_use]
pub fn rule_below(on: bool) -> String {
    if on { format!("border-bottom: 1px solid {RULE};") } else { String::new() }
}

/// A tape colour as ink: the grey tape reads as the third ink.
#[must_use]
pub fn tape_ink(tape: &str) -> &str {
    if tape == "#3f3f46" { INK_3 } else { tape }
}
