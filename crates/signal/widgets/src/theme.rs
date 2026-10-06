//! The rigs' look, as values: colours, type sizes, radii and the few style
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
/// Secondary ink — counts, subs, hints. 4.6:1 on `SIDEBAR` (the old
/// `#63636b` was 3.2:1, below the floor for text).
pub const FAINT: &str = "#80808a";
/// An idle dot, a disabled glyph — never text (1.8:1).
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

/// Counts, key · tempo, footnotes — the floor for anything read (only the
/// uppercase `EYEBROW` goes smaller).
pub const T_META: &str = "11px";
/// Buttons, menu rows, sub-lines.
pub const T_SMALL: &str = "11px";
/// Body text and list rows.
pub const T_BODY: &str = "12px";

// ── Shape ──────────────────────────────────────────────────────────────────

pub const R_SM: &str = "6px";
/// Popovers and bars — the largest radius the rig uses.
pub const R_MD: &str = "8px";
/// A phone's width: the iPhone 16 Pro's, 402pt portrait. Both sidebars are
/// exactly this wide, so each is designed once — a sidebar on the desktop,
/// the whole screen on a phone held upright (landscape phone layouts are
/// their own).
pub const PHONE_W: &str = "402px";
/// A left sidebar's width — a phone's (see [`PHONE_W`]).
pub const SIDEBAR_W: &str = PHONE_W;
/// The right (module) sidebar's width — a phone's (see [`PHONE_W`]).
pub const INSPECTOR_W: &str = PHONE_W;

// ── Fragments ──────────────────────────────────────────────────────────────

/// The uppercase label over a group.
pub const EYEBROW: &str = "font-size: 10px; font-weight: 700; letter-spacing: 0.12em; \
                           text-transform: uppercase; color: #80808a; white-space: nowrap;";

/// The pick of a few tabs or views: pressed into its surface, never a box
/// on it — the bar, the sidebars' tabs, a face's style selector.
pub const PRESSED: &str = "background: rgba(0,0,0,0.5); color: #fafafa; \
                           box-shadow: inset 0 1px 2px rgba(0,0,0,0.75), inset 0 -1px 0 rgba(255,255,255,0.05);";
/// [`PRESSED`] for a pick that is rig state, not a view — the play mode,
/// which every remote and the footswitches follow: a green floor under it.
pub const PRESSED_LIVE: &str = "background: rgba(0,0,0,0.5); color: #fafafa; \
                                box-shadow: inset 0 -2px 0 #22c55e, inset 0 1px 2px rgba(0,0,0,0.75);";

/// The states a style attribute cannot carry (hover), as classes every
/// surface shares. Mounted once at the rig's root (`document::Style`).
///
/// - `sg-hover`: a row, a menu item, a bare button — lifts on hover.
/// - `sg-row` / `sg-reveal`: a row's ⋯ waits, faint, until the row is
///   hovered (touch shows it always: see `reveal`).
/// - `sg-ink`: bare text buttons brighten on hover.
pub const CSS: &str = ".sg-hover:hover{background:rgba(255,255,255,0.05);}\
.sg-row .sg-reveal{opacity:0.35;}\
.sg-row:hover .sg-reveal{opacity:1;}\
.sg-ink{color:#a1a1aa;}\
.sg-ink:hover{color:#e4e4e7;}";
