//! Kontakt 8.7.1's AHDSR at its control rate, ported from KONTRA
//! (`crates/engine/voice/src/envelope.rs`, `Kontakt`), which decoded it from
//! the macOS arm64 `Kontakt 8` binary (addresses in it).
//!
//! `BEnvelopeAHDSR::Generate` (0x66f6e8) makes one value per 32-frame
//! control step: every stage is `(state − 0.075)·scale + offset`, then
//! `state *= coef`; a stage of N steps outputs N values before the next
//! stage's first, and a stage of no steps is skipped. The attack curve `c`
//! (−1..=1) picks the attack's ratio (`c > 0` starts slowly); decay and
//! release fall by `(3/43)^(1/N)` a step, from 1.075 to 0.075 over the stage.
//! `ProcessVoice` ramps the values linearly over each step, one step late:
//! frames 0..32 hold the first value, frames 32k..32k+32 go from value k−1
//! to value k, the phase counted from the voice's first frame. A key-up
//! enters the release at the next step, from the last value; the voice ends
//! once the release has run out and its ramp reached 0.
//!
//! Plain arithmetic on inline state: no allocation.

/// Frames per control step.
const STEP: u32 = 32;

/// An AHDSR's settings, in control steps (`trunc(ms·0.001·rate/32)`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub attack: u32,
    pub hold: u32,
    pub decay: u32,
    pub sustain: f32,
    pub release: u32,
    /// The attack curve, −1..=1 (0 linear-ish, positive starts slowly).
    pub curve: f32,
    /// The key-up is ignored and the decay falls to silence, ending it.
    pub ahd_only: bool,
}

impl Settings {
    /// From times in ms at `rate` Hz.
    #[must_use]
    pub fn from_ms(
        [attack, hold, decay, release]: [f32; 4],
        sustain: f32,
        curve: f32,
        ahd_only: bool,
        rate: u32,
    ) -> Self {
        let steps = |ms: f32| (f64::from(ms.max(0.0)) * 0.001 * f64::from(rate) / 32.0) as u32;
        Self {
            attack: steps(attack),
            hold: steps(hold),
            decay: steps(decay),
            sustain: sustain.clamp(0.0, 1.0),
            release: steps(release),
            curve: curve.clamp(-1.0, 1.0),
            ahd_only,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Attack,
    Hold,
    Decay,
    Sustain,
    Release,
    Off,
}

/// A running envelope.
#[derive(Clone, Copy, Debug)]
pub struct Ahdsr {
    s: Settings,
    stage: Stage,
    left: u32,
    state: f32,
    coef: f32,
    scale: f32,
    offset: f32,
    /// The last value made (a release starts from it).
    out: f32,
    /// The ramp's two ends, and the last two values made.
    prev: f32,
    cur: f32,
    raw: [f32; 2],
    phase: u32,
    started: bool,
    release_pending: bool,
}

impl Ahdsr {
    #[must_use]
    pub fn new(s: Settings) -> Self {
        Self {
            s,
            stage: Stage::Attack,
            left: 0,
            state: 0.0,
            coef: 1.0,
            scale: 0.0,
            offset: 0.0,
            out: 0.0,
            prev: 0.0,
            cur: 0.0,
            raw: [1.0; 2],
            phase: 0,
            started: false,
            release_pending: false,
        }
    }

    /// The key-up: the release starts at the next control step.
    pub fn release(&mut self) {
        self.release_pending = true;
    }

    /// The release has run out and the ramp reached 0.
    #[must_use]
    pub fn is_off(&self) -> bool {
        self.stage == Stage::Off && self.raw == [0.0, 0.0] && self.phase == 0
    }

    fn enter(&mut self, stage: Stage) {
        let s = self.s;
        match stage {
            Stage::Attack => {
                let n = s.attack;
                if n == 0 {
                    return self.enter(Stage::Hold);
                }
                let c = f64::from(s.curve);
                let (start, coef, scale, offset) = if c > 0.0 {
                    let x = (13.122_363 * (1.0 - c) - 9.903_488).exp();
                    (
                        x + 1.0,
                        (x / (x + 1.0)).powf(1.0 / f64::from(n)),
                        -1.0,
                        x + 1.0 - 0.075,
                    )
                } else {
                    let x = (13.122_363 * (1.0 + c) - 9.903_488).exp();
                    (x, ((x + 1.0) / x).powf(1.0 / f64::from(n)), 1.0, 0.075 - x)
                };
                self.set(
                    Stage::Attack,
                    n,
                    start as f32,
                    coef as f32,
                    scale as f32,
                    offset as f32,
                );
            }
            Stage::Hold => {
                if s.hold == 0 {
                    return self.enter(Stage::Decay);
                }
                self.set(Stage::Hold, s.hold, 1.075, 1.0, 1.0, 0.0);
            }
            Stage::Decay => {
                let n = s.decay;
                let coef = || (3.0f64 / 43.0).powf(1.0 / f64::from(n)) as f32;
                if s.ahd_only {
                    if n == 0 {
                        return self.enter(Stage::Off);
                    }
                    return self.set(Stage::Decay, n, 1.075, coef(), 1.0, 0.0);
                }
                if n == 0 || s.sustain >= 1.0 {
                    return self.enter(Stage::Sustain);
                }
                self.set(Stage::Decay, n, 1.075, coef(), 1.0 - s.sustain, s.sustain);
            }
            Stage::Sustain => self.set(Stage::Sustain, 0, 1.075, 1.0, 0.0, s.sustain),
            Stage::Release => {
                let n = s.release;
                if n == 0 {
                    return self.enter(Stage::Off);
                }
                let coef = (3.0f64 / 43.0).powf(1.0 / f64::from(n)) as f32;
                self.set(Stage::Release, n, 1.075, coef, self.out, 0.0);
            }
            Stage::Off => self.set(Stage::Off, 0, 0.075, 1.0, 0.0, 0.0),
        }
    }

    fn set(&mut self, stage: Stage, left: u32, state: f32, coef: f32, scale: f32, offset: f32) {
        self.stage = stage;
        self.left = left;
        self.state = state;
        self.coef = coef;
        self.scale = scale;
        self.offset = offset;
    }

    /// The next control step's value.
    fn step(&mut self) -> f32 {
        if !self.started {
            self.enter(Stage::Attack);
        }
        if self.release_pending && self.s.ahd_only {
            self.release_pending = false;
        }
        if self.release_pending && self.stage != Stage::Release && self.stage != Stage::Off {
            self.release_pending = false;
            self.enter(Stage::Release);
        }
        let out = (self.state - 0.075).mul_add(self.scale, self.offset);
        self.out = out;
        self.state *= self.coef;
        if self.left > 0 {
            self.left -= 1;
            if self.left == 0 {
                let next = match self.stage {
                    Stage::Attack => Stage::Hold,
                    Stage::Hold => Stage::Decay,
                    Stage::Decay if self.s.ahd_only => Stage::Off,
                    Stage::Decay => Stage::Sustain,
                    _ => Stage::Off,
                };
                self.enter(next);
            }
        }
        out
    }

    /// The next frame's level (0..=1), the step values ramped one step late.
    pub fn tick(&mut self) -> f32 {
        if self.phase == 0 {
            let raw = self.step();
            self.raw = [self.raw[1], raw];
            let v = raw.max(0.0);
            if self.started {
                self.prev = self.cur;
            } else {
                self.prev = v;
                self.started = true;
            }
            self.cur = v;
        }
        let level = self.prev + (self.phase as f32 * 0.031_25) * (self.cur - self.prev);
        self.phase = (self.phase + 1) % STEP;
        level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(attack: u32, release: u32) -> Settings {
        Settings {
            attack,
            hold: 0,
            decay: 0,
            sustain: 1.0,
            release,
            curve: 0.0,
            ahd_only: false,
        }
    }

    /// The first step's value holds over the first 32 frames; the ramp then
    /// reaches each value a step late, and full level after the attack.
    #[test]
    fn an_attack_ramps_one_step_late_to_full_level() {
        let mut e = Ahdsr::new(settings(4, 10));
        let first: Vec<f32> = (0..32).map(|_| e.tick()).collect();
        assert!(first.iter().all(|&x| x == first[0]));
        for _ in 0..32 * 6 {
            e.tick();
        }
        assert!((e.tick() - 1.0).abs() < 1e-5);
    }

    /// The release runs its steps from the key-up's next step, then the voice
    /// is off once the ramp has reached 0.
    #[test]
    fn a_release_ends_on_its_step_count() {
        let mut e = Ahdsr::new(settings(0, 10));
        for _ in 0..320 {
            e.tick();
        }
        e.release();
        let mut frames = 0;
        while !e.is_off() {
            e.tick();
            frames += 1;
            assert!(frames < 2000);
        }
        // the step it was pending in, 10 release steps, the ramp's late step
        assert_eq!(frames, 32 * 12);
    }
}
