//! Signal's plugin-hosting layer.
//!
//! Wraps `daw::plugin::PluginInstance` (CLAP / VST3, format-agnostic) with
//! the conventions Signal's audio pipeline expects:
//!
//! - **Interleaved stereo I/O.** Signal's `EngineInstance` / `PresetRuntime`
//!   render into interleaved-stereo `[L, R, L, R, …]` buffers; the host
//!   trait deals in planar `(&[f32], &[f32]) → (&mut [f32], &mut [f32])`.
//!   [`HostedPlugin::process_interleaved`] does the de/re-interleave with
//!   reusable scratch buffers (no per-block allocation).
//!
//! - **UI-thread param writes.** UI code calls [`HostedPlugin::set_param`]
//!   from the GUI thread; pending writes are drained at the top of
//!   `process_interleaved` and applied as a `PluginEvents` block.
//!
//! - **Format-agnostic load.** [`HostedPlugin::load`] sniffs the extension
//!   (`.clap` / `.vst3`) and dispatches through `daw::plugin::load_plugin`.
//!
//! This crate is the runtime — it holds the live `Box<dyn PluginInstance>`.
//! The data model (parameter mappings, virtual-module grouping, plugin
//! identity) lives in `signal-proto::plugin_block`.

use std::path::Path;
use std::sync::Mutex;

pub use daw::plugin::{
    PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance, PluginMidiEvent,
    PluginNoteExpression, PluginParamInfo,
};

/// One pending parameter write from the UI thread, drained at the top of
/// the next audio block.
#[derive(Clone, Copy, Debug)]
struct PendingParam {
    id: u32,
    value: f64,
}

/// A live plugin instance owned by Signal — adds interleaved-stereo I/O,
/// scratch buffers, and a UI-thread → audio-thread parameter queue on top
/// of `daw::plugin::PluginInstance`.
pub struct HostedPlugin {
    inner: Box<dyn PluginInstance>,
    descriptor: PluginDescriptor,
    in_l: Vec<f32>,
    in_r: Vec<f32>,
    out_l: Vec<f32>,
    out_r: Vec<f32>,
    /// Pending UI-thread param writes. `Mutex` is acceptable for first cut
    /// (drum-mixer Phase F replaces this with a lock-free SPSC if profiling
    /// shows contention). The audio thread `try_lock`s — never blocks.
    pending: Mutex<Vec<PendingParam>>,
}

impl HostedPlugin {
    /// Load a plugin from `path`. Sniffs the format from the extension
    /// (`.clap` / `.vst3`); see [`PluginFormat::from_path_or_name`]. Returns
    /// `Ok(None)` if `path` resolves to the synthetic backend (caller should
    /// fall back to its own DSP).
    pub fn load(path: impl AsRef<Path>) -> Result<Option<Self>, PluginError> {
        let path_str = path.as_ref().to_string_lossy();
        let Some(mut inner) = daw::plugin::load_plugin(&path_str)? else {
            return Ok(None);
        };
        let descriptor = inner.descriptor();
        Ok(Some(Self {
            inner,
            descriptor,
            in_l: Vec::new(),
            in_r: Vec::new(),
            out_l: Vec::new(),
            out_r: Vec::new(),
            pending: Mutex::new(Vec::new()),
        }))
    }

    pub fn descriptor(&self) -> &PluginDescriptor {
        &self.descriptor
    }

    pub fn format(&self) -> PluginFormat {
        self.descriptor.format
    }

    /// Snapshot the plugin's parameter list. Called once after load to build
    /// the UI; stable for the plugin's life.
    pub fn params(&mut self) -> Vec<PluginParamInfo> {
        self.inner.params()
    }

    /// Current plain value of `id`, if the plugin reports it. Polled by the
    /// UI to sync sliders with internal changes (host automation, presets).
    pub fn param_value(&mut self, id: u32) -> Option<f64> {
        self.inner.param_value(id)
    }

    pub fn value_to_text(&mut self, id: u32, value: f64) -> Option<String> {
        self.inner.value_to_text(id, value)
    }

    pub fn text_to_value(&mut self, id: u32, text: &str) -> Option<f64> {
        self.inner.text_to_value(id, text)
    }

    pub fn latency(&mut self) -> u32 {
        self.inner.latency()
    }

    /// Activate the plugin. Must be called before `process_interleaved`.
    pub fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.in_l.resize(block_size as usize, 0.0);
        self.in_r.resize(block_size as usize, 0.0);
        self.out_l.resize(block_size as usize, 0.0);
        self.out_r.resize(block_size as usize, 0.0);
        self.inner.prepare(sample_rate, block_size)
    }

    pub fn is_prepared(&self) -> bool {
        self.inner.is_prepared()
    }

    /// Queue a parameter write from the UI thread. Applied at the top of the
    /// next `process_interleaved` call.
    pub fn set_param(&self, id: u32, value: f64) {
        if let Ok(mut q) = self.pending.lock() {
            q.push(PendingParam { id, value });
        }
    }

    /// Render one block. `inout` is interleaved-stereo (`L, R, L, R, …`),
    /// processed in-place: the plugin receives the current contents as
    /// audio input and overwrites it with its output. `extra` carries MIDI
    /// + note expressions; UI param writes are drained automatically.
    pub fn process_interleaved(
        &mut self,
        inout: &mut [f32],
        midi: &[PluginMidiEvent],
        note_expressions: &[PluginNoteExpression],
    ) -> Result<(), PluginError> {
        let frames = inout.len() / 2;
        if frames > self.in_l.len() {
            return Err(PluginError::BlockTooLarge);
        }

        // De-interleave into the prepared planar scratch.
        for i in 0..frames {
            self.in_l[i] = inout[2 * i];
            self.in_r[i] = inout[2 * i + 1];
        }

        // Drain pending UI writes; build the (param_id, value) slice for
        // this block. Try-lock so we never block the audio thread.
        let mut param_writes: Vec<(u32, f64)> = Vec::new();
        if let Ok(mut q) = self.pending.try_lock() {
            param_writes.reserve(q.len());
            for p in q.drain(..) {
                param_writes.push((p.id, p.value));
            }
        }

        let events = PluginEvents {
            params: &param_writes,
            midi,
            note_expressions,
        };

        self.inner.process_block(
            &self.in_l[..frames],
            &self.in_r[..frames],
            &mut self.out_l[..frames],
            &mut self.out_r[..frames],
            &events,
        )?;

        // Re-interleave the planar output back into the caller's buffer.
        for i in 0..frames {
            inout[2 * i] = self.out_l[i];
            inout[2 * i + 1] = self.out_r[i];
        }
        Ok(())
    }

    /// Open the plugin's editor UI (no-op for backends without GUI).
    pub fn open_gui(&mut self) -> Result<(), PluginError> {
        self.inner.open_gui()
    }

    pub fn close_gui(&mut self) -> Result<(), PluginError> {
        self.inner.close_gui()
    }

    pub fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        self.inner.save_state()
    }

    pub fn load_state(&mut self, state: &[u8]) -> Result<(), PluginError> {
        self.inner.load_state(state)
    }

    pub fn deactivate(&mut self) {
        self.inner.deactivate();
    }
}

impl std::fmt::Debug for HostedPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostedPlugin")
            .field("descriptor", &self.descriptor)
            .field("prepared", &self.inner.is_prepared())
            .finish()
    }
}

// ── Plugin scanner (standard directories) ───────────────────────────────────

/// One plugin discovered on disk by [`scan_plugins`]. The display name is
/// derived from the filename (stem) — the host doesn't open the bundle
/// during the scan, so this is fast even with hundreds of installed plugins.
/// Open the actual descriptor by calling [`HostedPlugin::load`] on `path`.
#[derive(Clone, Debug)]
pub struct PluginEntry {
    pub display_name: String,
    pub path: std::path::PathBuf,
    pub format: PluginFormat,
}

/// Standard install directories for `format`, in the order they should be
/// searched (user dirs first so per-user installs shadow system installs).
pub fn standard_dirs(format: PluginFormat) -> Vec<std::path::PathBuf> {
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);

    match format {
        PluginFormat::Clap => {
            // CLAP_PATH override + per-OS defaults. Linux convention: ~/.clap,
            // /usr/lib/clap, /usr/local/lib/clap; macOS: ~/Library/Audio/Plug-Ins/CLAP
            // + /Library/...; Windows: %COMMONPROGRAMFILES%\CLAP + %LOCALAPPDATA%\Programs\Common\CLAP.
            if let Some(p) = std::env::var_os("CLAP_PATH") {
                for d in std::env::split_paths(&p) {
                    dirs.push(d);
                }
            }
            if let Some(h) = home.clone() {
                #[cfg(target_os = "linux")]
                dirs.push(h.join(".clap"));
                #[cfg(target_os = "macos")]
                dirs.push(h.join("Library/Audio/Plug-Ins/CLAP"));
            }
            #[cfg(target_os = "linux")]
            {
                dirs.push("/usr/lib/clap".into());
                dirs.push("/usr/local/lib/clap".into());
            }
            #[cfg(target_os = "macos")]
            dirs.push("/Library/Audio/Plug-Ins/CLAP".into());
            #[cfg(target_os = "windows")]
            {
                if let Some(p) = std::env::var_os("COMMONPROGRAMFILES") {
                    dirs.push(std::path::PathBuf::from(p).join("CLAP"));
                }
                if let Some(p) = std::env::var_os("LOCALAPPDATA") {
                    dirs.push(std::path::PathBuf::from(p).join("Programs/Common/CLAP"));
                }
            }
        }
        PluginFormat::Vst3 => {
            if let Some(p) = std::env::var_os("VST3_PATH") {
                for d in std::env::split_paths(&p) {
                    dirs.push(d);
                }
            }
            if let Some(h) = home {
                #[cfg(target_os = "linux")]
                dirs.push(h.join(".vst3"));
                #[cfg(target_os = "macos")]
                dirs.push(h.join("Library/Audio/Plug-Ins/VST3"));
            }
            #[cfg(target_os = "linux")]
            {
                dirs.push("/usr/lib/vst3".into());
                dirs.push("/usr/local/lib/vst3".into());
            }
            #[cfg(target_os = "macos")]
            dirs.push("/Library/Audio/Plug-Ins/VST3".into());
            #[cfg(target_os = "windows")]
            {
                if let Some(p) = std::env::var_os("COMMONPROGRAMFILES") {
                    dirs.push(std::path::PathBuf::from(p).join("VST3"));
                }
            }
        }
        PluginFormat::Lv2 | PluginFormat::Synthetic => {}
    }
    dirs
}

/// Walk `standard_dirs(fmt)` for each requested format and collect plugin
/// entries. Recurses up to `max_depth` levels into subdirectories (CLAP
/// vendors commonly nest plugins one level deep; VST3 is a directory
/// bundle itself so depth 1 suffices). Duplicates by path are dropped.
pub fn scan_plugins(formats: &[PluginFormat], max_depth: usize) -> Vec<PluginEntry> {
    use std::collections::HashSet;
    let mut seen: HashSet<std::path::PathBuf> = HashSet::new();
    let mut out: Vec<PluginEntry> = Vec::new();
    for &fmt in formats {
        for dir in standard_dirs(fmt) {
            if dir.exists() {
                walk_for(&dir, fmt, max_depth, &mut seen, &mut out);
            }
        }
    }
    out.sort_by(|a, b| {
        a.display_name
            .to_lowercase()
            .cmp(&b.display_name.to_lowercase())
    });
    out
}

fn walk_for(
    dir: &std::path::Path,
    fmt: PluginFormat,
    depth: usize,
    seen: &mut std::collections::HashSet<std::path::PathBuf>,
    out: &mut Vec<PluginEntry>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let matches_fmt = match (fmt, ext) {
                (PluginFormat::Clap, "clap") => true,
                (PluginFormat::Vst3, "vst3") => true,
                _ => false,
            };
            if matches_fmt {
                if seen.insert(path.clone()) {
                    let display_name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    out.push(PluginEntry {
                        display_name,
                        path,
                        format: fmt,
                    });
                }
                continue;
            }
        }
        // Recurse into plain subdirs (vendor folders). Skip symlinks to
        // avoid loops; cap depth.
        if depth > 0 {
            if let Ok(meta) = entry.metadata() {
                if meta.is_dir() && !meta.file_type().is_symlink() {
                    walk_for(&path, fmt, depth - 1, seen, out);
                }
            }
        }
    }
}
