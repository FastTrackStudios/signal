//! Wurlitzer 200A power amplifier — feature-toggled between circuit and behavioral models.
//!
//! Default: melange-generated 7-BJT Class AB circuit solver.
//! `--features legacy-power-amp`: behavioral closed-loop NR approximation (A/B diagnostics only).
//!
//! Rail sag (melange path only): under load, the unregulated ±22 V rails sag from
//! their idle ~±24.5 V toward the spec ±22 V. Modeled by [`RailDynamics`] and pushed
//! per-sample into the melange solver via the `.runtime V` directives on V1/V2.
//! Calibration anchor: `docs/research/output-stage.md` §4.3.1.

/// Open-circuit (idle, light-load) rail magnitude in volts.
/// Service-manual measurement; matches `tb_power_supply.cir` light-load test.
const RAIL_V_OPEN: f64 = 24.5;

/// Static DC bias of V1/V2 in `wurli-power-amp.cir` (matches the spec ±22 V
/// nominal). The runtime offset is `actual_rail - RAIL_DC_BIAS`.
const RAIL_DC_BIAS: f64 = 22.5;

/// Effective DC source impedance per rail [Ω]. Back-solved from the documented
/// load line: ±24.5 V at idle → ±22 V at rated 20 W into 8 Ω (avg per-rail
/// current Ipk/π = 0.71 A → 2.5 V drop / 0.71 A = 3.5 Ω).
const RAIL_R_EFF: f64 = 3.5;

/// Speaker / output load impedance [Ω]. Two 16 Ω drivers in parallel.
/// Used to compute load current for rail sag from the melange amp's output voltage.
const SPEAKER_LOAD_OHMS: f64 = 8.0;

/// Attack time constant — how fast a rail sags when load increases.
/// Approximated by R_eff × C_filter (3.5 Ω × 2200 µF) ≈ 7.7 ms; rounded to 8 ms.
const RAIL_TAU_ATTACK: f64 = 0.008;

/// Release time constant — how fast a rail recovers when load drops.
/// Slower than attack: full-wave rectifier only conducts during AC peaks
/// (8.3 ms apart at 60 Hz), so recovery is gated by line cycle.
const RAIL_TAU_RELEASE: f64 = 0.015;

/// Current-averaging envelope time constant. The rail target depends on the
/// *average* output current, not the instantaneous value — physically, filter
/// caps integrate audio-rate ripple. 2200 µF at audio frequencies has
/// ~0.07 Ω impedance vs ~8 Ω load, so the cap supplies audio-rate current
/// with negligible rail-voltage change; only the slow envelope of |v_out|
/// matters. 30 ms cleanly suppresses anything above ~5 Hz reaching the rails,
/// killing the AM-IM artifacts that audio-rate target updates would generate
/// at H7+ of upper-register notes (8–12 kHz click band).
const RAIL_TAU_I_AVG: f64 = 0.030;

/// Behavioral rail sag dynamics. Tracks the two unregulated rail magnitudes
/// based on the *averaged* output current (not instantaneous), asymmetric one-pole.
/// See module-level docs and `docs/research/output-stage.md` §4.3.1 for the
/// calibration rationale.
///
/// Two-stage filtering:
/// 1. Current envelope follower (30 ms tau) — models filter-cap audio-rate
///    integration. The target current the rail sees has no audio-rate content.
/// 2. Rail dynamics (8 ms attack / 15 ms release) — models how fast the cap
///    voltage actually moves toward the new target.
///
/// Result: rails respond to the slow envelope of load (note attacks, sustained
/// chord, release) but never AM-modulate the amp at audio rate. Removes the
/// click-band IM artifacts (H7+) that an instantaneous-target implementation
/// produced.
///
/// Per-sample cost: 6 muls, 6 adds, 4 compares — no transcendentals.
#[derive(Debug, Clone, Copy)]
pub struct RailDynamics {
    /// Current positive rail magnitude [V]. Idle ~+24.5 V, sags toward +22 V at rated load.
    v_rail_pos: f64,
    /// Current negative rail magnitude [V] (positive number).
    v_rail_neg: f64,
    /// Smoothed magnitude of positive output current (envelope). Audio-rate
    /// content has been integrated out — only sub-audio load envelope remains.
    i_avg_pos: f64,
    /// Smoothed magnitude of negative output current.
    i_avg_neg: f64,
    /// Precomputed `1 - exp(-dt / tau_attack)` — fast attack toward the load-line target.
    alpha_attack: f64,
    /// Precomputed `1 - exp(-dt / tau_release)` — slower release back toward V_OPEN.
    alpha_release: f64,
    /// Precomputed `1 - exp(-dt / tau_i_avg)` — current-averaging envelope coefficient.
    alpha_i_avg: f64,
}

impl RailDynamics {
    /// Starts rails at the static DC bias (`RAIL_DC_BIAS = 22.5 V`) — i.e.
    /// runtime offsets begin at zero. The cached melange settled state was
    /// computed at this same bias, so the solver opens cleanly. Dynamics then
    /// pull rails toward `RAIL_V_OPEN = 24.5 V` (idle target) over ~80 ms,
    /// which is well within typical plugin warmup.
    pub fn new(sample_rate: f64) -> Self {
        let mut s = Self {
            v_rail_pos: RAIL_DC_BIAS,
            v_rail_neg: RAIL_DC_BIAS,
            i_avg_pos: 0.0,
            i_avg_neg: 0.0,
            alpha_attack: 0.0,
            alpha_release: 0.0,
            alpha_i_avg: 0.0,
        };
        s.set_sample_rate(sample_rate);
        s
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        let dt = 1.0 / sample_rate;
        self.alpha_attack = 1.0 - (-dt / RAIL_TAU_ATTACK).exp();
        self.alpha_release = 1.0 - (-dt / RAIL_TAU_RELEASE).exp();
        self.alpha_i_avg = 1.0 - (-dt / RAIL_TAU_I_AVG).exp();
    }

    pub fn reset(&mut self) {
        self.v_rail_pos = RAIL_DC_BIAS;
        self.v_rail_neg = RAIL_DC_BIAS;
        self.i_avg_pos = 0.0;
        self.i_avg_neg = 0.0;
    }

    /// Rail magnitudes as `(positive, negative)`, both as positive numbers.
    pub fn rail_voltages(&self) -> (f64, f64) {
        (self.v_rail_pos, self.v_rail_neg)
    }

    /// Step one sample. `v_out` is the previous output voltage in volts (raw,
    /// pre-normalization). Positive output draws from +rail, negative from −rail.
    /// Audio-rate content in v_out is integrated out by the current envelope
    /// follower before reaching the rail target — physical filter caps don't
    /// see audio-rate ripple as net charge change.
    #[inline]
    pub fn step(&mut self, v_out: f64) {
        // Stage 1: instantaneous current magnitude per rail (only the conducting one)
        let i_pos = (v_out / SPEAKER_LOAD_OHMS).max(0.0);
        let i_neg = (-v_out / SPEAKER_LOAD_OHMS).max(0.0);

        // Stage 2: average via 30 ms envelope follower — models filter-cap
        // audio-rate integration. After this, i_avg has no audio-rate content.
        self.i_avg_pos += self.alpha_i_avg * (i_pos - self.i_avg_pos);
        self.i_avg_neg += self.alpha_i_avg * (i_neg - self.i_avg_neg);

        // Stage 3: rail target from the smoothed (sub-audio) average current.
        let target_pos = RAIL_V_OPEN - self.i_avg_pos * RAIL_R_EFF;
        let target_neg = RAIL_V_OPEN - self.i_avg_neg * RAIL_R_EFF;

        // Stage 4: rail follows target with attack (sag) / release (recovery).
        let alpha_p = if target_pos < self.v_rail_pos {
            self.alpha_attack
        } else {
            self.alpha_release
        };
        let alpha_n = if target_neg < self.v_rail_neg {
            self.alpha_attack
        } else {
            self.alpha_release
        };
        self.v_rail_pos += alpha_p * (target_pos - self.v_rail_pos);
        self.v_rail_neg += alpha_n * (target_neg - self.v_rail_neg);
    }

    /// Runtime V offsets to push into the melange CircuitState
    /// (additive to V1/V2's ±22.5 V DC bias).
    pub fn offsets(&self) -> (f64, f64) {
        (
            self.v_rail_pos - RAIL_DC_BIAS,
            self.v_rail_neg - RAIL_DC_BIAS,
        )
    }
}

#[cfg(feature = "legacy-power-amp")]
mod behavioral {
    //! Behavioral closed-loop negative feedback model.

    const OPEN_LOOP_GAIN: f64 = 19_000.0;
    const FEEDBACK_BETA: f64 = 220.0 / (220.0 + 15_000.0);
    const HEADROOM: f64 = 22.0;
    const CROSSOVER_VT: f64 = 0.013;
    const QUIESCENT_GAIN: f64 = 0.1;
    const NR_MAX_ITER: usize = 8;
    const NR_TOL: f64 = 1e-6;

    pub struct PowerAmp {
        open_loop_gain: f64,
        feedback_beta: f64,
        crossover_vt: f64,
        rail_limit: f64,
        closed_loop_gain: f64,
        quiescent_gain: f64,
    }

    impl PowerAmp {
        pub fn new() -> Self {
            Self {
                open_loop_gain: OPEN_LOOP_GAIN,
                feedback_beta: FEEDBACK_BETA,
                crossover_vt: CROSSOVER_VT,
                rail_limit: HEADROOM,
                closed_loop_gain: OPEN_LOOP_GAIN / (1.0 + OPEN_LOOP_GAIN * FEEDBACK_BETA),
                quiescent_gain: QUIESCENT_GAIN,
            }
        }

        /// Behavioral model is rate-independent (closed-form `tanh`, no integrator).
        /// Argument is accepted for API parity with the melange path.
        pub fn new_at_sample_rate(_sample_rate: f64) -> Self {
            Self::new()
        }

        pub fn process(&mut self, input: f64) -> f64 {
            let mut y = (input * self.closed_loop_gain)
                .clamp(-self.rail_limit + NR_TOL, self.rail_limit - NR_TOL);

            for _ in 0..NR_MAX_ITER {
                let error = input - self.feedback_beta * y;
                let v = self.open_loop_gain * error;
                let (f_val, f_deriv) = self.forward_path(v);
                let residual = y - f_val;
                let jacobian = 1.0 + self.open_loop_gain * self.feedback_beta * f_deriv;
                let delta = residual / jacobian;
                y -= delta;
                if delta.abs() < NR_TOL {
                    break;
                }
            }

            y / self.rail_limit
        }

        #[inline]
        fn forward_path(&self, v: f64) -> (f64, f64) {
            let v_sq = v * v;
            let vt_sq = self.crossover_vt * self.crossover_vt;
            let exp_term = (-v_sq / vt_sq).exp();
            let q = self.quiescent_gain;
            let cross_gain = q + (1.0 - q) * (1.0 - exp_term);
            let v_cross = v * cross_gain;
            let dcross_dv = cross_gain + v * (1.0 - q) * (2.0 * v / vt_sq) * exp_term;
            let tanh_arg = v_cross / self.rail_limit;
            let tanh_val = tanh_arg.tanh();
            let f_val = self.rail_limit * tanh_val;
            let f_deriv = (1.0 - tanh_val * tanh_val) * dcross_dv;
            (f_val, f_deriv)
        }

        /// Behavioral model has no solver, no divergence to detect — returns
        /// the same clamped result as `process`. Exists only for API parity
        /// with the melange adapter's diagnostic probe.
        pub fn diag_raw_process(&mut self, input: f64) -> f64 {
            self.process(input)
        }

        pub fn diag_snapshot(&self) -> (u64, u64, f64) {
            (0, 0, 0.0)
        }

        pub fn reset(&mut self) {}

        /// No-op on the behavioral path — rails are folded into the closed-loop
        /// approximation, so dynamic sag isn't separable. Kept for API parity.
        pub fn set_rail_sag(&mut self, _on: bool) {}

        pub fn rail_sag_enabled(&self) -> bool {
            false
        }

        pub fn rail_voltages(&self) -> (f64, f64) {
            (super::RAIL_DC_BIAS, super::RAIL_DC_BIAS)
        }
    }

    impl Default for PowerAmp {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(feature = "legacy-power-amp")]
pub use behavioral::PowerAmp;

#[cfg(not(feature = "legacy-power-amp"))]
mod melange_adapter {
    //! Melange-generated 7-BJT Class AB circuit solver.

    use crate::gen_power_amp::{self, CircuitState};
    use std::sync::OnceLock;

    /// Rail headroom for output normalization (matches behavioral model).
    const HEADROOM: f64 = 22.0;

    static SETTLED_STATE: OnceLock<CircuitState> = OnceLock::new();

    fn compute_settled_state() -> CircuitState {
        let mut s = CircuitState::default();
        for _ in 0..44100 {
            gen_power_amp::process_sample(0.0, &mut s);
        }
        s
    }

    fn init_state(sample_rate: f64) -> CircuitState {
        let cached = SETTLED_STATE.get_or_init(compute_settled_state);
        let mut state = cached.clone();
        if (sample_rate - gen_power_amp::SAMPLE_RATE).abs() > 0.5 {
            state.set_sample_rate(sample_rate);
        }
        state
    }

    pub struct PowerAmp {
        state: CircuitState,
        sample_rate: f64,
        /// Last confirmed-good adapter output; used to hold continuity when
        /// the melange solver diverges and we have to reset.
        last_good: f64,
        /// Behavioral rail dynamics; only consulted when `rail_sag_on` is true.
        rails: super::RailDynamics,
        /// When true, push per-sample rail offsets into the solver via runtime V.
        /// Default false to preserve historical ideal-rail behavior; flip for A/B.
        rail_sag_on: bool,
    }

    impl PowerAmp {
        pub fn new() -> Self {
            Self::new_at_sample_rate(44100.0)
        }

        /// Construct at a specific sample rate. The melange BE integrator's
        /// per-sample timestep is `1 / sample_rate`; running it at the engine's
        /// upsampled rate (88.2 kHz at 44.1k host) instead of the base rate
        /// halves the integration error per harmonic order, which is critical
        /// for keeping the high-order harmonics realistic on harmonic-rich
        /// inputs (the pickup pumps content past Nyquist into the amp at ff
        /// playing). See `engine.rs` `render_voices_to_preamp_out` — the
        /// power amp is now run inside the same upsample/downsample block as
        /// the preamp.
        pub fn new_at_sample_rate(sample_rate: f64) -> Self {
            Self {
                state: init_state(sample_rate),
                sample_rate,
                last_good: 0.0,
                rails: super::RailDynamics::new(sample_rate),
                // Rail sag is correct physics with negligible CPU cost (+0.66%
                // measured) and a small audible effect (~0.5 dB chord
                // compression, ~+0.3 dB single-note headroom). Default ON.
                // Toggle via `set_rail_sag(false)` for A/B against ideal rails.
                rail_sag_on: true,
            }
        }

        /// Enable / disable rail sag modeling. When off, rails stay fixed at
        /// ±22.5 V (V1/V2 DC bias, runtime offsets = 0) — bit-compat with the
        /// pre-rail-sag adapter. Defaults to on.
        pub fn set_rail_sag(&mut self, on: bool) {
            self.rail_sag_on = on;
            if !on {
                self.state.v_rail_pos_offset = 0.0;
                self.state.v_rail_neg_offset = 0.0;
            }
        }

        pub fn rail_sag_enabled(&self) -> bool {
            self.rail_sag_on
        }

        /// Current rail magnitudes `(positive, negative)` as positive numbers.
        /// When rail sag is off, returns the fixed ±22.5 V DC bias.
        pub fn rail_voltages(&self) -> (f64, f64) {
            if self.rail_sag_on {
                self.rails.rail_voltages()
            } else {
                (super::RAIL_DC_BIAS, super::RAIL_DC_BIAS)
            }
        }

        pub fn process(&mut self, input: f64) -> f64 {
            // Push runtime rail offsets BEFORE process_sample so the solver
            // sees the rail state computed from the previous sample's draw.
            if self.rail_sag_on {
                let (off_pos, off_neg) = self.rails.offsets();
                self.state.v_rail_pos_offset = off_pos;
                self.state.v_rail_neg_offset = off_neg;
            }

            let raw = gen_power_amp::process_sample(input, &mut self.state)[0];
            let result = raw / HEADROOM;

            // Divergence guard. Under continuous polyphonic playing, the
            // melange 7-BJT NR solver intermittently fails to converge and
            // the BE fallback also produces non-physical output (observed
            // internal node voltages up to 1e272 V on stress tests). The
            // visible symptom at the output is a clamp-saturated rail slam
            // that the speaker's HPF/LPF ring on and POST_SPEAKER_GAIN
            // amplifies to +20 dBFS spikes — enough to trip DAW peak-protect
            // muting. Three signals we use to detect it:
            //
            //   1. Non-finite raw output (NaN/Inf propagation)
            //   2. NR exhausted MAX_ITER without converging (signals the
            //      BE fallback ran, which can also silently diverge)
            //   3. Any internal node voltage above 100 V (physical rails
            //      are ±22 V + supplies; anything past this is garbage)
            //
            // On any signal, reset the solver state to its cached DC
            // operating point and hold the last confirmed-good output.
            // Holding (rather than silencing) keeps the waveform continuous
            // across a divergence burst — otherwise a 25-sample run of
            // zeros would click audibly. Next sample's NR starts from a
            // clean state and normally picks up tracking the input.
            //
            // Upstream: this is a melange robustness issue with the Class AB
            // push-pull topology under certain polyphonic transient patterns.
            // File upstream once a minimal reproducer is extracted.
            let nr_failed = self.state.last_nr_iterations >= gen_power_amp::MAX_ITER as u32 - 1;
            let state_insane = self
                .state
                .v_prev
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 100.0);
            if !result.is_finite() || nr_failed || state_insane {
                self.reset();
                return self.last_good;
            }
            let clamped = result.clamp(-1.0, 1.0);
            self.last_good = clamped;

            // Update rail state from the just-computed output for the NEXT
            // sample. `raw` is in volts (pre-normalization), which is what
            // RailDynamics expects to compute load current via v_out / 8 Ω.
            if self.rail_sag_on {
                self.rails.step(raw);
            }

            clamped
        }

        /// Raw, pre-clamp output of the melange solver. Audio-unsafe for normal
        /// use (not bounded), but needed for diagnostic probing of solver
        /// divergence: if |raw| exceeds the circuit rails (±22 V physically,
        /// ±1.0 after the HEADROOM normalization), the NR converged to a
        /// non-physical branch. Not called by the normal plugin path.
        pub fn diag_raw_process(&mut self, input: f64) -> f64 {
            gen_power_amp::process_sample(input, &mut self.state)[0] / HEADROOM
        }

        /// Snapshot of melange NR diagnostics:
        /// `(clamp_count, nr_max_iter_count, peak_output_volts)`.
        pub fn diag_snapshot(&self) -> (u64, u64, f64) {
            (
                self.state.diag_clamp_count,
                self.state.diag_nr_max_iter_count,
                self.state.diag_peak_output,
            )
        }

        pub fn reset(&mut self) {
            self.state = init_state(self.sample_rate);
            self.rails.reset();
            // Do NOT clear last_good — the divergence-guard hold relies on it
            // surviving the reset. Only `new()` zeros it.
        }
    }

    impl Default for PowerAmp {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(not(feature = "legacy-power-amp"))]
pub use melange_adapter::PowerAmp;
