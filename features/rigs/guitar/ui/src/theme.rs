//! The rig's look, as values: colours, type sizes, radii and the few style
//! fragments every surface repeats.
//!
//! Inline, because the rig must lay out without Tailwind (CLAUDE.md), and
//! the rig's own greys rather than the app theme's so every surface — the
//! sidebars, the library, the menus — reads as one instrument. Each surface
//! used to carry its own copy of these (two different `LINE`s among them);
//! a colour is changed here or nowhere.

// ── Grounds ────────────────────────────────────────────────────────────────

/// The library and other modal grounds.
pub const BG: &str = "#0c0c0f";
/// A sidebar's ground.
pub const SIDEBAR: &str = "#0e0e11";
/// A raised pane inside a surface (the library's rail and detail).
pub const PANE: &str = "#101014";
/// A text field's ground.
pub const FIELD: &str = "#0a0a0d";
/// Menus and popovers — the perform grid's switch menu set this.
pub const MENU: &str = "#0d0d10";

// ── Lines ──────────────────────────────────────────────────────────────────

/// Hairlines between regions and around quiet boxes.
pub const LINE: &str = "#222228";
/// Borders of controls you press or type in.
pub const LINE_STRONG: &str = "#2b2b31";

// ── Ink ────────────────────────────────────────────────────────────────────

pub const TEXT: &str = "#e4e4e7";
pub const MUTED: &str = "#a1a1aa";
pub const FAINT: &str = "#63636b";
/// An idle dot, a disabled glyph.
pub const DIM: &str = "#3f3f46";

// ── States ─────────────────────────────────────────────────────────────────

/// The selected row (focus, the song that is up).
pub const FOCUS_BG: &str = "#1b2331";
pub const FOCUS_FG: &str = "#bfdbfe";
/// Playing.
pub const LIVE: &str = "#22c55e";
pub const LIVE_BG: &str = "rgba(34,197,94,0.12)";
/// The live sound differs from what is saved.
pub const MODIFIED: &str = "#f59e0b";
/// The one thing you came to do.
pub const PRIMARY: &str = "#2563eb";
pub const DANGER: &str = "#f87171";
pub const DANGER_LINE: &str = "#4a1f22";
pub const DANGER_INK: &str = "#1a0505";

// ── Type ───────────────────────────────────────────────────────────────────

/// Counts, key · tempo, footnotes.
pub const T_META: &str = "10px";
/// Buttons, menu rows, sub-lines.
pub const T_SMALL: &str = "11px";
/// Body text and list rows.
pub const T_BODY: &str = "12px";

// ── Shape ──────────────────────────────────────────────────────────────────

pub const R_SM: &str = "6px";
/// Popovers and bars — the largest radius the rig uses.
pub const R_MD: &str = "8px";
/// A left sidebar's width.
pub const SIDEBAR_W: &str = "272px";
/// The right (module) sidebar's width.
pub const INSPECTOR_W: &str = "300px";

// ── Fragments ──────────────────────────────────────────────────────────────

/// The uppercase label over a group.
pub const EYEBROW: &str = "font-size: 9px; font-weight: 700; letter-spacing: 0.14em; \
                           text-transform: uppercase; color: #63636b; white-space: nowrap;";
