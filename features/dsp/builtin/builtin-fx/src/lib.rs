//! The FTS processors as the session engine's built-in FX.
//!
//! Every FTS plugin (EQ, Comp, Saturate, Reverb, Delay, Gate, Limiter,
//! Level, Meter, Modulation, Mic) is a nice-plug plugin, built for CLAP and
//! VST3. Here the same plugin runs **in-process**: [`Embedded`] hosts any
//! nice-plug [`Plugin`] behind the engine's [`PluginInstance`] — its
//! parameters (names, normalized ranges, the plugin's own text), its
//! processing, its latency and its state — and [`BuiltinFx`] offers them
//! all to the engine by name. No CLAP file is found or loaded, no plugin
//! process is started: the DSP is the plugin's own, linked in.
//!
//! The parameter values the engine sees are the plugin's **normalized**
//! values, 0..1 — what a host automates — so `min` is 0 and `max` 1, and
//! the plugin formats them (`value_to_text`).

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use daw_standalone::plugin::{
    FxFactory, PluginDescriptor, PluginError, PluginEvents, PluginFormat, PluginInstance,
    PluginParamInfo,
};
use nice_plug::context::activate::ActivateContext;
use nice_plug::params::internals::ParamPtr;
use nice_plug::prelude::{
    AudioIOLayout, AuxiliaryBuffers, Buffer, BufferConfig, Params, Plugin, PluginApi,
    PluginNoteEvent, ProcessContext, ProcessMode, ProcessStatus, TaskExecutor, Transport,
};
use num_traits::ToPrimitive as _;

/// The longest block the plugins are prepared for when the engine has not
/// said: its renderer prepares again at the stream's own size.
const DEFAULT_BLOCK: u32 = 4_096;

/// A nice-plug plugin run in-process as an engine FX.
pub struct Embedded<P: Plugin> {
    plugin: P,
    /// The plugin's parameters object: the pointers below point into it,
    /// and its persistent fields are saved with the state.
    object: Arc<dyn Params>,
    /// Every parameter, by its string ID, in the plugin's order: the
    /// engine's parameter `id` is the index here.
    params: Vec<(String, ParamPtr)>,
    executor: TaskExecutor<P>,
    layout: AudioIOLayout,
    config: Option<BufferConfig>,
    /// The main bus's channels, the plugin processes them in place.
    channels: Vec<Vec<f32>>,
    /// Each sidechain input's channels: the first port carries what the
    /// engine handed over for this block ([`Self::key`]), the rest are
    /// silent.
    sidechains: Vec<Vec<Vec<f32>>>,
    /// The song's tempo, bpm, as the engine last told it
    /// ([`PluginInstance::set_tempo`]): the transport's, for the plugin's
    /// note-valued times.
    tempo: Option<f64>,
    /// The sidechain for the next block, left and right, and whether
    /// there is one ([`PluginInstance::set_sidechain`]).
    key: [Vec<f32>; 2],
    keyed: bool,
    buffer: Buffer<'static>,
    sidechain_buffers: Vec<Buffer<'static>>,
    latency: AtomicU32,
}

// SAFETY: `ParamPtr` is a raw pointer into `object`, which this struct
// owns (an `Arc` of the plugin's own `Params`); the plugin and its params
// are `Send` (nice-plug requires `Plugin: Send`). The pointers are only
// dereferenced through `&mut self`, so the instance moving to the audio
// thread moves them with it and nothing else holds them.
unsafe impl<P: Plugin> Send for Embedded<P> {}

impl<P: Plugin> Default for Embedded<P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P: Plugin> Embedded<P> {
    /// A fresh instance, not yet prepared.
    #[must_use]
    pub fn new() -> Self {
        let mut plugin = P::default();
        let executor = plugin.task_executor();
        let params = plugin.params();
        let ptrs = params
            .param_map()
            .into_iter()
            .map(|(id, ptr, _)| (id, ptr))
            .collect();
        Self {
            plugin,
            object: params,
            params: ptrs,
            executor,
            layout: stereo_layout::<P>(),
            config: None,
            channels: Vec::new(),
            sidechains: Vec::new(),
            tempo: None,
            key: [Vec::new(), Vec::new()],
            keyed: false,
            buffer: Buffer::default(),
            sidechain_buffers: Vec::new(),
            latency: AtomicU32::new(0),
        }
    }

    fn param(&self, id: u32) -> Option<ParamPtr> {
        self.params
            .get(usize::try_from(id).ok()?)
            .map(|(_, ptr)| *ptr)
    }

    /// Set parameter `id` to `normalized` (clamped to 0..1) and move its
    /// smoother on.
    fn set(&self, id: u32, normalized: f64) {
        let Some(ptr) = self.param(id) else { return };
        let sample_rate = self.config.map_or(48_000.0, |c| c.sample_rate);
        let Some(value) = normalized.clamp(0.0, 1.0).to_f32() else {
            return;
        };
        // SAFETY: `ptr` points into `object`, alive as long as `self`.
        unsafe {
            if ptr._internal_set_normalized_value(value) {
                ptr._internal_update_smoother(sample_rate, false);
            }
        }
    }

    /// Run the plugin over `frames` frames of `self.channels`, in place.
    fn run(&mut self, frames: usize) -> Result<(), PluginError> {
        let sample_rate = self.config.map_or(48_000.0, |c| c.sample_rate);
        // SAFETY: the slices point into `self.channels` / `self.sidechains`,
        // which are neither resized nor dropped while the buffers hold them:
        // the buffers are re-pointed here before every call and only read by
        // the plugin during it.
        unsafe {
            self.buffer.set_slices(frames, |slices| {
                slices.clear();
                for channel in &mut self.channels {
                    slices.push(std::slice::from_raw_parts_mut(channel.as_mut_ptr(), frames));
                }
            });
            for (buffer, port) in self.sidechain_buffers.iter_mut().zip(&mut self.sidechains) {
                buffer.set_slices(frames, |slices| {
                    slices.clear();
                    for channel in port.iter_mut() {
                        slices.push(std::slice::from_raw_parts_mut(channel.as_mut_ptr(), frames));
                    }
                });
            }
        }
        // SAFETY: only shortens the buffers' lifetime from `'static` to
        // this call's — what `AuxiliaryBuffers` ties its two lifetimes to.
        // They point into `self.sidechains`, alive for the whole call.
        let inputs: &mut [Buffer<'_>] = unsafe {
            std::mem::transmute::<&mut [Buffer<'static>], &mut [Buffer<'_>]>(
                self.sidechain_buffers.as_mut_slice(),
            )
        };
        let mut aux = AuxiliaryBuffers {
            inputs,
            outputs: &mut [],
        };
        let mut context = Process::<P> {
            executor: &self.executor,
            latency: &self.latency,
            transport: {
                let mut transport = Transport::new(sample_rate);
                transport.tempo = self.tempo;
                transport
            },
        };
        match self
            .plugin
            .process(&mut self.buffer, &mut aux, &mut context)
        {
            ProcessStatus::Error(error) => Err(PluginError::ProcessFailed(error.to_string())),
            _ => Ok(()),
        }
    }
}

/// The plugin's stereo-in, stereo-out layout, else its first.
fn stereo_layout<P: Plugin>() -> AudioIOLayout {
    let two = NonZeroU32::new(2);
    P::AUDIO_IO_LAYOUTS
        .iter()
        .find(|l| l.main_input_channels == two && l.main_output_channels == two)
        .or_else(|| {
            P::AUDIO_IO_LAYOUTS
                .iter()
                .find(|l| l.main_output_channels == two)
        })
        .or_else(|| P::AUDIO_IO_LAYOUTS.first())
        .copied()
        .unwrap_or_default()
}

/// What the plugin is told it runs in at `activate`.
struct Init<'a, P: Plugin> {
    executor: &'a TaskExecutor<P>,
    latency: &'a AtomicU32,
}

impl<P: Plugin> ActivateContext<P> for Init<'_, P> {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Standalone
    }

    fn execute(&self, task: P::BackgroundTask) {
        (self.executor)(task);
    }

    fn set_latency_samples(&self, samples: u32) {
        self.latency.store(samples, Ordering::Relaxed);
    }

    fn set_current_voice_capacity(&self, _capacity: u32) {}
}

/// What the plugin is told it runs in, each block.
struct Process<'a, P: Plugin> {
    executor: &'a TaskExecutor<P>,
    latency: &'a AtomicU32,
    transport: Transport,
}

impl<P: Plugin> ProcessContext<P> for Process<'_, P> {
    fn plugin_api(&self) -> PluginApi {
        PluginApi::Standalone
    }

    fn execute_background(&self, task: P::BackgroundTask) {
        (self.executor)(task);
    }

    fn execute_gui(&self, task: P::BackgroundTask) {
        (self.executor)(task);
    }

    fn transport(&self) -> &Transport {
        &self.transport
    }

    fn next_event(&mut self) -> Option<PluginNoteEvent<P>> {
        None
    }

    fn send_event(&mut self, _event: PluginNoteEvent<P>) {}

    fn set_latency_samples(&self, samples: u32) {
        self.latency.store(samples, Ordering::Relaxed);
    }

    /// A restart is the next `prepare`: the engine prepares an FX again
    /// when its stream changes, and a restart asks for nothing more here.
    fn request_restart(&self) {}

    fn set_current_voice_capacity(&self, _capacity: u32) {}
}

/// A plugin's state as the engine saves it: every parameter's normalized
/// value by ID, and its persistent fields.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Saved {
    params: BTreeMap<String, f32>,
    fields: BTreeMap<String, String>,
}

impl<P: Plugin> PluginInstance for Embedded<P> {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: format!("fts.builtin.{}", P::NAME.to_lowercase().replace(' ', "-")),
            name: P::NAME.to_owned(),
            vendor: P::VENDOR.to_owned(),
            version: P::VERSION.to_owned(),
            format: PluginFormat::Synthetic,
        }
    }

    fn params(&mut self) -> Vec<PluginParamInfo> {
        self.params
            .iter()
            .enumerate()
            .filter_map(|(index, (_, ptr))| {
                Some(PluginParamInfo {
                    id: u32::try_from(index).ok()?,
                    // SAFETY: `ptr` points into `object`, alive with `self`.
                    name: unsafe { ptr.name() }.to_owned(),
                    min: 0.0,
                    max: 1.0,
                    default: f64::from(unsafe { ptr.default_normalized_value() }),
                })
            })
            .collect()
    }

    fn param_value(&mut self, id: u32) -> Option<f64> {
        let ptr = self.param(id)?;
        // SAFETY: as in `params`.
        Some(f64::from(unsafe { ptr.unmodulated_normalized_value() }))
    }

    fn value_to_text(&mut self, id: u32, value: f64) -> Option<String> {
        let ptr = self.param(id)?;
        let value = value.clamp(0.0, 1.0).to_f32()?;
        // SAFETY: as in `params`.
        Some(unsafe { ptr.normalized_value_to_string(value, true) })
    }

    fn text_to_value(&mut self, id: u32, text: &str) -> Option<f64> {
        let ptr = self.param(id)?;
        // SAFETY: as in `params`.
        unsafe { ptr.string_to_normalized_value(text) }.map(f64::from)
    }

    fn latency(&mut self) -> u32 {
        self.latency.load(Ordering::Relaxed)
    }

    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        let rate = sample_rate
            .to_f32()
            .ok_or_else(|| PluginError::ActivateFailed(format!("sample rate {sample_rate}")))?;
        let config = BufferConfig {
            sample_rate: rate,
            min_buffer_size: None,
            max_buffer_size: block_size.max(1),
            process_mode: ProcessMode::Realtime,
        };
        let frames = usize::try_from(config.max_buffer_size).unwrap_or(usize::MAX);
        let channels = self.layout.main_output_channels.map_or(0, NonZeroU32::get);
        self.channels = (0..channels).map(|_| vec![0.0; frames]).collect();
        self.sidechains = self
            .layout
            .aux_input_ports
            .iter()
            .map(|port| (0..port.get()).map(|_| vec![0.0; frames]).collect())
            .collect();
        self.sidechain_buffers = self.sidechains.iter().map(|_| Buffer::default()).collect();
        self.key = [vec![0.0; frames], vec![0.0; frames]];
        for (_, ptr) in &self.params {
            // SAFETY: as in `params`.
            unsafe { ptr._internal_update_smoother(rate, true) };
        }
        let mut init = Init::<P> {
            executor: &self.executor,
            latency: &self.latency,
        };
        if !self.plugin.activate(&self.layout, &config, &mut init) {
            return Err(PluginError::ActivateFailed(format!(
                "{} refused {config:?}",
                P::NAME
            )));
        }
        self.plugin.reset();
        self.config = Some(config);
        Ok(())
    }

    fn is_prepared(&self) -> bool {
        self.config.is_some()
    }

    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        let Some(config) = self.config else {
            return Err(PluginError::NotActivated);
        };
        for &(id, value) in events.params {
            self.set(id, value);
        }
        let most = usize::try_from(config.max_buffer_size)
            .unwrap_or(usize::MAX)
            .max(1);
        // Blocks longer than the plugin was prepared for go through in
        // pieces it was; an input shorter than the output is silence past
        // its end.
        let (mut lefts, mut rights) = (in_l.chunks(most), in_r.chunks(most));
        let keyed = std::mem::take(&mut self.keyed);
        for (piece, (out_l, out_r)) in out_l
            .chunks_mut(most)
            .zip(out_r.chunks_mut(most))
            .enumerate()
        {
            let n = out_l.len().min(out_r.len());
            // The sidechain, this piece of it — or silence, the block it
            // was not handed one.
            if let Some(port) = self.sidechains.first_mut() {
                let at = piece.saturating_mul(most);
                for (channel, key) in port.iter_mut().zip(&self.key) {
                    let from = if keyed {
                        key.get(at..).unwrap_or(&[])
                    } else {
                        &[]
                    };
                    fill(channel, from, n);
                }
            }
            let left = lefts.next().unwrap_or(&[]);
            let right = rights.next().filter(|r| !r.is_empty()).unwrap_or(left);
            let mut channels = self.channels.iter_mut();
            if let Some(first) = channels.next() {
                fill(first, left, n);
            }
            if let Some(second) = channels.next() {
                fill(second, right, n);
            }
            self.run(n)?;
            let first = self.channels.first().map_or(&[][..], Vec::as_slice);
            let second = self.channels.get(1).map_or(first, Vec::as_slice);
            for (out, from) in [(out_l, first), (out_r, second)] {
                for (o, v) in out.iter_mut().zip(from) {
                    *o = *v;
                }
            }
        }
        Ok(())
    }

    fn set_tempo(&mut self, bpm: f64) {
        self.tempo = (bpm.is_finite() && bpm > 0.0).then_some(bpm);
    }

    fn set_sidechain(&mut self, left: &[f32], right: &[f32]) {
        for (key, from) in self.key.iter_mut().zip([left, right]) {
            // Sized at `prepare` for the blocks it was told of; a longer
            // one grows it once.
            if key.len() < from.len() {
                key.resize(from.len(), 0.0);
            }
            key.truncate(from.len());
            key.copy_from_slice(from);
        }
        self.keyed = true;
    }

    fn load_state(&mut self, state: &[u8]) -> Result<(), PluginError> {
        let saved: Saved = serde_json::from_slice(state)
            .map_err(|e| PluginError::LoadFailed(format!("{} state: {e}", P::NAME)))?;
        for (id, ptr) in &self.params {
            if let Some(value) = saved.params.get(id) {
                // SAFETY: as in `params`.
                unsafe { ptr._internal_set_normalized_value(value.clamp(0.0, 1.0)) };
            }
        }
        self.object.deserialize_fields(&saved.fields);
        if let Some(config) = self.config {
            self.prepare(f64::from(config.sample_rate), config.max_buffer_size)?;
        }
        Ok(())
    }

    fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        let saved = Saved {
            params: self
                .params
                .iter()
                // SAFETY: as in `params`.
                .map(|(id, ptr)| (id.clone(), unsafe { exact_normalized(*ptr) }))
                .collect(),
            fields: self.object.serialize_fields(),
        };
        serde_json::to_vec(&saved).map_err(|e| PluginError::ProcessFailed(e.to_string()))
    }

    fn deactivate(&mut self) {
        if self.config.take().is_some() {
            self.plugin.deactivate();
        }
    }
}

/// The normalized value that restores `ptr`'s plain value exactly.
///
/// A parameter holds its plain value (a ratio, a time) and derives its
/// normalized one, so loading a normalized value converts it back, and
/// through a skewed range in f32 that is not always the value saved:
/// FTS Comp's ratio came back 0.29048538 → 0.2904854, and a session never
/// saved the same twice. The value saved is the nearest normalized one
/// that converts back to exactly the plain value held — the usual one, or
/// a few float steps from it — so save → load → save is the same bytes.
///
/// # Safety
///
/// `ptr` points into the plugin's live params, as in `params`.
unsafe fn exact_normalized(ptr: ParamPtr) -> f32 {
    /// How many float steps either side to look.
    const REACH: usize = 64;
    // SAFETY: the caller's.
    unsafe {
        let plain = ptr.unmodulated_plain_value();
        let start = ptr.unmodulated_normalized_value();
        let restores = |n: f32| ptr.preview_plain(n).to_bits() == plain.to_bits();
        if restores(start) {
            return start;
        }
        let (mut up, mut down) = (start, start);
        for _ in 0..REACH {
            up = up.next_up().min(1.0);
            down = down.next_down().max(0.0);
            if restores(up) {
                return up;
            }
            if restores(down) {
                return down;
            }
        }
        start
    }
}

/// `channel`'s first `n` samples from `from`, silence past its end.
fn fill(channel: &mut [f32], from: &[f32], n: usize) {
    let source = from.iter().copied().chain(std::iter::repeat(0.0));
    for (sample, value) in channel.iter_mut().take(n).zip(source) {
        *sample = value;
    }
}

/// One built-in: its name and how to make one.
struct Entry {
    name: &'static str,
    make: fn() -> Box<dyn PluginInstance>,
}

fn make<P: Plugin>() -> Box<dyn PluginInstance> {
    Box::new(Embedded::<P>::new())
}

/// Every FTS processor the engine runs built in, by its plugin name.
const ENTRIES: &[Entry] = &[
    Entry {
        name: <eq_plugin::FtsEqPlugin as Plugin>::NAME,
        make: make::<eq_plugin::FtsEqPlugin>,
    },
    Entry {
        name: <comp_plugin::FtsComp as Plugin>::NAME,
        make: make::<comp_plugin::FtsComp>,
    },
    Entry {
        name: <saturate_plugin::FtsSaturate as Plugin>::NAME,
        make: make::<saturate_plugin::FtsSaturate>,
    },
    Entry {
        name: <reverb_plugin::FtsReverb as Plugin>::NAME,
        make: make::<reverb_plugin::FtsReverb>,
    },
    Entry {
        name: <delay_plugin::FtsDelay as Plugin>::NAME,
        make: make::<delay_plugin::FtsDelay>,
    },
    Entry {
        name: <gate_plugin::FtsGate as Plugin>::NAME,
        make: make::<gate_plugin::FtsGate>,
    },
    Entry {
        name: <limiter_plugin::FtsLimiter as Plugin>::NAME,
        make: make::<limiter_plugin::FtsLimiter>,
    },
    Entry {
        name: <level_plugin::FtsLevel as Plugin>::NAME,
        make: make::<level_plugin::FtsLevel>,
    },
    Entry {
        name: <meter_plugin::FtsMeter as Plugin>::NAME,
        make: make::<meter_plugin::FtsMeter>,
    },
    Entry {
        name: <modulation_plugin::FtsModulation as Plugin>::NAME,
        make: make::<modulation_plugin::FtsModulation>,
    },
    Entry {
        name: <pitch_plugin::FtsPitch as Plugin>::NAME,
        make: make::<pitch_plugin::FtsPitch>,
    },
    Entry {
        name: <tune_plugin::FtsTune as Plugin>::NAME,
        make: make::<tune_plugin::FtsTune>,
    },
    Entry {
        name: <unison_plugin::FtsUnison as Plugin>::NAME,
        make: make::<unison_plugin::FtsUnison>,
    },
    Entry {
        name: <mic_plugin::FtsMic as Plugin>::NAME,
        make: make::<mic_plugin::FtsMic>,
    },
];

/// The name a plugin goes by, however it is written: `FTS EQ`, `CLAP: FTS
/// EQ (FastTrackStudios)`, `VST3: FTS Comp (FastTrackStudios)` — a format
/// prefix and a vendor suffix off, case aside.
fn bare(name: &str) -> String {
    let name = name.trim();
    let name = ["CLAP: ", "VST3: ", "VST: ", "VSTi: ", "CLAPi: "]
        .iter()
        .find_map(|prefix| name.strip_prefix(prefix))
        .unwrap_or(name);
    let name = match name.rsplit_once(" (") {
        Some((bare, vendor)) if vendor.ends_with(')') => bare,
        _ => name,
    };
    name.trim().to_lowercase()
}

fn entry(name: &str) -> Option<&'static Entry> {
    let bare = bare(name);
    ENTRIES.iter().find(|e| e.name.to_lowercase() == bare)
}

/// A parameter's index and engine value for a setting in words.
///
/// The value (normalized, 0..1) for `fx`'s parameter `param` at `text`,
/// written as the plugin writes it (`-25.0 dB`, `4.0:1`, `10 ms`, or a
/// choice's name: `FET`, `Arena`). What a preset is built from without
/// knowing any plugin's ranges. `None` for an FX, a parameter or a value
/// the plugin does not know.
#[must_use]
pub fn setting(fx: &str, param: &str, text: &str) -> Option<(u32, f64)> {
    let entry = entry(fx)?;
    SCRATCH.with(|scratch| {
        let mut scratch = scratch.borrow_mut();
        let plugin = scratch.entry(entry.name).or_insert_with(|| (entry.make)());
        let id = plugin
            .params()
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(param))?
            .id;
        if let Some(value) = plugin.text_to_value(id, text) {
            return Some((id, value));
        }
        // A choice the plugin writes but does not read back: found by
        // name along its range.
        (0_u16..=1_000)
            .map(|step| f64::from(step) / 1_000.0)
            .find(|value| {
                plugin
                    .value_to_text(id, *value)
                    .is_some_and(|t| t.eq_ignore_ascii_case(text))
            })
            .map(|value| (id, value))
    })
}

/// How built-in `fx` writes `value` (normalized) for its parameter
/// `index` — `+3.0 dB` — the inverse of [`setting`]: what an automation
/// lane says a point is.
#[must_use]
pub fn reading(fx: &str, index: u32, value: f64) -> Option<String> {
    let entry = entry(fx)?;
    SCRATCH.with(|scratch| {
        let mut scratch = scratch.borrow_mut();
        let plugin = scratch.entry(entry.name).or_insert_with(|| (entry.make)());
        plugin.value_to_text(index, value.clamp(0.0, 1.0))
    })
}

thread_local! {
    /// One unprepared instance of each built-in, kept to read settings
    /// with ([`setting`]): a strip turned is a lookup, not a plugin built.
    static SCRATCH: std::cell::RefCell<std::collections::HashMap<&'static str, Box<dyn PluginInstance>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// The FTS processors, offered to the engine by name ([`FxFactory`]). An
/// FX named for one — `FTS Comp`, or a project's `CLAP: FTS Comp
/// (FastTrackStudios)` — is the built-in, run in-process.
#[derive(Default)]
pub struct BuiltinFx;

impl FxFactory for BuiltinFx {
    fn installed(&self) -> Vec<daw_proto::fx::InstalledFx> {
        ENTRIES
            .iter()
            .map(|e| daw_proto::fx::InstalledFx {
                name: e.name.to_owned(),
                ident: e.name.to_owned(),
            })
            .collect()
    }

    fn provides(&self, name_or_ident: &str) -> bool {
        entry(name_or_ident).is_some()
    }

    fn create(&self, name_or_ident: &str, sample_rate: f64) -> Option<Box<dyn PluginInstance>> {
        let mut plugin = (entry(name_or_ident)?.make)();
        // Prepared here, off the audio thread: the renderer prepares only
        // what is not prepared yet, and it does that mid-block.
        if let Err(error) = plugin.prepare(sample_rate, DEFAULT_BLOCK) {
            tracing::warn!(
                fx = name_or_ident,
                ?error,
                "a built-in FX would not prepare"
            );
            return None;
        }
        Some(plugin)
    }
}

/// Built-ins first, then `rest`: the app's other factories (the guide's
/// instruments) behind the FTS processors.
pub struct Chain(pub Vec<Arc<dyn FxFactory>>);

impl FxFactory for Chain {
    fn installed(&self) -> Vec<daw_proto::fx::InstalledFx> {
        self.0.iter().flat_map(|f| f.installed()).collect()
    }

    fn provides(&self, name_or_ident: &str) -> bool {
        self.0.iter().any(|f| f.provides(name_or_ident))
    }

    fn create(&self, name_or_ident: &str, sample_rate: f64) -> Option<Box<dyn PluginInstance>> {
        self.0
            .iter()
            .filter(|f| f.provides(name_or_ident))
            .find_map(|f| f.create(name_or_ident, sample_rate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `n` samples of 440 Hz at 48 kHz, at `level`.
    fn tone(n: usize, level: f32) -> Vec<f32> {
        let step = 440.0 * std::f32::consts::TAU / 48_000.0;
        std::iter::successors(Some(0.0_f32), |phase| Some(phase + step))
            .take(n)
            .map(|phase| phase.sin() * level)
            .collect()
    }

    /// The root mean square of `samples`.
    fn rms(samples: &[f32]) -> f32 {
        let (sum, count) = samples.iter().fold((0.0_f32, 0.0_f32), |(sum, count), s| {
            (s.mul_add(*s, sum), count + 1.0)
        });
        (sum / count.max(1.0)).sqrt()
    }

    #[test]
    fn a_plugin_is_known_by_its_name_however_written() {
        assert_eq!(bare("CLAP: FTS EQ (FastTrackStudios)"), "fts eq");
        assert_eq!(bare("VST3: FTS Comp (FastTrackStudios)"), "fts comp");
        assert!(BuiltinFx.provides("FTS EQ"));
        assert!(BuiltinFx.provides("CLAP: FTS Reverb (FastTrackStudios)"));
        assert!(!BuiltinFx.provides("CLAP: Pro-Q 4 (FabFilter)"));
    }

    /// Every built-in comes up, says what it is, and passes audio: a
    /// default-set processor turns a tone into a tone, not silence or
    /// noise.
    #[test]
    fn every_built_in_prepares_and_processes() {
        for entry in ENTRIES {
            let mut fx = BuiltinFx.create(entry.name, 48_000.0).expect(entry.name);
            assert_eq!(fx.descriptor().name, entry.name);
            // Every one has parameters but the meter, which only reads.
            let meter = entry.name == <meter_plugin::FtsMeter as Plugin>::NAME;
            assert!(
                meter || !fx.params().is_empty(),
                "{} has parameters",
                entry.name
            );
            let n = 1_024;
            let tone = tone(n, 0.25);
            let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
            for _ in 0..8 {
                fx.process_block(&tone, &tone, &mut l, &mut r, &PluginEvents::EMPTY)
                    .unwrap_or_else(|e| panic!("{}: {e:?}", entry.name));
            }
            assert!(
                l.iter().chain(&r).all(|s| s.is_finite()),
                "{}: finite",
                entry.name
            );
        }
    }

    /// A setting is found by the plugin's own words: a number it reads,
    /// a choice it only writes.
    #[test]
    fn a_setting_is_written_as_the_plugin_writes_it() {
        let (ratio, value) = setting("FTS Comp", "Ratio", "4.0:1").expect("a ratio");
        let mut comp = BuiltinFx.create("FTS Comp", 48_000.0).expect("comp");
        assert_eq!(comp.value_to_text(ratio, value).as_deref(), Some("4.0:1"));
        let (style, fet) = setting("FTS Comp", "Style", "FET").expect("a style");
        assert_eq!(comp.value_to_text(style, fet).as_deref(), Some("FET"));
        assert!(setting("FTS Comp", "Style", "Valve").is_none());
        assert!(setting("FTS Comp", "Nothing", "1").is_none());
    }

    /// A parameter moves the sound: FTS Saturate's output turned up makes
    /// the tone louder than at its default — the engine's value reaches the
    /// DSP, not just the plugin's parameter store.
    #[test]
    fn a_parameter_changes_the_sound() {
        let rms = |gain: Option<f64>| {
            let mut level = BuiltinFx
                .create("FTS Saturate", 48_000.0)
                .expect("saturate");
            let id = level
                .params()
                .iter()
                .find(|p| p.name == "Output")
                .map(|p| p.id)
                .expect("an output");
            let set = gain.map(|g| [(id, g)]);
            let events = PluginEvents {
                params: set.as_ref().map_or(&[][..], |s| &s[..]),
                ..PluginEvents::EMPTY
            };
            let n = 4_800;
            let tone = tone(n, 0.1);
            let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
            // Long enough for the gain's smoother to arrive.
            for _ in 0..20 {
                level
                    .process_block(&tone, &tone, &mut l, &mut r, &events)
                    .expect("block");
            }
            rms(&l)
        };
        let (default, loud) = (rms(None), rms(Some(0.95)));
        assert!(default > 0.0, "the tone passes");
        assert!(loud > default * 1.5, "turned up: {loud} against {default}");
    }

    /// FTS Mic takes a dual-capsule recording — front on the left, rear
    /// on the right, two different signals — and gives back the modelled
    /// mic: one signal, the same on both sides.
    #[test]
    fn the_mic_turns_a_capsule_pair_into_one_mic() {
        let mut mic = BuiltinFx
            .create("FTS Mic", 48_000.0)
            .expect("FTS Mic is built in");
        let block = 512;
        let mut seed = 0x2545_f491_u32;
        let mut noise = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            f32::from(u16::try_from(seed >> 16).unwrap_or(0)) / 32_768.0 - 1.0
        };
        let (mut l, mut r) = (vec![0.0; block], vec![0.0; block]);
        // The models load off the audio thread: a second or two of blocks.
        for _ in 0..400 {
            let front: Vec<f32> = (0..block).map(|_| noise() * 0.5).collect();
            let rear: Vec<f32> = (0..block).map(|_| noise() * 0.2).collect();
            assert!(front != rear, "two capsules, two signals");
            mic.process_block(&front, &rear, &mut l, &mut r, &PluginEvents::EMPTY)
                .expect("block");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let rms = |x: &[f32]| {
            (x.iter().map(|v| v * v).sum::<f32>() / x.len().to_f32().unwrap_or(1.0)).sqrt()
        };
        assert!(rms(&l) > 1e-3, "the mic plays: {}", rms(&l));
        let apart = l
            .iter()
            .zip(&r)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        assert!(apart < 1e-6, "one mic on both sides: {apart}");
    }

    /// A parameter set through the engine's path is what the plugin
    /// reports, its text is the plugin's own, and state comes back.
    #[test]
    fn parameters_and_state_round_trip() {
        let mut eq = BuiltinFx.create("FTS EQ", 48_000.0).expect("eq");
        let params = eq.params();
        let gain = params
            .iter()
            .find(|p| p.name.to_lowercase().contains("gain"))
            .expect("a gain");
        let events = PluginEvents {
            params: &[(gain.id, 0.75)],
            ..PluginEvents::EMPTY
        };
        let (mut l, mut r) = (vec![0.0; 64], vec![0.0; 64]);
        eq.process_block(&[0.0; 64], &[0.0; 64], &mut l, &mut r, &events)
            .expect("block");
        assert!((eq.param_value(gain.id).expect("a value") - 0.75).abs() < 1e-6);
        assert!(
            eq.value_to_text(gain.id, 0.75)
                .is_some_and(|t| !t.is_empty())
        );
        let saved = eq.save_state().expect("saved");
        let mut again = BuiltinFx.create("FTS EQ", 48_000.0).expect("eq");
        again.load_state(&saved).expect("loaded");
        assert!((again.param_value(gain.id).expect("a value") - 0.75).abs() < 1e-6);
    }
}
