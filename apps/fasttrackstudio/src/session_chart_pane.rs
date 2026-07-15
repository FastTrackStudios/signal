//! Browser chart pane — the active song's keyflow chart, synced to the
//! session playhead.
//!
//! Renders the current song's chart with the same CPU engraver pipeline the
//! website uses (`keyflow::engraver` layout → SVG string, wasm-safe — no
//! canvas/wgpu), and drives a highlight overlay from the transport's musical
//! position (`session_ui::ACTIVE_PLAYBACK_MUSICAL`, fed by the
//! `SetlistService` streams the `SessionRemoteBridge` pumps):
//!
//! - **chart source**: `SONG_CHARTS[project_guid].chart_text` — hydrated by
//!   the engine (MIDI analysis on REAPER; ext-state `FTS/chart_text` on the
//!   standalone demo engine) and backfilled over the `song_chart` RPC.
//! - **static layer**: chart text → `keyflow::parse` → continuous-scroll
//!   layout → one fontless SVG, re-generated only when the text changes.
//!   Fonts are injected once as `@font-face` data URIs (Bravura for SMuFL
//!   symbols, MuseJazz for chords, FreeSans for text) so every re-render
//!   serializes ~KBs of paths, not ~4MB of font data.
//! - **highlight overlay**: a second SVG with the same viewBox stacked on
//!   top; `ChartCursor` (MeasureHighlight) turns the playhead tick into
//!   renderer-agnostic draw commands which become overlay `rect`/`line`
//!   elements — so the active measure highlight scales pixel-perfectly with
//!   the chart under it, and only the tiny overlay re-renders at transport
//!   rate.
//! - **auto-advance**: the pane derives everything from `ACTIVE_INDICES`,
//!   so when the engine moves to the next song the chart follows; while
//!   playing, the view auto-scrolls the cursor into view.
//!
//! Playhead model: the cursor rides `ACTIVE_INDICES.song_progress` (the
//! ~10 Hz cursor stream that flows on every backend) mapped onto the chart's
//! own timeline — `chart seconds = count_in + progress × song duration`,
//! resolved through the layout's per-beat `time_start/time_end` table
//! (`ChartCursor::compute_at_time`), which honors mid-chart meter changes.
//! The demo songs' sections are generated FROM these charts, so chart time
//! and project time agree by construction. (`TransportUpdate`'s musical
//! position would be finer-grained, but that stream does not currently
//! reach standalone-engine subscribers — see the probe.)

use dioxus::prelude::*;
use std::cell::RefCell;

use keyflow::engraver::export::{SvgExportConfig, SvgSerializer};
use keyflow::engraver::fonts::ChartFontBundle;
use keyflow::engraver::layout::ChartLayoutMode;
use keyflow::engraver::layout::chart::cursor::{
    ChartCursor, CursorConfig, CursorState, CursorStyle, HighlightCommand,
};
use keyflow::engraver::layout::chart::{
    Breakpoint, ChartLayoutConfig, ChartLayoutEngine, ChartLayoutResult,
};
use keyflow::engraver::style::MStyle;
use session_ui::{ACTIVE_INDICES, SETLIST_STRUCTURE, SONG_CHARTS};

/// Layout width in points. Fixed (tablet/desktop breakpoint); the SVG scales
/// to the pane's CSS width, so this only picks type sizes / measures-per-line.
const LAYOUT_WIDTH_PT: f64 = 640.0;

// ─── Layout cache (one per page — wasm is single-threaded) ────────────────

struct Pane {
    engine: ChartLayoutEngine,
    font_bundle: ChartFontBundle,
    /// (key, layout, static svg, content width pt, content height pt)
    layout: Option<(u64, ChartLayoutResult, String, f64, f64)>,
}

thread_local! {
    static PANE: RefCell<Option<Pane>> = const { RefCell::new(None) };
}

fn with_pane<R>(f: impl FnOnce(&mut Pane) -> R) -> Option<R> {
    PANE.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            let font_bundle = match ChartFontBundle::new() {
                Ok(b) => b,
                Err(e) => {
                    tracing::error!("chart pane: font bundle failed: {e}");
                    return None;
                }
            };
            let style: &'static MStyle = Box::leak(Box::new(MStyle::new()));
            let engine = font_bundle.create_layout_engine(style);
            *slot = Some(Pane {
                engine,
                font_bundle,
                layout: None,
            });
        }
        slot.as_mut().map(f)
    })
}

fn text_key(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

impl Pane {
    /// Parse + layout + serialize `text` if it isn't the cached chart.
    /// Returns `(svg, width_pt, height_pt)` on success.
    fn ensure(&mut self, text: &str) -> Option<(String, f64, f64)> {
        let key = text_key(text);
        if let Some((cached_key, _, svg, w, h)) = &self.layout {
            if *cached_key == key {
                return Some((svg.clone(), *w, *h));
            }
        }

        let chart = match keyflow::parse(text) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("chart pane: parse failed: {e}");
                return None;
            }
        };

        // Continuous scroll — one unbounded vertical column, no page breaks
        // (the browser scrolls), with the responsive preset for this width.
        let breakpoint = Breakpoint::from_viewport_pt(LAYOUT_WIDTH_PT);
        let config = ChartLayoutConfig::responsive_for(breakpoint);
        let mode = ChartLayoutMode::ContinuousScroll {
            width: LAYOUT_WIDTH_PT,
        };
        let layout = self.engine.layout_chart_with_config(&chart, &mode, &config);

        let (w, h) = (
            layout.total_width.max(LAYOUT_WIDTH_PT),
            layout.total_height.max(60.0),
        );
        // Fontless SVG (the page carries @font-face for the same families).
        let config = SvgExportConfig::for_page(0.0, 0.0, w, h);
        let mut serializer = SvgSerializer::new(config);
        let mut svg = serializer.serialize(&layout.scene);
        // Make the inline SVG fluid: width tracks the pane, the viewBox
        // keeps the aspect ratio (browsers derive height from it).
        if let Some(pos) = svg.find("width=\"") {
            if let Some(end) = svg[pos..].find(" viewBox") {
                svg.replace_range(pos..pos + end, "width=\"100%\"");
            }
        }

        self.layout = Some((key, layout, svg.clone(), w, h));
        Some((svg, w, h))
    }

    /// Playhead (seconds on the chart's own timeline, count-in at 0) →
    /// cursor state against the cached layout.
    fn cursor_state_at_time(&self, key: u64, chart_seconds: f64) -> Option<CursorState> {
        let (cached_key, layout, ..) = self.layout.as_ref()?;
        if *cached_key != key {
            return None;
        }
        let cursor = ChartCursor::new(CursorConfig {
            style: CursorStyle::MeasureHighlight,
            accent_color: [59, 130, 246, 255], // blue-500
            fill_alpha: 0.18,
            highlight_notehead: false,
            show_when_stopped: true,
            ..CursorConfig::default()
        });
        cursor.compute_at_time(layout, chart_seconds)
    }
}

/// `@font-face` CSS with the engraver's embedded fonts as data URIs —
/// injected once so fontless chart SVGs resolve their families.
fn font_face_css() -> String {
    thread_local! {
        static CSS: RefCell<Option<String>> = const { RefCell::new(None) };
    }
    CSS.with(|cell| {
        let mut slot = cell.borrow_mut();
        if let Some(css) = slot.as_ref() {
            return css.clone();
        }
        let css = with_pane(|pane| {
            use base64::Engine as _;
            let b64 = |data: &[u8]| base64::engine::general_purpose::STANDARD.encode(data);
            let face = |family: &str, data: &[u8]| {
                format!(
                    "@font-face{{font-family:'{family}';src:url(data:font/otf;base64,{}) format('opentype');}}",
                    b64(data)
                )
            };
            let symbol = pane.font_bundle.symbol_font_data().as_slice();
            let text = pane.font_bundle.text_font_data().as_slice();
            let aux = pane.font_bundle.aux_font_data().as_slice();
            [
                face("Bravura", symbol),
                face("MuseJazzText", text),
                face("MuseJazz", text),
                face("FreeSans", aux),
                face("title-bold", aux),
                face("part-name-bold", aux),
            ]
            .join("\n")
        })
        .unwrap_or_default();
        *slot = Some(css.clone());
        css
    })
}

// ─── Components ────────────────────────────────────────────────────────────

/// The chart pane: active song's chart + playhead highlight, auto-following
/// the setlist cursor. Shows a quiet placeholder when the song has no chart.
#[component]
pub fn SessionChartPane() -> Element {
    // (guid-independent) chart text for the ACTIVE song. Reads the cursor,
    // the setlist structure and the hydrated chart cache — recomputes only
    // when one of those changes, not on transport frames.
    let chart_text = use_memo(move || {
        let indices = ACTIVE_INDICES.read();
        let idx = indices.song_index?;
        drop(indices);
        let setlist = SETLIST_STRUCTURE.read();
        let song = setlist.songs.get(idx)?;
        let charts = SONG_CHARTS.read();
        charts
            .get(&song.project_guid)
            .map(|c| c.chart_text.clone())
            .or_else(|| song.chart_text.clone())
    });

    match chart_text() {
        Some(text) => rsx! {
            ChartCanvas { text }
        },
        None => rsx! {
            div { style: "display:flex; align-items:center; justify-content:center; flex:1; min-height:0;",
                span { style: "font-size:12px; color:#52525b;", "No chart for this song." }
            }
        },
    }
}

/// Static chart layer + overlay stack. Re-renders only when the chart text
/// changes; the 60 Hz playhead lives in `ChartCursorOverlay` below.
#[component]
fn ChartCanvas(text: String) -> Element {
    let key = text_key(&text);
    let rendered = with_pane(|pane| pane.ensure(&text)).flatten();

    let Some((svg, w, h)) = rendered else {
        return rsx! {
            div { style: "display:flex; align-items:center; justify-content:center; flex:1; min-height:0;",
                span { style: "font-size:12px; color:#ef4444;", "Chart failed to render." }
            }
        };
    };

    rsx! {
        document::Style { {font_face_css()} }
        div {
            id: "kf-chart-scroll",
            style: "flex:1; min-height:0; overflow-y:auto; overflow-x:hidden; background:#ffffff;",
            div { style: "position:relative; width:100%;",
                div { dangerous_inner_html: "{svg}" }
                ChartCursorOverlay { layout_key: key, view_w: w, view_h: h }
            }
        }
    }
}

/// The playhead overlay: same viewBox as the static SVG, absolutely
/// positioned over it, containing only the cursor's highlight commands.
#[component]
fn ChartCursorOverlay(layout_key: u64, view_w: f64, view_h: f64) -> Element {
    // Cursor-rate signal (~10 Hz progress + structural changes) — only this
    // small component re-renders on it.
    let (progress, playing) = {
        let indices = ACTIVE_INDICES.read();
        (indices.song_progress, indices.is_playing)
    };

    // progress (0..1 over SONGSTART..SONGEND) → seconds on the chart's own
    // timeline, whose 0 is the start of the count-in.
    let chart_seconds = progress.and_then(|p| {
        let indices = ACTIVE_INDICES.read();
        let idx = indices.song_index?;
        drop(indices);
        let setlist = SETLIST_STRUCTURE.read();
        let song = setlist.songs.get(idx)?;
        let duration = song.duration();
        (duration > 0.0).then(|| song.count_in_seconds.unwrap_or(0.0) + p.clamp(0.0, 1.0) * duration)
    });

    let state = chart_seconds
        .and_then(|t| with_pane(|pane| pane.cursor_state_at_time(layout_key, t)).flatten());

    // Auto-follow: keep the cursor in view while playing. Runs when the
    // active measure changes (the marker element moved), not per tick.
    let measure = state.as_ref().map(|s| s.measure);
    use_effect(use_reactive!(|(measure, playing)| {
        if playing && measure.is_some() {
            document::eval(
                "document.getElementById('kf-playhead')?.scrollIntoView({block:'center', behavior:'smooth'});",
            );
        }
    }));

    let Some(state) = state else {
        return rsx! {};
    };

    rsx! {
        svg {
            view_box: "0 0 {view_w} {view_h}",
            preserve_aspect_ratio: "xMinYMin meet",
            style: "position:absolute; inset:0; width:100%; height:100%; pointer-events:none;",
            // Scroll anchor at the cursor position (invisible).
            circle {
                id: "kf-playhead",
                cx: "{state.cursor_x}",
                cy: "{state.cursor_y + state.cursor_height / 2.0}",
                r: "1",
                fill: "none",
            }
            for (i, cmd) in state.commands.iter().enumerate() {
                {render_command(i, cmd)}
            }
            // Thin playhead line inside the highlighted measure.
            line {
                x1: "{state.cursor_x}",
                y1: "{state.cursor_y - 4.0}",
                x2: "{state.cursor_x}",
                y2: "{state.cursor_y + state.cursor_height + 4.0}",
                stroke: "rgba(59,130,246,0.9)",
                stroke_width: "1.5",
            }
        }
    }
}

fn rgba_css(c: &[u8; 4], alpha_mul: f32) -> String {
    let a = (c[3] as f32 / 255.0 * alpha_mul).clamp(0.0, 1.0);
    format!("rgba({},{},{},{a:.3})", c[0], c[1], c[2])
}

/// One cursor draw command → overlay SVG element. Glyph commands (notehead
/// glow) are skipped — the pane uses measure highlighting only.
fn render_command(i: usize, cmd: &HighlightCommand) -> Element {
    match cmd {
        HighlightCommand::FillRect {
            x,
            y,
            width,
            height,
            color,
        } => rsx! {
            rect {
                key: "{i}",
                x: "{x}",
                y: "{y}",
                width: "{width}",
                height: "{height}",
                fill: rgba_css(color, 1.0),
            }
        },
        HighlightCommand::FillRoundedRect {
            x,
            y,
            width,
            height,
            radius,
            color,
        } => rsx! {
            rect {
                key: "{i}",
                x: "{x}",
                y: "{y}",
                width: "{width}",
                height: "{height}",
                rx: "{radius}",
                fill: rgba_css(color, 1.0),
            }
        },
        HighlightCommand::StrokeLine {
            x,
            y_top,
            y_bottom,
            color,
            width,
        } => rsx! {
            line {
                key: "{i}",
                x1: "{x}",
                y1: "{y_top}",
                x2: "{x}",
                y2: "{y_bottom}",
                stroke: rgba_css(color, 1.0),
                stroke_width: "{width}",
            }
        },
        HighlightCommand::StrokeGlyph { .. } | HighlightCommand::FillGlyph { .. } => rsx! {},
    }
}
