//! The Time module's two effects of a kind in parallel, not in series.
//!
//! A patch's chain runs its blocks one after another, so `DLY 1 → DLY 2`
//! meant the second delay repeated the first one's repeats, and `VERB 1 →
//! VERB 2` reverberated a reverb. The Time module is two stages instead:
//!
//! ```text
//! delay in  ─┬─ Delay 1 ─┐          reverb in ─┬─ Reverb 1 ─┐
//!            ├─ Dry ─────┼─ out  →             ├─ Dry ──────┼─ out
//!            └─ Delay 2 ─┘                     └─ Reverb 2 ─┘
//! ```
//!
//! done, like the dual-amp stage (`amp_blend`), without changing the chain's
//! shape — every block keeps its slot, id, bypass and params:
//!
//! - the first of the pair ([`Role::Tap`]) keeps the stage input it is
//!   handed and plays as it is: the dry (its `dry` knob is the stage's Dry)
//!   plus its own effect;
//! - the second ([`Role::Add`]) runs its effect on the kept input, fully
//!   wet, and adds that to what arrives.
//!
//! A tap the host skips (bypassed) keeps nothing, and the add block then
//! plays the stage input it receives: the dry at unity plus its effect.

use std::sync::{Arc, Mutex};

use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginInstance, PluginParamInfo,
};

/// The stages, by block name: (tap, add).
pub const PAIRS: [(&str, &str); 2] = [("DLY 1", "DLY 2"), ("VERB 1", "VERB 2")];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Tap,
    Add,
}

/// What a stage's two blocks share, allocated once per chain.
pub struct Shared {
    in_l: Vec<f32>,
    in_r: Vec<f32>,
    /// The tap kept this block's input.
    fresh: bool,
}

impl Shared {
    #[must_use]
    pub fn new(max_block: usize) -> Arc<Mutex<Self>> {
        let shared = Arc::new(Mutex::new(Self {
            in_l: vec![0.0; max_block],
            in_r: vec![0.0; max_block],
            fresh: false,
        }));
        // Lock it once here: a platform mutex may be set up lazily on first
        // use (a heap allocation), and that must not be the audio thread's.
        drop(shared.lock());
        shared
    }
}

/// One block of a time stage, wrapping the block's own processor.
pub struct TimeStage {
    inner: Box<dyn PluginInstance>,
    role: Role,
    shared: Arc<Mutex<Shared>>,
    /// Add role: the inner block's `dry` param, held at 0 (wet only) — the
    /// stage's dry is the tap's.
    dry_id: Option<u32>,
    /// Add role: the wet-only write still has to reach the block (the
    /// first block, and again after any param write that could restore it).
    force_wet: bool,
    /// Add role: the events handed on, plus the wet-only write — reserved
    /// once, so the audio thread never allocates.
    events_buf: Vec<(u32, f64)>,
    /// Add role: the effect's wet output, and the stage input copied out
    /// of the shared buffers — both reserved once (no audio-thread alloc).
    wet_l: Vec<f32>,
    wet_r: Vec<f32>,
    kept_l: Vec<f32>,
    kept_r: Vec<f32>,
}

impl TimeStage {
    #[must_use]
    pub fn new(
        mut inner: Box<dyn PluginInstance>,
        role: Role,
        shared: Arc<Mutex<Shared>>,
        max_block: usize,
    ) -> Self {
        let dry_id = inner
            .params()
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case("dry"))
            .map(|p| p.id);
        Self {
            inner,
            role,
            shared,
            dry_id,
            force_wet: true,
            events_buf: Vec::with_capacity(64),
            wet_l: vec![0.0; max_block],
            wet_r: vec![0.0; max_block],
            kept_l: vec![0.0; max_block],
            kept_r: vec![0.0; max_block],
        }
    }
}

impl PluginInstance for TimeStage {
    fn descriptor(&self) -> PluginDescriptor {
        self.inner.descriptor()
    }
    fn params(&mut self) -> Vec<PluginParamInfo> {
        self.inner.params()
    }
    fn param_value(&mut self, id: u32) -> Option<f64> {
        self.inner.param_value(id)
    }
    fn value_to_text(&mut self, id: u32, value: f64) -> Option<String> {
        self.inner.value_to_text(id, value)
    }
    fn text_to_value(&mut self, id: u32, text: &str) -> Option<f64> {
        self.inner.text_to_value(id, text)
    }
    fn latency(&mut self) -> u32 {
        self.inner.latency()
    }
    fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), PluginError> {
        self.force_wet = true;
        self.inner.prepare(sample_rate, block_size)
    }
    fn is_prepared(&self) -> bool {
        self.inner.is_prepared()
    }
    fn deactivate(&mut self) {
        self.inner.deactivate();
    }
    fn load_state(&mut self, state: &[u8]) -> Result<(), PluginError> {
        self.force_wet = true;
        self.inner.load_state(state)
    }
    fn save_state(&mut self) -> Result<Vec<u8>, PluginError> {
        self.inner.save_state()
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        self.inner.as_any_mut()
    }

    fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), PluginError> {
        let n = in_l.len().min(in_r.len()).min(out_l.len()).min(out_r.len());
        match self.role {
            Role::Tap => {
                if let Ok(mut s) = self.shared.try_lock() {
                    if n <= s.in_l.len() {
                        s.in_l[..n].copy_from_slice(&in_l[..n]);
                        s.in_r[..n].copy_from_slice(&in_r[..n]);
                        s.fresh = true;
                    }
                }
                self.inner.process_block(in_l, in_r, out_l, out_r, events)
            }
            Role::Add => {
                if n > self.wet_l.len() {
                    return self.inner.process_block(in_l, in_r, out_l, out_r, events);
                }
                // Wet only: the stage's dry is the tap's. Sent as a param
                // write alongside whatever else this block carries.
                let wet_only = self.dry_id.filter(|_| self.force_wet || !events.params.is_empty());
                let ev_store;
                let ev = match wet_only {
                    Some(id) => {
                        self.events_buf.clear();
                        self.events_buf.extend(
                            events.params.iter().copied().filter(|(p, _)| *p != id),
                        );
                        self.events_buf.push((id, 0.0));
                        self.force_wet = false;
                        ev_store = PluginEvents {
                            params: &self.events_buf,
                            ..*events
                        };
                        &ev_store
                    }
                    None => events,
                };
                // The input the stage split: the tap's, or — with the tap
                // bypassed — what arrives here.
                let kept = self.shared.try_lock().ok().is_some_and(|mut s| {
                    let fresh = std::mem::take(&mut s.fresh);
                    if fresh && n <= s.in_l.len() {
                        self.kept_l[..n].copy_from_slice(&s.in_l[..n]);
                        self.kept_r[..n].copy_from_slice(&s.in_r[..n]);
                    }
                    fresh && n <= s.in_l.len()
                });
                let result = match kept {
                    true => self.inner.process_block(
                        &self.kept_l[..n],
                        &self.kept_r[..n],
                        &mut self.wet_l[..n],
                        &mut self.wet_r[..n],
                        ev,
                    ),
                    false => self.inner.process_block(
                        in_l,
                        in_r,
                        &mut self.wet_l[..n],
                        &mut self.wet_r[..n],
                        ev,
                    ),
                };
                // What arrives (dry + the tap's effect) plus this effect.
                for i in 0..n {
                    out_l[i] = in_l[i] + self.wet_l[i];
                    out_r[i] = in_r[i] + self.wet_r[i];
                }
                result
            }
        }
    }
}

/// The roles for a chain's blocks, by index: a stage exists when both of a
/// pair are present, tap first.
#[must_use]
pub fn roles(names: &[&str]) -> Vec<Option<Role>> {
    let mut out = vec![None; names.len()];
    let find = |want: &str| names.iter().position(|n| n.eq_ignore_ascii_case(want));
    for (tap, add) in PAIRS {
        if let (Some(t), Some(a)) = (find(tap), find(add)) {
            if t < a {
                out[t] = Some(Role::Tap);
                out[a] = Some(Role::Add);
            }
        }
    }
    out
}

/// Wrap each stage's blocks in [`TimeStage`]s, in place; every other block
/// is left exactly where it is.
pub fn wrap(boxes: &mut [Option<Box<dyn PluginInstance>>], names: &[&str], max_block: usize) {
    let roles = roles(names);
    for (tap, add) in PAIRS {
        let find = |want: &str| names.iter().position(|n| n.eq_ignore_ascii_case(want));
        let (Some(t), Some(a)) = (find(tap), find(add)) else {
            continue;
        };
        if roles[t] != Some(Role::Tap) || roles[a] != Some(Role::Add) {
            continue;
        }
        let shared = Shared::new(max_block);
        for (i, role) in [(t, Role::Tap), (a, Role::Add)] {
            if let Some(inner) = boxes.get_mut(i).and_then(Option::take) {
                boxes[i] = Some(Box::new(TimeStage::new(inner, role, shared.clone(), max_block)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_plugin_host::PluginFormat;

    /// A parallel effect: `dry·in + wet·k`, where "wet" is a constant `k`
    /// (enough to tell series from parallel). Param 0 is `dry`.
    struct Fx {
        k: f32,
        dry: f32,
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
            vec![PluginParamInfo { id: 0, name: "dry".into(), min: 0.0, max: 1.0, default: 1.0 }]
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
            Ok(())
        }
        fn is_prepared(&self) -> bool {
            true
        }
        fn process_block(
            &mut self,
            in_l: &[f32],
            in_r: &[f32],
            out_l: &mut [f32],
            out_r: &mut [f32],
            ev: &PluginEvents<'_>,
        ) -> Result<(), PluginError> {
            for &(id, v) in ev.params {
                if id == 0 {
                    self.dry = v as f32;
                }
            }
            // "Wet" is the input times k: a series pair would multiply.
            for i in 0..in_l.len() {
                out_l[i] = self.dry * in_l[i] + self.k * in_l[i];
                out_r[i] = self.dry * in_r[i] + self.k * in_r[i];
            }
            Ok(())
        }
        fn deactivate(&mut self) {}
    }

    fn run(chain: &mut [Box<dyn PluginInstance>], input: f32) -> f32 {
        let mut buf = [input; 4];
        let events = PluginEvents::default();
        for block in chain.iter_mut() {
            let (inl, inr) = (buf, buf);
            let (mut ol, mut or) = ([0.0; 4], [0.0; 4]);
            block.process_block(&inl, &inr, &mut ol, &mut or, &events).unwrap();
            buf = ol;
        }
        buf[0]
    }

    fn chain(names: &[&str], ks: &[f32]) -> Vec<Box<dyn PluginInstance>> {
        let mut boxes: Vec<Option<Box<dyn PluginInstance>>> = ks
            .iter()
            .map(|&k| Some(Box::new(Fx { k, dry: 1.0 }) as Box<dyn PluginInstance>))
            .collect();
        wrap(&mut boxes, names, 8);
        boxes.into_iter().flatten().collect()
    }

    /// Both delays hear the stage input: 1 + 0.5 + 0.25, not the series
    /// (1 + 0.5)(1 + 0.25).
    #[test]
    fn the_two_delays_run_in_parallel_with_the_dry() {
        let mut c = chain(&["DLY 1", "DLY 2"], &[0.5, 0.25]);
        assert!((run(&mut c, 1.0) - 1.75).abs() < 1e-6);
        assert!((run(&mut c, 1.0) - 1.75).abs() < 1e-6, "no state leaks between blocks");
    }

    /// The reverbs split what the delay stage made: (1.75)·(1 + 0.5 + 0.25).
    #[test]
    fn the_reverb_stage_hears_the_delay_stage() {
        let mut c = chain(&["DLY 1", "DLY 2", "VERB 1", "VERB 2"], &[0.5, 0.25, 0.5, 0.25]);
        assert!((run(&mut c, 1.0) - 1.75 * 1.75).abs() < 1e-5);
    }

    /// Anything else in the chain is left alone.
    #[test]
    fn blocks_outside_a_pair_are_untouched() {
        assert_eq!(roles(&["Gate", "DLY 1", "Boost"]), vec![None, None, None]);
    }
}
