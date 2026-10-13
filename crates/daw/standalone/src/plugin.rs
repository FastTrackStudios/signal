//! Format-agnostic plugin abstraction.
//!
//! The renderer + Effects service talk to plugins through
//! [`PluginInstance`]. CLAP is the only format wired today; VST3 and
//! LV2 backends slot in by implementing the same trait.
//!
//! The trait is intentionally minimal — it covers what the project
//! renderer needs (prepare / process_block / param query) plus the
//! UI surface (descriptor, value↔text). Format-specific extras
//! (state chunks, view embedding, MIDI ports) live as inherent
//! methods on the concrete backends.

use std::fmt;
use std::path::Path;

/// Plugin format identifier — used by `Effects::add` to pick the
/// right backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum PluginFormat {
    Clap,
    Vst3,
    Lv2,
    /// Synthetic placeholder (8 generic params, no DSP). Default
    /// when no real backend is available.
    #[default]
    Synthetic,
}

impl PluginFormat {
    /// Sniff the format from a path or identifier string.
    /// - `*.clap` → Clap
    /// - `*.vst3` → Vst3
    /// - `*.lv2` → Lv2
    /// - everything else → Synthetic
    pub fn from_path_or_name(name: &str) -> Self {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".clap") {
            Self::Clap
        } else if lower.ends_with(".vst3") {
            Self::Vst3
        } else if lower.ends_with(".lv2") {
            Self::Lv2
        } else {
            Self::Synthetic
        }
    }
}

/// Plugin-format-neutral descriptor.
#[derive(Clone, Debug, Default)]
pub struct PluginDescriptor {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub format: PluginFormat,
}

/// A MIDI event scheduled at a sample offset inside the current
/// process block. Used to drive virtual instruments (CLAPi/VST3i)
/// and to feed effect plugins that respond to CC/program changes.
#[derive(Clone, Debug)]
pub struct PluginMidiEvent {
    /// Sample offset from the start of the block (0..block_size).
    pub offset: u32,
    pub message: daw_proto::MidiEvent,
}

/// One per-note expression event scheduled at a sample offset.
/// Surfaces CLAP's `NoteExpressionEvent` and VST3's
/// `NoteExpressionValueEvent` through a single host-side type — the
/// renderer emits these alongside MIDI; backends translate to the
/// format-native event.
#[derive(Clone, Copy, Debug)]
pub struct PluginNoteExpression {
    pub offset: u32,
    pub channel: u8,
    /// Target note pitch, or `0xFF` for "any active voice".
    pub note: u8,
    pub dimension: daw_proto::midi::NoteExpressionDim,
    pub value: f64,
}

/// Per-block inputs to [`PluginInstance::process_block`].
///
/// Borrowed slices so the renderer can pass scratch buffers without
/// extra allocation. An empty `PluginEvents` is the common case
/// for static effects with no automation or note input.
#[derive(Clone, Copy, Debug, Default)]
pub struct PluginEvents<'a> {
    /// `(param_id, plain_value)` changes applied at sample 0.
    pub params: &'a [(u32, f64)],
    /// Sorted-by-offset MIDI events for this block.
    pub midi: &'a [PluginMidiEvent],
    /// Sorted-by-offset per-note expression events for this block.
    pub note_expressions: &'a [PluginNoteExpression],
}

impl<'a> PluginEvents<'a> {
    pub const EMPTY: PluginEvents<'static> = PluginEvents {
        params: &[],
        midi: &[],
        note_expressions: &[],
    };
}

/// One parameter exposed by the plugin.
#[derive(Clone, Debug)]
pub struct PluginParamInfo {
    pub id: u32,
    pub name: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
}

/// Factory turning a built-in FX name/ident into a live DSP
/// [`PluginInstance`].
///
/// The built-in FX themselves live ABOVE this crate (`signal-fx` wraps
/// the DSP cores, and depends on the `daw` facade — a direct dep here
/// would be a cycle), so the factory is **injected**: the app installs
/// one via `Standalone::set_fx_factory`, and `Effects::add` consults
/// it for any name the plugin-file backends (CLAP/VST3) don't claim —
/// the same `plugin_instances` seam signal rigs already use via
/// `Standalone::insert_plugin_instance`.
pub trait FxFactory: Send + Sync {
    /// Catalog of creatable built-ins. Appended to
    /// `Effects::list_installed` so browsers can offer them.
    fn installed(&self) -> Vec<daw_proto::fx::InstalledFx>;

    /// Instantiate `name_or_ident` at `sample_rate` (the instance is
    /// re-`prepare`d at the stream's real rate by the renderer).
    /// `None` if the name isn't one of this factory's built-ins.
    fn create(&self, name_or_ident: &str, sample_rate: f64) -> Option<Box<dyn PluginInstance>>;

    /// Whether `name_or_ident` is one of this factory's built-ins —
    /// without instantiating it.
    ///
    /// The project loader asks this before it goes looking for a plugin
    /// bundle on disk (which a built-in never has), so an FX a saved
    /// project names by its factory name comes back through
    /// [`create`](Self::create). The default matches the
    /// [`installed`](Self::installed) catalog by name or ident; a factory
    /// that recognises more spellings than it lists overrides it.
    fn provides(&self, name_or_ident: &str) -> bool {
        self.installed()
            .iter()
            .any(|fx| fx.name == name_or_ident || fx.ident == name_or_ident)
    }
}

/// A boxed, format-neutral plugin instance.
///
/// All methods take `&mut self` because plugin internals are
/// inherently stateful (sample rate, activation state, parameter
/// values). The renderer holds these inside a `Mutex` and acquires
/// it per audio callback.
pub trait PluginInstance: Send {
    /// Metadata about the plugin (id / name / vendor / format).
    fn descriptor(&self) -> PluginDescriptor;

    /// All parameters this plugin exposes.
    fn params(&mut self) -> Vec<PluginParamInfo>;

    /// Current value of a parameter, if the plugin can report it.
    fn param_value(&mut self, id: u32) -> Option<f64>;

    /// Format the value for display (e.g. `"-12 dB"`). `None` if the
    /// plugin doesn't provide a formatter.
    fn value_to_text(&mut self, id: u32, value: f64) -> Option<String>;

    /// Inverse of `value_to_text` — parse `"−12 dB"` back to a value.
    fn text_to_value(&mut self, id: u32, text: &str) -> Option<f64>;

    /// A stepped parameter's choices — a mode, a switch, a scene: each
    /// its value normalized along the parameter's range (0..1) and its
    /// name. What a host lists to pick from and parses a name against.
    /// Empty (the default) for a continuous parameter.
    fn param_choices(&mut self, _id: u32) -> Vec<(f64, String)> {
        Vec::new()
    }

    /// Reported plugin latency in samples (0 if not reported).
    fn latency(&mut self) -> u32;

    /// Activate the plugin at the given audio config. Must be called
    /// before any `process_block` calls.
    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError>;

    /// Whether `prepare` has been called and the plugin is ready.
    fn is_prepared(&self) -> bool;

    /// Whether this plugin may process on a render worker thread, at the
    /// same time as other tracks' plugins. Only a plugin that shares no
    /// per-block state with any other track may say yes: the guide
    /// instruments, which hand cues between tracks keyed by
    /// [`render_cycle`], must run on the render thread in track order. A
    /// track is rendered in parallel only when every plugin on it says yes.
    fn parallel_safe(&self) -> bool {
        false
    }

    /// Render one block. `events` carries per-block parameter changes
    /// and MIDI input (needed for virtual instruments like CLAPi /
    /// VST3i, which produce audio from note events rather than
    /// processing an audio input).
    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError>;

    /// The sidechain for the next [`process_block`](Self::process_block)
    /// only: what the track received on channels 3/4 this block (a
    /// drum's trigger keying its gate). Not called on a block with
    /// nothing there, so the plugin hears silence. Ignored by default —
    /// a plugin without a sidechain input has nowhere to put it.
    fn set_sidechain(&mut self, _left: &[f32], _right: &[f32]) {}

    /// An output beyond the main pair, after a
    /// [`process_block`](Self::process_block): pair 0 is the track's
    /// channels 3/4, pair 1 its 5/6 — what a send from those channels
    /// takes (a kit's overhead mics on 3/4, sent into the overheads).
    /// `None` for a pair the plugin does not have, which is every pair by
    /// default.
    fn aux_out(&self, _pair: usize) -> Option<(&[f32], &[f32])> {
        None
    }

    /// The song's tempo for the next [`process_block`](Self::process_block),
    /// bpm, from the project's tempo map at the block's start: what a
    /// plugin's note-valued times read (a synced delay, a reverb's tail or
    /// pre-delay in notes). A renderer that knows the song calls it before
    /// every block; a plugin with no use for a tempo ignores it.
    fn set_tempo(&mut self, _bpm: f64) {}

    /// Restore plugin state from a host-defined byte blob. The
    /// blob format is backend-specific — for round-trip safety the
    /// blob must have been produced by [`save_state`] from a plugin
    /// of the same backend, or built from a host-side parser (see
    /// [`crate::rpp_state`] for REAPER `.rpp` chunk decoding).
    ///
    /// Calling this on an *activated* plugin is allowed but
    /// implementation-defined — VST3 plugins generally tolerate it,
    /// CLAP plugins typically need a deactivate/load/activate cycle
    /// for some param changes to apply.
    fn load_state(&mut self, _state: &[u8]) -> Result<(), PluginError> {
        Err(PluginError::UnsupportedFormat(self.descriptor().format))
    }

    /// Save plugin state to a host-defined byte blob suitable for
    /// passing back to [`load_state`] on a fresh instance of the same
    /// backend.
    fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        Err(PluginError::UnsupportedFormat(self.descriptor().format))
    }

    /// Open the plugin's editor UI, if the backend/plugin supports it.
    fn open_gui(&mut self) -> Result<(), PluginError> {
        Err(PluginError::UnsupportedFormat(self.descriptor().format))
    }

    /// Close a previously-opened plugin editor UI. Backends that do
    /// not support plugin editors may treat this as a no-op.
    fn close_gui(&mut self) -> Result<(), PluginError> {
        Ok(())
    }

    /// Release activation. Idempotent — calling twice is fine.
    fn deactivate(&mut self);

    /// Downcast hook for native (built-in Rust DSP) instances whose control
    /// surface isn't expressed as host params. Hosts that mutate a concrete
    /// instance in place (e.g. Signal's NAM amp trims via
    /// [`Standalone::with_plugin_instance`](crate::sync::Standalone::with_plugin_instance))
    /// downcast through this. Defaults to `None` for hosted CLAP/VST3 plugins,
    /// which expose their parameters through the host param path instead.
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        None
    }
}

/// Errors any plugin backend can raise.
#[derive(Debug)]
pub enum PluginError {
    /// Path didn't resolve or the bundle couldn't be opened.
    LoadFailed(String),
    /// `process_block` called before `prepare`.
    NotActivated,
    /// Activation failed (plugin rejected the config).
    ActivateFailed(String),
    /// Block size exceeded the prepared max.
    BlockTooLarge,
    /// Backend reported an error during processing.
    ProcessFailed(String),
    /// Backend not compiled in (e.g. `vst3` backend requested
    /// without the `vst3-host` feature).
    UnsupportedFormat(PluginFormat),
}

impl fmt::Display for PluginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoadFailed(s) => write!(f, "plugin load failed: {s}"),
            Self::NotActivated => write!(f, "plugin not activated"),
            Self::ActivateFailed(s) => write!(f, "plugin activation failed: {s}"),
            Self::BlockTooLarge => write!(f, "process block exceeds prepared size"),
            Self::ProcessFailed(s) => write!(f, "plugin process failed: {s}"),
            Self::UnsupportedFormat(fmt_) => {
                write!(f, "plugin format not supported in this build: {fmt_:?}")
            }
        }
    }
}

impl std::error::Error for PluginError {}

pub use thread_serialized::ThreadSerialized;

#[allow(unsafe_code)]
mod thread_serialized {
    /// Marker wrapper that asserts (via unsafe) that the wrapped value
    /// is safe to send between threads as long as all access is
    /// serialized externally (e.g. through a `Mutex`).
    ///
    /// Lives here (always-on) so format-specific host modules can share
    /// it without depending on each other's feature gates.
    pub struct ThreadSerialized<T>(T);

    impl<T> ThreadSerialized<T> {
        /// Construct a `ThreadSerialized<T>` wrapper.
        ///
        /// # Safety
        ///
        /// The caller must guarantee that the wrapped value is only ever
        /// accessed from one thread at a time. The DAW enforces this by
        /// storing instances inside `Mutex<HashMap<..>>`.
        pub unsafe fn new(value: T) -> Self {
            Self(value)
        }
    }

    unsafe impl<T> Send for ThreadSerialized<T> {}

    impl<T> core::ops::Deref for ThreadSerialized<T> {
        type Target = T;
        fn deref(&self) -> &T {
            &self.0
        }
    }
    impl<T> core::ops::DerefMut for ThreadSerialized<T> {
        fn deref_mut(&mut self) -> &mut T {
            &mut self.0
        }
    }
}

/// Format-aware instantiator. Tries to load `name_or_path` using the
/// first backend that recognizes the format. Returns
/// `PluginError::UnsupportedFormat` if no backend can handle it.
///
/// `Synthetic` returns `Ok(None)` — caller falls back to the
/// existing synthetic FX path (see `crate::fx`).
pub fn load_plugin(name_or_path: &str) -> Result<Option<Box<dyn PluginInstance>>, PluginError> {
    let format = PluginFormat::from_path_or_name(name_or_path);
    match format {
        PluginFormat::Synthetic => Ok(None),
        PluginFormat::Clap => load_clap(name_or_path),
        PluginFormat::Vst3 => load_vst3(name_or_path),
        PluginFormat::Lv2 => Err(PluginError::UnsupportedFormat(format)),
    }
}

#[cfg(feature = "clap-host")]
fn load_clap(path: &str) -> Result<Option<Box<dyn PluginInstance>>, PluginError> {
    use crate::audio_engine::plugin_host::ClapHost;
    let host = ClapHost::default();
    let plugin = host
        .load(Path::new(path), 0)
        .map_err(|e| PluginError::LoadFailed(format!("{e:?}")))?;
    // Wrap so the instance is `Send` for storage in the shared
    // `Mutex<HashMap<..>>`. Safety: all access goes through the
    // mutex, so concurrent thread access is impossible.
    Ok(Some(Box::new(plugin.into_send())))
}

#[cfg(not(feature = "clap-host"))]
fn load_clap(_path: &str) -> Result<Option<Box<dyn PluginInstance>>, PluginError> {
    Err(PluginError::UnsupportedFormat(PluginFormat::Clap))
}

#[cfg(feature = "vst3-host")]
fn load_vst3(path: &str) -> Result<Option<Box<dyn PluginInstance>>, PluginError> {
    use crate::audio_engine::vst3_host::Vst3Host;
    let plugin = Vst3Host::new()
        .load(Path::new(path), 0)
        .map_err(|e| PluginError::LoadFailed(format!("{e:?}")))?;
    Ok(Some(Box::new(plugin.into_send())))
}

#[cfg(not(feature = "vst3-host"))]
fn load_vst3(_path: &str) -> Result<Option<Box<dyn PluginInstance>>, PluginError> {
    Err(PluginError::UnsupportedFormat(PluginFormat::Vst3))
}

// Convenience so `Path` import on the cfg(feature) branch is used.
#[allow(dead_code)]
fn _path_import(_: &Path) {}

// ── Render context ──────────────────────────────────────────────────────

std::thread_local! {
    static RENDER_FRAME: std::cell::Cell<Option<(u64, u64)>> = const { std::cell::Cell::new(None) };
}

/// Every block any renderer renders gets the next number, so "this block"
/// is unambiguous even when a loop or a seek renders the same frames again.
static RENDER_CYCLE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Where on the timeline the block being processed starts, in output
/// samples — `Some` only while the project renderer is calling plugins.
///
/// `PluginEvents` carries no transport, and a plugin normally needs none.
/// This is for the few that must agree with each other about *when*: the
/// guide instruments on the Count and Guide tracks, where a spoken cue
/// takes the place of the count note under it.
#[must_use]
pub fn render_frame() -> Option<u64> {
    RENDER_FRAME
        .with(std::cell::Cell::get)
        .map(|(_, frame)| frame)
}

/// Which render pass this is — a number no other block shares. Plugins
/// that pass information between tracks within a block key it by this,
/// not by [`render_frame`]: the same frames are rendered again on every
/// loop pass and after a seek back.
#[must_use]
pub fn render_cycle() -> Option<u64> {
    RENDER_FRAME
        .with(std::cell::Cell::get)
        .map(|(cycle, _)| cycle)
}

/// Publishes [`render_frame`] for the life of the guard.
pub(crate) struct RenderFrameGuard;

impl RenderFrameGuard {
    pub(crate) fn enter(start_frame: u64) -> Self {
        let cycle = RENDER_CYCLE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        RENDER_FRAME.with(|c| c.set(Some((cycle, start_frame))));
        Self
    }
}

impl RenderFrameGuard {
    /// This thread's render context, for a worker to [`adopt`](Self::adopt).
    pub(crate) fn current() -> Option<(u64, u64)> {
        RENDER_FRAME.with(std::cell::Cell::get)
    }

    /// Publish the render thread's context on a worker rendering part of
    /// the same block — the same cycle, not a new one.
    pub(crate) fn adopt(context: Option<(u64, u64)>) -> Self {
        RENDER_FRAME.with(|c| c.set(context));
        Self
    }
}

impl Drop for RenderFrameGuard {
    fn drop(&mut self) {
        RENDER_FRAME.with(|c| c.set(None));
    }
}

std::thread_local! {
    static TRACK_MUTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the track whose FX chain is being processed is muted. Mute is
/// applied at the fader, after the chain, so a muted track's plugins still
/// run — this is how one knows its output will not be heard.
#[must_use]
pub fn track_muted() -> bool {
    TRACK_MUTED.with(std::cell::Cell::get)
}

/// Publishes [`track_muted`] while a track's chain is processed.
pub(crate) struct TrackMutedGuard;

impl TrackMutedGuard {
    pub(crate) fn enter(muted: bool) -> Self {
        TRACK_MUTED.with(|c| c.set(muted));
        Self
    }
}

impl Drop for TrackMutedGuard {
    fn drop(&mut self) {
        TRACK_MUTED.with(|c| c.set(false));
    }
}
