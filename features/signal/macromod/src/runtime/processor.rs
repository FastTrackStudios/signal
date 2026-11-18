//! Modulation processor — orchestrates all modulation sources and produces per-target offsets.
//!
//! The processor holds parallel arrays of [`ModulationRoute`] configs and runtime
//! `SourceState` instances. Calling `tick()` advances all sources and sums their
//! contributions per [`ParamTarget`], producing bipolar offsets that the downstream
//! runtime adds to tracked base values.

use std::collections::HashMap;

use crate::routing::ModulationRoute;
use crate::sources::ModulationSource;
use crate::sources::follower::FollowerConfig;
use crate::sources::random::RandomConfig;
use crate::target::ParamTarget;

use super::envelope_state::EnvelopeState;
use super::lfo_state::LfoState;
use super::pattern_state::PatternState;

/// Context passed to each tick of the modulation processor.
#[derive(Debug, Clone, Copy)]
pub struct TickContext {
    /// Time elapsed since last tick (seconds).
    pub dt: f64,
    /// Current tempo in BPM (for tempo-synced LFOs).
    pub bpm: f64,
}

/// A single modulation output: a bipolar offset for a parameter target.
#[derive(Debug, Clone)]
pub struct ModulationOutput {
    /// The parameter being modulated.
    pub target: ParamTarget,
    /// Bipolar offset to add to the base value. Typically in [-1, 1] but may exceed
    /// when multiple routes accumulate.
    pub offset: f64,
}

/// Runtime state for the envelope follower source.
#[derive(Debug, Clone, Default)]
pub struct FollowerState {
    /// Current envelope follower level (0.0–1.0).
    level: f64,
    /// External audio level input (set by the host).
    input_level: f64,
}

impl FollowerState {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            level: 0.0,
            input_level: 0.0,
        }
    }

    /// Advance the follower by one tick.
    pub fn tick(&mut self, dt: f64, config: &FollowerConfig) -> f64 {
        let target = self.input_level;
        let tau = if target > self.level {
            f64::from(config.attack_ms) / 1000.0
        } else {
            f64::from(config.release_ms) / 1000.0
        };
        // One-pole smoothing: coeff = 1 - e^(-dt/tau)
        let coeff = if tau > 0.0 {
            1.0 - (-dt / tau).exp()
        } else {
            1.0
        };
        self.level += (target - self.level) * coeff;
        self.level
    }
}

/// Runtime state for the random modulation source.
#[derive(Debug, Clone)]
pub struct RandomState {
    /// Current output value (0.0–1.0).
    current: f64,
    /// Target value we're interpolating toward.
    target: f64,
    /// Phase accumulator for timing new random values.
    phase: f64,
    /// PRNG state (xorshift64).
    rng_state: u64,
}

impl RandomState {
    #[must_use]
    pub fn new(seed: Option<u64>) -> Self {
        let rng_state = seed.unwrap_or(0x1234_5678_9ABC_DEF0);
        let mut s = Self {
            current: 0.5,
            target: 0.5,
            phase: 0.0,
            rng_state,
        };
        s.target = s.next_random();
        s
    }

    fn next_random(&mut self) -> f64 {
        // xorshift64
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        // Map to [0, 1]
        (self.rng_state as f64) / (u64::MAX as f64)
    }

    /// Advance the random source by one tick.
    pub fn tick(&mut self, dt: f64, config: &RandomConfig, bpm: f64) -> f64 {
        let rate = if config.tempo_sync {
            let beats_per_sec = bpm / 60.0;
            let div_beats = config.sync_division.map_or(1.0, |d| f64::from(d.beats()));
            beats_per_sec / div_beats
        } else {
            f64::from(config.rate_hz)
        };

        self.phase += dt * rate;

        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.current = self.target;
            self.target = self.next_random();
        }

        // Interpolate between current and target based on smoothing
        let smoothing = f64::from(config.smoothing);
        if smoothing <= 0.0 {
            // Stepped: hold current value until next trigger
            self.current
        } else {
            // Smooth interpolation within the current period
            let t = self.phase * smoothing;
            (self.target - self.current).mul_add(t.min(1.0), self.current)
        }
    }
}

/// Runtime state for a single modulation source.
#[derive(Debug, Clone)]
enum SourceState {
    Lfo(LfoState),
    Pattern(PatternState),
    Envelope(EnvelopeState),
    Follower(FollowerState),
    Random(RandomState),
    /// No runtime state needed — value comes from external input.
    External,
}

/// The modulation processor — owns all route configs and their runtime states.
///
/// # Usage
///
/// ```ignore
/// let mut proc = ModulationProcessor::new(routes);
/// // Each frame (~30Hz):
/// let outputs = proc.tick(TickContext { dt: 0.033, bpm: 120.0 });
/// for out in outputs {
///     // Apply out.offset to the parameter identified by out.target
/// }
/// ```
#[derive(Debug, Clone)]
pub struct ModulationProcessor {
    routes: Vec<ModulationRoute>,
    states: Vec<SourceState>,
    /// External input values: MIDI CC (keyed by CC number), [0, 1].
    midi_cc_values: HashMap<u8, f64>,
    /// Expression pedal value [0, 1].
    expression_value: f64,
    /// Macro knob values (keyed by `knob_id`), [0, 1].
    macro_values: HashMap<String, f64>,
    /// Follower audio input levels (keyed by route index), [0, 1].
    follower_inputs: HashMap<usize, f64>,
}

impl ModulationProcessor {
    /// Create a new processor from a set of modulation routes.
    #[must_use]
    pub fn new(routes: Vec<ModulationRoute>) -> Self {
        let states = routes
            .iter()
            .map(|route| match &route.source {
                ModulationSource::Lfo(config) => SourceState::Lfo(LfoState::from_config(config)),
                ModulationSource::Pattern(config) => {
                    SourceState::Pattern(PatternState::from_config(config))
                }
                ModulationSource::Envelope(_) => SourceState::Envelope(EnvelopeState::new()),
                ModulationSource::Follower(_) => SourceState::Follower(FollowerState::new()),
                ModulationSource::Random(config) => {
                    SourceState::Random(RandomState::new(config.seed))
                }
                ModulationSource::MidiCc { .. }
                | ModulationSource::Expression
                | ModulationSource::Macro { .. } => SourceState::External,
            })
            .collect();

        Self {
            routes,
            states,
            midi_cc_values: HashMap::new(),
            expression_value: 0.0,
            macro_values: HashMap::new(),
            follower_inputs: HashMap::new(),
        }
    }

    /// Number of routes.
    #[must_use]
    pub const fn route_count(&self) -> usize {
        self.routes.len()
    }

    /// Advance all modulation sources and produce per-target offsets.
    ///
    /// Returns one [`ModulationOutput`] per unique target that has at least one
    /// active route. Multiple routes targeting the same parameter are **summed**.
    pub fn tick(&mut self, ctx: TickContext) -> Vec<ModulationOutput> {
        let mut target_offsets: HashMap<ParamTarget, f64> = HashMap::new();

        // Update follower input levels before processing
        for (&route_idx, &level) in &self.follower_inputs {
            if let Some(SourceState::Follower(state)) = self.states.get_mut(route_idx) {
                state.input_level = level;
            }
        }

        for (i, route) in self.routes.iter().enumerate() {
            if !route.enabled {
                continue;
            }

            let raw_value = match (&route.source, &mut self.states[i]) {
                (ModulationSource::Lfo(config), SourceState::Lfo(state)) => {
                    // LFO tick returns bipolar [-1, 1]
                    let waveform_val = state.tick(ctx.dt, config, ctx.bpm);
                    waveform_val * f64::from(config.depth)
                }

                (ModulationSource::Pattern(config), SourceState::Pattern(state)) => {
                    // Pattern tick returns unipolar [0, 1], map to bipolar
                    let v = state.tick(ctx.dt, config, ctx.bpm);
                    v.mul_add(2.0, -1.0) * f64::from(config.depth)
                }

                (ModulationSource::Envelope(config), SourceState::Envelope(state)) => {
                    // Envelope tick returns unipolar [0, 1]
                    let env_val = state.tick(ctx.dt, config);
                    env_val * f64::from(config.depth)
                }

                (ModulationSource::Follower(config), SourceState::Follower(state)) => {
                    // Follower tick returns unipolar [0, 1], map to bipolar
                    let follower_val = state.tick(ctx.dt, config);
                    follower_val.mul_add(2.0, -1.0) * f64::from(config.depth)
                }

                (ModulationSource::Random(config), SourceState::Random(state)) => {
                    // Random tick returns unipolar [0, 1], map to bipolar
                    let random_val = state.tick(ctx.dt, config, ctx.bpm);
                    random_val.mul_add(2.0, -1.0) * f64::from(config.depth)
                }

                (ModulationSource::MidiCc { cc_number }, SourceState::External) => {
                    let cc_val = self.midi_cc_values.get(cc_number).copied().unwrap_or(0.0);
                    // Map [0, 1] to bipolar [-1, 1]
                    cc_val * 2.0 - 1.0
                }

                (ModulationSource::Expression, SourceState::External) => {
                    // Map [0, 1] to bipolar [-1, 1]
                    self.expression_value.mul_add(2.0, -1.0)
                }

                (ModulationSource::Macro { knob_id }, SourceState::External) => {
                    let macro_val = self.macro_values.get(knob_id).copied().unwrap_or(0.5);
                    // Map [0, 1] to bipolar [-1, 1]
                    macro_val * 2.0 - 1.0
                }

                // Mismatched state — shouldn't happen, but be safe
                _ => 0.0,
            };

            // Scale by route amount (-1..1, allows inversion)
            let offset = raw_value * f64::from(route.amount);

            *target_offsets.entry(route.target.clone()).or_default() += offset;
        }

        target_offsets
            .into_iter()
            .map(|(target, offset)| ModulationOutput { target, offset })
            .collect()
    }

    // ─── External input methods ────────────────────────────────

    /// Set a MIDI CC value (0.0–1.0).
    pub fn set_midi_cc(&mut self, cc_number: u8, value: f64) {
        self.midi_cc_values.insert(cc_number, value.clamp(0.0, 1.0));
    }

    /// Set the expression pedal value (0.0–1.0).
    pub const fn set_expression(&mut self, value: f64) {
        self.expression_value = value.clamp(0.0, 1.0);
    }

    /// Set a macro knob value (0.0–1.0).
    pub fn set_macro(&mut self, knob_id: &str, value: f64) {
        self.macro_values
            .insert(knob_id.to_string(), value.clamp(0.0, 1.0));
    }

    /// Set the audio input level for a follower route by route index (0.0–1.0).
    pub fn set_follower_input(&mut self, route_index: usize, level: f64) {
        self.follower_inputs
            .insert(route_index, level.clamp(0.0, 1.0));
    }

    // ─── Trigger methods ────────────────────────────────────────

    /// Trigger gate-on for all envelope sources.
    pub fn gate_on_all_envelopes(&mut self) {
        for state in &mut self.states {
            if let SourceState::Envelope(env) = state {
                env.gate_on();
            }
        }
    }

    /// Trigger gate-off for all envelope sources.
    pub fn gate_off_all_envelopes(&mut self) {
        for state in &mut self.states {
            if let SourceState::Envelope(env) = state {
                env.gate_off();
            }
        }
    }

    /// Retrigger all LFOs that have retrigger mode set.
    pub fn retrigger_lfos(&mut self) {
        for (i, state) in self.states.iter_mut().enumerate() {
            match state {
                SourceState::Lfo(lfo) => {
                    if let ModulationSource::Lfo(config) = &self.routes[i].source {
                        lfo.retrigger(config);
                    }
                }
                SourceState::Pattern(pat) => {
                    if let ModulationSource::Pattern(config) = &self.routes[i].source {
                        pat.retrigger(config);
                    }
                }
                _ => {}
            }
        }
    }

    /// Reset all source states. Useful for scene changes.
    pub fn reset(&mut self) {
        for (i, state) in self.states.iter_mut().enumerate() {
            match state {
                SourceState::Lfo(lfo) => {
                    if let ModulationSource::Lfo(config) = &self.routes[i].source {
                        lfo.reset(config.phase_offset);
                    }
                }
                SourceState::Pattern(pat) => {
                    if let ModulationSource::Pattern(config) = &self.routes[i].source {
                        pat.reset(config.phase_offset);
                    }
                }
                SourceState::Envelope(env) => {
                    *env = EnvelopeState::new();
                }
                SourceState::Follower(follower) => {
                    *follower = FollowerState::new();
                }
                SourceState::Random(random) => {
                    if let ModulationSource::Random(config) = &self.routes[i].source {
                        *random = RandomState::new(config.seed);
                    }
                }
                SourceState::External => {}
            }
        }
        self.midi_cc_values.clear();
        self.expression_value = 0.0;
        self.macro_values.clear();
        self.follower_inputs.clear();
    }
}
