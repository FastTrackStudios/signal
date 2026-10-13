//! Shimmer reverb — pitch-shifted feedback reverb.
//!
//! Based on `CloudSeedCore` + Strymon `BigSky` MX Shimmer:
//! A reverb tank with pitch-shifted signal fed back into the input,
//! creating evolving harmonic tails. Two independent pitch voices
//! (Shift 1 + Shift 2, each −oct..+oct) share a single Amount level,
//! and the voices' source is selectable (`BigSky` "Feedback" mode):
//! Input (one-pass shimmer, no laddering), Regenerative (shift inside
//! the loop → octave ladders), or both summed.
//!
//! The voices are pitch-dsp `SpectralShifter`s (phase-locked phase
//! vocoder, 2048-sample frames = 43 ms of extra loop delay, which a
//! multi-second tail does not notice). They replaced dual-grain delay-line
//! shifters whose 50 ms grain crossfades amplitude-modulated and
//! phase-scrambled the tail every pass — the loop compounded that into the
//! warbly, gritty shimmer. Measured on a steady tone at +12 (one clean
//! pass): non-harmonic energy −6 dB → −68 dB, frame-to-frame spectral flux
//! 7.9 → 0.0. In the loop each shifted line is deliberately widened to
//! `LINE_WIDTH_HZ`, the voices' source is high-passed (`VOICE_HP_HZ`) and a
//! `Governor` keeps the ladder from building on the tank's colouration —
//! see those for why a clean ladder needs all three. Voice gain stays
//! below unity on every material measured (−1.0 … −4.5 dB) and the
//! governor only lowers it, so the `update_injection` loop-gain bound —
//! which assumes a voice ≤ unity — still holds.

use dsp_core::num;

use crate::algorithm::{AlgorithmParams, ReverbAlgorithm, ShimmerFeedbackMode, ShimmerParams};
use crate::primitives::allpass_diffuser::AllpassDiffuser;
use crate::primitives::fdn::{Fdn, MixMatrix};
use crate::primitives::one_pole::Lp1;
use audiocore_dsp::biquad::{Biquad, FilterType};
use audiocore_dsp::dc_blocker::DcBlocker;
use audiocore_dsp::one_pole::OnePoleHp;
use pitch_dsp::spectral::SpectralShifter;

pub struct Shimmer {
    // Reverb tank
    fdn_l: Fdn,
    fdn_r: Fdn,
    // Input diffusion
    diffuser_l: AllpassDiffuser,
    diffuser_r: AllpassDiffuser,
    // Pitch voices (voice 1 always active, voice 2 optional)
    shifter1_l: SpectralShifter,
    shifter1_r: SpectralShifter,
    shifter2_l: SpectralShifter,
    shifter2_r: SpectralShifter,
    /// Voice 2 sits on the same interval as voice 1: its output would be
    /// identical, so voice 1's is reused (half the CPU).
    voice2_shared: bool,
    /// High-pass on what the voices shift (both channels). A clean shifter
    /// carries anything up the ladder — including subsonic rumble, which
    /// climbs 12 → 24 → … → 768 Hz and arrives as a howl (+19 dB over the
    /// input on a live DI with handling rumble). Nothing below the guitar's
    /// range gets regenerated.
    voice_hp: Biquad,
    // Feedback damping
    fb_damp_l: Lp1,
    fb_damp_r: Lp1,
    // DC blockers — pitch-shifted feedback accumulates subsonic offset
    fb_dc_l: DcBlocker,
    fb_dc_r: DcBlocker,
    /// Loop governor: keeps the ladder off the tank's colouration humps.
    gov: Governor,
    // Feedback state (shifted signal, injected next tick)
    fb_l: f64,
    fb_r: f64,
    // Subsonic cleanup on the wet output (the grain shifters this loop
    // used to run left ~1.5% of IR energy below 20 Hz; kept as a guard).
    out_hp_l: OnePoleHp,
    out_hp_r: OnePoleHp,
    // Shimmer amount (how much pitch-shifted signal feeds back)
    amount: f64,
    /// What is actually fed back: `amount`, capped so the loop stays under
    /// unity gain (see `update_injection`).
    injection: f64,
    decay: f64,
    // MX param overlay (voice intervals / amount / feedback mode).
    mx: ShimmerParams,
    // Legacy coarse voice-1 speed from extra_b (used when
    // `mx.shift1_semitones` is None).
    legacy_speed: f64,
    // Legacy amount from extra_a (used when `mx.amount` is None).
    legacy_amount: f64,
    sample_rate: f64,
}

impl Shimmer {
    #[must_use]
    pub fn new(sample_rate: f64) -> Self {
        let mut shimmer = Self {
            fdn_l: Self::make_fdn(sample_rate, false),
            fdn_r: Self::make_fdn(sample_rate, true),
            diffuser_l: AllpassDiffuser::with_defaults(sample_rate, 0.7),
            diffuser_r: AllpassDiffuser::with_defaults(sample_rate, 0.7),
            shifter1_l: SpectralShifter::new(),
            shifter1_r: SpectralShifter::new(),
            shifter2_l: SpectralShifter::new(),
            shifter2_r: SpectralShifter::new(),
            voice2_shared: true,
            voice_hp: Biquad::new(),
            fb_damp_l: Lp1::new(),
            fb_damp_r: Lp1::new(),
            fb_dc_l: DcBlocker::new(),
            fb_dc_r: DcBlocker::new(),
            gov: Governor::new(),
            fb_l: 0.0,
            fb_r: 0.0,
            out_hp_l: OnePoleHp::new(24.0, sample_rate),
            out_hp_r: OnePoleHp::new(24.0, sample_rate),
            amount: 0.5,
            injection: 0.5,
            decay: 0.8,
            mx: ShimmerParams::default(),
            legacy_speed: 2.0,
            legacy_amount: 0.5 * 0.7,
            sample_rate,
        };

        for s in [
            &mut shimmer.shifter1_l,
            &mut shimmer.shifter1_r,
            &mut shimmer.shifter2_l,
            &mut shimmer.shifter2_r,
        ] {
            s.speed = 2.0; // Octave up
            s.mix = 1.0;
            s.fft_size = 2048;
            s.overlap = 8;
            s.line_width_hz = LINE_WIDTH_HZ;
            s.update(sample_rate);
        }
        shimmer.voice_hp.set(
            FilterType::Highpass,
            VOICE_HP_HZ,
            std::f64::consts::FRAC_1_SQRT_2,
            sample_rate,
        );
        shimmer.fb_damp_l.set_freq(6000.0, sample_rate);
        shimmer.fb_damp_r.set_freq(6000.0, sample_rate);
        shimmer.update_injection();

        shimmer
    }

    fn make_fdn(sample_rate: f64, offset: bool) -> Fdn {
        let base = if offset {
            [1117, 1381, 1613, 1873, 2131, 2371, 2617, 2879]
        } else {
            [1049, 1327, 1559, 1801, 2069, 2297, 2557, 2803]
        };
        let scale = sample_rate / 48000.0;
        let delays: Vec<usize> = base
            .iter()
            .map(|&d| num::f64_to_index(f64::from(d) * scale))
            .collect();
        Fdn::new(&delays, MixMatrix::Householder)
    }

    /// Re-derive shifter speeds / amount from the MX overlay + legacy
    /// mappings.
    fn apply_voice_config(&mut self) {
        let speed1 = match self.mx.shift1_semitones {
            Some(st) => semitones_to_speed(st),
            None => self.legacy_speed,
        };
        self.shifter1_l.speed = speed1;
        self.shifter1_r.speed = speed1;

        if self.mx.voice2 {
            let speed2 = semitones_to_speed(self.mx.shift2_semitones.unwrap_or(12.0));
            let shared = (speed2 - speed1).abs() < 1e-12;
            if self.voice2_shared && !shared {
                // Voice 2 wakes up: drop whatever it held when it last ran.
                self.shifter2_l.reset();
                self.shifter2_r.reset();
            }
            self.voice2_shared = shared;
            self.shifter2_l.speed = speed2;
            self.shifter2_r.speed = speed2;
        }

        self.amount = self
            .mx
            .amount
            .map_or(self.legacy_amount, |a| a.clamp(0.0, 1.0));
        self.update_injection();
    }

    /// Cap the fed-back amount so the shimmer loop cannot run away.
    ///
    /// The loop is: pitch voices → back into the tank → out of the tank
    /// into the pitch voices. The tank returns up to `1/√(1 − g²)` of what
    /// goes in (its energy gain at feedback `g`), and two voices can add
    /// coherently (√2 once normalised — both on +12 is the case). So the
    /// loop gain is about `amount · voices / √(1 − g²)`, and anything over 1
    /// grows without end: two octave voices at amount 0.5 into a long tail
    /// went to infinity in about six seconds. Capped at 0.8, the shimmer
    /// still builds — the regeneration is the sound — but always decays.
    fn update_injection(&mut self) {
        const MAX_LOOP_GAIN: f64 = 0.8;
        let g = self.decay.clamp(0.0, 0.999);
        let tank = 1.0 / (1.0 - g * g).sqrt();
        let voices = if self.mx.voice2 {
            std::f64::consts::SQRT_2
        } else {
            1.0
        };
        self.injection = self.amount.min(MAX_LOOP_GAIN / (tank * voices));
        self.gov
            .configure(g, self.injection * voices, voices, self.sample_rate);
    }
}

/// Line width of the shifted voices (Hz FWHM, `SpectralShifter::line_width_hz`).
///
/// A clean shifter turns every note into pure lines that climb octave by
/// octave through the tank, and a long FDN's response to a pure line is a
/// comb of sharp modes (~3 Hz apart, under 1 Hz wide at long decays) on top
/// of broader colouration — a line that parks on a peak is amplified far
/// above the tank's average gain, and the ladder multiplies that every
/// octave. Unbroadened, sustained live playing built +10 dB bursts (peak 5)
/// that the grain shifters never did: their grain sidebands smeared each
/// line over ±20–40 Hz. A 12 Hz line (a phase random walk: no pitch drift,
/// no periodic AM — ~5× narrower than that smear) spans several modes, so
/// the loop sees something near the tank's average gain again.
const LINE_WIDTH_HZ: f64 = 12.0;
/// Corner of the voices' source high-pass (Hz), 2nd-order Butterworth.
const VOICE_HP_HZ: f64 = 80.0;
/// Mean FDN line length at 48 kHz (both tanks, `make_fdn`) — sets how fast
/// the governor's input reference decays.
const MEAN_DELAY_48K: f64 = 1965.0;
/// How far (dB, power) the voices may run above what an evenly coloured
/// tank would feed them before the governor steps in.
const GOVERNOR_MARGIN_DB: f64 = -7.0;

/// Keeps the octave ladder from building on the tank's colouration.
///
/// The ladder's level is the product of the tank's gain at every rung
/// (f, 2f, 4f, …), and this FDN's gain is far from flat: its line lengths
/// step by a near-constant ~250 samples (1049, 1327, 1559, … and 1117,
/// 1381, 1613, …), so every line is nearly in phase at multiples of
/// ~190 Hz and the response carries broad +5 … +8 dB humps there (380, 570,
/// 760, 1520 Hz …). A note near 190 Hz — or 185, 196, 220, 277 — lands
/// every rung of its ladder on a hump: a sustained 180 Hz sine rendered
/// −22 dBFS where its neighbours sat at −34 (the grain shifters built the
/// same notes +4 … +8 dB, just less). Line broadening cannot average humps
/// this wide, and re-drawing the delays only moves them (random prime sets
/// measured worse: +10 … +18 dB on other notes); an 8-line tank is this
/// lumpy by nature.
///
/// So the loop measures itself instead: the voices' output power (per
/// channel, 100 ms) is held under what the loop would carry through an
/// evenly coloured tank for the input it was given — the input's power,
/// released at the tank's own decay rate, times the tank's energy gain
/// `1/(1 − g²)`, the ladder sum `1/(1 − ρ)` and the voices' gain — less
/// `GOVERNOR_MARGIN_DB`. On most notes it never acts; on the humped ones it
/// eases the injection down over ~50 ms (no pumping: the cap moves with the
/// input's envelope). Semitone sweep 110–622 Hz on "Heaven Shimmer": the
/// shimmer's excess over the dry tank went from +0.8 median / +11 dB worst
/// (185 Hz) to +1.3 / +3.5 dB. It only ever lowers the loop gain, so the
/// `update_injection` bound still holds.
struct Governor {
    /// Cap on the voices' power per unit of the input reference.
    expect: f64,
    in_att: f64,
    in_rel: f64,
    fb_coef: f64,
    gain_coef: f64,
    env_in: f64,
    env_l: f64,
    env_r: f64,
    gain_l: f64,
    gain_r: f64,
}

impl Governor {
    const fn new() -> Self {
        Self {
            expect: 1.0,
            in_att: 0.0,
            in_rel: 0.0,
            fb_coef: 0.0,
            gain_coef: 0.0,
            env_in: 0.0,
            env_l: 0.0,
            env_r: 0.0,
            gain_l: 1.0,
            gain_r: 1.0,
        }
    }

    const fn reset(&mut self) {
        self.env_in = 0.0;
        self.env_l = 0.0;
        self.env_r = 0.0;
        self.gain_l = 1.0;
        self.gain_r = 1.0;
    }

    /// `g`: tank feedback; `loop_amp`: injection × voice gain (amplitude);
    /// `voices`: the voices' gain (amplitude).
    fn configure(&mut self, g: f64, loop_amp: f64, voices: f64, sample_rate: f64) {
        let tank = 1.0 / g.mul_add(-g, 1.0);
        let rho = loop_amp * loop_amp * tank;
        let margin = 10f64.powf(GOVERNOR_MARGIN_DB / 10.0);
        self.expect = tank / (1.0 - rho).max(0.05) * voices * voices * margin;
        let d = MEAN_DELAY_48K * sample_rate / 48_000.0;
        self.in_rel = g.max(1e-3).powf(2.0 / d);
        self.in_att = (-1.0 / (0.005 * sample_rate)).exp();
        self.fb_coef = (-1.0 / (0.1 * sample_rate)).exp();
        self.gain_coef = (-1.0 / (0.05 * sample_rate)).exp();
    }

    /// Input (`l`, `r`) and the voices' output (`vl`, `vr`) → gains for the
    /// voices.
    #[inline]
    fn tick(&mut self, l: f64, r: f64, vl: f64, vr: f64) -> (f64, f64) {
        let p = 0.5 * l.mul_add(l, r * r);
        let c = if p > self.env_in {
            self.in_att
        } else {
            self.in_rel
        };
        self.env_in = c.mul_add(self.env_in - p, p);
        self.env_l = self.fb_coef.mul_add(vl.mul_add(-vl, self.env_l), vl * vl);
        self.env_r = self.fb_coef.mul_add(vr.mul_add(-vr, self.env_r), vr * vr);
        let cap = self.env_in.mul_add(self.expect, 1e-24);
        let tl = if self.env_l > cap {
            (cap / self.env_l).sqrt()
        } else {
            1.0
        };
        let tr = if self.env_r > cap {
            (cap / self.env_r).sqrt()
        } else {
            1.0
        };
        self.gain_l = self.gain_coef.mul_add(self.gain_l - tl, tl);
        self.gain_r = self.gain_coef.mul_add(self.gain_r - tr, tr);
        (self.gain_l, self.gain_r)
    }
}

#[inline]
fn semitones_to_speed(st: f64) -> f64 {
    (st.clamp(-12.0, 12.0) / 12.0).exp2()
}

impl ReverbAlgorithm for Shimmer {
    fn reset(&mut self) {
        self.fdn_l.reset();
        self.fdn_r.reset();
        self.diffuser_l.reset();
        self.diffuser_r.reset();
        self.shifter1_l.reset();
        self.shifter1_r.reset();
        self.shifter2_l.reset();
        self.shifter2_r.reset();
        self.voice_hp.reset();
        self.fb_damp_l.reset();
        self.fb_damp_r.reset();
        self.fb_dc_l.reset();
        self.fb_dc_r.reset();
        self.out_hp_l.reset();
        self.out_hp_r.reset();
        self.fb_l = 0.0;
        self.fb_r = 0.0;
        self.gov.reset();
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        let mx = self.mx;
        *self = Self::new(sample_rate);
        self.mx = mx;
        self.apply_voice_config();
    }

    fn set_vintage(&mut self, on: bool) -> bool {
        self.fdn_l.set_vintage_reads(on, self.sample_rate);
        self.fdn_r.set_vintage_reads(on, self.sample_rate);
        true
    }

    fn set_params(&mut self, params: &AlgorithmParams) {
        // Decay
        self.decay = params.decay.mul_add(0.55, 0.4);
        self.fdn_l.set_decay(self.decay);
        self.fdn_r.set_decay(self.decay);
        self.update_injection();

        // Damping
        let damp_coeff = params.damping * 0.5;
        self.fdn_l.set_damping_coeff(damp_coeff);
        self.fdn_r.set_damping_coeff(damp_coeff);

        // Shimmer amount (extra_a) — legacy mapping, overridable by
        // the MX `amount` param.
        self.legacy_amount = params.extra_a * 0.7;

        // Legacy coarse pitch voice selection (extra_b)
        // 0.0 = octave up, 0.5 = fifth up, 1.0 = octave down
        self.legacy_speed = if params.extra_b < 0.33 {
            2.0 // Octave up
        } else if params.extra_b < 0.66 {
            1.5 // Fifth up
        } else {
            0.5 // Octave down
        };
        self.apply_voice_config();

        // Modulation
        self.diffuser_l
            .set_modulation(0.8, params.modulation * 10.0, self.sample_rate);
        self.diffuser_r
            .set_modulation(0.8, params.modulation * 10.0, self.sample_rate);

        // Diffusion
        let stages = num::f64_to_index(params.diffusion * 8.0);
        self.diffuser_l.set_active_stages(stages);
        self.diffuser_r.set_active_stages(stages);

        // Feedback damping
        let freq = (1.0 - params.damping).mul_add(8000.0, 3000.0);
        self.fb_damp_l.set_freq(freq, self.sample_rate);
        self.fb_damp_r.set_freq(freq, self.sample_rate);
    }

    fn set_shimmer_params(&mut self, params: &ShimmerParams) -> bool {
        self.mx = *params;
        self.apply_voice_config();
        true
    }

    #[inline]
    fn tick(&mut self, left: f64, right: f64) -> (f64, f64) {
        // Mix input with pitch-shifted feedback
        let in_l = self.fb_l.mul_add(self.injection, left);
        let in_r = self.fb_r.mul_add(self.injection, right);

        // Diffuse
        let diff_l = self.diffuser_l.tick(in_l);
        let diff_r = self.diffuser_r.tick(in_r);

        // FDN reverb
        let wet_l = self.fdn_l.tick(diff_l);
        let wet_r = self.fdn_r.tick(diff_r);

        // Pitch-voice source per the MX feedback mode. Regenerative
        // uses THIS tick's wet (legacy behavior, zero extra latency);
        // Input takes the dry input, InputPlusRegen sums both.
        let (src_l, src_r) = match self.mx.feedback_mode {
            ShimmerFeedbackMode::Regenerative => (wet_l, wet_r),
            ShimmerFeedbackMode::Input => (left, right),
            ShimmerFeedbackMode::InputPlusRegen => (left + wet_l, right + wet_r),
        };

        let src_l = self.voice_hp.tick(src_l, 0);
        let src_r = self.voice_hp.tick(src_r, 1);

        // Pitch shift for the injection path (both voices, shared level)
        let mut shifted_l = self.shifter1_l.tick(src_l);
        let mut shifted_r = self.shifter1_r.tick(src_r);
        if self.mx.voice2 {
            // Two voices share the level (an average in power), so adding
            // the second does not double what goes round the loop. On the
            // same interval the voices are identical: (v + v)/√2 = √2·v.
            if self.voice2_shared {
                shifted_l *= std::f64::consts::SQRT_2;
                shifted_r *= std::f64::consts::SQRT_2;
            } else {
                shifted_l =
                    (shifted_l + self.shifter2_l.tick(src_l)) * std::f64::consts::FRAC_1_SQRT_2;
                shifted_r =
                    (shifted_r + self.shifter2_r.tick(src_r)) * std::f64::consts::FRAC_1_SQRT_2;
            }
        }

        // Hold the ladder to an evenly coloured tank's level (`Governor`).
        let (gl, gr) = self.gov.tick(left, right, shifted_l, shifted_r);
        shifted_l *= gl;
        shifted_r *= gr;

        // Block DC, damp, and store for next iteration
        self.fb_l = self.fb_damp_l.tick(self.fb_dc_l.tick(shifted_l));
        self.fb_r = self.fb_damp_r.tick(self.fb_dc_r.tick(shifted_r));

        (self.out_hp_l.tick(wet_l), self.out_hp_r.tick(wet_r))
    }
}
