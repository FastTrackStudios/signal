//! Standalone EQ GUI — Blitz native renderer, no DAW host.
//!
//! ```sh
//! cargo run -p eq-standalone
//! ```
//!
//! Rebuilt against the latest blitz / dioxus-native API (commit 32d3ba6e+).
//! The earlier hand-rolled `launch_with_redraw_pump` + `WindowIdCaptureApp`
//! wrapper relied on `winit`/`blitz_shell` internals that no longer exist;
//! we now lean on `dioxus_native::launch_cfg`, which handles the
//! `EventLoop` / winit App plumbing for us.
//!
//! Known trade-off: without a custom Poll-event pump the spectrum/meter
//! atomics only propagate to the DOM when a real user event (mouse move,
//! key press) arrives. Add a redraw driver back in a follow-up once we
//! have a handle on the new `BlitzShellProxy` lifecycle.

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

mod pipewire_capture;

use dioxus_native::launch_cfg;
use dioxus_native::prelude::{document, rsx};
use eq_ui::params::{EqUiState, FtsEqParams, NUM_BANDS, SPECTRUM_BINS};
use audiocore_core::prelude::Param;
use nice_plug::context::gui::{GuiContext, GuiContextInner};
use nice_plug::prelude::*;
use nice_plug_dioxus::{ParamContext, SharedState};

/// Compiled by `tailwindcss -i tailwind.css -o assets/tailwind.css`.
const TAILWIND_CSS: &str = include_str!("../assets/tailwind.css");

fn main() {
    eprintln!("FTS EQ — standalone (Blitz native)");

    // Demo animations (auto-moving bands + a synthetic moving spectrum) were a
    // testing aid. They're opt-in via `--animate` (alias `--demo`); by default
    // the standalone is a static, fully interactive base EQ you can edit.
    let animate = std::env::args().any(|a| a == "--animate" || a == "--demo");

    let params = Arc::new(FtsEqParams::default());
    let ui_state = Arc::new(EqUiState::new(params));

    seed_demo_state(&ui_state);

    // Capture live system audio into the analyzer. Default: native PipeWire,
    // which auto-connects to the default sink's monitor (no manual routing).
    // `FTS_EQ_CAPTURE=cpal` forces the cpal/ALSA path (kept for non-PipeWire
    // systems); cpal also runs as a fallback if PipeWire setup fails. Output is
    // never touched. The cpal stream is held in scope for the session.
    let force_cpal = std::env::var("FTS_EQ_CAPTURE").as_deref() == Ok("cpal");
    let mut _capture_stream = None;
    if force_cpal {
        _capture_stream = start_system_capture(&ui_state)
            .map_err(|e| eprintln!("(cpal capture unavailable: {e} — analyzer idle)"))
            .ok();
    } else if let Err(e) = pipewire_capture::spawn(&ui_state) {
        eprintln!("(pipewire capture setup failed: {e}; trying cpal)");
        _capture_stream = start_system_capture(&ui_state)
            .map_err(|e| eprintln!("(cpal capture unavailable: {e} — analyzer idle)"))
            .ok();
    }

    if animate {
        eprintln!("(demo animations enabled — run without --animate for a static base EQ)");
        {
            let ui = ui_state.clone();
            std::thread::spawn(move || animate_spectrum(ui));
        }
        {
            let ui = ui_state.clone();
            std::thread::spawn(move || animate_demo_bands(ui));
        }
    }

    let shared = SharedState::new(ui_state);

    let gui_context = GuiContext::new(Arc::new(StandaloneGuiContext));
    let needs_redraw = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let param_ctx = ParamContext::new(gui_context, needs_redraw);

    // Track-info backend for the cheat-sheet overlay. The standalone has no host
    // track, so use the static/env provider: set `FTS_EQ_TRACK_NAME="Kick In"`
    // (etc.) to test how the overlay auto-selects per track. The plugin build
    // swaps in a CLAP track-info-backed provider instead.
    let track_provider: Arc<dyn eq_ui::cheatsheet::TrackInfoProvider> =
        Arc::new(eq_ui::cheatsheet::StaticTrackProvider::from_env());

    let contexts: Vec<Box<dyn Fn() -> Box<dyn Any> + Send + Sync>> = vec![
        Box::new(move || Box::new(param_ctx.clone()) as Box<dyn Any>),
        Box::new(move || Box::new(shared.clone()) as Box<dyn Any>),
        Box::new(move || Box::new(track_provider.clone()) as Box<dyn Any>),
    ];

    launch_cfg(standalone_shell, contexts, vec![]);
}

fn standalone_shell() -> dioxus_core::Element {
    rsx! {
        document::Style { {TAILWIND_CSS} }
        eq_ui::control_view::App {}
    }
}

/// No-op `GuiContext` for standalone use. Mirrors `nice_plug_dioxus`'
/// private `StandaloneGuiContext` so knob drags pipe straight to `ParamPtr`.
struct StandaloneGuiContext;

impl GuiContextInner for StandaloneGuiContext {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Clap
    }
    unsafe fn raw_begin_set_parameter(&self, _param: ParamPtr) {}
    unsafe fn raw_set_parameter_normalized(&self, param: ParamPtr, normalized: f32) {
        unsafe { param._internal_set_normalized_value(normalized) };
    }
    unsafe fn raw_end_set_parameter(&self, _param: ParamPtr) {}
    fn get_state(&self) -> PluginState {
        PluginState {
            version: String::new(),
            params: BTreeMap::new(),
            fields: BTreeMap::new(),
        }
    }
    fn set_state(&self, _state: PluginState) {}
}

// ─────────────────────────────────────────────────────────────────────
// Demo data drivers
// ─────────────────────────────────────────────────────────────────────

fn animate_demo_bands(ui: Arc<EqUiState>) {
    use std::time::Instant;
    let start = Instant::now();
    let params = &ui.params;

    let shapes: [(usize, i32); 4] = [(0, 0), (1, 1), (2, 0), (3, 3)];
    for &(idx, shape) in &shapes {
        if idx >= NUM_BANDS {
            continue;
        }
        let bp = &params.bands[idx];
        unsafe {
            bp.filter_type
                .as_ptr()
                ._internal_set_normalized_value(bp.filter_type.preview_normalized(shape));
        }
    }

    loop {
        let t = start.elapsed().as_secs_f32();
        let cycle = (t % 8.0) / 8.0;

        if NUM_BANDS > 0 {
            let bp = &params.bands[0];
            let freq = 80.0 * 100.0_f32.powf(cycle);
            let gain = 6.0 * (t * 0.7).sin();
            unsafe {
                bp.enabled
                    .as_ptr()
                    ._internal_set_normalized_value(bp.enabled.preview_normalized(1.0));
                bp.freq_hz
                    .as_ptr()
                    ._internal_set_normalized_value(bp.freq_hz.preview_normalized(freq));
                bp.gain_db
                    .as_ptr()
                    ._internal_set_normalized_value(bp.gain_db.preview_normalized(gain));
                bp.q.as_ptr()
                    ._internal_set_normalized_value(bp.q.preview_normalized(1.0));
            }
        }
        if NUM_BANDS > 1 {
            let bp = &params.bands[1];
            let gain = 4.0 * (t * 0.45 + 1.5).sin() - 1.0;
            unsafe {
                bp.enabled
                    .as_ptr()
                    ._internal_set_normalized_value(bp.enabled.preview_normalized(1.0));
                bp.freq_hz
                    .as_ptr()
                    ._internal_set_normalized_value(bp.freq_hz.preview_normalized(80.0));
                bp.gain_db
                    .as_ptr()
                    ._internal_set_normalized_value(bp.gain_db.preview_normalized(gain));
                bp.q.as_ptr()
                    ._internal_set_normalized_value(bp.q.preview_normalized(0.7));
            }
        }
        if NUM_BANDS > 2 {
            let bp = &params.bands[2];
            let on = ((t * 0.25).sin() + 1.0) * 0.5;
            let enabled = if on > 0.5 { 1.0 } else { 0.0 };
            unsafe {
                bp.enabled
                    .as_ptr()
                    ._internal_set_normalized_value(bp.enabled.preview_normalized(enabled));
                bp.freq_hz
                    .as_ptr()
                    ._internal_set_normalized_value(bp.freq_hz.preview_normalized(40.0));
                bp.q.as_ptr()
                    ._internal_set_normalized_value(bp.q.preview_normalized(0.7));
            }
        }
        if NUM_BANDS > 3 {
            let bp = &params.bands[3];
            let gain = 3.0 * (t * 0.6 + 2.5).sin();
            unsafe {
                bp.enabled
                    .as_ptr()
                    ._internal_set_normalized_value(bp.enabled.preview_normalized(1.0));
                bp.freq_hz
                    .as_ptr()
                    ._internal_set_normalized_value(bp.freq_hz.preview_normalized(6000.0));
                bp.gain_db
                    .as_ptr()
                    ._internal_set_normalized_value(bp.gain_db.preview_normalized(gain));
                bp.q.as_ptr()
                    ._internal_set_normalized_value(bp.q.preview_normalized(0.7));
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(33));
    }
}

/// Open the default audio input device and feed its samples into the analyzer.
///
/// On Linux, cpal uses the ALSA/PipeWire/Pulse backend, so the running app
/// appears as a capture node. Connect the system-audio monitor to it in a
/// patchbay to visualize whatever is playing. Returns the live stream, which the
/// caller must keep alive.
fn start_system_capture(ui: &Arc<EqUiState>) -> anyhow::Result<cpal::Stream> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();

    // Device selection priority:
    //   1. A device whose name says "monitor"/"loopback" (rare on PipeWire's
    //      ALSA bridge, common on PulseAudio) — captures system audio directly.
    //   2. The "pipewire" device — registers a clean, named capture node you can
    //      wire the system-audio monitor into from a patchbay (qpwgraph/Helvum)
    //      or pavucontrol's Recording tab.
    //   3. The default input (usually the mic) as a last resort.
    let env_dev = std::env::var("FTS_EQ_AUDIO_INPUT").ok();
    let mut monitor = None;
    let mut pipewire = None;
    let mut env_match = None;
    if let Ok(inputs) = host.input_devices() {
        eprintln!("available audio inputs:");
        for d in inputs {
            let name = d
                .description()
                .map(|desc| desc.name().to_string())
                .unwrap_or_else(|_| "<unknown>".into());
            eprintln!("  - {name}");
            let lname = name.to_lowercase();
            if let Some(want) = &env_dev {
                if env_match.is_none() && name == *want {
                    env_match = Some(d);
                    continue;
                }
            }
            if monitor.is_none() && (lname.contains("monitor") || lname.contains("loopback")) {
                monitor = Some(d);
            } else if pipewire.is_none() && lname == "pipewire" {
                pipewire = Some(d);
            }
        }
    }
    let device = env_match
        .or(monitor)
        .or(pipewire)
        .or_else(|| host.default_input_device())
        .ok_or_else(|| anyhow::anyhow!("no input device available"))?;
    let dev_name = device
        .description()
        .map(|desc| desc.name().to_string())
        .unwrap_or_else(|_| "<unknown>".into());
    eprintln!("capturing from: {dev_name}");
    eprintln!(
        "  ↳ to visualize system audio: open `pavucontrol` → Recording tab and set\n     this app's source to \"Monitor of <your output>\", or connect the sink\n     monitor to this node in `qpwgraph`. Override the device with\n     FTS_EQ_AUDIO_INPUT=\"<name>\"."
    );
    let config = device.default_input_config()?;
    let sample_rate = config.sample_rate() as f32;
    let channels = config.channels() as usize;

    ui.sample_rate.store(sample_rate, Ordering::Relaxed);
    ui.analyzer.set_sample_rate(sample_rate);
    ui.analyzer.set_label("System Audio");

    let mut feed = ui
        .take_audio_feed()
        .ok_or_else(|| anyhow::anyhow!("analyzer feed already taken"))?;
    let err_fn = |e| eprintln!("audio stream error: {e}");

    // Reused scratch for the downmixed mono signal (audio-thread side).
    let mut mono: Vec<f32> = Vec::new();
    let downmix_push = move |data: &[f32]| {
        if channels == 0 {
            return;
        }
        let frames = data.len() / channels;
        mono.clear();
        mono.reserve(frames);
        for frame in data.chunks_exact(channels) {
            let sum: f32 = frame.iter().copied().sum();
            mono.push(sum / channels as f32);
        }
        // No EQ is applied to the captured signal, so pre == post.
        feed.push_pre(&mono);
        feed.push_post(&mono);
    };

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let mut push = downmix_push;
            device.build_input_stream(
                config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| push(data),
                err_fn,
                None,
            )?
        }
        cpal::SampleFormat::I16 => {
            let mut push = downmix_push;
            let mut buf: Vec<f32> = Vec::new();
            device.build_input_stream(
                config.into(),
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    buf.clear();
                    buf.extend(data.iter().map(|&s| s as f32 / i16::MAX as f32));
                    push(&buf);
                },
                err_fn,
                None,
            )?
        }
        cpal::SampleFormat::U16 => {
            let mut push = downmix_push;
            let mut buf: Vec<f32> = Vec::new();
            device.build_input_stream(
                config.into(),
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    buf.clear();
                    buf.extend(
                        data.iter()
                            .map(|&s| (s as f32 / u16::MAX as f32) * 2.0 - 1.0),
                    );
                    push(&buf);
                },
                err_fn,
                None,
            )?
        }
        other => return Err(anyhow::anyhow!("unsupported sample format: {other:?}")),
    };

    stream.play()?;
    eprintln!("system audio capture: {sample_rate:.0} Hz, {channels} ch");
    Ok(stream)
}

fn animate_spectrum(ui: Arc<EqUiState>) {
    let log_min = 20.0_f32.log10();
    let log_max = 20_000.0_f32.log10();
    let start = std::time::Instant::now();
    loop {
        let t = start.elapsed().as_secs_f32();
        let peak_freq = 200.0 * (1.0 + 6.0 * (0.5 * (t * 0.3).sin() + 0.5));
        for i in 0..SPECTRUM_BINS {
            let u = i as f32 / (SPECTRUM_BINS - 1) as f32;
            let freq = 10.0_f32.powf(log_min + u * (log_max - log_min));
            let octaves_from_1k = (freq / 1000.0).log2();
            let pink = -3.0 * octaves_from_1k;
            let dist_oct = (freq / peak_freq).log2().abs();
            let bump = 18.0 * (-dist_oct.powi(2) * 0.6).exp();
            let wobble = 1.5 * ((u * 18.0 + t * 1.2).sin());
            let db = -28.0 + pink + bump + wobble;
            ui.spectrum_bins[i].store(db, Ordering::Relaxed);
        }
        let in_db = -16.0 + 4.0 * (t * 1.7).sin();
        let out_db = -18.0 + 4.0 * (t * 1.7 + 0.3).sin();
        ui.input_peak_db.store(in_db, Ordering::Relaxed);
        ui.output_peak_db.store(out_db, Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_millis(16));
    }
}

fn seed_demo_state(ui: &EqUiState) {
    ui.sample_rate.store(48_000.0, Ordering::Relaxed);
    ui.input_peak_db.store(-12.0, Ordering::Relaxed);
    ui.output_peak_db.store(-14.0, Ordering::Relaxed);
    let log_min = 20.0_f32.log10();
    let log_max = 20_000.0_f32.log10();
    for i in 0..SPECTRUM_BINS {
        let t = i as f32 / (SPECTRUM_BINS - 1) as f32;
        let freq = 10.0_f32.powf(log_min + t * (log_max - log_min));
        let octaves_from_1k = (freq / 1000.0).log2();
        let pink = -3.0 * octaves_from_1k;
        let ripple = 2.0 * (octaves_from_1k * std::f32::consts::PI).sin();
        let db = -24.0 + pink + ripple;
        ui.spectrum_bins[i].store(db, Ordering::Relaxed);
    }
}
