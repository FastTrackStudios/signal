//! Piano keyboard component — Neothesia-style.
//!
//! Renders a full-width chromatic keyboard with a falling-note waterfall above.
//!
//! ## Rendering tiers
//!
//! | Target | Waterfall rendering |
//! |---|---|
//! | **Blitz / VST** (`not(wasm32)`, production) | Vello [`VelloCanvas`] foreground overlay — GPU-accelerated, layered glow |
//! | **WebKit / WRY** (`not(wasm32)`, dev)        | VelloCanvas no-ops (transparent); SVG note blocks with CSS `drop-shadow` show through |
//! | **WASM / browser** (`wasm32`)                | SVG note blocks with CSS `drop-shadow`; no Vello import |
//!
//! The SVG note blocks are **always** present in the DOM.  In Blitz the
//! [`VelloCanvas`] sits in front of the SVG (foreground, `z-index:1`) and
//! paints a fully-opaque GPU waterfall that visually replaces the SVG
//! blocks.  In WebKit/WASM the canvas is absent or transparent, so the SVG
//! blocks are what the user sees.
//!
//! No runtime `cfg!()` checks are needed — all branching is compile-time.
//!
//! # Usage
//!
//! ```rust,ignore
//! Piano {
//!     start_note: 21,          // A0 (full 88-key range)
//!     end_note:   108,         // C8
//!     active_notes: active,
//!     waterfall_notes: notes,
//!     on_note_on:  move |n| play(n),
//!     on_note_off: move |n| stop(n),
//! }
//! ```
//!
//! MIDI middle C = note 60.

// Vello / audiocore-gui imports — only compiled for native targets.
// On wasm32 neither `nice_plug_dioxus` nor `audiocore-gui` is available
// (they depend on `baseview`, which is native-only).
#[cfg(not(target_arch = "wasm32"))]
use std::cell::RefCell;
#[cfg(not(target_arch = "wasm32"))]
use std::rc::Rc;

#[cfg(not(target_arch = "wasm32"))]
use audiocore_gui::viz::{CanvasPainter, VelloCanvas};
use dioxus::prelude::*;
#[cfg(not(target_arch = "wasm32"))]
use nice_plug_dioxus::prelude::vello::kurbo::{Affine, Rect, RoundedRect};
#[cfg(not(target_arch = "wasm32"))]
use nice_plug_dioxus::prelude::vello::peniko::{Color, Fill, Gradient};

// ── Geometry constants (SVG user units; scaled to container via viewBox) ──────

/// Width of one white key in SVG user units.
const WW: f64 = 24.0;
/// Height of one white key.
const WH: f64 = 120.0;
/// Width of one black key.
const BW: f64 = 14.0;
/// Height of one black key.
const BH: f64 = 76.0;
/// Height of the waterfall section above the keys (SVG user units).
const FALL_H: f64 = 180.0;
/// Corner radius for key rectangles.
const KEY_R: f64 = 3.0;
/// Corner radius for waterfall note blocks.
const NOTE_R: f64 = 4.0;

// ── Note theory helpers ───────────────────────────────────────────────────────

/// Semitone offsets of white keys within an octave.
const WHITE_SEMITONES: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
/// Semitone offsets of black keys within an octave.
const BLACK_SEMITONES: [u8; 5] = [1, 3, 6, 8, 10];
/// White-key boundary indices where black keys sit.
const BLACK_BOUNDARIES: [u8; 5] = [1, 2, 4, 5, 6];

pub fn is_black(semitone: u8) -> bool {
    matches!(semitone % 12, 1 | 3 | 6 | 8 | 10)
}

/// Count of white keys from MIDI note 0 up to (but not including) `note`.
fn white_key_count_before(note: u8) -> u32 {
    let octave = note as u32 / 12;
    let semitone = note % 12;
    let whites_in_full_octaves = octave * 7;
    let partial = WHITE_SEMITONES.iter().filter(|&&s| s < semitone).count() as u32;
    whites_in_full_octaves + partial
}

/// X position (left edge) of a black key for `note`, relative to `origin_white`.
/// Returns None if note is not a black key or falls outside the visible range.
fn black_x(note: u8, origin_white: u32) -> Option<f64> {
    if !is_black(note) {
        return None;
    }
    let semitone = note % 12;
    let boundary = BLACK_SEMITONES
        .iter()
        .zip(BLACK_BOUNDARIES.iter())
        .find(|(&s, _)| s == semitone)
        .map(|(_, &b)| b)?;
    let octave = note as u32 / 12;
    let abs_boundary = octave * 7 + boundary as u32;
    if abs_boundary < origin_white {
        return None;
    }
    let x = (abs_boundary as f64 * WW) - (BW / 2.0) - (origin_white as f64 * WW);
    Some(x)
}

// ── Waterfall note ────────────────────────────────────────────────────────────

/// A note block displayed in the falling waterfall above the keyboard.
#[derive(Clone, PartialEq)]
pub struct WaterfallNote {
    /// Unique ID used as the Dioxus key — prevents collisions when the same
    /// pitch appears multiple times simultaneously.
    pub id: u32,
    pub note: u8,
    /// 1.0 = keyboard (bottom of waterfall), decreasing toward 0.0 = top.
    /// Stays at 1.0 while the note is held; decreases once released.
    pub y_frac: f64,
    /// Height as a fraction of the waterfall height.
    /// Grows while the note is held; fixed once released.
    pub height_frac: f64,
    pub color: Option<String>,
    /// `true` while the key is physically held — block grows taller each tick.
    /// Set to `false` via [`WaterfallState::release`] on note-off.
    pub held: bool,
}

// ── Color helper ──────────────────────────────────────────────────────────────

/// Parse a CSS hex color string like `"#3b82f6"` into `[r, g, b]`.
fn parse_hex_rgb(hex: &str) -> [u8; 3] {
    let h = hex.trim_start_matches('#');
    if h.len() < 6 {
        return [59, 130, 246];
    }
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(128);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(128);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(128);
    [r, g, b]
}

// ── Vello waterfall painter (native only) ────────────────────────────────────

/// Vello-accelerated waterfall painter.
///
/// Implements [`CanvasPainter`] so it can be hosted in [`VelloCanvas`] as a
/// Blitz foreground scene overlay (GPU-accelerated, full glow effects).
/// Updated each render via [`WaterfallPainter::update`].
///
/// Not compiled for `wasm32` — use the SVG fallback there.
#[cfg(not(target_arch = "wasm32"))]
pub struct WaterfallPainter {
    notes: Vec<WaterfallNote>,
    num_white: u32,
    origin_white: u32,
    start_note: u8,
    end_note: u8,
    accent_rgb: [u8; 3],
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for WaterfallPainter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl WaterfallPainter {
    pub fn new() -> Self {
        Self {
            notes: Vec::new(),
            num_white: 52,
            origin_white: 0,
            start_note: 21,
            end_note: 108,
            accent_rgb: [59, 130, 246],
        }
    }

    /// Copy the latest note/layout state into the painter. Called each render.
    pub fn update(
        &mut self,
        notes: &[WaterfallNote],
        start_note: u8,
        end_note: u8,
        origin_white: u32,
        num_white: u32,
        accent_rgb: [u8; 3],
    ) {
        self.notes.clear();
        self.notes.extend_from_slice(notes);
        self.start_note = start_note;
        self.end_note = end_note;
        self.origin_white = origin_white;
        self.num_white = num_white;
        self.accent_rgb = accent_rgb;
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl CanvasPainter for WaterfallPainter {
    fn paint(
        &self,
        scene: &mut nice_plug_dioxus::prelude::vello::Scene,
        transform: Affine,
        width: f64,
        height: f64,
    ) {
        if width < 1.0 || height < 1.0 || self.num_white == 0 {
            return;
        }

        // ── Background ────────────────────────────────────────────────
        scene.fill(
            Fill::NonZero,
            transform,
            Color::from_rgb8(13, 13, 15),
            None,
            &Rect::new(0.0, 0.0, width, height),
        );

        // Subtle vignette: slightly darker at top edge
        let vignette = Gradient::new_linear((0.0, 0.0), (0.0, height * 0.35)).with_stops([
            (0.0f32, Color::from_rgba8(0, 0, 0, 60)),
            (1.0f32, Color::from_rgba8(0, 0, 0, 0)),
        ]);
        scene.fill(
            Fill::NonZero,
            transform,
            &vignette,
            None,
            &Rect::new(0.0, 0.0, width, height * 0.35),
        );

        // ── Coordinate scale (SVG user units → CSS pixels) ────────────
        let key_w_px = width / self.num_white as f64;
        let scale = key_w_px / WW;

        // ── Clip layer — prevents glow bleed beyond canvas edges ───────
        // vello 0.9: push_clip_layer gained a leading clip-style arg.
        scene.push_clip_layer(
            Fill::NonZero,
            transform,
            &Rect::new(0.0, 0.0, width, height),
        );

        // ── Note blocks ───────────────────────────────────────────────
        let [ar, ag, ab] = self.accent_rgb;

        for wn in &self.notes {
            let note = wn.note;
            if note < self.start_note || note > self.end_note {
                continue;
            }

            let (note_x, note_w) = if is_black(note) {
                let Some(x_svg) = black_x(note, self.origin_white) else {
                    continue;
                };
                (x_svg * scale, BW * scale)
            } else {
                let wi = white_key_count_before(note).saturating_sub(self.origin_white);
                (wi as f64 * key_w_px + scale, (WW - 2.0) * scale)
            };

            let block_h = (wn.height_frac * height).max(6.0);
            let block_y = wn.y_frac * (height - block_h);

            let [r, g, b] = if let Some(ref col) = wn.color {
                parse_hex_rgb(col)
            } else {
                [ar, ag, ab]
            };

            let radius = (NOTE_R * scale).max(2.0);

            // Layer 1: outer aura
            let g1 = 7.0 * scale;
            scene.fill(
                Fill::NonZero,
                transform,
                Color::from_rgba8(r, g, b, 12),
                None,
                &RoundedRect::new(
                    note_x - g1,
                    block_y - g1,
                    note_x + note_w + g1,
                    block_y + block_h + g1,
                    radius + g1,
                ),
            );

            // Layer 2: inner glow halo
            let g2 = 2.5 * scale;
            scene.fill(
                Fill::NonZero,
                transform,
                Color::from_rgba8(r, g, b, 38),
                None,
                &RoundedRect::new(
                    note_x - g2,
                    block_y - g2,
                    note_x + note_w + g2,
                    block_y + block_h + g2,
                    radius + g2,
                ),
            );

            // Layer 3: solid body with vertical gradient
            let body_top = Color::from_rgba8(r, g, b, 230);
            let body_bot = Color::from_rgba8(
                (r as f64 * 0.7) as u8,
                (g as f64 * 0.7) as u8,
                (b as f64 * 0.7) as u8,
                210,
            );
            let body_grad = Gradient::new_linear((0.0, block_y), (0.0, block_y + block_h))
                .with_stops([(0.0f32, body_top), (1.0f32, body_bot)]);
            scene.fill(
                Fill::NonZero,
                transform,
                &body_grad,
                None,
                &RoundedRect::new(note_x, block_y, note_x + note_w, block_y + block_h, radius),
            );

            // Layer 4: top shine stripe
            let inset = (2.0 * scale).max(1.5);
            let shine_h = (3.0 * scale).max(2.0).min(block_h * 0.35);
            if note_w - 2.0 * inset > 1.0 && shine_h > 0.5 {
                scene.fill(
                    Fill::NonZero,
                    transform,
                    Color::from_rgba8(255, 255, 255, 95),
                    None,
                    &RoundedRect::new(
                        note_x + inset,
                        block_y + scale,
                        note_x + note_w - inset,
                        block_y + scale + shine_h,
                        2.0,
                    ),
                );
            }
        }

        // ── Bottom fade — blends waterfall into keyboard divider ───────
        let fade_h = (18.0 * scale).max(12.0);
        let fade = Gradient::new_linear((0.0, height - fade_h), (0.0, height)).with_stops([
            (0.0f32, Color::from_rgba8(0, 0, 0, 0)),
            (1.0f32, Color::from_rgba8(13, 13, 15, 255)),
        ]);
        scene.fill(
            Fill::NonZero,
            transform,
            &fade,
            None,
            &Rect::new(0.0, height - fade_h, width, height),
        );

        scene.pop_layer();
    }
}

// ── Piano component ───────────────────────────────────────────────────────────

/// Fraction of the component height occupied by the waterfall section.
const WATERFALL_FRAC: f64 = FALL_H / (FALL_H + WH); // ≈ 0.60

/// Full-width piano keyboard with falling-note waterfall.
///
/// The SVG fills the container 100% wide and `height` pixels tall.
/// Keys are drawn with a fixed-ratio `viewBox` stretched via
/// `preserveAspectRatio="none"` so they always fill the available space.
///
/// ## Waterfall rendering
///
/// SVG note blocks with CSS `drop-shadow` are **always rendered** as the
/// base layer — they work in every target (WebKit, WASM, Blitz).
///
/// On native targets (Blitz / VST plugin), a [`VelloCanvas`] foreground
/// overlay (`z-index:1`) sits in front of the SVG and repaints the
/// waterfall zone with GPU-accelerated Vello glow.  The SVG blocks below
/// are visually covered.  In WebKit dev mode the canvas is a no-op
/// transparent div, so the SVG blocks show through.
#[component]
pub fn Piano(
    #[props(default = 21)] start_note: u8,
    #[props(default = 108)] end_note: u8,
    #[props(default)] active_notes: Vec<u8>,
    #[props(default)] waterfall_notes: Vec<WaterfallNote>,
    on_note_on: EventHandler<u8>,
    on_note_off: EventHandler<u8>,
    #[props(default = true)] show_labels: bool,
    /// Optional per-key text label (MIDI note → text), e.g. the sample/piece a
    /// key plays. Shown on the key face; takes precedence over the C-octave
    /// label. Keys without an entry fall back to the `show_labels` behaviour.
    #[props(default)]
    labels: std::collections::HashMap<u8, String>,
    /// Keys to tint as "mapped"/of-interest (e.g. the notes a drum kit plays)
    /// — a subtle base colour so the set stands out across the whole board,
    /// without cramming text on narrow keys. Independent of `labels`.
    #[props(default)]
    highlight_keys: Vec<u8>,
    /// Show the falling-note "waterfall" zone above the keys. When `false`
    /// (e.g. a compact drum keyboard) the SVG is keys-only and preserves key
    /// aspect ratio instead of stretching to fill the container width.
    #[props(default = true)]
    waterfall: bool,
    #[props(default = "#3b82f6".to_string())] accent_color: String,
    /// CSS height of the piano container (e.g. "280px", "35vh").
    #[props(default = "280px".to_string())]
    height: String,
    #[props(default = "".to_string())] style: String,
) -> Element {
    let origin_white = white_key_count_before(start_note);
    let end_white = white_key_count_before(end_note.saturating_add(1));
    let num_white = (end_white.saturating_sub(origin_white)).max(1);
    let svg_w = num_white as f64 * WW;
    // Keys-only when no waterfall: drop the falling-note zone so the keyboard
    // fills the height, and lay keys at the top (y=0) instead of below FALL_H.
    let fall_h = if waterfall { FALL_H } else { 0.0 };
    let svg_h = fall_h + WH;
    let key_y = fall_h;

    let notes: Vec<u8> = (start_note..=end_note).collect();
    let white_notes: Vec<u8> = notes.iter().copied().filter(|&n| !is_black(n)).collect();
    let black_notes: Vec<u8> = notes.iter().copied().filter(|&n| is_black(n)).collect();

    let accent = accent_color.clone();
    let active = active_notes.clone();

    // ── Vello waterfall painter (native only) ─────────────────────────
    #[cfg(not(target_arch = "wasm32"))]
    let painter = use_hook(|| Rc::new(RefCell::new(WaterfallPainter::new())));

    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut p = painter.borrow_mut();
        p.update(
            &waterfall_notes,
            start_note,
            end_note,
            origin_white,
            num_white,
            parse_hex_rgb(&accent_color),
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    let dyn_painter: Rc<RefCell<dyn CanvasPainter>> = painter.clone();

    #[cfg(not(target_arch = "wasm32"))]
    let waterfall_pct = WATERFALL_FRAC * 100.0;

    // ── Vello overlay element (native only) ────────────────────────────
    // Built outside rsx! to avoid #[cfg] inside the macro (unsupported).
    // On wasm32 this is an empty fragment; on native it's the VelloCanvas.
    #[cfg(not(target_arch = "wasm32"))]
    let vello_overlay = rsx! {
        VelloCanvas {
            painter: dyn_painter,
            fill: true,
            background: false,
            style: Some(format!(
                "position:absolute; top:0; left:0; width:100%; \
                 height:{waterfall_pct:.3}%; pointer-events:none; z-index:1;"
            )),
        }
    };
    #[cfg(target_arch = "wasm32")]
    let vello_overlay = rsx! {};

    // ── SVG waterfall note blocks (universal fallback) ─────────────────
    // These render in ALL targets.  In Blitz the VelloCanvas foreground
    // overlay (below) covers this zone with GPU rendering, making these
    // invisible.  In WebKit / WASM they are what the user sees.
    let wf_notes = waterfall_notes.clone();
    let svg_note_blocks = wf_notes.iter().filter_map(|wn| {
        if wn.note < start_note || wn.note > end_note {
            return None;
        }
        let note = wn.note;

        let (note_x, note_w) = if is_black(note) {
            let x = black_x(note, origin_white)?;
            (x, BW)
        } else {
            let wi = white_key_count_before(note).saturating_sub(origin_white);
            (wi as f64 * WW + 1.0, WW - 2.0)
        };

        let block_h_svg = (wn.height_frac * FALL_H).max(2.0);
        let block_y_svg = wn.y_frac * (FALL_H - block_h_svg);

        // Cull blocks that have scrolled fully off the top
        if block_y_svg + block_h_svg < 0.0 {
            return None;
        }

        let color = wn.color.as_deref().unwrap_or("#3b82f6");
        let [r, g, b] = parse_hex_rgb(color);

        Some(rsx! {
            rect { key: "wf-{wn.id}",
                x: "{note_x}",
                y: "{block_y_svg}",
                width: "{note_w}",
                height: "{block_h_svg}",
                rx: "{NOTE_R}",
                fill: "{color}",
                style: "filter: drop-shadow(0 0 3px rgba({r},{g},{b},0.9));",
                pointer_events: "none",
            }
        })
    });

    rsx! {
        div {
            style: format!(
                "position:relative; width:100%; height:{height}; overflow:hidden; \
                 background:#0d0d0f; user-select:none; touch-action:none; {style}",
            ),

            // ── SVG: grid lines + waterfall note blocks + keyboard ──────
            // Note blocks are rendered here first (z-order: below VelloCanvas).
            // VelloCanvas foreground (when present) covers them in Blitz.
            svg {
                xmlns: "http://www.w3.org/2000/svg",
                width: "100%",
                height: "100%",
                view_box: "0 0 {svg_w} {svg_h}",
                preserve_aspect_ratio: "none",
                style: "display:block; position:absolute; top:0; left:0;",

                // Waterfall zone (background + octave grid + falling blocks +
                // divider) — only when the waterfall is shown.
                if waterfall {
                    rect {
                        x: "0", y: "0",
                        width: "{svg_w}", height: "{fall_h}",
                        fill: "#0d0d0f",
                        pointer_events: "none",
                    }
                    {(0..=num_white).map(|i| {
                        let note = i + origin_white;
                        if (note % 7) != (origin_white % 7) {
                            return rsx! { g { key: "grid-{i}" } };
                        }
                        let x = i as f64 * WW;
                        rsx! {
                            line { key: "grid-{i}",
                                x1: "{x}", y1: "0",
                                x2: "{x}", y2: "{fall_h}",
                                stroke: "#1e1e24", stroke_width: "0.5",
                            }
                        }
                    })}
                    {svg_note_blocks}
                    rect {
                        x: "0", y: "{fall_h - 2.0}",
                        width: "{svg_w}", height: "2",
                        fill: "#2a2a3a",
                    }
                }

                // ── White keys ──────────────────────────────────────────
                {white_notes.iter().map(|&note| {
                    let wi = white_key_count_before(note).saturating_sub(origin_white);
                    let x = wi as f64 * WW;
                    let y = key_y;
                    let pressed = active.contains(&note);
                    let is_c = note % 12 == 0;

                    // Keys with a label (e.g. a drum sample mapped here) get a
                    // subtle tint so the mapped set stands out across the board.
                    let mapped = highlight_keys.contains(&note);
                    let fill = if pressed {
                        accent.clone()
                    } else if mapped {
                        "#cbd5f5".to_string()
                    } else {
                        "#f0f0f2".to_string()
                    };
                    let note_clone = note;

                    let label = labels.get(&note).cloned().unwrap_or_else(|| {
                        if show_labels && is_c {
                            format!("C{}", (note as i32 / 12) - 1)
                        } else {
                            String::new()
                        }
                    });

                    rsx! {
                        g { key: "wk-{note}",
                            rect {
                                x: "{x + 0.5}",
                                y: "{y}",
                                width: "{WW - 1.0}",
                                height: "{WH}",
                                rx: "{KEY_R}",
                                fill: "{fill}",
                                stroke: "#999",
                                stroke_width: "0.5",
                                cursor: "pointer",
                                onpointerdown: move |e| { e.stop_propagation(); on_note_on.call(note_clone); },
                                onpointerup:   move |e| { e.stop_propagation(); on_note_off.call(note_clone); },
                                onpointerleave: move |e| { e.stop_propagation(); on_note_off.call(note_clone); },
                            }
                            if pressed {
                                rect {
                                    x: "{x + 0.5}", y: "{y}",
                                    width: "{WW - 1.0}", height: "{WH}",
                                    rx: "{KEY_R}",
                                    fill: "{accent}",
                                    fill_opacity: "0.3",
                                    pointer_events: "none",
                                }
                            }
                            if !label.is_empty() {
                                text {
                                    x: "{x + WW / 2.0}",
                                    y: "{y + WH - 8.0}",
                                    text_anchor: "middle",
                                    font_size: "9",
                                    font_family: "-apple-system, sans-serif",
                                    fill: if pressed { "white" } else { "#888" },
                                    pointer_events: "none",
                                    "{label}"
                                }
                            }
                        }
                    }
                })}

                // ── Black keys (on top) ─────────────────────────────────
                {black_notes.iter().filter_map(|&note| {
                    let x = black_x(note, origin_white)?;
                    let y = key_y;
                    let pressed = active.contains(&note);
                    let mapped = highlight_keys.contains(&note);
                    let fill = if pressed {
                        accent.clone()
                    } else if mapped {
                        "#3b4a8c".to_string()
                    } else {
                        "#18181b".to_string()
                    };
                    let note_clone = note;

                    Some(rsx! {
                        g { key: "bk-{note}",
                            // Drop shadow
                            rect {
                                x: "{x + 1.5}", y: "{y + 2.0}",
                                width: "{BW}", height: "{BH}",
                                rx: "{KEY_R}",
                                fill: "black", fill_opacity: "0.45",
                                pointer_events: "none",
                            }
                            // Key body
                            rect {
                                x: "{x}", y: "{y}",
                                width: "{BW}", height: "{BH}",
                                rx: "{KEY_R}",
                                fill: "{fill}",
                                cursor: "pointer",
                                onpointerdown: move |e| { e.stop_propagation(); on_note_on.call(note_clone); },
                                onpointerup:   move |e| { e.stop_propagation(); on_note_off.call(note_clone); },
                                onpointerleave: move |e| { e.stop_propagation(); on_note_off.call(note_clone); },
                            }
                            // Highlight stripe
                            rect {
                                x: "{x + 2.5}", y: "{y + 3.0}",
                                width: "{BW - 5.0}", height: "14",
                                rx: "2",
                                fill: if pressed { "white" } else { "#2e2e38" },
                                fill_opacity: if pressed { "0.35" } else { "1.0" },
                                pointer_events: "none",
                            }
                            // Pressed accent glow
                            if pressed {
                                rect {
                                    x: "{x + 1.0}", y: "{y}",
                                    width: "{BW - 2.0}", height: "{BH}",
                                    rx: "{KEY_R}",
                                    fill: "{accent}",
                                    fill_opacity: "0.45",
                                    pointer_events: "none",
                                }
                            }
                            // Per-key sample label (e.g. hi-hat piece on a sharp).
                            if let Some(text_label) = labels.get(&note) {
                                text {
                                    x: "{x + BW / 2.0}",
                                    y: "{y + BH - 5.0}",
                                    text_anchor: "middle",
                                    font_size: "7",
                                    font_family: "-apple-system, sans-serif",
                                    fill: "white",
                                    pointer_events: "none",
                                    "{text_label}"
                                }
                            }
                        }
                    })
                })}
            }

            // ── Vello waterfall overlay ─────────────────────────────────
            // On native (Blitz/VST): VelloCanvas foreground covers the SVG
            //   note blocks with GPU glow rendering.
            // On WebKit dev: transparent no-op div; SVG blocks show through.
            // On wasm32: empty fragment; SVG blocks are the sole renderer.
            {vello_overlay}
        }
    }
}

// ── Waterfall animation helper ────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct WaterfallState {
    pub notes: Vec<WaterfallNote>,
    next_id: u32,
}

impl WaterfallState {
    /// Spawn a new note block at the keyboard level.  The block starts tiny
    /// and grows taller each [`tick`](Self::tick) while `held == true`.
    /// Call [`release`](Self::release) on note-off to start it scrolling.
    pub fn spawn(&mut self, note: u8, color: Option<String>) {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.notes.push(WaterfallNote {
            id,
            note,
            y_frac: 1.0,
            height_frac: 0.02, // tiny seed; grows while held
            color,
            held: true,
        });
    }

    /// Mark the most-recently-spawned held note for `note` as released.
    /// The block stops growing and starts scrolling upward from its current size.
    pub fn release(&mut self, note: u8) {
        if let Some(n) = self
            .notes
            .iter_mut()
            .rev()
            .find(|n| n.note == note && n.held)
        {
            n.held = false;
        }
    }

    /// Advance the waterfall by `delta_secs`.
    ///
    /// - **Held** notes: `height_frac` grows (block extends downward from growing top).
    /// - **Released** notes: `y_frac` decreases (whole block scrolls upward).
    ///
    /// Returns the pitches of blocks that have fully scrolled off the top.
    pub fn tick(&mut self, delta_secs: f64) -> Vec<u8> {
        // dy = fraction of waterfall height traversed per second / 2
        // → full crossing in 2 s; a 2-second held note fills the whole waterfall.
        let dy = delta_secs / 2.0;
        let mut completed = Vec::new();
        self.notes.retain_mut(|n| {
            if n.held {
                // Grow the block downward from the top; bottom stays at keyboard.
                n.height_frac = (n.height_frac + dy).min(0.97);
            } else {
                // Scroll the whole block upward.
                n.y_frac -= dy;
                if n.y_frac + n.height_frac < 0.0 {
                    completed.push(n.note);
                    return false;
                }
            }
            true
        });
        completed
    }

    pub fn clear(&mut self) {
        self.notes.clear();
    }
}

// ── Mini keyboard ─────────────────────────────────────────────────────────────

#[component]
pub fn MiniKeyboard(
    #[props(default = 60)] root_note: u8,
    #[props(default)] active_notes: Vec<u8>,
    on_note_on: EventHandler<u8>,
    on_note_off: EventHandler<u8>,
    #[props(default = "#3b82f6".to_string())] accent_color: String,
    #[props(default = "".to_string())] style: String,
) -> Element {
    rsx! {
        Piano {
            start_note: root_note,
            end_note: root_note + 12,
            active_notes,
            waterfall_notes: vec![],
            on_note_on,
            on_note_off,
            show_labels: false,
            accent_color,
            height: "140px".to_string(),
            style,
        }
    }
}
