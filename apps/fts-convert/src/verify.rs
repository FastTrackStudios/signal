//! Rendering the original and the replacement, and reporting the gap.
//!
//! A converter that only rewrites text is asking to be trusted on the
//! strength of its author's confidence. This is the part that earns it: each
//! converted instance is loaded back into the real `FabFilter` plugin, the
//! parameters we wrote are loaded into the real FTS EQ, both are fed the same
//! stimulus, and the two are compared band by band.
//!
//! It runs the FTS **plugin**, not the FTS engine, on purpose. The engine has
//! already been measured against Pro-Q across the whole factory library; what
//! has not been measured is the layer this converter adds — the parameter
//! names and the unit conversions between the engine and the plugin. A
//! swapped shape index or a Q off by √2 is invisible in a diff and obvious
//! here, and a state that named only the parameters its preset used — so
//! that everything else inherited the preceding preset — was found exactly
//! this way, six dB deep and otherwise silent.
//!
//! The stimulus is broadband noise, which is the right question for an
//! equalizer and only half of it for a compressor: it says the two agree on
//! how much they are pulling down at each frequency, not that they agree on
//! how they get there. Reading a compressor properly wants programme
//! material with a crest factor; this is the floor, not the ceiling.
//!
//! ## Pro-C 3 cannot be measured here yet
//!
//! Hosted through `signal-plugin-host`, Pro-C 3 outputs silence — at its own
//! default, with no state loaded, where Pro-Q 4 through the same path
//! renders correctly. The host feeds a plugin one stereo input and nothing
//! else, and Pro-C declares a side-chain bus it never receives; that is the
//! likely cause, and it is not settled. What matters here is that the
//! comparison knows the difference between "these disagree" and "one of them
//! said nothing", and reports the second as itself rather than as an
//! infinite error.

use signal_analyzer::eq_transfer::{self, Difference, Stimulus};
use signal_fx::NativeEq;
use signal_import::rpp::convert::Family;
use signal_plugin_host::{HostedPlugin, PluginEvents, PluginInstance};

const SR: f64 = 48_000.0;
const BLOCK: usize = 512;
/// The level the preset library was measured at.
const LEVEL_DBFS: f64 = -18.8;

/// A loaded pair of plugins, kept across instances — loading a bridged plugin
/// costs seconds and a project has dozens of instances.
/// What `curves` returns, in order: the reference plugin's response, our
/// converted state rendered through the same plugin, our native engine's
/// response, and the band centres all three are sampled at. Every response is
/// in dB relative to the dry signal.
pub type Curves = (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>);

pub struct Rig {
    /// One pair per family, opened lazily — a project full of equalizers
    /// should not pay to bridge a compressor it never meets.
    pairs: Vec<(Family, HostedPlugin, HostedPlugin)>,
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
            Self::NotFound(p) => write!(f, "no plugin at {p}"),
            Self::Load(p) => write!(f, "could not load {p}"),
        }
    }
}

impl Rig {
    /// Open with no plugins loaded; each family is bridged on first use.
    pub fn new() -> Self {
        Self {
            pairs: Vec::new(),
            dry: eq_transfer::stimulus(
                eq_transfer::frames_needed(),
                10.0f64.powf(LEVEL_DBFS / 20.0),
                Stimulus::Flat,
                SR,
            ),
        }
    }

    /// Where a family's two plugins live, unless overridden.
    ///
    /// `FabFilter` ship Windows binaries, so on Linux theirs are reached
    /// through yabridge; ours are native and sit directly in `~/.clap`.
    fn paths(family: Family) -> (String, String) {
        let home = |rest: &str| {
            std::env::var("HOME").map_or_else(|_| rest.to_string(), |h| format!("{h}/{rest}"))
        };
        match family {
            Family::ProQ4 => (
                home(".clap/yabridge/FabFilter Pro-Q 4.clap"),
                home(".clap/FTS EQ.clap"),
            ),
            Family::ProC3 => (
                home(".clap/yabridge/FabFilter Pro-C 3.clap"),
                home(".clap/FTS Comp.clap"),
            ),
        }
    }

    fn load(path: &str) -> Result<HostedPlugin, RigError> {
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
    }

    /// The pair for `family`, bridging them the first time they are asked
    /// for. Loading a bridged plugin costs seconds and a project holds dozens
    /// of instances, so they are kept for the whole run.
    fn pair(&mut self, family: Family) -> Result<usize, RigError> {
        if let Some(i) = self.pairs.iter().position(|(f, _, _)| *f == family) {
            return Ok(i);
        }
        let (reference, ours) = Self::paths(family);
        let pair = (family, Self::load(&reference)?, Self::load(&ours)?);
        self.pairs.push(pair);
        Ok(self.pairs.len() - 1)
    }

    /// Measure one instance: `reference_state` is what the project held,
    /// `our_state` is what we are about to write in its place.
    pub fn compare(
        &mut self,
        family: Family,
        reference_state: &[u8],
        our_state: &[u8],
    ) -> Option<Difference> {
        let i = self.pair(family).ok()?;
        let (a, b) = {
            let (_, reference, ours) = &mut self.pairs[i];
            reference.load_state(reference_state).ok()?;
            ours.load_state(our_state).ok()?;
            let a = render(reference, &self.dry.0, &self.dry.1)?;
            let b = render(ours, &self.dry.0, &self.dry.1)?;
            (a, b)
        };
        // A silent side is not a large error, it is an absent measurement,
        // and saying "inf dB" for it invites someone to go looking for a
        // translation bug that is not there.
        if silent(&a.0) || silent(&b.0) {
            return None;
        }
        Some(eq_transfer::difference(
            (&self.dry.0, &self.dry.1),
            (&a.0, &a.1),
            (&b.0, &b.1),
            SR,
        ))
    }

    /// The same comparison against the **engine** driven by the translated
    /// parameters, skipping our plugin entirely.
    ///
    /// The diagnostic that says which half is at fault: the EQ engine has
    /// been measured against Pro-Q across the whole factory library, so if it
    /// agrees here and the plugin does not, the gap is in the parameter map
    /// this converter writes rather than in the DSP.
    pub fn compare_engine(
        &mut self,
        family: Family,
        reference_state: &[u8],
        native_params: &[(String, f64)],
    ) -> Option<Difference> {
        if family != Family::ProQ4 {
            return None;
        }
        let i = self.pair(family).ok()?;
        let a = {
            let (_, reference, _) = &mut self.pairs[i];
            reference.load_state(reference_state).ok()?;
            render(reference, &self.dry.0, &self.dry.1)?
        };
        let b = render_engine(native_params, &self.dry.0, &self.dry.1);
        Some(eq_transfer::difference(
            (&self.dry.0, &self.dry.1),
            (&a.0, &a.1),
            (&b.0, &b.1),
            SR,
        ))
    }

    /// The three mid-channel response curves for one instance, on the shared
    /// third-octave grid: the reference plugin, ours, and the engine.
    ///
    /// A single number says two engines disagree; only the curves say where
    /// and by how much, which is the difference between a hypothesis and a
    /// diagnosis.
    pub fn curves(
        &mut self,
        family: Family,
        reference_state: &[u8],
        our_state: &[u8],
        native_params: &[(String, f64)],
    ) -> Option<Curves> {
        let centres = eq_transfer::band_centres();
        let i = self.pair(family).ok()?;
        let (a, b) = {
            let (_, reference, ours) = &mut self.pairs[i];
            reference.load_state(reference_state).ok()?;
            let a = render(reference, &self.dry.0, &self.dry.1)?;
            ours.load_state(our_state).ok()?;
            let b = render(ours, &self.dry.0, &self.dry.1)?;
            (a, b)
        };
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

/// True when a render carries essentially no energy — a plugin that declined
/// to process rather than one that processed to nothing.
fn silent(v: &[f32]) -> bool {
    let e: f64 = v.iter().map(|x| f64::from(*x) * f64::from(*x)).sum();
    e / v.len().max(1) as f64 <= 1.0e-12
}

fn render_engine(params: &[(String, f64)], left: &[f32], right: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let mut eq = NativeEq::new(SR);
    for (name, value) in params {
        eq.set_named(name, *value);
    }
    eq.prepare(SR, BLOCK as u32).expect("prepare");
    let events = PluginEvents::default();
    let (mut ol, mut or) = (
        Vec::with_capacity(left.len()),
        Vec::with_capacity(left.len()),
    );
    let mut pos = 0;
    while pos < left.len() {
        let n = BLOCK.min(left.len() - pos);
        let (mut l, mut r) = (vec![0.0f32; n], vec![0.0f32; n]);
        eq.process_block(
            &left[pos..pos + n],
            &right[pos..pos + n],
            &mut l,
            &mut r,
            &events,
        )
        .expect("process");
        ol.extend_from_slice(&l);
        or.extend_from_slice(&r);
        pos += n;
    }
    (ol, or)
}

fn render(plugin: &mut HostedPlugin, left: &[f32], right: &[f32]) -> Option<(Vec<f32>, Vec<f32>)> {
    let (mut ol, mut or) = (
        Vec::with_capacity(left.len()),
        Vec::with_capacity(left.len()),
    );
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
