//! Gapless patch switching, with the old patch's delays and reverbs ringing
//! on under the new one.
//!
//! A switch used to take the outgoing chain's blocks out of the engine
//! between two blocks: the tail stopped dead, and nothing crossfaded the two
//! chains. The [`TailStage`] sits after the chain (the rig's output tap) and
//! is handed the outgoing chain as a [`Voice`]:
//!
//! 1. **Crossfade** — for [`FADE_SECS`] the voice runs the *whole* old chain
//!    on the live input, its pre-Time output faded out (equal power) while
//!    the new chain's output fades in. Both chains play the guitar; neither
//!    steps.
//! 2. **Tail** — then only the old chain's Time section onward runs, on
//!    silence: its repeats and reverb ring out at the old patch's level, and
//!    anything after it (master EQ, limiter) still shapes them.
//! 3. **Done** — quiet for [`QUIET_HOLD_SECS`] (or older than
//!    [`MAX_TAIL_SECS`], faded), the voice is finished; the rig collects its
//!    blocks on the control thread and they go back to their chain.
//!
//! Switching back to a patch whose tail is still ringing takes its blocks
//! back out of the voice as they are — the tail carries on under the playing
//! instead of being cleared.
//!
//! Everything here runs on the audio thread and allocates nothing: voices
//! arrive whole from the control thread, under the lock the renderer holds.

use std::sync::{Arc, Mutex};

use signal_plugin_host::{PluginEvents, PluginInstance};

/// The crossfade between two chains.
pub const FADE_SECS: f32 = 0.008;
/// A tail quieter than −90 dBFS …
const QUIET: f32 = 3.2e-5;
/// … for this long is over (long enough that the gap between two delay
/// repeats does not count as the end).
pub const QUIET_HOLD_SECS: f32 = 2.0;
/// No tail rings longer than this (a frozen reverb, feedback at 1).
pub const MAX_TAIL_SECS: f32 = 30.0;
/// A voice made room for fades out over this.
const STEAL_SECS: f32 = 0.03;
/// Tails ringing at once. A fifth switch in quick succession takes the
/// oldest one's place.
pub const MAX_VOICES: usize = 4;

/// The chain's input, shared from the input probe (the head of the chain)
/// to the stage (its tail), so a voice fading out can go on hearing the
/// guitar. Written and read on the audio thread, one after the other.
pub struct InputShare {
    buf: Mutex<(Vec<f32>, Vec<f32>, usize)>,
}

impl InputShare {
    #[must_use]
    pub fn new(max_block: usize) -> Arc<Self> {
        let share = Arc::new(Self {
            buf: Mutex::new((vec![0.0; max_block], vec![0.0; max_block], 0)),
        });
        // Lock it once here: a platform mutex may be set up lazily on first
        // use (a heap allocation), and that first use must not be the audio
        // thread's.
        drop(share.buf.lock());
        share
    }

    /// Keep this block's chain input.
    pub fn write(&self, l: &[f32], r: &[f32]) {
        if let Ok(mut b) = self.buf.try_lock() {
            let n = l.len().min(r.len()).min(b.0.len());
            b.0[..n].copy_from_slice(&l[..n]);
            b.1[..n].copy_from_slice(&r[..n]);
            b.2 = n;
        }
    }

    /// This block's chain input into `l`/`r`; silence where there is none.
    fn read(&self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len().min(r.len());
        match self.buf.try_lock() {
            Ok(b) if b.2 >= n => {
                l[..n].copy_from_slice(&b.0[..n]);
                r[..n].copy_from_slice(&b.1[..n]);
            }
            _ => {
                l[..n].fill(0.0);
                r[..n].fill(0.0);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// Crossfading out; `pos` samples in.
    Fading,
    /// Ringing on silence.
    Tail,
    /// Fading to nothing — made room for, or rang too long; `pos` samples in.
    Stealing,
    /// Over; waiting for the control thread to collect it.
    Finished,
}

/// An outgoing chain, ringing out.
pub struct Voice {
    /// Whose blocks these are (the rig's chain id).
    pub chain: u32,
    /// The chain's blocks, in order.
    pub boxes: Vec<Option<Box<dyn PluginInstance>>>,
    /// Where its Time section starts; `boxes.len()` when it has none.
    time_start: usize,
    /// Its patch's output level, linear.
    trim: f32,
    state: State,
    pos: usize,
    quiet: usize,
    age: usize,
}

impl Voice {
    #[must_use]
    pub fn new(chain: u32, boxes: Vec<Option<Box<dyn PluginInstance>>>, time_start: usize) -> Self {
        let time_start = time_start.min(boxes.len());
        Self {
            chain,
            boxes,
            time_start,
            trim: 1.0,
            state: State::Fading,
            pos: 0,
            quiet: 0,
            age: 0,
        }
    }

    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.state == State::Finished
    }
}

/// See the module docs.
pub struct TailStage {
    voices: [Option<Voice>; MAX_VOICES],
    input: Arc<InputShare>,
    /// The new chain's fade-in, `fade_pos` samples in.
    fade_pos: usize,
    fade_len: usize,
    steal_len: usize,
    quiet_hold: usize,
    max_age: usize,
    /// The patch's output level (linear): target, and where it has got to.
    trim_target: f32,
    trim: f32,
    trim_k: f32,
    // Reserved once.
    x_l: Vec<f32>,
    x_r: Vec<f32>,
    a_l: Vec<f32>,
    a_r: Vec<f32>,
    b_l: Vec<f32>,
    b_r: Vec<f32>,
}

impl TailStage {
    #[must_use]
    pub fn new(input: Arc<InputShare>, sample_rate: f64, max_block: usize) -> Self {
        let sr = sample_rate.max(1.0) as f32;
        let fade_len = ((FADE_SECS * sr) as usize).max(1);
        Self {
            voices: std::array::from_fn(|_| None),
            input,
            fade_pos: fade_len,
            fade_len,
            steal_len: ((STEAL_SECS * sr) as usize).max(1),
            quiet_hold: (QUIET_HOLD_SECS * sr) as usize,
            max_age: (MAX_TAIL_SECS * sr) as usize,
            trim_target: 1.0,
            trim: 1.0,
            // ~10 ms one-pole on a level change that is not a switch.
            trim_k: 1.0 - (-1.0 / (0.010 * sr)).exp(),
            x_l: vec![0.0; max_block],
            x_r: vec![0.0; max_block],
            a_l: vec![0.0; max_block],
            a_r: vec![0.0; max_block],
            b_l: vec![0.0; max_block],
            b_r: vec![0.0; max_block],
        }
    }

    /// Tails still ringing.
    #[must_use]
    pub fn ringing(&self) -> usize {
        self.voices
            .iter()
            .flatten()
            .filter(|v| !v.is_finished())
            .count()
    }

    /// The patch's output level (dB), followed smoothly — a level change
    /// that is not a switch.
    pub fn set_trim_db(&mut self, db: f32) {
        self.trim_target = db_to_lin(db);
    }

    /// A switch: the outgoing chain (if any) becomes a voice at the level it
    /// was playing at, and the incoming one fades in at `trim_db`. Returns a
    /// voice that had to make room, for the caller to collect.
    #[must_use]
    pub fn switch(&mut self, outgoing: Option<Voice>, trim_db: f32) -> Option<Voice> {
        let mut evicted = None;
        if let Some(mut v) = outgoing {
            v.trim = self.trim;
            evicted = self.push(v);
        }
        // A switch lands its level with the crossfade, not after it.
        self.trim_target = db_to_lin(trim_db);
        self.trim = self.trim_target;
        self.fade_pos = 0;
        evicted
    }

    /// Take `chain`'s voice back — switching to a patch whose tail is still
    /// ringing. The new fade-in starts where that voice's fade-out had got to.
    pub fn take(&mut self, chain: u32) -> Option<Voice> {
        let slot = self
            .voices
            .iter()
            .position(|v| v.as_ref().is_some_and(|v| v.chain == chain))?;
        let v = self.voices[slot].take()?;
        if v.state == State::Fading {
            self.fade_pos = self.fade_len.saturating_sub(v.pos);
        }
        Some(v)
    }

    /// The blocks of `chain`'s voice, still ringing — for a param write to
    /// reach a chain whose blocks are out here (control thread, under the
    /// renderer's lock).
    pub fn voice_boxes_mut(&mut self, chain: u32) -> Option<&mut [Option<Box<dyn PluginInstance>>]> {
        self.voices
            .iter_mut()
            .flatten()
            .find(|v| v.chain == chain)
            .map(|v| v.boxes.as_mut_slice())
    }

    /// Hand every finished voice to `out` (control thread).
    pub fn collect_finished(&mut self, out: &mut Vec<Voice>) {
        for slot in &mut self.voices {
            if slot.as_ref().is_some_and(Voice::is_finished) {
                if let Some(v) = slot.take() {
                    out.push(v);
                }
            }
        }
    }

    /// Place `v`; with every place taken, the oldest voice gives up its own
    /// and is returned. With all but one taken, the oldest starts fading so
    /// the next switch finds room.
    fn push(&mut self, v: Voice) -> Option<Voice> {
        let mut evicted = None;
        let slot = match self.voices.iter().position(Option::is_none) {
            Some(s) => s,
            None => {
                // Finished first, then Stealing, then the oldest.
                let s = self
                    .voices
                    .iter()
                    .position(|x| x.as_ref().is_some_and(Voice::is_finished))
                    .or_else(|| {
                        self.voices
                            .iter()
                            .position(|x| x.as_ref().is_some_and(|x| x.state == State::Stealing))
                    })
                    .unwrap_or_else(|| self.oldest().unwrap_or(0));
                evicted = self.voices[s].take();
                s
            }
        };
        self.voices[slot] = Some(v);
        if self.voices.iter().all(Option::is_some) {
            if let Some(o) = self.oldest_ringing() {
                if let Some(x) = self.voices[o].as_mut() {
                    x.state = State::Stealing;
                    x.pos = 0;
                }
            }
        }
        evicted
    }

    fn oldest(&self) -> Option<usize> {
        (0..MAX_VOICES).max_by_key(|&i| self.voices[i].as_ref().map_or(0, |v| v.age))
    }

    fn oldest_ringing(&self) -> Option<usize> {
        (0..MAX_VOICES)
            .filter(|&i| {
                self.voices[i]
                    .as_ref()
                    .is_some_and(|v| matches!(v.state, State::Tail | State::Fading))
            })
            .max_by_key(|&i| self.voices[i].as_ref().map_or(0, |v| v.age))
    }

    /// `out = main·fade-in·trim + every voice`. `main` is the new chain's
    /// output. Returns whether any voice sounded (the caller then guards the
    /// sum against clipping).
    pub fn process(
        &mut self,
        main_l: &[f32],
        main_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
    ) -> bool {
        let n = main_l.len().min(main_r.len()).min(out_l.len()).min(out_r.len());
        let fading = self.fade_pos < self.fade_len;
        for i in 0..n {
            self.trim += (self.trim_target - self.trim) * self.trim_k;
            let g = if fading {
                equal_power_in((self.fade_pos + i) as f32 / self.fade_len as f32)
            } else {
                1.0
            };
            out_l[i] = main_l[i] * g * self.trim;
            out_r[i] = main_r[i] * g * self.trim;
        }
        self.fade_pos = (self.fade_pos + n).min(self.fade_len);

        if self.voices.iter().all(|v| v.as_ref().is_none_or(Voice::is_finished)) {
            return false;
        }
        if n > self.x_l.len() {
            // A block larger than the scratch: tails pause for it.
            return false;
        }
        let mut any = false;
        let wants_input = self
            .voices
            .iter()
            .flatten()
            .any(|v| v.state == State::Fading);
        if wants_input {
            self.input.read(&mut self.x_l[..n], &mut self.x_r[..n]);
        }
        for slot in 0..MAX_VOICES {
            let Some(v) = self.voices[slot].as_mut() else { continue };
            if v.is_finished() {
                continue;
            }
            any = true;
            let bufs = Bufs {
                x_l: &self.x_l,
                x_r: &self.x_r,
                a_l: &mut self.a_l,
                a_r: &mut self.a_r,
                b_l: &mut self.b_l,
                b_r: &mut self.b_r,
            };
            render_voice(
                v,
                n,
                bufs,
                out_l,
                out_r,
                self.fade_len,
                self.steal_len,
                self.quiet_hold,
                self.max_age,
            );
        }
        any
    }
}

struct Bufs<'a> {
    x_l: &'a [f32],
    x_r: &'a [f32],
    a_l: &'a mut [f32],
    a_r: &'a mut [f32],
    b_l: &'a mut [f32],
    b_r: &'a mut [f32],
}

#[expect(clippy::too_many_arguments, reason = "one voice, rendered with the stage's settings")]
fn render_voice(
    v: &mut Voice,
    n: usize,
    b: Bufs<'_>,
    out_l: &mut [f32],
    out_r: &mut [f32],
    fade_len: usize,
    steal_len: usize,
    quiet_hold: usize,
    max_age: usize,
) {
    let Bufs { x_l, x_r, a_l, a_r, b_l, b_r } = b;
    let ts = v.time_start;
    // What goes into the Time section, in `a`.
    match v.state {
        State::Fading => {
            a_l[..n].copy_from_slice(&x_l[..n]);
            a_r[..n].copy_from_slice(&x_r[..n]);
            let in_a = run(&mut v.boxes[..ts], n, a_l, a_r, b_l, b_r);
            if !in_a {
                a_l[..n].copy_from_slice(&b_l[..n]);
                a_r[..n].copy_from_slice(&b_r[..n]);
            }
            for i in 0..n {
                let g = equal_power_out((v.pos + i) as f32 / fade_len as f32);
                a_l[i] *= g;
                a_r[i] *= g;
            }
        }
        _ => {
            a_l[..n].fill(0.0);
            a_r[..n].fill(0.0);
        }
    }
    let in_a = run(&mut v.boxes[ts..], n, a_l, a_r, b_l, b_r);
    let (yl, yr): (&[f32], &[f32]) = if in_a { (a_l, a_r) } else { (b_l, b_r) };

    let mut peak = 0.0f32;
    for i in 0..n {
        let g = match v.state {
            State::Stealing => 1.0 - ((v.pos + i) as f32 / steal_len as f32).min(1.0),
            _ => 1.0,
        };
        let (l, r) = (yl[i] * v.trim * g, yr[i] * v.trim * g);
        peak = peak.max(l.abs()).max(r.abs());
        out_l[i] += l;
        out_r[i] += r;
    }

    v.age = v.age.saturating_add(n);
    match v.state {
        State::Fading => {
            v.pos += n;
            if v.pos >= fade_len {
                v.state = if ts < v.boxes.len() { State::Tail } else { State::Finished };
                v.pos = 0;
            }
        }
        State::Tail => {
            if peak < QUIET {
                v.quiet = v.quiet.saturating_add(n);
            } else {
                v.quiet = 0;
            }
            if v.quiet >= quiet_hold {
                v.state = State::Finished;
            } else if v.age >= max_age {
                v.state = State::Stealing;
                v.pos = 0;
            }
        }
        State::Stealing => {
            v.pos += n;
            if v.pos >= steal_len {
                v.state = State::Finished;
            }
        }
        State::Finished => {}
    }
}

/// Run `boxes` in order on the signal in `a`, ping-ponging through `b`.
/// Returns whether the result ended up in `a`.
fn run(
    boxes: &mut [Option<Box<dyn PluginInstance>>],
    n: usize,
    a_l: &mut [f32],
    a_r: &mut [f32],
    b_l: &mut [f32],
    b_r: &mut [f32],
) -> bool {
    let ev = PluginEvents::EMPTY;
    let mut in_a = true;
    for bx in boxes.iter_mut().flatten() {
        let ok = if in_a {
            bx.process_block(&a_l[..n], &a_r[..n], &mut b_l[..n], &mut b_r[..n], &ev)
        } else {
            bx.process_block(&b_l[..n], &b_r[..n], &mut a_l[..n], &mut a_r[..n], &ev)
        };
        if ok.is_ok() {
            in_a = !in_a;
        }
    }
    in_a
}

fn equal_power_in(t: f32) -> f32 {
    (t.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2).sin()
}

fn equal_power_out(t: f32) -> f32 {
    (t.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2).cos()
}

fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_plugin_host::{PluginDescriptor, PluginError, PluginFormat, PluginParamInfo};

    /// A gain (a stand-in amp) or a decaying "reverb": `y = x + e`,
    /// `e ← decay·e + (1 − decay)·x`.
    struct Fx {
        gain: f32,
        decay: f32,
        e: f32,
        prepared_count: usize,
    }
    impl Fx {
        fn amp(gain: f32) -> Box<dyn PluginInstance> {
            Box::new(Self { gain, decay: 0.0, e: 0.0, prepared_count: 0 })
        }
        fn verb(decay: f32) -> Box<dyn PluginInstance> {
            Box::new(Self { gain: 1.0, decay, e: 0.0, prepared_count: 0 })
        }
    }
    impl PluginInstance for Fx {
        fn descriptor(&self) -> PluginDescriptor {
            PluginDescriptor {
                id: "fx".into(),
                name: "fx".into(),
                vendor: String::new(),
                version: String::new(),
                format: PluginFormat::Clap,
            }
        }
        fn params(&mut self) -> Vec<PluginParamInfo> {
            Vec::new()
        }
        fn param_value(&mut self, _: u32) -> Option<f64> {
            None
        }
        fn value_to_text(&mut self, _: u32, _: f64) -> Option<String> {
            None
        }
        fn text_to_value(&mut self, _: u32, _: &str) -> Option<f64> {
            None
        }
        fn latency(&mut self) -> u32 {
            0
        }
        fn prepare(&mut self, _: f64, _: u32) -> Result<(), PluginError> {
            self.prepared_count += 1;
            self.e = 0.0;
            Ok(())
        }
        fn is_prepared(&self) -> bool {
            true
        }
        fn process_block(
            &mut self,
            in_l: &[f32],
            _in_r: &[f32],
            out_l: &mut [f32],
            out_r: &mut [f32],
            _: &PluginEvents<'_>,
        ) -> Result<(), PluginError> {
            for i in 0..in_l.len() {
                let x = in_l[i] * self.gain;
                let y = if self.decay > 0.0 {
                    let y = x + self.e;
                    self.e = self.decay * self.e + (1.0 - self.decay) * x;
                    y
                } else {
                    x
                };
                out_l[i] = y;
                out_r[i] = y;
            }
            Ok(())
        }
        fn deactivate(&mut self) {}
    }

    const SR: f64 = 48_000.0;
    const N: usize = 128;

    /// Chain: amp (gain) then a reverb (the Time section, from index 1).
    fn chain(gain: f32) -> Vec<Option<Box<dyn PluginInstance>>> {
        vec![Some(Fx::amp(gain)), Some(Fx::verb(0.9995))]
    }

    fn block(stage: &mut TailStage, input: &Arc<InputShare>, main: &mut [Box<dyn PluginInstance>], x: f32) -> Vec<f32> {
        let xin = vec![x; N];
        input.write(&xin, &xin);
        let (mut a, mut b) = (xin.clone(), xin.clone());
        for bx in main.iter_mut() {
            let (mut ol, mut or) = (vec![0.0; N], vec![0.0; N]);
            bx.process_block(&a, &b, &mut ol, &mut or, &PluginEvents::EMPTY).unwrap();
            a = ol;
            b = or;
        }
        let (mut ol, mut or) = (vec![0.0; N], vec![0.0; N]);
        stage.process(&a, &b, &mut ol, &mut or);
        ol
    }

    #[test]
    fn the_old_tail_rings_on_after_a_switch() {
        let input = InputShare::new(1024);
        let mut stage = TailStage::new(input.clone(), SR, 1024);
        let mut a: Vec<Box<dyn PluginInstance>> = chain(1.0).into_iter().flatten().collect();
        for _ in 0..50 {
            block(&mut stage, &input, &mut a, 0.5);
        }
        // Switch to a dry chain; the guitar stops.
        let voice = Voice::new(0, a.into_iter().map(Some).collect(), 1);
        assert!(stage.switch(Some(voice), 0.0).is_none());
        let mut b: Vec<Box<dyn PluginInstance>> = vec![Fx::amp(1.0)];
        let mut out = Vec::new();
        for _ in 0..40 {
            out.extend(block(&mut stage, &input, &mut b, 0.0));
        }
        assert!(out[out.len() - 1] > 0.01, "A's reverb still rings: {}", out[out.len() - 1]);
        assert_eq!(stage.ringing(), 1);
    }

    #[test]
    fn a_switch_crossfades_without_a_step() {
        let input = InputShare::new(1024);
        let mut stage = TailStage::new(input.clone(), SR, 1024);
        let mut a: Vec<Box<dyn PluginInstance>> = vec![Fx::amp(2.0)];
        let before = block(&mut stage, &input, &mut a, 0.25);
        let voice = Voice::new(0, a.into_iter().map(Some).collect(), 1);
        let _ = stage.switch(Some(voice), 0.0);
        let mut b: Vec<Box<dyn PluginInstance>> = vec![Fx::amp(0.5)];
        let mut out = before.clone();
        for _ in 0..8 {
            out.extend(block(&mut stage, &input, &mut b, 0.25));
        }
        let max_jump = out.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        // 0.5 → 0.125 in ~384 samples: nothing near the 0.375 a hard switch steps.
        assert!(max_jump < 0.01, "largest step {max_jump}");
        assert!((out[out.len() - 1] - 0.125).abs() < 1e-4, "lands on B");
    }

    #[test]
    fn a_quiet_tail_finishes_and_is_collected() {
        let input = InputShare::new(1024);
        let mut stage = TailStage::new(input.clone(), SR, 1024);
        let a: Vec<Option<Box<dyn PluginInstance>>> = vec![Some(Fx::amp(1.0)), Some(Fx::amp(1.0))];
        let _ = stage.switch(Some(Voice::new(7, a, 1)), 0.0);
        let mut b: Vec<Box<dyn PluginInstance>> = vec![Fx::amp(1.0)];
        for _ in 0..((QUIET_HOLD_SECS as f64 * SR) as usize / N + 10) {
            block(&mut stage, &input, &mut b, 0.0);
        }
        assert_eq!(stage.ringing(), 0);
        let mut done = Vec::new();
        stage.collect_finished(&mut done);
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].chain, 7);
        assert_eq!(done[0].boxes.len(), 2, "the blocks come back whole");
    }

    #[test]
    fn switching_back_resumes_the_ringing_chain() {
        let input = InputShare::new(1024);
        let mut stage = TailStage::new(input.clone(), SR, 1024);
        let _ = stage.switch(Some(Voice::new(3, chain(1.0), 1)), 0.0);
        let back = stage.take(3).expect("still ringing");
        assert_eq!(back.boxes.len(), 2);
        assert_eq!(stage.ringing(), 0);
    }

    #[test]
    fn rapid_switches_keep_at_most_four_tails() {
        let input = InputShare::new(1024);
        let mut stage = TailStage::new(input.clone(), SR, 1024);
        let mut evicted = 0;
        for id in 0..7 {
            if stage.switch(Some(Voice::new(id, chain(1.0), 1)), 0.0).is_some() {
                evicted += 1;
            }
        }
        assert!(stage.ringing() <= MAX_VOICES);
        assert_eq!(evicted, 3, "each switch past four hands one back");
    }
}
