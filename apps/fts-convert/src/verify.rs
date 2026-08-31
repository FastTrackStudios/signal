//! Rendering the original and the replacement, and reporting the gap.
//!
//! A converter that only rewrites text is asking to be trusted on the
//! strength of its author's confidence. This is the part that earns it: each
//! converted instance is loaded back into the real FabFilter plugin, the
//! parameters we wrote are loaded into the real FTS EQ, both are fed the same
//! stimulus, and the two are compared band by band.
//!
//! It runs the FTS **plugin**, not the FTS engine, on purpose. The engine has
//! already been measured against Pro-Q across the whole factory library; what
//! has not been measured is the layer this converter adds — the parameter
//! names and the two unit conversions between the engine and the plugin. A
//! swapped shape index or a Q off by √2 is invisible in a diff and obvious
//! here.

use signal_analyzer::eq_transfer::{self, Difference, Stimulus};
use signal_fx::NativeEq;
use signal_plugin_host::{HostedPlugin, PluginEvents, PluginInstance};

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;
/// The level the preset library was measured at.
const LEVEL_DBFS: f64 = -18.8;

/// A loaded pair of plugins, kept across instances — loading a bridged plugin
/// costs seconds and a project has dozens of instances.
pub struct Rig {
    reference: HostedPlugin,
    ours: HostedPlugin,
    dry: (Vec<f32>, Vec<f32>),
}

#[derive(Debug)]
pub enum RigError {
    NotFound(String),
    Load(String),
}

impl std::fmt::Display for RigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RigError::NotFound(p) => write!(f, "no plugin at {p}"),
            RigError::Load(p) => write!(f, "could not load {p}"),
        }
    }
}

impl Rig {
    pub fn open(reference: &str, ours: &str) -> Result<Rig, RigError> {
        let load = |path: &str| -> Result<HostedPlugin, RigError> {
            if !std::path::Path::new(path).exists() {
                return Err(RigError::NotFound(path.into()));
            }
            match HostedPlugin::load(path) {
                Ok(Some(mut p)) => {
                    p.prepare(SR, BLOCK as u32)
                        .map_err(|_| RigError::Load(path.into()))?;
                    Ok(p)
                }
                _ => Err(RigError::Load(path.into())),
            }
        };
        let frames = eq_transfer::frames_needed();
        let amplitude = 10.0f64.powf(LEVEL_DBFS / 20.0);
        Ok(Rig {
            reference: load(reference)?,
            ours: load(ours)?,
            dry: eq_transfer::stimulus(frames, amplitude, Stimulus::Flat, SR),
        })
    }

    /// Measure one instance: `reference_state` is what the project held,
    /// `our_state` is what we are about to write in its place.
    pub fn compare(&mut self, reference_state: &[u8], our_state: &[u8]) -> Option<Difference> {
        self.reference.load_state(reference_state).ok()?;
        self.ours.load_state(our_state).ok()?;
        let a = render(&mut self.reference, &self.dry.0, &self.dry.1)?;
        let b = render(&mut self.ours, &self.dry.0, &self.dry.1)?;
        Some(eq_transfer::difference(
            (&self.dry.0, &self.dry.1),
            (&a.0, &a.1),
            (&b.0, &b.1),
            SR,
        ))
    }

    /// The same comparison against the EQ **engine** driven by the translated
    /// parameters, skipping the plugin entirely.
    ///
    /// This is the diagnostic that says which half is at fault. The engine
    /// has been measured against Pro-Q across the whole factory library; if
    /// it agrees here and the plugin does not, the gap is in the parameter
    /// map this converter writes, not in the DSP.
    pub fn compare_engine(
        &mut self,
        reference_state: &[u8],
        native_params: &[(String, f64)],
    ) -> Option<Difference> {
        self.reference.load_state(reference_state).ok()?;
        let a = render(&mut self.reference, &self.dry.0, &self.dry.1)?;
        let b = render_engine(native_params, &self.dry.0, &self.dry.1);
        Some(eq_transfer::difference(
            (&self.dry.0, &self.dry.1),
            (&a.0, &a.1),
            (&b.0, &b.1),
            SR,
        ))
    }
}

impl Rig {
    /// Load our state into the FTS plugin and ask it what it ended up with.
    ///
    /// The shortest path from "the plugin disagrees" to "which parameter" —
    /// nih-plug writes plain values, so the round trip is directly readable
    /// and anything that failed to land shows up as a difference.
    pub fn readback(&mut self, our_state: &[u8]) -> Option<Vec<u8>> {
        self.ours.load_state(our_state).ok()?;
        self.ours.save_state().ok()
    }
}

/// The three mid-channel response curves for one instance, on the shared
/// third-octave grid: Pro-Q, the FTS plugin, the FTS engine.
///
/// A single number says two engines disagree; only the curves say where and
/// by how much, which is the difference between a hypothesis and a diagnosis.
impl Rig {
    pub fn curves(
        &mut self,
        reference_state: &[u8],
        our_state: &[u8],
        native_params: &[(String, f64)],
    ) -> Option<(Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>)> {
        let centres = eq_transfer::band_centres();
        self.reference.load_state(reference_state).ok()?;
        let a = render(&mut self.reference, &self.dry.0, &self.dry.1)?;
        self.ours.load_state(our_state).ok()?;
        let b = render(&mut self.ours, &self.dry.0, &self.dry.1)?;
        let c = render_engine(native_params, &self.dry.0, &self.dry.1);
        let mid = |p: &(Vec<f32>, Vec<f32>)| {
            let (m, _) = eq_transfer::to_ms(&p.0, &p.1);
            eq_transfer::spectrum(&m)
        };
        let dry = mid(&self.dry.clone());
        Some((
            eq_transfer::response_db(&dry, &mid(&a), &centres, SR),
            eq_transfer::response_db(&dry, &mid(&b), &centres, SR),
            eq_transfer::response_db(&dry, &mid(&c), &centres, SR),
            centres,
        ))
    }
}

fn render_engine(params: &[(String, f64)], left: &[f32], right: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let mut eq = NativeEq::new(SR);
    for (name, value) in params {
        eq.set_named(name, *value);
    }
    eq.prepare(SR, BLOCK as u32).expect("prepare");
    let events = PluginEvents::default();
    let (mut ol, mut or) = (Vec::with_capacity(left.len()), Vec::with_capacity(left.len()));
    let mut pos = 0;
    while pos < left.len() {
        let n = BLOCK.min(left.len() - pos);
        let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
        eq.process_block(&left[pos..pos + n], &right[pos..pos + n], &mut l, &mut r, &events)
            .expect("process");
        ol.extend_from_slice(&l);
        or.extend_from_slice(&r);
        pos += n;
    }
    (ol, or)
}

fn render(plugin: &mut HostedPlugin, left: &[f32], right: &[f32]) -> Option<(Vec<f32>, Vec<f32>)> {
    let (mut ol, mut or) = (Vec::with_capacity(left.len()), Vec::with_capacity(left.len()));
    let mut pos = 0;
    while pos < left.len() {
        let n = BLOCK.min(left.len() - pos);
        let mut buf = vec![0.0f32; n * 2];
        for i in 0..n {
            buf[2 * i] = left[pos + i];
            buf[2 * i + 1] = right[pos + i];
        }
        plugin.process_interleaved(&mut buf, &[], &[]).ok()?;
        ol.extend((0..n).map(|i| buf[2 * i]));
        or.extend((0..n).map(|i| buf[2 * i + 1]));
        pos += n;
    }
    Some((ol, or))
}

/// Where the plugins live, unless told otherwise.
pub fn default_reference() -> String {
    // FabFilter ships Windows binaries; on Linux they are reached through
    // yabridge, which is where the CLAP bridge puts them.
    home(".clap/yabridge/FabFilter Pro-Q 4.clap")
}

pub fn default_ours() -> String {
    home(".clap/FTS EQ.clap")
}

fn home(rest: &str) -> String {
    std::env::var("HOME")
        .map(|h| format!("{h}/{rest}"))
        .unwrap_or_else(|_| rest.to_string())
}
