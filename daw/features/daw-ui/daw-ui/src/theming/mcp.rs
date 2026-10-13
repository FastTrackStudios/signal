//! The MCP (Mixer Control Panel) theme context.
//!
//! Mirrors REAPER's `mcp.*` WALTER vocabulary (see
//! `reaper-theme/docs/walter-elements.md` §mcp): every element a theme can
//! position gets a [`Coord`] field of the same name, the `.color/.font/
//! .margin` attributes get typed counterparts, and layouts are **named**
//! ([`McpLayout`]) so themes ship variants the user/importer picks between —
//! exactly REAPER's `Layout "name" … EndLayout` model.
//!
//! Element coverage: `trackidx label volume volume.label pan pan.label width
//! meter mute solo recarm phase fx fxbyp io env folder` (+ `size margin`).
//! `recinput recmode recmon extmixer sendlist fxlist fxparm` belong to strip
//! extensions we don't render yet; an importer maps them when the widgets
//! exist.

use super::theme::Color;
use super::walter::{ColorPair, Coord, FaderMode, FontSpec, Margin, ThemeParam};

/// One named MCP strip layout — geometry, fonts and modes for every element.
/// A REAPER `Layout "…" … EndLayout` block flattens into one of these.
#[derive(Clone, PartialEq, Debug)]
pub struct McpLayout {
    pub name: String,

    /// `mcp.size` — natural strip size (px); coordinates below are authored at
    /// this size and spring from it via their attach scales.
    pub size: (f32, f32),
    /// `mcp.size` minimums (the "not seeing everything" threshold).
    pub min_size: (f32, f32),
    /// `mcp.margin`.
    pub margin: Margin,

    // ── texts ──
    /// `mcp.trackidx` (+ font/margin).
    pub trackidx: Coord,
    pub trackidx_font: FontSpec,
    pub trackidx_margin: Margin,
    /// `mcp.label` — the track-name footer (+ font/margin).
    pub label: Coord,
    pub label_font: FontSpec,
    pub label_margin: Margin,

    // ── faders / knobs ──
    /// `mcp.volume` + `mcp.volume.fadermode`.
    pub volume: Coord,
    pub volume_fadermode: FaderMode,
    /// `mcp.volume.label` — the volume readout (+ font/margin).
    pub volume_label: Coord,
    pub volume_label_font: FontSpec,
    pub volume_label_margin: Margin,
    /// `mcp.pan` + `mcp.pan.fadermode`.
    pub pan: Coord,
    pub pan_fadermode: FaderMode,
    /// `mcp.pan.label` (+ font/margin).
    pub pan_label: Coord,
    pub pan_label_font: FontSpec,
    pub pan_label_margin: Margin,
    /// `mcp.width` (stereo width; hidden until the engine carries it).
    pub width: Coord,
    pub width_fadermode: FaderMode,

    // ── meter ──
    /// `mcp.meter`.
    pub meter: Coord,

    /// `mcp.width.label` (+ margin).
    pub width_label: Coord,
    pub width_label_margin: Margin,

    // ── buttons ──
    /// `mcp.mute` / `mcp.solo` / `mcp.recarm`.
    pub mute: Coord,
    pub solo: Coord,
    pub recarm: Coord,
    /// `mcp.phase` / `mcp.fx` / `mcp.fxbyp` / `mcp.io` / `mcp.env` /
    /// `mcp.folder`.
    pub phase: Coord,
    pub fx: Coord,
    pub fxbyp: Coord,
    pub io: Coord,
    pub env: Coord,
    pub folder: Coord,

    // ── record row ──
    /// `mcp.recinput` (+ margin) — the record-input selector.
    pub recinput: Coord,
    pub recinput_margin: Margin,
    /// `mcp.recmode` / `mcp.recmon` / `mcp.fxin`.
    pub recmode: Coord,
    pub recmon: Coord,
    pub fxin: Coord,

    // ── theme-drawn chrome ──
    /// `mcp.custom.*` boxes (REAPER 7 themes draw the strip's backgrounds —
    /// dark fader well, track-colour tint, name/idx bars — as custom
    /// elements), in WALTER z-order (assignment order).
    pub customs: Vec<McpCustom>,
    /// Per-layout colour overrides (`mcp.*.color` resolved by the layout's
    /// WALTER); fall back to the theme-level [`McpColors`], then tokens.
    pub colors: McpColors,
}

/// A knob filmstrip skin, pre-sliced: one image per frame (index = value
/// position). Sliced at import because blitz paints `background-position`
/// offsets outside the element box — the whole stack leaked as a column of
/// ghost knobs.
#[derive(Clone, PartialEq, Debug)]
pub struct KnobSkin {
    /// Frame images (data URIs), value-ordered. The frames carry only the
    /// rotating indicator — REAPER composites them over the static knob
    /// `face` (`*_knob_small`, the guide: "a static image of a knob, over
    /// which the code line is drawn").
    pub frames: Vec<String>,
    pub frame_w: u32,
    pub frame_h: u32,
    /// The knob body (`tcp_vol_knob_small` / `gen_knob_bg_small`), drawn
    /// centered at native size under the frame overlay.
    pub face: Option<SkinImage>,
}

/// One `mcp.custom.*` element: a positioned box. Per the SDK, `.color`'s
/// first four components are the custom's *text* colour and the second four
/// the **background fill** — only the fill paints the box. Colours may be
/// the [`Color::TRACK`] sentinel (substituted with the live track accent).
/// Customs declared with a button image (`custom … 'tcp_labelBlock_bg'`)
/// carry the sliced art.
#[derive(Clone, PartialEq, Debug)]
pub struct McpCustom {
    /// Full attribute name (`mcp.custom.mcpDarkBox`).
    pub name: String,
    pub coord: Coord,
    /// `*.color` foreground — the custom's text colour (not a fill).
    pub fg: Option<Color>,
    /// `*.color` background fill (RGBA).
    pub bg: Option<Color>,
    /// Button image from the `custom` declaration (pink-margin sliced).
    pub image: Option<SkinImage>,
}

impl McpLayout {
    /// The FTS default vertical strip (natural 64×300) — pan knob over
    /// solo/mute, fader beside meter, readout + routing + name footer pinned
    /// to the bottom. Mirrors the pre-WALTER `ChannelStrip` appearance.
    pub fn vertical() -> Self {
        Self {
            name: "vertical".to_string(),
            size: (64.0, 300.0),
            min_size: (48.0, 220.0),
            margin: Margin::default(),

            trackidx: Coord::hidden(),
            trackidx_font: FontSpec::new(9.0, 700),
            trackidx_margin: Margin::default(),

            // Footer spans the strip, pinned to the bottom edge.
            label: Coord::new(0.0, 280.0, 64.0, 20.0, 0.0, 1.0, 1.0, 1.0),
            label_font: FontSpec::new(11.0, 700),
            label_margin: Margin::new(6.0, 4.0, 6.0, 4.0, 0.5),

            // Fader fills the left column down to the readout.
            volume: Coord::new(6.0, 88.0, 30.0, 140.0, 0.0, 0.0, 0.0, 1.0),
            volume_fadermode: FaderMode::Vertical,
            volume_label: Coord::new(4.0, 232.0, 56.0, 12.0, 0.0, 1.0, 1.0, 1.0),
            volume_label_font: FontSpec::new(10.0, 600),
            volume_label_margin: Margin::new(0.0, 0.0, 0.0, 0.0, 0.5),

            // Pan knob centered at the top.
            pan: Coord::new(15.0, 8.0, 34.0, 34.0, 0.5, 0.0, 0.5, 0.0),
            pan_fadermode: FaderMode::Vertical,
            pan_label: Coord::hidden(),
            pan_label_font: FontSpec::new(9.0, 600),
            pan_label_margin: Margin::default(),
            width: Coord::hidden(),
            width_fadermode: FaderMode::Vertical,

            // Meter beside the fader; right edge follows the strip width.
            meter: Coord::new(42.0, 88.0, 16.0, 140.0, 0.0, 0.0, 1.0, 1.0),

            // Solo | Mute row under the pan knob.
            solo: Coord::new(6.0, 52.0, 25.0, 22.0, 0.0, 0.0, 0.5, 0.0),
            mute: Coord::new(33.0, 52.0, 25.0, 22.0, 0.5, 0.0, 1.0, 0.0),
            recarm: Coord::hidden(),

            phase: Coord::hidden(),
            fx: Coord::hidden(),
            fxbyp: Coord::hidden(),
            // Routing button centered above the footer.
            io: Coord::new(19.0, 250.0, 26.0, 26.0, 0.5, 1.0, 0.5, 1.0),
            env: Coord::hidden(),
            folder: Coord::hidden(),

            width_label: Coord::hidden(),
            width_label_margin: Margin::default(),
            recinput: Coord::hidden(),
            recinput_margin: Margin::default(),
            recmode: Coord::hidden(),
            recmon: Coord::hidden(),
            fxin: Coord::hidden(),
            customs: Vec::new(),
            colors: McpColors::default(),
        }
    }

    /// The FTS default TCP row (natural 260×56) — a horizontal track-control
    /// row: recarm | name, volume slider under it | mute/solo | pan knob,
    /// with a slim meter on the right edge. The TCP analog of
    /// [`McpLayout::vertical`]; an imported REAPER theme replaces it with the
    /// theme's own `tcp.*` layouts.
    pub fn tcp_row() -> Self {
        Self {
            name: "row".to_string(),
            size: (260.0, 56.0),
            min_size: (120.0, 24.0),

            // Track number in the left gutter.
            trackidx: Coord::new(0.0, 0.0, 18.0, 56.0, 0.0, 0.0, 0.0, 1.0),

            // Name in the top row; right edge tracks the panel.
            label: Coord::new(26.0, 4.0, 138.0, 18.0, 0.0, 0.0, 1.0, 0.0),
            label_font: FontSpec::new(12.0, 600),
            label_margin: Margin::new(4.0, 0.0, 4.0, 0.0, 0.0),

            // Horizontal volume slider under the name.
            volume: Coord::new(26.0, 28.0, 130.0, 14.0, 0.0, 0.0, 1.0, 0.0),
            volume_fadermode: FaderMode::Horizontal,
            volume_label: Coord::hidden(),

            // Pan knob right of the mute/solo cluster.
            pan: Coord::new(214.0, 14.0, 28.0, 28.0, 1.0, 0.0, 1.0, 0.0),
            pan_fadermode: FaderMode::Knob,

            // Slim vertical meter on the right edge.
            meter: Coord::new(248.0, 2.0, 10.0, 52.0, 1.0, 0.0, 1.0, 1.0),

            // Mute/solo cluster right of the name.
            mute: Coord::new(166.0, 6.0, 22.0, 18.0, 1.0, 0.0, 1.0, 0.0),
            solo: Coord::new(190.0, 6.0, 22.0, 18.0, 1.0, 0.0, 1.0, 0.0),
            recarm: Coord::new(4.0, 18.0, 18.0, 18.0, 0.0, 0.0, 0.0, 0.0),

            io: Coord::hidden(),

            ..Self::vertical()
        }
    }

    /// A narrow 40px variant — pan/io hidden, stacked solo/mute (the
    /// `w<N` conditional branch a WALTER theme would express inline).
    pub fn narrow() -> Self {
        Self {
            name: "narrow".to_string(),
            size: (40.0, 300.0),
            min_size: (32.0, 180.0),

            label: Coord::new(0.0, 282.0, 40.0, 18.0, 0.0, 1.0, 1.0, 1.0),
            label_font: FontSpec::new(9.0, 700),
            label_margin: Margin::new(2.0, 3.0, 2.0, 3.0, 0.5),

            volume: Coord::new(4.0, 56.0, 20.0, 214.0, 0.0, 0.0, 0.0, 1.0),
            volume_label: Coord::hidden(),

            pan: Coord::hidden(),
            meter: Coord::new(27.0, 56.0, 9.0, 214.0, 0.0, 0.0, 1.0, 1.0),

            solo: Coord::new(4.0, 6.0, 32.0, 20.0, 0.0, 0.0, 1.0, 0.0),
            mute: Coord::new(4.0, 30.0, 32.0, 20.0, 0.0, 0.0, 1.0, 0.0),
            io: Coord::hidden(),

            ..Self::vertical()
        }
    }
}

/// Per-element colour overrides — the `mcp.*.color` attributes. `None` falls
/// back to the semantic token derivation, so a plain theme needs nothing here
/// while an imported REAPER theme can pin every slot.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct McpColors {
    /// `mcp.label.color`.
    pub label: Option<ColorPair>,
    /// `mcp.trackidx.color`.
    pub trackidx: Option<ColorPair>,
    /// `mcp.volume.color` — fader accent.
    pub volume: Option<Color>,
    /// `mcp.volume.label.color`.
    pub volume_label: Option<ColorPair>,
    /// `mcp.pan.color`.
    pub pan: Option<Color>,
    /// `mcp.pan.label.color`.
    pub pan_label: Option<ColorPair>,
    /// `mcp.meter.scale.color.lit.top` / `…lit.bottom` — when set, the meter
    /// fill becomes a bottom→top gradient instead of the token zone colours.
    pub meter_lit_top: Option<Color>,
    pub meter_lit_bottom: Option<Color>,
    /// `mcp.meter.scale.color.unlit.top` / `…unlit.bottom` — the meter well.
    pub meter_unlit_top: Option<Color>,
    pub meter_unlit_bottom: Option<Color>,
    /// `mcp.meter.readout.color`.
    pub meter_readout: Option<ColorPair>,
}

/// One skin image, delivered as a CSS-loadable URL (data URI or file) plus
/// its native px size (for aspect-correct placement).
#[derive(Clone, PartialEq, Debug)]
pub struct SkinImage {
    pub url: String,
    pub w: u32,
    pub h: u32,
    /// 9-slice patches from the art's pink marker lines (the "Pink Line
    /// Crush" technique): fixed margins render 1:1, only the unmarked bands
    /// absorb box-size differences. `None` = uniform stretch.
    pub slices: Option<Box<NineSlice>>,
}

/// A pink-marked image pre-cut into a 3×3 patch grid. Margins are native px;
/// zero-sized patches (when a margin is 0) are simply absent from rendering.
#[derive(Clone, PartialEq, Debug)]
pub struct NineSlice {
    /// Fixed margins: left, top, right, bottom (px).
    pub l: u32,
    pub t: u32,
    pub r: u32,
    pub b: u32,
    /// Row-major patches: `[tl, tm, tr, ml, c, mr, bl, bm, br]` — corners
    /// fixed both axes, edges stretch one axis, the centre stretches both.
    pub patches: Vec<SkinImage>,
}

/// The three interaction states of one 3-slice button image.
#[derive(Clone, PartialEq, Debug)]
pub struct ButtonStateSkin {
    pub normal: SkinImage,
    pub hover: SkinImage,
    pub pressed: SkinImage,
}

/// A two-state (off/on) image-skinned button, each with its 3-slice
/// interaction states.
#[derive(Clone, PartialEq, Debug)]
pub struct ButtonSkin {
    pub off: ButtonStateSkin,
    pub on: ButtonStateSkin,
}

/// The optional image skin for the MCP strip — REAPER's atlas vocabulary
/// sliced into per-element images (`mcp_*` → `track_*` → `gen_*` fallback
/// chain). Elements without an entry render vector styles.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct McpSkin {
    /// `*_mute_off/on`.
    pub mute: Option<ButtonSkin>,
    /// `*_solo_off/on`.
    pub solo: Option<ButtonSkin>,
    /// `*_recarm_off/on`.
    pub recarm: Option<ButtonSkin>,
    /// `mcp_io` / `track_io`.
    pub io: Option<ButtonStateSkin>,
    /// `*_fx_empty` (off = no FX) / `*_fx_norm` (on).
    pub fx: Option<ButtonSkin>,
    /// `*_fxoff_h` / `*_fxon_h` — the FX bypass on/off switch.
    pub fxbyp: Option<ButtonSkin>,
    /// `*_env`.
    pub env: Option<ButtonSkin>,
    /// `*_phase_norm` (off) / `*_phase_inv` (on).
    pub phase: Option<ButtonSkin>,
    /// `*_recmode_off` / `*_recmode_in`.
    pub recmode: Option<ButtonSkin>,
    /// `*_folder_off` / `*_folder_on`.
    pub folder: Option<ButtonSkin>,
    /// `*_fx_in_empty` / `*_fx_in_norm` — input FX.
    pub fxin: Option<ButtonSkin>,
    /// `mcp_recinput` — the record-input selector background.
    pub recinput_bg: Option<SkinImage>,
    /// `*_pan_knob_stack` — the pan knob filmstrip (square frames stacked
    /// vertically; the frame index maps the knob position).
    pub pan_knob: Option<KnobSkin>,
    /// `*_vol_knob_stack` — the volume knob filmstrip (TCP volume renders as
    /// a knob when `*.volume.fadermode` is 1).
    pub vol_knob: Option<KnobSkin>,
    /// `mcp_volbg` — the fader well.
    pub volbg: Option<SkinImage>,
    /// `mcp_volthumb` — the fader cap.
    pub volthumb: Option<SkinImage>,
    /// `mcp_panbg` — the pan slider groove (REAPER's MCP pan is horizontal:
    /// `mcp.pan.fadermode` = 1 in the default theme's WALTER).
    pub panbg: Option<SkinImage>,
    /// `mcp_panthumb` — the pan slider cap.
    pub panthumb: Option<SkinImage>,
    /// `meter_strip_v` — the lit meter column (stretched over the fill).
    pub meter_strip: Option<SkinImage>,
    /// `meter_bg_v` — the meter well.
    pub meter_bg: Option<SkinImage>,
}

/// A runtime WALTER layout engine: `(ctx, layout, w, h, armed) → McpLayout`.
///
/// REAPER re-runs WALTER on every panel resize — flow-based themes
/// (Reapertips' `then`-macro chain) reorder, wrap, shrink and cull elements
/// per the *actual* panel size, which a one-shot anchor bake cannot
/// reproduce. Importers that can re-evaluate (the REAPER importer) install
/// one of these; renderers that know their px size call it instead of the
/// baked layouts. Results must be internally cached (renderers call this
/// per frame).
/// Track-state scalars a [`LayoutEngine`] evaluation bakes in — themes
/// resize/show elements per state (`recarm`, `track_selected`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct StripState {
    pub armed: bool,
    pub selected: bool,
}

/// The engine's evaluation function: `(ctx, layout, w, h, state)`.
pub type LayoutEngineFn =
    dyn Fn(&str, &str, f32, f32, StripState) -> Option<McpLayout> + Send + Sync;

#[derive(Clone)]
pub struct LayoutEngine(pub std::sync::Arc<LayoutEngineFn>);

impl LayoutEngine {
    /// Evaluate `ctx.{…}` (`"mcp"`/`"tcp"`) layout `name` at an exact panel
    /// size, with the track-state scalars set (themes reflow per state —
    /// selection switches the Anti-Theme's name field to its light variant).
    /// `None` = the theme has no WALTER program for this context/layout
    /// (callers fall back to the baked layouts).
    pub fn layout_at(
        &self,
        ctx: &str,
        name: &str,
        w: f32,
        h: f32,
        state: StripState,
    ) -> Option<McpLayout> {
        (self.0)(ctx, name, w, h, state)
    }
}

impl PartialEq for LayoutEngine {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for LayoutEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LayoutEngine(..)")
    }
}

/// The MCP theme context: named layouts + colour overrides + author params.
#[derive(Clone, PartialEq, Debug)]
pub struct McpTheme {
    pub layouts: Vec<McpLayout>,
    pub colors: McpColors,
    /// `define_parameter` knobs. [`McpTheme::param`] resolves by name;
    /// `mcp_show_pan` and `mcp_strip_width` are honoured by the strip widget.
    pub params: Vec<ThemeParam>,
    /// Image skin (imported themes); `None` = pure vector rendering.
    pub skin: Option<McpSkin>,
}

impl McpTheme {
    /// FTS defaults: vertical + narrow layouts, token-derived colours, and the
    /// two starter adjuster knobs.
    pub fn fts_default() -> Self {
        Self {
            layouts: vec![McpLayout::vertical(), McpLayout::narrow()],
            colors: McpColors::default(),
            params: vec![
                ThemeParam::new("mcp_show_pan", "Show pan knob", 1.0, 0.0, 1.0),
                ThemeParam::new(
                    "mcp_strip_width",
                    "Strip width (0 = layout default)",
                    0.0,
                    0.0,
                    128.0,
                ),
            ],
            skin: None,
        }
    }

    /// FTS defaults for the TCP context — the [`McpLayout::tcp_row`] layout.
    pub fn fts_default_tcp() -> Self {
        Self {
            layouts: vec![McpLayout::tcp_row()],
            colors: McpColors::default(),
            params: Vec::new(),
            skin: None,
        }
    }

    /// Resolve a layout by name; unknown/`None` falls back to the first.
    pub fn layout(&self, name: Option<&str>) -> &McpLayout {
        name.and_then(|n| self.layouts.iter().find(|l| l.name == n))
            .or(self.layouts.first())
            .expect("McpTheme has at least one layout")
    }

    /// Look up a `define_parameter` knob's current value.
    pub fn param(&self, name: &str) -> Option<f32> {
        self.params.iter().find(|p| p.name == name).map(|p| p.value)
    }
}

impl Default for McpTheme {
    fn default() -> Self {
        Self::fts_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_has_named_layouts() {
        let mcp = McpTheme::fts_default();
        assert_eq!(mcp.layout(Some("narrow")).name, "narrow");
        assert_eq!(mcp.layout(Some("vertical")).name, "vertical");
        // Unknown / None fall back to the first layout.
        assert_eq!(mcp.layout(Some("nope")).name, "vertical");
        assert_eq!(mcp.layout(None).name, "vertical");
    }

    #[test]
    fn narrow_layout_hides_pan_and_io() {
        let narrow = McpLayout::narrow();
        assert!(narrow.pan.is_hidden());
        assert!(narrow.io.is_hidden());
        assert!(!narrow.volume.is_hidden());
        assert!(!narrow.meter.is_hidden());
    }

    #[test]
    fn params_resolve_by_name() {
        let mcp = McpTheme::fts_default();
        assert_eq!(mcp.param("mcp_show_pan"), Some(1.0));
        assert_eq!(mcp.param("missing"), None);
    }
}
