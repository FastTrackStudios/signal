//! Offline effect rendering: drive any [`PluginInstance`] — a hosted
//! VST3/CLAP or a built-in native block — over a whole buffer, with its
//! parameters set by name the way a person would set them.
//!
//! Built for A/B work against reference plugins: the same stimulus, the
//! same settle time, the same block size, through two instances that
//! share nothing but this trait. The hosted side needs the main run loop
//! pumped (see [`pump_main_run_loop`]) — JUCE- and PACE-wrapped plugins
//! finish loading and licence checks through messages posted to it, and
//! nothing runs it in a CLI host.
//!
//! ```ignore
//! let mut bigsky = offline_fx::load_vst3(Path::new(".../BigSky.vst3"), 0)?;
//! let set = offline_fx::resolve_all(&mut *bigsky, &["EFFECT TYPE=CLOUD", "MIX~=1"])?;
//! let (l, r) = offline_fx::render(&mut *bigsky, &in_l, &in_r, &RenderOptions {
//!     params: set, ..RenderOptions::default()
//! })?;
//! ```

use crate::plugin::{PluginError, PluginEvents, PluginInstance, PluginParamInfo};

/// How one render is driven.
#[derive(Clone, Debug)]
pub struct RenderOptions {
    pub sample_rate: f64,
    pub block_size: usize,
    /// Silence rendered (and discarded) before the input, with
    /// [`Self::params`] pushed on every block, so parameter smoothing
    /// and any previous tail have finished before the stimulus.
    pub preroll_secs: f64,
    /// Silence appended after the input, to catch the tail.
    pub tail_secs: f64,
    /// `(id, plain value)` changes held for the whole pre-roll.
    pub params: Vec<(u32, f64)>,
    /// Pump the main run loop between pre-roll blocks (hosted plugins).
    pub pump_run_loop: bool,
    /// Drop the plugin's reported latency from the front of the output,
    /// so time zero is the input's time zero.
    pub compensate_latency: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            block_size: 256,
            preroll_secs: 2.0,
            tail_secs: 6.0,
            params: Vec::new(),
            pump_run_loop: false,
            compensate_latency: true,
        }
    }
}

/// Render `in_l`/`in_r` (plus the tail) through `plugin`. Prepares the
/// plugin at `opts.sample_rate` if it is not already prepared.
pub fn render(
    plugin: &mut dyn PluginInstance,
    in_l: &[f32],
    in_r: &[f32],
    opts: &RenderOptions,
) -> Result<(Vec<f32>, Vec<f32>), PluginError> {
    let block = opts.block_size.max(1);
    if !plugin.is_prepared() {
        plugin.prepare(opts.sample_rate, block as u32)?;
    }

    let zeros = vec![0.0f32; block];
    let (mut sl, mut sr) = (vec![0.0f32; block], vec![0.0f32; block]);
    let pre = (opts.preroll_secs.max(0.0) * opts.sample_rate) as usize;
    let mut done = 0;
    // At least one block, so the parameters always reach the plugin.
    while done < pre.max(1) {
        let ev = PluginEvents { params: &opts.params, midi: &[], note_expressions: &[] };
        plugin.process_block(&zeros, &zeros, &mut sl, &mut sr, &ev)?;
        if opts.pump_run_loop {
            pump_main_run_loop(0.0005);
        }
        done += block;
    }

    let latency = if opts.compensate_latency { plugin.latency() as usize } else { 0 };
    let n_in = in_l.len().max(in_r.len());
    let total = n_in + (opts.tail_secs.max(0.0) * opts.sample_rate) as usize + latency;
    let (mut l, mut r) = (vec![0.0f32; total], vec![0.0f32; total]);
    let (mut il, mut ir) = (vec![0.0f32; block], vec![0.0f32; block]);
    let mut pos = 0;
    while pos < total {
        let n = block.min(total - pos);
        for k in 0..n {
            il[k] = in_l.get(pos + k).copied().unwrap_or(0.0);
            ir[k] = in_r.get(pos + k).copied().unwrap_or(0.0);
        }
        plugin.process_block(
            &il[..n],
            &ir[..n],
            &mut l[pos..pos + n],
            &mut r[pos..pos + n],
            &PluginEvents::EMPTY,
        )?;
        pos += n;
    }
    l.drain(..latency);
    r.drain(..latency);
    Ok((l, r))
}

/// Find a parameter by name (case-insensitive) or by numeric id.
pub fn find_param<'a>(params: &'a [PluginParamInfo], name: &str) -> Option<&'a PluginParamInfo> {
    let name = name.trim();
    params
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
        .or_else(|| params.iter().find(|p| p.id.to_string() == name))
}

/// The distinct display texts a parameter shows across its range, in
/// order, with the plain value where each first appears — how a
/// selector's positions (an algorithm list, a mode switch) are
/// discovered when the plugin reports it as continuous. Samples 256
/// points; returns `None` when the texts look numeric (a real knob).
pub fn param_positions(plugin: &mut dyn PluginInstance, info: &PluginParamInfo) -> Option<Vec<(String, f64)>> {
    let mut out: Vec<(String, f64)> = Vec::new();
    for k in 0..=255u32 {
        let plain = info.min + (info.max - info.min) * f64::from(k) / 255.0;
        let text = plugin.value_to_text(info.id, plain)?.trim().to_string();
        if out.last().map(|(t, _)| t != &text).unwrap_or(true) {
            out.push((text, plain));
        }
        if out.len() > 48 {
            return None;
        }
    }
    let numeric = out
        .iter()
        .filter(|(t, _)| t.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '-' || c == '+'))
        .count();
    (numeric * 2 < out.len()).then_some(out)
}

/// Resolve one setting to `(id, plain)`:
///
/// - `NAME=text`   — the plugin's own display text (`"Type=Cloud"`,
///   `"Decay=2340"`), parsed by the plugin, falling back to matching a
///   selector position's name;
/// - `NAME:=plain` — a plain value in the parameter's own units;
/// - `NAME~=norm`  — a normalized 0..1 position.
pub fn resolve(plugin: &mut dyn PluginInstance, params: &[PluginParamInfo], spec: &str) -> Result<(u32, f64), String> {
    let (name, op, value) = if let Some((n, v)) = spec.split_once(":=") {
        (n, ":=", v)
    } else if let Some((n, v)) = spec.split_once("~=") {
        (n, "~=", v)
    } else if let Some((n, v)) = spec.split_once('=') {
        (n, "=", v)
    } else {
        return Err(format!("expected NAME=text, NAME:=plain or NAME~=normalized, got {spec:?}"));
    };
    let info = find_param(params, name).ok_or_else(|| format!("no parameter named {:?}", name.trim()))?.clone();
    let value = value.trim();
    let num = || value.parse::<f64>().map_err(|e| format!("{}: {value:?}: {e}", info.name));
    let plain = match op {
        ":=" => num()?,
        "~=" => info.min + (info.max - info.min) * num()?.clamp(0.0, 1.0),
        _ => match plugin.text_to_value(info.id, value) {
            // Some plugins "parse" by returning the current value for text
            // they do not know; only trust a parse that formats back.
            Some(v) if plugin.value_to_text(info.id, v).is_some_and(|t| same_text(&t, value)) => v,
            other => param_positions(plugin, &info)
                .and_then(|pos| pos.into_iter().find(|(t, _)| same_text(t, value)).map(|(_, v)| v))
                .or(other)
                .ok_or_else(|| format!("{}: plugin did not parse {value:?}", info.name))?,
        },
    };
    Ok((info.id, plain))
}

/// Resolve every setting, in order.
pub fn resolve_all(plugin: &mut dyn PluginInstance, specs: &[&str]) -> Result<Vec<(u32, f64)>, String> {
    let params = plugin.params();
    specs.iter().map(|s| resolve(plugin, &params, s)).collect()
}

fn same_text(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase();
    norm(a) == norm(b)
}

/// Load a VST3 effect for offline use, pumping the run loop around the
/// load so wrapped plugins finish initialising.
#[cfg(feature = "vst3-host")]
pub fn load_vst3(
    bundle: &std::path::Path,
    index: usize,
) -> Result<Box<dyn PluginInstance>, PluginError> {
    use crate::audio_engine::vst3_host::Vst3Host;
    let plugin = Vst3Host::new()
        .load(bundle, index)
        .map_err(|e| PluginError::LoadFailed(format!("{}: {e:?}", bundle.display())))?;
    pump_main_run_loop(0.5);
    Ok(Box::new(plugin.into_send()))
}

/// Run the main thread's run loop for `secs`. Call from the main thread.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)] // one CoreFoundation FFI call
pub fn pump_main_run_loop(secs: f64) {
    use std::ffi::c_void;
    use std::time::{Duration, Instant};
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFRunLoopDefaultMode: *const c_void;
        fn CFRunLoopRunInMode(mode: *const c_void, seconds: f64, ret: u8) -> i32;
    }
    let deadline = Instant::now() + Duration::from_secs_f64(secs.max(0.0));
    loop {
        // SAFETY: plain CoreFoundation call; the mode constant is static.
        unsafe {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.005_f64.min(secs.max(0.0)), 0);
        }
        if Instant::now() >= deadline {
            break;
        }
    }
}

/// No main run loop to pump off macOS; plugins there post nothing to it.
#[cfg(not(target_os = "macos"))]
pub fn pump_main_run_loop(_secs: f64) {}
