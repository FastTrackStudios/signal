//! Signal's CLAP backend, on the desktop (signal#26 phase 5): a block run
//! by a CLAP through daw's plugin host (`signal-plugin-host`, clack).
//!
//! - [`HostedClap`]: a [`ClapInstance`] over a loaded CLAP. Parameter
//!   values and modulation go to it as sample-accurate
//!   `CLAP_EVENT_PARAM_VALUE` / `CLAP_EVENT_PARAM_MOD`, by the CLAP ids
//!   its `signal/1` maps to the block's stable ids; its modulation sources
//!   come back over `signal/1`.
//! - [`ClapFactory`]: a [`BlockFactory`] for the CLAP backend: third-party
//!   plugins by id, and Signal's own blocks overridden by their CLAP form
//!   (the compiled-in block's parameter schema, so a preset doesn't change).
//! - [`BlockDev`]: the hot-reload override: watch a block's built `.clap`;
//!   on a rebuild copy it to a fresh name (a library is never unloaded and
//!   reloaded under one name), build the new backend for every block of
//!   that kind and swap them in at a block boundary, the song playing on.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use daw::plugin::{
    LoadOptions, PluginInstance, PluginParamEvent, PluginParamEventKind, load_plugin_with,
};
use daw_clap_ext::host::{HostActions, SignalPluginClient};
use signal_core::block::{
    Backend, BlockFactory, BlockKind, BuildContext, Built, ClapInstance, ClapParamEvent,
    ClapParamKind, HostedPlugin, Midi, PluginFormat, Unavailable,
};
use signal_core::engine::Command;
use signal_core::param::ParamSpec;
use signal_core::platform::Platforms;
use signal_core::state::{BlockSpec, Preset};

/// Events per piece a [`HostedClap`] passes on (preallocated).
const EVENT_CAPACITY: usize = 512;

/// A loaded CLAP as a block's backend.
pub struct HostedClap {
    plugin: Box<dyn PluginInstance>,
    signal: Option<SignalPluginClient>,
    /// The input, copied out: the chain processes in place, the host API
    /// takes separate input and output.
    in_l: Vec<f32>,
    in_r: Vec<f32>,
    events: Vec<PluginParamEvent>,
    max_block: usize,
    /// Read once, at activation.
    latency: u32,
}

impl HostedClap {
    /// Load `path` (a `.clap`), activate it for `sample_rate` and blocks of
    /// `max_block`, offering it `actions`.
    ///
    /// # Errors
    ///
    /// Why it couldn't be loaded or activated.
    pub fn load(
        path: &Path,
        sample_rate: f32,
        max_block: usize,
        actions: Option<Arc<dyn HostActions>>,
    ) -> Result<Self, Unavailable> {
        let options = LoadOptions {
            actions,
            ..LoadOptions::default()
        };
        let mut plugin = load_plugin_with(&path.to_string_lossy(), &options)
            .map_err(|e| Unavailable::Failed {
                why: format!("{}: {e}", path.display()),
            })?
            .ok_or_else(|| Unavailable::Failed {
                why: format!("{}: not a CLAP", path.display()),
            })?;
        plugin
            .prepare(
                f64::from(sample_rate),
                u32::try_from(max_block).unwrap_or(1024),
            )
            .map_err(|e| Unavailable::Failed {
                why: format!("{}: {e}", path.display()),
            })?;
        let signal = plugin.signal();
        let latency = plugin.latency();
        Ok(Self {
            latency,
            plugin,
            signal,
            in_l: vec![0.0; max_block],
            in_r: vec![0.0; max_block],
            events: Vec::with_capacity(EVENT_CAPACITY),
            max_block,
        })
    }

    /// The CLAP id of each of `specs`, in order: through `signal/1`'s
    /// stable ids, else by name. `None` where the CLAP has no such
    /// parameter.
    #[must_use]
    pub fn param_ids(&mut self, specs: &[ParamSpec]) -> Vec<Option<u32>> {
        let infos = self.plugin.params();
        specs
            .iter()
            .map(|spec| {
                infos
                    .iter()
                    .find(|info| match &self.signal {
                        Some(s) => s.param_stable_id(info.id).as_deref() == Some(spec.id),
                        None => info.name == spec.name,
                    })
                    .map(|info| info.id)
            })
            .collect()
    }

    /// The plugin's `signal/1`, if it has it.
    #[must_use]
    pub const fn signal(&self) -> Option<SignalPluginClient> {
        self.signal
    }

    /// Load `algorithm` in a family CLAP (`SIGNAL_ALGORITHM` over
    /// `signal/1`), then run what it asked the host's main thread to (its
    /// rescan).
    ///
    /// # Errors
    ///
    /// When the CLAP has no `signal/1`, or refuses the algorithm.
    pub fn select_algorithm(&mut self, algorithm: &str) -> Result<(), String> {
        let signal = self.signal.ok_or("no signal/1: not a family CLAP")?;
        let args = format!("{{\"algorithm\":{}}}", json_string(algorithm));
        signal
            .run_action("SIGNAL_ALGORITHM", &args)
            .map_err(|e| e.to_string())?;
        self.plugin.pump_main_thread();
        Ok(())
    }

    /// The plugin, for an embedder's own calls (its parameters' names after
    /// a rescan, its main thread).
    pub fn plugin_mut(&mut self) -> &mut dyn PluginInstance {
        &mut *self.plugin
    }
}

impl ClapInstance for HostedClap {
    fn process(&mut self, left: &mut [f32], right: &mut [f32], events: &[ClapParamEvent]) {
        let n = left.len().min(right.len()).min(self.max_block);
        let (Some(il), Some(ir)) = (self.in_l.get_mut(..n), self.in_r.get_mut(..n)) else {
            return;
        };
        il.copy_from_slice(&left[..n]);
        ir.copy_from_slice(&right[..n]);
        self.events.clear();
        for e in events {
            if self.events.len() == self.events.capacity() {
                break;
            }
            let kind = match e.kind {
                ClapParamKind::Value(v) => PluginParamEventKind::Value(v),
                ClapParamKind::Mod(m) => PluginParamEventKind::Mod(m),
            };
            self.events.push(PluginParamEvent {
                offset: e.offset,
                param_id: e.param_id,
                kind,
            });
        }
        let (il, ir) = (&self.in_l[..n], &self.in_r[..n]);
        if self
            .plugin
            .process_block_param_events(il, ir, &mut left[..n], &mut right[..n], &self.events, &[])
            .is_err()
        {
            // A block the CLAP refused passes through, rather than
            // playing silence into the rest of the chain.
            left[..n].copy_from_slice(il);
            right[..n].copy_from_slice(ir);
        }
    }

    fn reset(&mut self) {
        // CLAP's reset is the plugin's own on (re)activation; a fresh
        // activation is what a swap gives.
    }

    fn latency(&self) -> u32 {
        self.latency
    }

    fn save_state(&mut self) -> Option<Vec<u8>> {
        self.plugin.save_state().ok()
    }

    fn load_state(&mut self, state: &[u8]) -> bool {
        self.plugin.load_state(state).is_ok()
    }

    fn mod_source(&self, index: usize) -> Option<f32> {
        let index = u32::try_from(index).ok()?;
        self.signal?.mod_source_value(index).map(|v| v as f32)
    }

    fn midi(&mut self, _event: Midi) {}

    fn describe(&self) -> Option<String> {
        self.signal?.chain().ok()
    }
}

/// The CLAP backend's factory.
#[derive(Default)]
pub struct ClapFactory {
    /// Third-party plugins: CLAP id → its `.clap`.
    pub installed: HashMap<String, PathBuf>,
    /// Signal's own blocks overridden by their CLAP: block name → `.clap`.
    pub overrides: HashMap<String, PathBuf>,
    /// Offered to every CLAP this loads, as `host-actions/1`.
    pub actions: Option<Arc<dyn HostActions>>,
}

impl ClapFactory {
    fn build_from(
        &self,
        path: &Path,
        specs: &'static [ParamSpec],
        sources: &'static [signal_core::block::ModSourceSpec],
        algorithm: Option<&str>,
        cx: &BuildContext<'_>,
    ) -> Result<Built, Unavailable> {
        let mut clap = HostedClap::load(path, cx.sample_rate, cx.max_block, self.actions.clone())?;
        // A family CLAP runs whichever algorithm it is told: the block's,
        // before its parameters are mapped (their stable ids follow it).
        if let Some(algorithm) = algorithm {
            clap.select_algorithm(algorithm)
                .map_err(|why| Unavailable::Failed {
                    why: format!("{}: {why}", path.display()),
                })?;
        }
        let ids = clap.param_ids(specs);
        if let Some((spec, _)) = specs.iter().zip(&ids).find(|(_, id)| id.is_none()) {
            return Err(Unavailable::Failed {
                why: format!(
                    "{}: no parameter {} (not this block's CLAP form?)",
                    path.display(),
                    spec.id
                ),
            });
        }
        let ids = ids.into_iter().flatten().collect();
        let hosted = HostedPlugin::new(Box::new(clap), ids, path.to_string_lossy());
        Ok(Built {
            backend: Backend::Clap(hosted),
            params: specs,
            sources,
        })
    }
}

impl BlockFactory for ClapFactory {
    fn runs_on(&self, kind: &BlockKind) -> Option<Platforms> {
        match kind {
            BlockKind::Signal(name) => self
                .overrides
                .contains_key(name)
                .then_some(Platforms::DESKTOP),
            BlockKind::Plugin(p) => (p.format == PluginFormat::Clap
                && self.installed.contains_key(&p.id))
            .then_some(Platforms::DESKTOP),
        }
    }

    fn build(&self, spec: &BlockSpec, cx: &BuildContext<'_>) -> Result<Built, Unavailable> {
        match &spec.kind {
            BlockKind::Signal(name) => {
                let path = self.overrides.get(name).ok_or(Unavailable::UnknownKind)?;
                // The algorithm's controls are the schema, whichever backend.
                let algo = plugin_processors::algorithms::find(name, spec.algorithm.as_deref())
                    .ok_or_else(|| Unavailable::Missing {
                        what: format!("{name} algorithm {:?}", spec.algorithm),
                    })?;
                self.build_from(
                    path,
                    algo.controls,
                    plugin_processors::sources_of(name),
                    Some(algo.id),
                    cx,
                )
            }
            BlockKind::Plugin(p) => {
                let path = self
                    .installed
                    .get(&p.id)
                    .ok_or_else(|| Unavailable::Missing {
                        what: p.name.clone(),
                    })?;
                // A third-party plugin's parameters aren't Signal's schema
                // (yet): its state is what the preset keeps.
                let mut built = self.build_from(path, &[], &[], None, cx)?;
                if let (Backend::Clap(hosted), Some(state)) =
                    (&mut built.backend, spec.state.as_deref())
                {
                    let bytes = decode_base64(state).ok_or_else(|| Unavailable::Failed {
                        why: format!("{}: bad state", p.name),
                    })?;
                    if !hosted.instance.load_state(&bytes) {
                        tracing::warn!(plugin = %p.name, "third-party state not restored");
                    }
                }
                Ok(built)
            }
        }
    }
}

/// `s` as a JSON string literal.
fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn decode_base64(s: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .ok()
}

// ── The hot-reload override ─────────────────────────────────────────────

/// One block under development: overridden by its CLAP, which is watched
/// and swapped in again on every rebuild.
pub struct BlockDev {
    /// The block's name (`comp`).
    pub block: String,
    /// The built `.clap` to watch.
    pub clap: PathBuf,
    /// Where fresh copies go.
    pub copies: PathBuf,
    seen: Option<SystemTime>,
    generation: u32,
}

impl BlockDev {
    #[must_use]
    pub fn new(
        block: impl Into<String>,
        clap: impl Into<PathBuf>,
        copies: impl Into<PathBuf>,
    ) -> Self {
        Self {
            block: block.into(),
            clap: clap.into(),
            copies: copies.into(),
            seen: None,
            generation: 0,
        }
    }

    /// A fresh copy of the `.clap` if it was built since the last poll
    /// (the first poll always gives one). Control thread.
    ///
    /// # Errors
    ///
    /// When the build can't be read or copied.
    pub fn poll(&mut self) -> std::io::Result<Option<PathBuf>> {
        let modified = std::fs::metadata(&self.clap)?.modified()?;
        if self.seen == Some(modified) {
            return Ok(None);
        }
        self.seen = Some(modified);
        self.generation = self.generation.saturating_add(1);
        std::fs::create_dir_all(&self.copies)?;
        let stem = self
            .clap
            .file_stem()
            .map_or_else(|| "block".into(), |s| s.to_string_lossy().into_owned());
        let fresh = self.copies.join(format!(
            "{stem}-{}-{}.clap",
            std::process::id(),
            self.generation
        ));
        std::fs::copy(&self.clap, &fresh)?;
        tracing::info!(block = %self.block, copy = %fresh.display(), generation = self.generation, "block CLAP rebuilt: a fresh copy");
        Ok(Some(fresh))
    }

    /// The swaps that put `clap` in place of every block of this kind in
    /// `preset`'s graph (built for `port`). Each backend is built with the
    /// compiled-in block's schema; the chain replays its parameters into
    /// it, so the preset is unchanged.
    ///
    /// # Errors
    ///
    /// Why the CLAP couldn't stand in for the block.
    pub fn swaps(
        &self,
        clap: &Path,
        preset: &Preset,
        port: Option<&str>,
        cx: &BuildContext<'_>,
        actions: Option<Arc<dyn HostActions>>,
    ) -> Result<Vec<Command>, Unavailable> {
        let factory = ClapFactory {
            overrides: HashMap::from([(self.block.clone(), clap.to_path_buf())]),
            actions,
            ..ClapFactory::default()
        };
        self.each_block(preset, port, |spec| factory.build(spec, cx))
    }

    /// The swaps that end the override: the compiled-in block back.
    ///
    /// # Errors
    ///
    /// When the block can't be built.
    pub fn end(
        &self,
        preset: &Preset,
        port: Option<&str>,
        cx: &BuildContext<'_>,
    ) -> Result<Vec<Command>, Unavailable> {
        self.each_block(preset, port, |spec| {
            signal_core::BuiltinFactory.build(spec, cx)
        })
    }

    /// A swap for every block of this kind, each built from its own spec
    /// (its algorithm: the CLAP form runs the same one, with the same
    /// schema, so the chain's parameters replay into it unchanged).
    fn each_block(
        &self,
        preset: &Preset,
        port: Option<&str>,
        mut build: impl FnMut(&BlockSpec) -> Result<Built, Unavailable>,
    ) -> Result<Vec<Command>, Unavailable> {
        let kind = BlockKind::Signal(self.block.clone());
        let mut out = Vec::new();
        for (lane, spec) in signal_core::graph::lanes_for(preset, port)
            .iter()
            .enumerate()
        {
            for (block, b) in spec
                .blocks
                .iter()
                .enumerate()
                .filter(|(_, b)| b.kind == kind)
            {
                let built = build(b)?;
                out.push(Command::SwapBackend {
                    lane,
                    block,
                    backend: Box::new(built.backend),
                });
            }
        }
        Ok(out)
    }
}
