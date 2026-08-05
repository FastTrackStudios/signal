//! EQ editor — Dioxus GUI root component.
//!
//! Pro-Q style spectrum analyzer with draggable band nodes, level meters, and
//! a per-band detail panel. Reusable widgets (knobs, meters, drag provider)
//! and the theme system come from [`fts_ui_audio`]; the EQ-specific graph
//! lives in [`crate::eq_graph`].

use std::sync::atomic::Ordering;

use audiocore_core::prelude::*;
// The concrete `GuiContext` is not in nice-plug's prelude (which carries the
// `GuiContextInner` trait); upstream's own backends import it directly too.
use nice_plug::context::gui::GuiContext;
use fts_ui::prelude::{
    Button, ButtonSize, ButtonVariant, Card, CardContent, CardHeader, CardTitle, SegmentedControl,
    SegmentedControlSize, Select, SelectContent, SelectItem, Switch, TabContent, TabList,
    TabTrigger, Tabs, ThemeMode, ThemeProvider, ThemeState, default_theme_preset,
};
use fts_ui_audio::prelude::*;

use crate::eq_graph::{EqBand, EqBandShape, EqGraph, OverlayChoice};
use crate::param_adapter::param_handle;
use crate::params::{EqUiState, NUM_BANDS, SPECTRUM_BINS};
use crate::profile_view::{
    Api550aProfileView, Neve1073ProfileView, PultecProfileView, SslEProfileView, SslGProfileView,
};
use spectrum_analyzer::dsp::AnalyzerSettings;
use spectrum_analyzer::ui::AnalyzerSettingsPanel;

/// A cheat-sheet [`TrackInfoProvider`](crate::cheatsheet::TrackInfoProvider)
/// backed by the host `GuiContext`. In the plugin this surfaces the track name
/// the CLAP `track-info` extension reported (cached in the wrapper), driving the
/// EQ overlay's "Auto" mode. In the standalone the host `GuiContext` has no
/// track, so the standalone injects a `StaticTrackProvider` instead and this is
/// never reached.
struct GuiContextTrackProvider {
    gui: GuiContext,
}

impl crate::cheatsheet::TrackInfoProvider for GuiContextTrackProvider {
    fn track_name(&self) -> Option<String> {
        self.gui.track_info().and_then(|info| info.name)
    }
}

fn shape_to_int(shape: EqBandShape) -> i32 {
    match shape {
        EqBandShape::Bell => 0,
        EqBandShape::LowShelf => 1,
        EqBandShape::LowCut => 2,
        EqBandShape::HighShelf => 3,
        EqBandShape::HighCut => 4,
        EqBandShape::Notch => 5,
        EqBandShape::BandPass => 6,
        EqBandShape::TiltShelf => 7,
        EqBandShape::FlatTilt => 8,
        EqBandShape::AllPass => 9,
    }
}

fn int_to_shape(v: i32) -> EqBandShape {
    match v {
        0 => EqBandShape::Bell,
        1 => EqBandShape::LowShelf,
        2 => EqBandShape::LowCut,
        3 => EqBandShape::HighShelf,
        4 => EqBandShape::HighCut,
        5 => EqBandShape::Notch,
        6 => EqBandShape::BandPass,
        7 => EqBandShape::TiltShelf,
        8 => EqBandShape::FlatTilt,
        9 => EqBandShape::AllPass,
        _ => EqBandShape::Bell,
    }
}

/// Root editor component.
///
/// Wraps the EQ shell in `fts_ui::ThemeProvider` so all shadcn-style
/// components (`Card`, `Button`, `Tabs`, etc.) pick up the active theme.
/// The plugin embedded path and the standalone path both go through here.
#[component]
pub fn App() -> Element {
    let theme_state = use_signal(|| ThemeState::new(default_theme_preset(), ThemeMode::Dark));
    rsx! {
        document::Style { {nice_plug_dioxus::TAILWIND_CSS} }
        // eq-ui's own compiled utilities + theme tokens — the framework CSS
        // above only covers nice-plug-dioxus's widgets, so without this every
        // layout-critical class (flex-1, min-h-0, …) is undefined and the
        // graph area collapses in the embedded editor. `just tailwind-eq`.
        document::Style { {include_str!("../assets/tailwind.css")} }
        ThemeProvider { state: theme_state, AppShell {} }
    }
}

/// Inner shell component — runs after the ThemeProvider context is in
/// scope so shadcn primitives can resolve their theme tokens.
#[component]
fn AppShell() -> Element {
    // Chrome (header / inspector / bottom bar) is parked while the editor is
    // graph-only — the controls migrate INTO the graph surface piece by piece
    // (issue #31). Flip to re-enable everything for reference.
    const SHOW_CHROME: bool = false;

    let t = use_init_theme();
    let t = *t.read();

    let shared = use_context::<SharedState>();
    let ui = shared.get::<EqUiState>().expect("EqUiState missing");
    let ctx = use_param_context();
    let params = &ui.params;

    // Cheat-sheet overlay track source. If the host shell already injected a
    // provider (the standalone injects a `StaticTrackProvider` for env-based
    // testing), leave it; otherwise back it with the host `GuiContext` so the
    // plugin's CLAP track-info drives the EQ overlay's Auto mode.
    if try_consume_context::<std::sync::Arc<dyn crate::cheatsheet::TrackInfoProvider>>().is_none() {
        let provider: std::sync::Arc<dyn crate::cheatsheet::TrackInfoProvider> =
            std::sync::Arc::new(GuiContextTrackProvider {
                gui: ctx.gui_context().clone(),
            });
        provide_context(provider);
    }

    // 60 Hz sampling tick. App owns the read-side of every Param atomic
    // (bands[i].freq_hz.value(), spectrum bins, meters), so even if a
    // descendant component re-renders, App must re-render too for those
    // reads to flow through to the EQ graph + meters. Spawn the OS thread
    // exactly once via use_hook and have it call schedule_update on the
    // App scope. EqGraph keeps its own tick for canvas re-paint.
    // 60Hz tick — matches both common display refresh and the audio
    // animator's update rate. Combined with the `data-frame` attribute
    // mutation below, this means every tick produces a real DOM diff,
    // forces request_redraw, and the canvas paint source fires. Faster
    // ticks just queue work that the event loop can't drain in time
    // (we tried 4ms and 8ms; both stalled the loop).
    let mut app_tick: Signal<u64> = use_signal(|| 0);
    use_hook(|| {
        let updater = dioxus_core::schedule_update();
        std::thread::spawn(move || {
            loop {
                // 120 Hz (~8.33 ms) for smoother metering/params on high-refresh
                // displays. Presentation is still vsync-bounded, so on a 60 Hz
                // display this just coalesces to 60.
                std::thread::sleep(std::time::Duration::from_micros(8_333));
                updater();
            }
        });
    });
    app_tick += 1;
    let _ = *app_tick.read();

    // ── FPS measurement ─────────────────────────────────────────────
    // Each App re-render is one frame from our perspective (the tick
    // thread above schedules them at 60Hz). We track the last 60
    // frame instants and compute the rolling FPS = N / (last - first)
    // window. Smooths out jitter and gives an honest read of how
    // close to the 60Hz target the redraw + paint pipeline is hitting.
    let frame_history =
        use_hook(|| std::sync::Arc::new(parking_lot::Mutex::new(FrameHistory::new())));
    let fps_value = {
        let mut hist = frame_history.lock();
        hist.tick();
        hist.fps()
    };

    let input_db = ui.input_peak_db.load(Ordering::Relaxed);
    let output_db = ui.output_peak_db.load(Ordering::Relaxed);
    let sample_rate = ui.sample_rate.load(Ordering::Relaxed) as f64;

    let db_range_idx = params.db_range.value();
    let db_range: f64 = match db_range_idx {
        0 => 3.0,
        1 => 6.0,
        2 => 12.0,
        3 => 18.0,
        4 => 24.0,
        _ => 30.0,
    };

    let spectrum: Vec<f32> = (0..SPECTRUM_BINS)
        .map(|i| ui.spectrum_bins[i].load(Ordering::Relaxed))
        .collect();

    // Drive the full analyzer engine from the UI tick and grab a snapshot for
    // the graph. The tick rate matches the redraw thread above (~120 Hz).
    ui.analyzer.tick(120.0);
    let analyzer_snapshot = ui.analyzer.snapshot();
    let mut analyzer_settings: Signal<AnalyzerSettings> = use_signal(|| ui.analyzer.settings());
    let analyzer_for_settings = ui.clone();
    let current_model = params.model.value();
    let hardware_mode_active = current_model != 0;
    let model_response: Option<Vec<f32>> = hardware_mode_active.then(|| {
        (0..SPECTRUM_BINS)
            .map(|i| ui.model_response_bins[i].load(Ordering::Relaxed))
            .collect()
    });

    let focused_band: Signal<Option<usize>> = use_signal(|| None);
    let mut inspector_tab: Signal<String> = use_signal(|| "band".to_string());
    // Cheat-sheet overlay/profile selection, shared between the inspector
    // selector below and the EqGraph (passed as a prop). `Auto` resolves from
    // the track name via the TrackInfoProvider.
    let mut overlay_sel: Signal<OverlayChoice> = use_signal(|| OverlayChoice::Auto);
    // String mirror of `overlay_sel` for the fts_ui `Select` component (it works
    // on string values). Kept in sync each render so the graph dropdown and this
    // selector stay consistent. "auto"/"off"/<profile index>.
    let profile_value: String = match *overlay_sel.read() {
        OverlayChoice::Auto => "auto".to_string(),
        OverlayChoice::Off => "off".to_string(),
        OverlayChoice::Pick(sel) => crate::cheatsheet::PROFILES
            .iter()
            .position(|p| std::ptr::eq(p, sel))
            .map_or_else(|| "off".to_string(), |i| i.to_string()),
    };
    let mut profile_sig = use_signal(|| profile_value.clone());
    if *profile_sig.read() != profile_value {
        profile_sig.set(profile_value.clone());
    }
    let mut profile_tab: Signal<String> = use_signal(|| "default".to_string());

    let gain_scale = params.gain_scale.value() / 100.0;

    let mut bands_vec: Vec<EqBand> = Vec::with_capacity(NUM_BANDS);
    for i in 0..NUM_BANDS {
        let bp = &params.bands[i];
        bands_vec.push(EqBand {
            index: i,
            used: bp.enabled.value() > 0.5,
            enabled: bp.enabled.value() > 0.5,
            frequency: bp.freq_hz.value(),
            gain: bp.gain_db.value() * gain_scale,
            q: bp.q.value(),
            shape: int_to_shape(bp.filter_type.value()),
            solo: bp.solo.value() > 0.5,
            stereo_mode: Default::default(),
            name: bp.name.read().clone(),
        });
    }

    let mut bands_signal = use_signal(|| bands_vec.clone());
    *bands_signal.write() = bands_vec;

    let active_count = bands_signal.read().iter().filter(|b| b.used).count();
    let graph_bands_vec = if hardware_mode_active {
        Vec::new()
    } else {
        bands_signal.read().clone()
    };
    let mut graph_bands_signal = use_signal(|| graph_bands_vec.clone());
    *graph_bands_signal.write() = graph_bands_vec;

    // Drive the root colors / typography from the fts-ui CSS variables so
    // the EQ shell respects the active ThemeProvider preset. The
    // fts_ui_audio theme is still in scope for spacing tokens but its
    // hardcoded palette is no longer used at this level.
    let base_css = "*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; } \
         html, body { width: 100%; height: 100%; overflow: hidden; \
         background: var(--background); color: var(--foreground); \
         font-family: var(--font-sans, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, sans-serif); \
         font-size: 13px; }";
    let root_style = "width:100vw; height:100vh; \
         display:flex; flex-direction:column; \
         color:var(--foreground); \
         background:var(--background); \
         font-family:var(--font-sans, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, sans-serif); \
         font-size:13px; \
         user-select:none; position:relative;";
    let _spacing_root = t.spacing_root;
    let _spacing_section = t.spacing_section;
    let _spacing_tight = t.spacing_tight;
    let _border = t.border;
    let _shadow_subtle = t.shadow_subtle;
    let _font_size_title = t.font_size_title;
    let _text_bright = t.text_bright;
    let _text_dim = t.text_dim;
    let style_label = t.style_label();
    let _style_inset = t.style_inset();
    let _style_card = t.style_card();
    let _style_value = t.style_value();

    // Frame counter sneaks into a `data-frame` attribute on the root —
    // the DOM mutation forces blitz to consider the document dirty
    // every render, which in turn forces a window.request_redraw() and
    // the canvas paint source to fire. Without this, idle redraws
    // collapse to 1–2/s because identical re-renders produce no diff.
    // The root has fixed `100vw × 100vh` so per-frame attribute writes
    // here don't trigger layout recompute (unlike when this lived on
    // the EqGraph wrapper).
    let frame_counter = *app_tick.read();

    rsx! {
        document::Style { {base_css} }

        DragProvider {
        div {
            style: format!("{root_style} overflow:hidden;"),
            "data-frame": "{frame_counter}",

            // ── Header ───────────────────────────────────────────
            //
            // Theme-driven: background `--card`, border `--border`, text
            // `--foreground` / `--muted-foreground`. Tailwind utilities
            // come from the fts-ui scan paths configured in tailwind.css.
            if SHOW_CHROME { // header (parked: graph-only editor, issue #31)
            div {
                class: "flex flex-wrap justify-between items-center gap-2 px-4 py-3 border-b border-border bg-card/50 backdrop-blur-sm",
                div { class: "flex items-baseline gap-3 shrink-0",
                    div {
                        class: "text-base font-bold tracking-wide text-foreground",
                        "FTS EQ"
                    }
                    div {
                        class: "text-xs text-muted-foreground uppercase tracking-wider",
                        if hardware_mode_active {
                            "{model_label(current_model)} profile active"
                        } else {
                            "{active_count} bands active"
                        }
                    }
                }
                div { class: "flex items-center gap-3 shrink-0",
                    // FPS readout — colour-coded vs the 60Hz target so a
                    // glance tells you whether the pipeline is hitting it.
                    {
                        let (label, color_class) = fps_status(fps_value);
                        rsx! {
                            div {
                                class: "flex items-baseline gap-1 text-xs font-mono tabular-nums",
                                title: "Frame rate (rolling 60-frame window)".to_string(),
                                span { class: "text-muted-foreground", "FPS" }
                                span { class: "{color_class} font-semibold", "{label}" }
                            }
                        }
                    }
                    div {
                        class: "text-xs text-muted-foreground tracking-wider",
                        "FastTrackStudio"
                    }
                }
            }
            }

            // ── Main EQ graph + inspector ─────────────────────────
            div {
                class: "flex-1 min-h-0 flex gap-1.5 px-1.5 py-1",

                div {
                    class: "flex-1 min-w-0 relative rounded-md border border-border bg-card/30 overflow-hidden",
                    style: "background-image:linear-gradient(180deg, color-mix(in oklab, var(--card) 30%, transparent) 0%, color-mix(in oklab, var(--background) 60%, transparent) 100%);",
                    EqGraph {
                        bands: graph_bands_signal,
                        db_range: db_range,
                        sample_rate: sample_rate,
                        spectrum_db: spectrum,
                        analyzer_snapshot: analyzer_snapshot,
                        model_response_db: model_response,
                        focused_band_out: focused_band,
                        overlay_sel: overlay_sel,
                        disabled: hardware_mode_active,

                        on_band_change: {
                            let ctx = ctx.clone();
                            let params = ui.params.clone();
                            move |(idx, band): (usize, EqBand)| {
                                if idx < NUM_BANDS {
                                    let bp = &params.bands[idx];

                                    ctx.begin_set_raw(bp.freq_hz.as_ptr());
                                    ctx.set_normalized_raw(
                                        bp.freq_hz.as_ptr(),
                                        bp.freq_hz.preview_normalized(band.frequency),
                                    );
                                    ctx.end_set_raw(bp.freq_hz.as_ptr());

                                    ctx.begin_set_raw(bp.gain_db.as_ptr());
                                    ctx.set_normalized_raw(
                                        bp.gain_db.as_ptr(),
                                        bp.gain_db.preview_normalized(band.gain),
                                    );
                                    ctx.end_set_raw(bp.gain_db.as_ptr());

                                    ctx.begin_set_raw(bp.q.as_ptr());
                                    ctx.set_normalized_raw(
                                        bp.q.as_ptr(),
                                        bp.q.preview_normalized(band.q),
                                    );
                                    ctx.end_set_raw(bp.q.as_ptr());

                                    let shape_int = shape_to_int(band.shape);
                                    ctx.begin_set_raw(bp.filter_type.as_ptr());
                                    ctx.set_normalized_raw(
                                        bp.filter_type.as_ptr(),
                                        bp.filter_type.preview_normalized(shape_int),
                                    );
                                    ctx.end_set_raw(bp.filter_type.as_ptr());

                                    let enabled_val = if band.enabled { 1.0_f32 } else { 0.0 };
                                    ctx.begin_set_raw(bp.enabled.as_ptr());
                                    ctx.set_normalized_raw(
                                        bp.enabled.as_ptr(),
                                        bp.enabled.preview_normalized(enabled_val),
                                    );
                                    ctx.end_set_raw(bp.enabled.as_ptr());

                                    let solo_val = if band.solo { 1.0_f32 } else { 0.0 };
                                    ctx.begin_set_raw(bp.solo.as_ptr());
                                    ctx.set_normalized_raw(
                                        bp.solo.as_ptr(),
                                        bp.solo.preview_normalized(solo_val),
                                    );
                                    ctx.end_set_raw(bp.solo.as_ptr());
                                }
                            }
                        },

                        on_band_add: {
                            let ctx = ctx.clone();
                            let params = ui.params.clone();
                            move |band: EqBand| {
                                let idx = band.index;
                                if idx < NUM_BANDS {
                                    let bp = &params.bands[idx];

                                    ctx.begin_set_raw(bp.enabled.as_ptr());
                                    ctx.set_normalized_raw(bp.enabled.as_ptr(), 1.0);
                                    ctx.end_set_raw(bp.enabled.as_ptr());

                                    ctx.begin_set_raw(bp.freq_hz.as_ptr());
                                    ctx.set_normalized_raw(
                                        bp.freq_hz.as_ptr(),
                                        bp.freq_hz.preview_normalized(band.frequency),
                                    );
                                    ctx.end_set_raw(bp.freq_hz.as_ptr());

                                    ctx.begin_set_raw(bp.gain_db.as_ptr());
                                    ctx.set_normalized_raw(
                                        bp.gain_db.as_ptr(),
                                        bp.gain_db.preview_normalized(band.gain),
                                    );
                                    ctx.end_set_raw(bp.gain_db.as_ptr());

                                    ctx.begin_set_raw(bp.q.as_ptr());
                                    ctx.set_normalized_raw(
                                        bp.q.as_ptr(),
                                        bp.q.preview_normalized(band.q),
                                    );
                                    ctx.end_set_raw(bp.q.as_ptr());

                                    let shape_int = shape_to_int(band.shape);
                                    ctx.begin_set_raw(bp.filter_type.as_ptr());
                                    ctx.set_normalized_raw(
                                        bp.filter_type.as_ptr(),
                                        bp.filter_type.preview_normalized(shape_int),
                                    );
                                    ctx.end_set_raw(bp.filter_type.as_ptr());
                                }
                            }
                        },

                        on_band_remove: {
                            let ctx = ctx.clone();
                            let params = ui.params.clone();
                            move |idx: usize| {
                                if idx < NUM_BANDS {
                                    let bp = &params.bands[idx];
                                    ctx.begin_set_raw(bp.enabled.as_ptr());
                                    ctx.set_normalized_raw(bp.enabled.as_ptr(), 0.0);
                                    ctx.end_set_raw(bp.enabled.as_ptr());

                                    ctx.begin_set_raw(bp.gain_db.as_ptr());
                                    ctx.set_normalized_raw(
                                        bp.gain_db.as_ptr(),
                                        bp.gain_db.preview_normalized(0.0),
                                    );
                                    ctx.end_set_raw(bp.gain_db.as_ptr());
                                }
                            }
                        },
                    }
                }

                if SHOW_CHROME { // inspector (parked: graph-only editor, issue #31)
                Card {
                    class: "w-[310px] min-w-[290px] max-w-[34vw] overflow-hidden rounded-md flex flex-col".to_string(),
                    CardHeader { class: "p-3 pb-2".to_string(),
                        div { class: "flex items-center justify-between gap-2",
                            CardTitle { class: "text-xs uppercase tracking-wider".to_string(), "Inspector" }
                            div { class: "text-[10px] text-muted-foreground tabular-nums", "{active_count}/{NUM_BANDS}" }
                        }
                        // Instrument cheat-sheet profile — manual select. `Auto`
                        // follows the host track name; drives the same selection
                        // the graph overlay uses (shared signal). Uses the fts_ui
                        // Select (Blitz doesn't render native <select>).
                        div { class: "flex items-center gap-2 mt-2",
                            div { class: "text-[10px] uppercase tracking-wider text-muted-foreground shrink-0", "Profile" }
                            div { class: "flex-1 min-w-0",
                                Select {
                                    value: profile_sig,
                                    placeholder: "Profile".to_string(),
                                    on_change: move |v: String| {
                                        let sel = match v.as_str() {
                                            "auto" => OverlayChoice::Auto,
                                            "off" => OverlayChoice::Off,
                                            other => other
                                                .parse::<usize>()
                                                .ok()
                                                .and_then(|i| crate::cheatsheet::PROFILES.get(i))
                                                .map(OverlayChoice::Pick)
                                                .unwrap_or(OverlayChoice::Off),
                                        };
                                        overlay_sel.set(sel);
                                    },
                                    SelectContent {
                                        SelectItem { value: "auto".to_string(), index: 0, "Auto (from track)" }
                                        SelectItem { value: "off".to_string(), index: 1, "Off" }
                                        for (i, p) in crate::cheatsheet::PROFILES.iter().enumerate() {
                                            SelectItem { value: "{i}", index: i + 2, "{p.name}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    CardContent { class: "p-3 pt-0 min-h-0 flex-1 overflow-hidden".to_string(),
                        Tabs {
                            value: Some(inspector_tab()),
                            on_change: move |v: String| inspector_tab.set(v),
                            default_value: "band".to_string(),
                            class: "h-full min-h-0".to_string(),
                            TabList { class: "w-full justify-start".to_string(),
                                TabTrigger { value: "band".to_string(), index: 0, class: "text-xs".to_string(), "Band" }
                                TabTrigger { value: "analyzer".to_string(), index: 1, class: "text-xs".to_string(), "Analyzer" }
                                TabTrigger { value: "profile".to_string(), index: 2, class: "text-xs".to_string(), "Profile" }
                            }

                            TabContent { value: "band".to_string(), index: 0, class: "min-h-0".to_string(),
                                {
                                    let focus_idx = *focused_band.read();
                                    if let Some(idx) = focus_idx {
                                        let bp = &params.bands[idx];
                                        let freq = bp.freq_hz.value();
                                        let gain = bp.gain_db.value();
                                        let q = bp.q.value();
                                        let slope = bp.slope.value();
                                        let shape = int_to_shape(bp.filter_type.value());
                                        let enabled = bp.enabled.value() > 0.5;
                                        let solo = bp.solo.value() > 0.5;
                                        let color = crate::eq_graph_model::freq_to_color(freq as f64);
                                        let freq_str = format_freq(freq);
                                        let current_shape_value = shape_to_int(shape).to_string();
                                        let mut shape_value_sig = use_signal(|| current_shape_value.clone());
                                        if *shape_value_sig.read() != current_shape_value {
                                            shape_value_sig.set(current_shape_value);
                                        }
                                        let mut enabled_sig = use_signal(|| enabled);
                                        if *enabled_sig.read() != enabled {
                                            enabled_sig.set(enabled);
                                        }
                                        let mut solo_value = use_signal(|| if solo { "solo".to_string() } else { "normal".to_string() });
                                        let desired_solo = if solo { "solo".to_string() } else { "normal".to_string() };
                                        if *solo_value.read() != desired_solo {
                                            solo_value.set(desired_solo);
                                        }
                                        let slope_value = slope.to_string();
                                        let mut slope_sig = use_signal(|| slope_value.clone());
                                        if *slope_sig.read() != slope_value {
                                            slope_sig.set(slope_value);
                                        }

                                        // Persisted band annotations (name + notes). Local
                                        // signals seeded from the params, re-synced if the
                                        // persisted value changes externally (e.g. preset load).
                                        let name_now = bp.name.read().clone();
                                        let mut name_sig = use_signal(|| name_now.clone());
                                        if *name_sig.read() != name_now {
                                            name_sig.set(name_now.clone());
                                        }
                                        let notes_now = bp.notes.read().clone();
                                        let mut notes_sig = use_signal(|| notes_now.clone());
                                        if *notes_sig.read() != notes_now {
                                            notes_sig.set(notes_now.clone());
                                        }

                                        let shape_options: Vec<(usize, String, String)> = EqBandShape::all()
                                            .iter()
                                            .enumerate()
                                            .map(|(i, sh)| {
                                                (i, shape_to_int(*sh).to_string(), sh.label().to_string())
                                            })
                                            .collect();

                                        rsx! {
                                            div { class: "flex flex-col gap-3 pt-3",
                                                div {
                                                    class: "rounded-md border border-border bg-background/40 p-3",
                                                    style: format!("border-left:3px solid {color};"),
                                                    div { class: "flex items-center justify-between gap-2",
                                                        div {
                                                            div { class: "text-[10px] uppercase tracking-wider text-muted-foreground", "Selected band" }
                                                            div { class: "text-lg font-semibold tabular-nums", "Band {idx + 1}" }
                                                        }
                                                        div { class: "text-right",
                                                            div { class: "text-xs font-semibold tabular-nums", "{freq_str} Hz" }
                                                            div { class: "text-xs text-muted-foreground tabular-nums", "{gain:+.1} dB / Q {q:.2}" }
                                                        }
                                                    }
                                                }

                                                // Band annotations — name + notes. Persisted
                                                // metadata (not automatable params): written
                                                // straight to the RwLocks, no host gesture.
                                                div { class: "rounded-md border border-border bg-muted/20 p-2 flex flex-col gap-2",
                                                    div { class: "text-[10px] uppercase tracking-wider text-muted-foreground", "Label" }
                                                    input {
                                                        r#type: "text",
                                                        class: "w-full bg-background/60 border border-border rounded px-2 py-1 text-sm",
                                                        placeholder: "Band name — e.g. Honk, Air, Mud…",
                                                        value: name_sig(),
                                                        oninput: {
                                                            let params = ui.params.clone();
                                                            let ctx_n = ctx.clone();
                                                            move |evt: FormEvent| {
                                                                let v = evt.value();
                                                                *params.bands[idx].name.write() = v.clone();
                                                                name_sig.set(v);
                                                                ctx_n.request_redraw();
                                                            }
                                                        },
                                                    }
                                                    textarea {
                                                        class: "w-full bg-background/60 border border-border rounded px-2 py-1 text-xs resize-none",
                                                        rows: "3",
                                                        placeholder: "Notes — why this move? e.g. basketball honk cut on the kick…",
                                                        value: notes_sig(),
                                                        oninput: {
                                                            let params = ui.params.clone();
                                                            let ctx_n = ctx.clone();
                                                            move |evt: FormEvent| {
                                                                let v = evt.value();
                                                                *params.bands[idx].notes.write() = v.clone();
                                                                notes_sig.set(v);
                                                                ctx_n.request_redraw();
                                                            }
                                                        },
                                                    }
                                                }

                                                // Ear-training context for this band's frequency —
                                                // its ISO-octave cue + what too much / too little
                                                // sounds like. Helps decide the move by ear.
                                                {
                                                    if let Some(eb) = crate::cheatsheet::ear_band_for(freq) {
                                                        rsx! {
                                                            div { class: "rounded-md border border-border bg-muted/20 p-2 text-[10px] leading-snug",
                                                                div {
                                                                    span { class: "font-semibold text-foreground", "{format_freq(eb.center_hz)} Hz · \"{eb.cue}\"" }
                                                                }
                                                                div { class: "mt-1",
                                                                    span { style: "color:#f08a8a;", "▲ too much: " }
                                                                    span { class: "text-muted-foreground", "{eb.too_much}" }
                                                                }
                                                                div {
                                                                    span { style: "color:#7fd6a0;", "▼ too little: " }
                                                                    span { class: "text-muted-foreground", "{eb.too_little}" }
                                                                }
                                                            }
                                                        }
                                                    } else {
                                                        rsx! {}
                                                    }
                                                }

                                                div { class: "grid grid-cols-2 gap-2",
                                                    div { class: "rounded-md border border-border bg-muted/20 p-2",
                                                        div { class: "text-[10px] uppercase tracking-wider text-muted-foreground mb-2", "Bypass" }
                                                        Switch {
                                                            checked: enabled_sig,
                                                            on_change: {
                                                                let ctx_en = ctx.clone();
                                                                let enabled_ptr = bp.enabled.as_ptr();
                                                                move |on: bool| {
                                                                    ctx_en.begin_set_raw(enabled_ptr);
                                                                    ctx_en.set_normalized_raw(enabled_ptr, if on { 1.0 } else { 0.0 });
                                                                    ctx_en.end_set_raw(enabled_ptr);
                                                                }
                                                            }
                                                        }
                                                    }
                                                    div { class: "rounded-md border border-border bg-muted/20 p-2",
                                                        div { class: "text-[10px] uppercase tracking-wider text-muted-foreground mb-2", "Monitor" }
                                                        SegmentedControl {
                                                            value: solo_value(),
                                                            size: SegmentedControlSize::Small,
                                                            options: vec![
                                                                ("normal".to_string(), "Normal".to_string()),
                                                                ("solo".to_string(), "Solo".to_string()),
                                                            ],
                                                            on_change: {
                                                                let ctx_solo = ctx.clone();
                                                                let solo_ptr = bp.solo.as_ptr();
                                                                move |value: String| {
                                                                    ctx_solo.begin_set_raw(solo_ptr);
                                                                    ctx_solo.set_normalized_raw(solo_ptr, if value == "solo" { 1.0 } else { 0.0 });
                                                                    ctx_solo.end_set_raw(solo_ptr);
                                                                }
                                                            },
                                                        }
                                                    }
                                                }

                                                div { class: "flex flex-col gap-2",
                                                    div {
                                                        Select {
                                                            value: shape_value_sig,
                                                            placeholder: "Filter".to_string(),
                                                            on_change: {
                                                                let ctx_ft = ctx.clone();
                                                                let ft_ptr = bp.filter_type.as_ptr();
                                                                move |v: String| {
                                                                    let Ok(idx) = v.parse::<i32>() else { return };
                                                                    ctx_ft.begin_set_raw(ft_ptr);
                                                                    ctx_ft.set_normalized_raw(ft_ptr, idx as f32 / 9.0);
                                                                    ctx_ft.end_set_raw(ft_ptr);
                                                                }
                                                            },
                                                            SelectContent {
                                                                for (idx, value, label) in shape_options {
                                                                    SelectItem { value, index: idx, "{label}" }
                                                                }
                                                            }
                                                        }
                                                    }

                                                    div { class: "grid grid-cols-3 gap-2",
                                                        InspectorKnob { label: "Freq".to_string(), value: freq_str, handle: param_handle(bp.freq_hz.as_ptr(), ctx.clone()) }
                                                        InspectorKnob { label: "Gain".to_string(), value: format!("{gain:+.1}"), handle: param_handle(bp.gain_db.as_ptr(), ctx.clone()) }
                                                        InspectorKnob { label: "Q".to_string(), value: format!("{q:.2}"), handle: param_handle(bp.q.as_ptr(), ctx.clone()) }
                                                    }

                                                    div { class: "rounded-md border border-border bg-muted/20 p-2",
                                                        div { class: "flex items-center justify-between mb-2",
                                                            div { class: "text-[10px] uppercase tracking-wider text-muted-foreground", "Slope" }
                                                            div { class: "text-xs tabular-nums text-foreground", "{slope_label(slope)}" }
                                                        }
                                                        SegmentedControl {
                                                            value: slope_sig(),
                                                            size: SegmentedControlSize::Small,
                                                            class: "flex-wrap justify-start".to_string(),
                                                            options: slope_options(),
                                                            on_change: {
                                                                let ctx_slope = ctx.clone();
                                                                let slope_ptr = bp.slope.as_ptr();
                                                                move |value: String| {
                                                                    let Ok(idx) = value.parse::<i32>() else { return };
                                                                    ctx_slope.begin_set_raw(slope_ptr);
                                                                    ctx_slope.set_normalized_raw(slope_ptr, idx as f32 / 10.0);
                                                                    ctx_slope.end_set_raw(slope_ptr);
                                                                }
                                                            },
                                                        }
                                                    }

                                                    Button {
                                                        variant: ButtonVariant::Destructive,
                                                        size: ButtonSize::Small,
                                                        on_click: {
                                                            let ctx_del = ctx.clone();
                                                            let enabled_ptr = bp.enabled.as_ptr();
                                                            let gain_ptr = bp.gain_db.as_ptr();
                                                            move |_| {
                                                                ctx_del.begin_set_raw(enabled_ptr);
                                                                ctx_del.set_normalized_raw(enabled_ptr, 0.0);
                                                                ctx_del.end_set_raw(enabled_ptr);
                                                                ctx_del.begin_set_raw(gain_ptr);
                                                                ctx_del.set_normalized_raw(gain_ptr, 0.5);
                                                                ctx_del.end_set_raw(gain_ptr);
                                                            }
                                                        },
                                                        "Delete Band"
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        rsx! {
                                            div { class: "pt-3",
                                                div { class: "rounded-md border border-dashed border-border bg-muted/20 p-4 text-sm text-muted-foreground",
                                                    "Select or add a band to edit frequency, gain, Q, slope, type, bypass, and solo."
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            TabContent { value: "analyzer".to_string(), index: 1,
                                div { class: "flex flex-col gap-3 pt-3",
                                    div { class: "grid grid-cols-2 gap-2",
                                        AnalyzerStat { label: "Input".to_string(), value: format!("{input_db:.1} dB") }
                                        AnalyzerStat { label: "Output".to_string(), value: format!("{output_db:.1} dB") }
                                        AnalyzerStat { label: "Rate".to_string(), value: format!("{sample_rate:.0} Hz") }
                                        AnalyzerStat { label: "Bins".to_string(), value: format!("{SPECTRUM_BINS}") }
                                    }
                                    // Full Pro-Q 4-style analyzer controls.
                                    div { class: "rounded-md border border-border bg-muted/20",
                                        AnalyzerSettingsPanel {
                                            settings: analyzer_settings(),
                                            on_change: move |s: AnalyzerSettings| {
                                                analyzer_for_settings.analyzer.set_settings(s);
                                                analyzer_settings.set(s);
                                            },
                                        }
                                    }
                                    div { class: "rounded-md border border-border bg-muted/20 p-3",
                                        div { class: "text-[10px] uppercase tracking-wider text-muted-foreground mb-2", "Display Range" }
                                        SegmentedControl {
                                            value: db_range_idx.to_string(),
                                            size: SegmentedControlSize::Small,
                                            class: "flex-wrap justify-start".to_string(),
                                            options: vec![
                                                ("0".to_string(), "3".to_string()),
                                                ("1".to_string(), "6".to_string()),
                                                ("2".to_string(), "12".to_string()),
                                                ("3".to_string(), "18".to_string()),
                                                ("4".to_string(), "24".to_string()),
                                                ("5".to_string(), "30".to_string()),
                                            ],
                                            on_change: {
                                                let ctx_range = ctx.clone();
                                                let db_range_param = params.db_range.as_ptr();
                                                move |value: String| {
                                                    let Ok(idx) = value.parse::<i32>() else { return };
                                                    ctx_range.begin_set_raw(db_range_param);
                                                    ctx_range.set_normalized_raw(db_range_param, idx as f32 / 5.0);
                                                    ctx_range.end_set_raw(db_range_param);
                                                }
                                            },
                                        }
                                    }
                                    div { class: "grid grid-cols-2 gap-2",
                                        InspectorKnob { label: "Scale".to_string(), value: format!("{:.0}%", params.gain_scale.value()), handle: param_handle(params.gain_scale.as_ptr(), ctx.clone()) }
                                        InspectorKnob { label: "Output".to_string(), value: format!("{:.1}", params.output_gain_db.value()), handle: param_handle(params.output_gain_db.as_ptr(), ctx.clone()) }
                                    }
                                }
                            }

                            TabContent { value: "profile".to_string(), index: 2,
                                div { class: "flex flex-col gap-3 pt-3",
                                    SegmentedControl {
                                        value: model_value(params.model.value()),
                                        size: SegmentedControlSize::Small,
                                        options: vec![
                                            ("default".to_string(), "Default".to_string()),
                                            ("pultec".to_string(), "Pultec".to_string()),
                                            ("neve".to_string(), "Neve".to_string()),
                                            ("api".to_string(), "API".to_string()),
                                            ("ssl_e".to_string(), "SSL E".to_string()),
                                            ("ssl_g".to_string(), "SSL G".to_string()),
                                        ],
                                        on_change: {
                                            let ctx_model = ctx.clone();
                                            let model_ptr = params.model.as_ptr();
                                            move |value: String| {
                                                ctx_model.begin_set_raw(model_ptr);
                                                ctx_model.set_normalized_raw(model_ptr, model_normalized(&value));
                                                ctx_model.end_set_raw(model_ptr);
                                            }
                                        },
                                    }
                                    SegmentedControl {
                                        value: profile_tab(),
                                        size: SegmentedControlSize::Small,
                                        options: vec![
                                            ("neve".to_string(), "Neve 1073".to_string()),
                                            ("pultec".to_string(), "Pultec".to_string()),
                                            ("api".to_string(), "API 550A".to_string()),
                                            ("ssl_e".to_string(), "SSL E".to_string()),
                                            ("ssl_g".to_string(), "SSL G".to_string()),
                                        ],
                                        on_change: move |value: String| profile_tab.set(value),
                                    }
                                    match profile_tab().as_str() {
                                        "neve" => rsx! {
                                            Neve1073ProfileView {}
                                            Neve1073Controls {}
                                        },
                                        "pultec" => rsx! {
                                            PultecProfileView {}
                                            PultecControls {}
                                        },
                                        "api" => rsx! {
                                            Api550aProfileView {}
                                            ApiControls {}
                                        },
                                        "ssl_e" => rsx! {
                                            SslEProfileView {}
                                            SslControls { e_series: true }
                                        },
                                        "ssl_g" => rsx! {
                                            SslGProfileView {}
                                            SslControls { e_series: false }
                                        },
                                        _ => rsx! {
                                            Card {
                                                class: "bg-background/40".to_string(),
                                                CardContent {
                                                    class: "p-3 text-xs text-muted-foreground".to_string(),
                                                    "Default is the full modern parametric EQ mode."
                                                }
                                            }
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
                }
            }

            // ── Bottom bar: meters + output gain ──────────────────
            //
            // `flex-wrap` lets the strip reflow into multiple rows when
            // the window is narrow, instead of clipping or overflowing.
            // Every panel is `shrink-0` so it stays whole if it can't
            // fit on the current row.
            if SHOW_CHROME { // bottom bar (parked: graph-only editor, issue #31)
            div {
                class: "flex flex-wrap items-center gap-4 px-4 py-3 border-t border-border bg-card/40",
                LevelMeterDb { level_db: input_db, label: "IN".to_string(), height: 40.0 }
                LevelMeterDb { level_db: output_db, label: "OUT".to_string(), height: 40.0 }

                div { style: "flex:1;" }

                {
                    let db_range_param = params.db_range.as_ptr();
                    rsx! {
                        div { class: "flex items-center gap-2 shrink-0",
                            span { class: "text-[10px] uppercase tracking-wider text-muted-foreground",
                                "Range"
                            }
                            SegmentedControl {
                                value: db_range_idx.to_string(),
                                size: SegmentedControlSize::Small,
                                options: vec![
                                    ("0".to_string(), "3 dB".to_string()),
                                    ("1".to_string(), "6 dB".to_string()),
                                    ("2".to_string(), "12 dB".to_string()),
                                    ("3".to_string(), "18 dB".to_string()),
                                    ("4".to_string(), "24 dB".to_string()),
                                    ("5".to_string(), "30 dB".to_string()),
                                ],
                                on_change: {
                                    let ctx = ctx.clone();
                                    move |value: String| {
                                        let Ok(idx) = value.parse::<i32>() else { return };
                                        ctx.begin_set_raw(db_range_param);
                                        ctx.set_normalized_raw(db_range_param, idx as f32 / 5.0);
                                        ctx.end_set_raw(db_range_param);
                                    }
                                },
                            }
                        }
                    }
                }
                div {
                    style: "display:flex; align-items:center; gap:8px;",
                    span { style: format!("{style_label}"), "Scale" }
                    Knob {
                        handle: param_handle(params.gain_scale.as_ptr(), ctx.clone()),
                        size: KnobSize::Small,
                    }
                }

                div {
                    style: "display:flex; align-items:center; gap:8px;",
                    span { style: format!("{style_label}"), "Output" }
                    Knob {
                        handle: param_handle(params.output_gain_db.as_ptr(), ctx.clone()),
                        size: KnobSize::Small,
                    }
                }
            }
            }
            ResizeHandle {
                min_width: 600,
                min_height: 400,
            }
        }
        } // DragProvider
    }
}

fn model_value(model: i32) -> String {
    match model {
        1 => "pultec",
        2 => "neve",
        3 => "api",
        4 => "ssl_e",
        5 => "ssl_g",
        _ => "default",
    }
    .to_string()
}

fn model_label(model: i32) -> &'static str {
    match model {
        1 => "Pultec",
        2 => "Neve 1073",
        3 => "API 550A",
        4 => "SSL E",
        5 => "SSL G",
        _ => "Default",
    }
}

fn model_normalized(value: &str) -> f32 {
    let model = match value {
        "pultec" => 1.0,
        "neve" => 2.0,
        "api" => 3.0,
        "ssl_e" => 4.0,
        "ssl_g" => 5.0,
        _ => 0.0,
    };
    model / 5.0
}

#[component]
fn PultecControls() -> Element {
    let shared = use_context::<SharedState>();
    let ui = shared.get::<EqUiState>().expect("EqUiState missing");
    let ctx = use_param_context();
    let params = &ui.params;

    let low = params.pultec_low_freq.value().to_string();
    let mut low_sig = use_signal(|| low.clone());
    if *low_sig.read() != low {
        low_sig.set(low);
    }
    let high_boost = params.pultec_high_boost_freq.value().to_string();
    let mut high_boost_sig = use_signal(|| high_boost.clone());
    if *high_boost_sig.read() != high_boost {
        high_boost_sig.set(high_boost);
    }
    let high_atten = params.pultec_high_atten_freq.value().to_string();
    let mut high_atten_sig = use_signal(|| high_atten.clone());
    if *high_atten_sig.read() != high_atten {
        high_atten_sig.set(high_atten);
    }

    rsx! {
        HardwareControlsCard { title: "Pultec EQP-1A DSP".to_string(), eq_ptr: params.pultec_eq_in.as_ptr(), eq_on: params.pultec_eq_in.value() > 0.5,
            ProfileSelect {
                label: "Low Freq".to_string(),
                value: low_sig,
                max_index: 3,
                ptr: params.pultec_low_freq.as_ptr(),
                options: vec![("0".to_string(), "20".to_string()), ("1".to_string(), "30".to_string()), ("2".to_string(), "60".to_string()), ("3".to_string(), "100".to_string())],
            }
            div { class: "grid grid-cols-2 gap-2",
                InspectorKnob { label: "Low Boost".to_string(), value: format!("{:+.1}", params.pultec_low_boost_db.value()), handle: param_handle(params.pultec_low_boost_db.as_ptr(), ctx.clone()) }
                InspectorKnob { label: "Low Atten".to_string(), value: format!("{:+.1}", params.pultec_low_atten_db.value()), handle: param_handle(params.pultec_low_atten_db.as_ptr(), ctx.clone()) }
            }
            ProfileSelect {
                label: "High Boost".to_string(),
                value: high_boost_sig,
                max_index: 6,
                ptr: params.pultec_high_boost_freq.as_ptr(),
                options: vec![("0".to_string(), "3k".to_string()), ("1".to_string(), "4k".to_string()), ("2".to_string(), "5k".to_string()), ("3".to_string(), "8k".to_string()), ("4".to_string(), "10k".to_string()), ("5".to_string(), "12k".to_string()), ("6".to_string(), "16k".to_string())],
            }
            div { class: "grid grid-cols-2 gap-2",
                InspectorKnob { label: "High Boost".to_string(), value: format!("{:+.1}", params.pultec_high_boost_db.value()), handle: param_handle(params.pultec_high_boost_db.as_ptr(), ctx.clone()) }
                InspectorKnob { label: "Bandwidth".to_string(), value: format!("{:.1}", params.pultec_bandwidth.value()), handle: param_handle(params.pultec_bandwidth.as_ptr(), ctx.clone()) }
            }
            ProfileSelect {
                label: "High Atten".to_string(),
                value: high_atten_sig,
                max_index: 2,
                ptr: params.pultec_high_atten_freq.as_ptr(),
                options: vec![("0".to_string(), "5k".to_string()), ("1".to_string(), "10k".to_string()), ("2".to_string(), "20k".to_string())],
            }
            div { class: "grid grid-cols-3 gap-2",
                InspectorKnob { label: "Atten".to_string(), value: format!("{:+.1}", params.pultec_high_atten_db.value()), handle: param_handle(params.pultec_high_atten_db.as_ptr(), ctx.clone()) }
                InspectorKnob { label: "Drive".to_string(), value: format!("{:.0}%", params.pultec_drive.value()), handle: param_handle(params.pultec_drive.as_ptr(), ctx.clone()) }
                InspectorKnob { label: "Trim".to_string(), value: format!("{:+.1}", params.pultec_trim_db.value()), handle: param_handle(params.pultec_trim_db.as_ptr(), ctx.clone()) }
            }
        }
    }
}

#[component]
fn ApiControls() -> Element {
    let shared = use_context::<SharedState>();
    let ui = shared.get::<EqUiState>().expect("EqUiState missing");
    let ctx = use_param_context();
    let params = &ui.params;

    let low = params.api_low_freq.value().to_string();
    let mid = params.api_mid_freq.value().to_string();
    let high = params.api_high_freq.value().to_string();
    let mut low_sig = use_signal(|| low.clone());
    let mut mid_sig = use_signal(|| mid.clone());
    let mut high_sig = use_signal(|| high.clone());
    if *low_sig.read() != low {
        low_sig.set(low);
    }
    if *mid_sig.read() != mid {
        mid_sig.set(mid);
    }
    if *high_sig.read() != high {
        high_sig.set(high);
    }

    rsx! {
        HardwareControlsCard { title: "API 550A DSP".to_string(), eq_ptr: params.api_eq_in.as_ptr(), eq_on: params.api_eq_in.value() > 0.5,
            ProfileSelect { label: "Low".to_string(), value: low_sig, max_index: 3, ptr: params.api_low_freq.as_ptr(), options: vec![("0".to_string(), "50".to_string()), ("1".to_string(), "100".to_string()), ("2".to_string(), "200".to_string()), ("3".to_string(), "400".to_string())] }
            InspectorKnob { label: "Low Gain".to_string(), value: format!("{:+.1}", params.api_low_gain_db.value()), handle: param_handle(params.api_low_gain_db.as_ptr(), ctx.clone()) }
            ProfileSelect { label: "Mid".to_string(), value: mid_sig, max_index: 4, ptr: params.api_mid_freq.as_ptr(), options: vec![("0".to_string(), "400".to_string()), ("1".to_string(), "800".to_string()), ("2".to_string(), "1.5k".to_string()), ("3".to_string(), "3k".to_string()), ("4".to_string(), "5k".to_string())] }
            InspectorKnob { label: "Mid Gain".to_string(), value: format!("{:+.1}", params.api_mid_gain_db.value()), handle: param_handle(params.api_mid_gain_db.as_ptr(), ctx.clone()) }
            ProfileSelect { label: "High".to_string(), value: high_sig, max_index: 4, ptr: params.api_high_freq.as_ptr(), options: vec![("0".to_string(), "5k".to_string()), ("1".to_string(), "7k".to_string()), ("2".to_string(), "10k".to_string()), ("3".to_string(), "12.5k".to_string()), ("4".to_string(), "15k".to_string())] }
            InspectorKnob { label: "High Gain".to_string(), value: format!("{:+.1}", params.api_high_gain_db.value()), handle: param_handle(params.api_high_gain_db.as_ptr(), ctx.clone()) }
            div { class: "grid grid-cols-2 gap-2",
                InspectorKnob { label: "Drive".to_string(), value: format!("{:.0}%", params.api_drive.value()), handle: param_handle(params.api_drive.as_ptr(), ctx.clone()) }
                InspectorKnob { label: "Trim".to_string(), value: format!("{:+.1}", params.api_trim_db.value()), handle: param_handle(params.api_trim_db.as_ptr(), ctx.clone()) }
            }
        }
    }
}

#[component]
fn SslControls(e_series: bool) -> Element {
    let shared = use_context::<SharedState>();
    let ui = shared.get::<EqUiState>().expect("EqUiState missing");
    let ctx = use_param_context();
    let params = &ui.params;
    let title = if e_series {
        "SSL E-Series DSP"
    } else {
        "SSL G-Series DSP"
    };

    rsx! {
        HardwareControlsCard { title: title.to_string(), eq_ptr: params.ssl_eq_in.as_ptr(), eq_on: params.ssl_eq_in.value() > 0.5,
            if e_series {
                div { class: "grid grid-cols-2 gap-2",
                    InspectorKnob { label: "HPF".to_string(), value: format_freq(params.ssl_hpf_hz.value()), handle: param_handle(params.ssl_hpf_hz.as_ptr(), ctx.clone()) }
                    InspectorKnob { label: "LPF".to_string(), value: format_freq(params.ssl_lpf_hz.value()), handle: param_handle(params.ssl_lpf_hz.as_ptr(), ctx.clone()) }
                }
            }
            div { class: "grid grid-cols-2 gap-2",
                InspectorKnob { label: "LF Freq".to_string(), value: format_freq(params.ssl_lf_freq_hz.value()), handle: param_handle(params.ssl_lf_freq_hz.as_ptr(), ctx.clone()) }
                InspectorKnob { label: "LF Gain".to_string(), value: format!("{:+.1}", params.ssl_lf_gain_db.value()), handle: param_handle(params.ssl_lf_gain_db.as_ptr(), ctx.clone()) }
                InspectorKnob { label: "LMF Freq".to_string(), value: format_freq(params.ssl_lmf_freq_hz.value()), handle: param_handle(params.ssl_lmf_freq_hz.as_ptr(), ctx.clone()) }
                InspectorKnob { label: "LMF Gain".to_string(), value: format!("{:+.1}", params.ssl_lmf_gain_db.value()), handle: param_handle(params.ssl_lmf_gain_db.as_ptr(), ctx.clone()) }
                InspectorKnob { label: "HMF Freq".to_string(), value: format_freq(params.ssl_hmf_freq_hz.value()), handle: param_handle(params.ssl_hmf_freq_hz.as_ptr(), ctx.clone()) }
                InspectorKnob { label: "HMF Gain".to_string(), value: format!("{:+.1}", params.ssl_hmf_gain_db.value()), handle: param_handle(params.ssl_hmf_gain_db.as_ptr(), ctx.clone()) }
                InspectorKnob { label: "HF Freq".to_string(), value: format_freq(params.ssl_hf_freq_hz.value()), handle: param_handle(params.ssl_hf_freq_hz.as_ptr(), ctx.clone()) }
                InspectorKnob { label: "HF Gain".to_string(), value: format!("{:+.1}", params.ssl_hf_gain_db.value()), handle: param_handle(params.ssl_hf_gain_db.as_ptr(), ctx.clone()) }
            }
            div { class: "grid grid-cols-2 gap-2",
                InspectorKnob { label: "Drive".to_string(), value: format!("{:.0}%", params.ssl_drive.value()), handle: param_handle(params.ssl_drive.as_ptr(), ctx.clone()) }
                InspectorKnob { label: "Trim".to_string(), value: format!("{:+.1}", params.ssl_trim_db.value()), handle: param_handle(params.ssl_trim_db.as_ptr(), ctx.clone()) }
            }
        }
    }
}

#[component]
fn HardwareControlsCard(
    title: String,
    eq_ptr: ParamPtr,
    eq_on: bool,
    children: Element,
) -> Element {
    let ctx = use_param_context();
    let mut eq_sig = use_signal(|| eq_on);
    if *eq_sig.read() != eq_on {
        eq_sig.set(eq_on);
    }

    rsx! {
        Card {
            class: "bg-background/40".to_string(),
            CardHeader {
                class: "pb-2".to_string(),
                CardTitle { class: "text-sm".to_string(), "{title}" }
            }
            CardContent {
                class: "flex flex-col gap-3".to_string(),
                div { class: "rounded-md border border-border bg-muted/20 p-2",
                    div { class: "text-[10px] uppercase tracking-wider text-muted-foreground mb-2", "EQ In" }
                    Switch {
                        checked: eq_sig,
                        on_change: move |on: bool| {
                            ctx.begin_set_raw(eq_ptr);
                            ctx.set_normalized_raw(eq_ptr, if on { 1.0 } else { 0.0 });
                            ctx.end_set_raw(eq_ptr);
                        },
                    }
                }
                {children}
            }
        }
    }
}

#[component]
fn Neve1073Controls() -> Element {
    let shared = use_context::<SharedState>();
    let ui = shared.get::<EqUiState>().expect("EqUiState missing");
    let ctx = use_param_context();
    let params = &ui.params;

    let hpf_value = params.neve_hpf.value().to_string();
    let mut hpf_sig = use_signal(|| hpf_value.clone());
    if *hpf_sig.read() != hpf_value {
        hpf_sig.set(hpf_value);
    }

    let low_freq_value = params.neve_low_freq.value().to_string();
    let mut low_freq_sig = use_signal(|| low_freq_value.clone());
    if *low_freq_sig.read() != low_freq_value {
        low_freq_sig.set(low_freq_value);
    }

    let mid_freq_value = params.neve_mid_freq.value().to_string();
    let mut mid_freq_sig = use_signal(|| mid_freq_value.clone());
    if *mid_freq_sig.read() != mid_freq_value {
        mid_freq_sig.set(mid_freq_value);
    }

    let eq_in = params.neve_eq_in.value() > 0.5;
    let mut eq_in_sig = use_signal(|| eq_in);
    if *eq_in_sig.read() != eq_in {
        eq_in_sig.set(eq_in);
    }

    let phase = params.neve_phase.value() > 0.5;
    let mut phase_sig = use_signal(|| phase);
    if *phase_sig.read() != phase {
        phase_sig.set(phase);
    }

    rsx! {
        Card {
            class: "bg-background/40".to_string(),
            CardHeader {
                class: "pb-2".to_string(),
                CardTitle { class: "text-sm".to_string(), "Neve 1073 DSP" }
            }
            CardContent {
                class: "flex flex-col gap-3".to_string(),
                div { class: "grid grid-cols-2 gap-2",
                    div { class: "rounded-md border border-border bg-muted/20 p-2",
                        div { class: "text-[10px] uppercase tracking-wider text-muted-foreground mb-2", "EQ In" }
                        Switch {
                            checked: eq_in_sig,
                            on_change: {
                                let ctx = ctx.clone();
                                let ptr = params.neve_eq_in.as_ptr();
                                move |on: bool| {
                                    ctx.begin_set_raw(ptr);
                                    ctx.set_normalized_raw(ptr, if on { 1.0 } else { 0.0 });
                                    ctx.end_set_raw(ptr);
                                }
                            },
                        }
                    }
                    div { class: "rounded-md border border-border bg-muted/20 p-2",
                        div { class: "text-[10px] uppercase tracking-wider text-muted-foreground mb-2", "Phase" }
                        Switch {
                            checked: phase_sig,
                            on_change: {
                                let ctx = ctx.clone();
                                let ptr = params.neve_phase.as_ptr();
                                move |on: bool| {
                                    ctx.begin_set_raw(ptr);
                                    ctx.set_normalized_raw(ptr, if on { 1.0 } else { 0.0 });
                                    ctx.end_set_raw(ptr);
                                }
                            },
                        }
                    }
                }

                div { class: "grid grid-cols-2 gap-2",
                    InspectorKnob { label: "Trim".to_string(), value: format!("{:+.1}", params.neve_trim_db.value()), handle: param_handle(params.neve_trim_db.as_ptr(), ctx.clone()) }
                    InspectorKnob { label: "Drive".to_string(), value: format!("{:.0}%", params.neve_drive.value()), handle: param_handle(params.neve_drive.as_ptr(), ctx.clone()) }
                }

                ProfileSelect {
                    label: "HPF".to_string(),
                    value: hpf_sig,
                    max_index: 4,
                    ptr: params.neve_hpf.as_ptr(),
                    options: vec![
                        ("0".to_string(), "Off".to_string()),
                        ("1".to_string(), "50".to_string()),
                        ("2".to_string(), "80".to_string()),
                        ("3".to_string(), "160".to_string()),
                        ("4".to_string(), "300".to_string()),
                    ],
                }

                ProfileSelect {
                    label: "Low".to_string(),
                    value: low_freq_sig,
                    max_index: 4,
                    ptr: params.neve_low_freq.as_ptr(),
                    options: vec![
                        ("0".to_string(), "Off".to_string()),
                        ("1".to_string(), "35".to_string()),
                        ("2".to_string(), "60".to_string()),
                        ("3".to_string(), "110".to_string()),
                        ("4".to_string(), "220".to_string()),
                    ],
                }
                InspectorKnob { label: "Low Gain".to_string(), value: format!("{:+.1}", params.neve_low_gain_db.value()), handle: param_handle(params.neve_low_gain_db.as_ptr(), ctx.clone()) }

                ProfileSelect {
                    label: "Mid".to_string(),
                    value: mid_freq_sig,
                    max_index: 6,
                    ptr: params.neve_mid_freq.as_ptr(),
                    options: vec![
                        ("0".to_string(), "Off".to_string()),
                        ("1".to_string(), "360".to_string()),
                        ("2".to_string(), "700".to_string()),
                        ("3".to_string(), "1.6k".to_string()),
                        ("4".to_string(), "3.2k".to_string()),
                        ("5".to_string(), "4.8k".to_string()),
                        ("6".to_string(), "7.2k".to_string()),
                    ],
                }
                InspectorKnob { label: "Mid Gain".to_string(), value: format!("{:+.1}", params.neve_mid_gain_db.value()), handle: param_handle(params.neve_mid_gain_db.as_ptr(), ctx.clone()) }
                InspectorKnob { label: "High".to_string(), value: format!("{:+.1}", params.neve_high_gain_db.value()), handle: param_handle(params.neve_high_gain_db.as_ptr(), ctx.clone()) }
            }
        }
    }
}

#[component]
fn ProfileSelect(
    label: String,
    value: Signal<String>,
    max_index: i32,
    ptr: ParamPtr,
    options: Vec<(String, String)>,
) -> Element {
    let ctx = use_param_context();
    rsx! {
        div { class: "rounded-md border border-border bg-muted/20 p-2",
            div { class: "text-[10px] uppercase tracking-wider text-muted-foreground mb-2", "{label}" }
            Select {
                value,
                placeholder: label,
                on_change: move |v: String| {
                    let Ok(idx) = v.parse::<i32>() else { return };
                    ctx.begin_set_raw(ptr);
                    ctx.set_normalized_raw(ptr, idx as f32 / max_index as f32);
                    ctx.end_set_raw(ptr);
                },
                SelectContent {
                    for (idx, (value, label)) in options.iter().enumerate() {
                        SelectItem { value: value.clone(), index: idx, "{label}" }
                    }
                }
            }
        }
    }
}

#[component]
fn InspectorKnob(label: String, value: String, handle: ParamHandle) -> Element {
    rsx! {
        div {
            class: "rounded-md border border-border bg-muted/20 p-2 flex flex-col items-center gap-1 min-w-0",
            div { class: "text-[10px] uppercase tracking-wider text-muted-foreground", "{label}" }
            Knob {
                handle,
                size: KnobSize::Small,
            }
            div { class: "text-xs font-semibold tabular-nums text-foreground truncate max-w-full", "{value}" }
        }
    }
}

#[component]
fn AnalyzerStat(label: String, value: String) -> Element {
    rsx! {
        div { class: "rounded-md border border-border bg-muted/20 p-2",
            div { class: "text-[10px] uppercase tracking-wider text-muted-foreground", "{label}" }
            div { class: "text-sm font-semibold tabular-nums text-foreground", "{value}" }
        }
    }
}

fn format_freq(freq: f32) -> String {
    if freq >= 1000.0 {
        format!("{:.1}k", freq / 1000.0)
    } else {
        format!("{freq:.0}")
    }
}

fn slope_label(idx: i32) -> &'static str {
    match idx {
        0 => "0 dB/oct",
        1 => "6 dB/oct",
        2 => "12 dB/oct",
        3 => "18 dB/oct",
        4 => "24 dB/oct",
        5 => "30 dB/oct",
        6 => "36 dB/oct",
        7 => "48 dB/oct",
        8 => "72 dB/oct",
        9 => "96 dB/oct",
        10 => "Brickwall",
        _ => "12 dB/oct",
    }
}

fn slope_options() -> Vec<(String, String)> {
    [
        (0, "0"),
        (1, "6"),
        (2, "12"),
        (3, "18"),
        (4, "24"),
        (5, "30"),
        (6, "36"),
        (7, "48"),
        (8, "72"),
        (9, "96"),
        (10, "BW"),
    ]
    .iter()
    .map(|(value, label)| (value.to_string(), label.to_string()))
    .collect()
}

// ─────────────────────────────────────────────────────────────────────
// FPS measurement helpers
// ─────────────────────────────────────────────────────────────────────

/// Rolling-window frame-time history. `tick()` records `Instant::now()`;
/// `fps()` returns the rate over the last `WINDOW` frames. Robust to
/// jitter and the schedule-update tick rate; what matters is the wall-
/// clock interval between the n-th most-recent and the latest frame.
struct FrameHistory {
    times: std::collections::VecDeque<std::time::Instant>,
}

impl FrameHistory {
    const WINDOW: usize = 60;

    fn new() -> Self {
        Self {
            times: std::collections::VecDeque::with_capacity(Self::WINDOW + 1),
        }
    }

    fn tick(&mut self) {
        self.times.push_back(std::time::Instant::now());
        while self.times.len() > Self::WINDOW {
            self.times.pop_front();
        }
    }

    fn fps(&self) -> f32 {
        if self.times.len() < 2 {
            return 0.0;
        }
        let first = *self.times.front().unwrap();
        let last = *self.times.back().unwrap();
        let elapsed = last.duration_since(first).as_secs_f32();
        if elapsed <= 0.0 {
            return 0.0;
        }
        (self.times.len() - 1) as f32 / elapsed
    }
}

/// Map an FPS value to a (display string, tailwind text-color class).
/// Greens at ≥55, ambers at ≥30, reds below — quick visual signal that
/// the redraw + paint pipeline is on target.
fn fps_status(fps: f32) -> (String, &'static str) {
    if fps <= 0.0 {
        return ("--".to_string(), "text-muted-foreground");
    }
    let label = format!("{fps:.0}");
    let color = if fps >= 55.0 {
        "text-emerald-400"
    } else if fps >= 30.0 {
        "text-amber-400"
    } else {
        "text-red-400"
    };
    (label, color)
}
