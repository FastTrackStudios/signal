//! Two amps in parallel on a serial chain — the dual-amp stage.
//!
//! A patch's chain runs its blocks one after another on one buffer. Two amps
//! blended ("Deluxe + AC30") are not in series: both hear the guitar, and
//! their outputs are summed. That is done here without changing the chain's
//! shape, so every block keeps its slot, its id, its bypass and its trims:
//!
//! - **Amp L** ([`Role::Tap`]) keeps the dry signal it is handed.
//! - **Amp R** ([`Role::Swap`]) keeps what arrives (Amp L → Cab L) and plays
//!   the dry signal instead.
//! - **Cab R** ([`Role::Merge`]) — or Amp R itself when there is no Cab R —
//!   sums its output with what was kept, each at half, so two amps of equal
//!   loudness blend at the loudness of one.
//!
//! All three run on the audio thread in chain order, so the shared buffers
//! are never contended; the lock is only there to make that sharing sound,
//! and a failed `try_lock` degrades to plain series rather than blocking.
//! A stage the host skips (bypassed Amp L) leaves `fresh` unset, and Amp R
//! then plays its own input — series, never a stale buffer.

use std::sync::{Arc, Mutex};

use signal_plugin_host::{
    PluginDescriptor, PluginError, PluginEvents, PluginInstance, PluginParamInfo,
};

/// The block names that make the stage.
pub const AMP_L: &str = "Amp L";
pub const AMP_R: &str = "Amp R";
pub const CAB_R: &str = "Cab R";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Tap,
    Swap { merge_here: bool },
    Merge,
}

/// Buffers the stage shares, allocated once per chain.
pub struct Shared {
    dry_l: Vec<f32>,
    dry_r: Vec<f32>,
    wet_l: Vec<f32>,
    wet_r: Vec<f32>,
    /// Amp L kept this block's dry signal.
    fresh: bool,
    /// Amp R kept Amp L's output for the merge.
    have_wet: bool,
}

impl Shared {
    #[must_use]
    pub fn new(max_block: usize) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            dry_l: vec![0.0; max_block],
            dry_r: vec![0.0; max_block],
            wet_l: vec![0.0; max_block],
            wet_r: vec![0.0; max_block],
            fresh: false,
            have_wet: false,
        }))
    }
}

/// One block of the stage, wrapping the block's own processor.
pub struct BlendStage {
    inner: Box<dyn PluginInstance>,
    role: Role,
    shared: Arc<Mutex<Shared>>,
}

impl BlendStage {
    #[must_use]
    pub fn new(inner: Box<dyn PluginInstance>, role: Role, shared: Arc<Mutex<Shared>>) -> Self {
        Self {
            inner,
            role,
            shared,
        }
    }
}

fn merge(out_l: &mut [f32], out_r: &mut [f32], s: &mut Shared) {
    if s.have_wet {
        let n = out_l.len();
        for i in 0..n {
            out_l[i] = 0.5 * (out_l[i] + s.wet_l[i]);
            out_r[i] = 0.5 * (out_r[i] + s.wet_r[i]);
        }
    }
    s.fresh = false;
    s.have_wet = false;
}

impl PluginInstance for BlendStage {
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
        self.inner.prepare(sample_rate, block_size)
    }
    fn is_prepared(&self) -> bool {
        self.inner.is_prepared()
    }
    fn deactivate(&mut self) {
        self.inner.deactivate();
    }
    fn load_state(&mut self, state: &[u8]) -> Result<(), PluginError> {
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
        let Ok(mut s) = self.shared.try_lock() else {
            return self.inner.process_block(in_l, in_r, out_l, out_r, events);
        };
        if n > s.dry_l.len() {
            drop(s);
            return self.inner.process_block(in_l, in_r, out_l, out_r, events);
        }
        match self.role {
            Role::Tap => {
                s.dry_l[..n].copy_from_slice(&in_l[..n]);
                s.dry_r[..n].copy_from_slice(&in_r[..n]);
                s.fresh = true;
                drop(s);
                self.inner.process_block(in_l, in_r, out_l, out_r, events)
            }
            Role::Swap { merge_here } => {
                let result = if s.fresh {
                    let sh = &mut *s;
                    sh.wet_l[..n].copy_from_slice(&in_l[..n]);
                    sh.wet_r[..n].copy_from_slice(&in_r[..n]);
                    sh.have_wet = true;
                    self.inner.process_block(
                        &sh.dry_l[..n],
                        &sh.dry_r[..n],
                        &mut out_l[..n],
                        &mut out_r[..n],
                        events,
                    )
                } else {
                    s.have_wet = false;
                    self.inner.process_block(in_l, in_r, out_l, out_r, events)
                };
                if merge_here {
                    merge(&mut out_l[..n], &mut out_r[..n], &mut s);
                }
                result
            }
            Role::Merge => {
                let result = self.inner.process_block(in_l, in_r, out_l, out_r, events);
                merge(&mut out_l[..n], &mut out_r[..n], &mut s);
                result
            }
        }
    }
}

/// The roles for a chain's blocks, by index: the stage exists only when
/// Amp L and a *loaded* Amp R (`r_loaded`) are both in the chain.
#[must_use]
pub fn roles(names: &[&str], r_loaded: bool) -> Vec<Option<Role>> {
    let mut out = vec![None; names.len()];
    let find = |want: &str| names.iter().position(|n| n.eq_ignore_ascii_case(want));
    let (Some(l), Some(r)) = (find(AMP_L), find(AMP_R)) else {
        return out;
    };
    if !r_loaded || r < l {
        return out;
    }
    let cab = find(CAB_R).filter(|&c| c > r);
    out[l] = Some(Role::Tap);
    out[r] = Some(Role::Swap {
        merge_here: cab.is_none(),
    });
    if let Some(c) = cab {
        out[c] = Some(Role::Merge);
    }
    out
}

/// Wrap the stage's blocks of a built chain in [`BlendStage`]s, in place;
/// every other block is left exactly where it is.
pub fn wrap(boxes: &mut [Option<Box<dyn PluginInstance>>], roles: &[Option<Role>], max_block: usize) {
    if roles.iter().all(Option::is_none) {
        return;
    }
    let shared = Shared::new(max_block);
    for (slot, role) in boxes.iter_mut().zip(roles) {
        // Only a block with a role is taken out. (This once read
        // `if let (Some(role), Some(inner)) = (role, slot.take())`, which
        // takes every box before matching — and dropped each one without a
        // role: a blend patch played its two amps and nothing else.)
        if let Some(role) = *role
            && let Some(inner) = slot.take()
        {
            *slot = Some(Box::new(BlendStage::new(inner, role, shared.clone())));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_plugin_host::PluginFormat;

    /// A block that adds a constant — enough to tell series from parallel.
    struct Add(f32);
    impl PluginInstance for Add {
        fn descriptor(&self) -> PluginDescriptor {
            PluginDescriptor {
                id: "add".into(),
                name: "add".into(),
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
            _: &PluginEvents<'_>,
        ) -> Result<(), PluginError> {
            for i in 0..in_l.len() {
                out_l[i] = in_l[i] + self.0;
                out_r[i] = in_r[i] + self.0;
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
            block
                .process_block(&inl, &inr, &mut ol, &mut or, &events)
                .unwrap();
            buf = ol;
        }
        buf[0]
    }

    fn stage(with_cab: bool) -> Vec<Box<dyn PluginInstance>> {
        let names: Vec<&str> = if with_cab {
            vec!["Amp L", "Cab L", "Amp R", "Cab R"]
        } else {
            vec!["Amp L", "Cab L", "Amp R"]
        };
        let adds = [10.0, 1.0, 100.0, 1000.0];
        let shared = Shared::new(8);
        roles(&names, true)
            .into_iter()
            .enumerate()
            .map(|(i, role)| -> Box<dyn PluginInstance> {
                let inner: Box<dyn PluginInstance> = Box::new(Add(adds[i]));
                match role {
                    Some(r) => Box::new(BlendStage::new(inner, r, shared.clone())),
                    None => inner,
                }
            })
            .collect()
    }

    /// Both amps hear the guitar, and meet at the end at half each:
    /// L path = 0+10+1 = 11, R path = 0+100+1000 = 1100 → (11+1100)/2.
    #[test]
    fn the_two_amps_run_in_parallel_and_blend() {
        let mut chain = stage(true);
        assert!((run(&mut chain, 0.0) - 555.5).abs() < 1e-4);
        // Every block, not just the first: no state leaks between blocks.
        assert!((run(&mut chain, 0.0) - 555.5).abs() < 1e-4);
    }

    /// With no cab after Amp R, Amp R merges itself: (11 + 100) / 2.
    #[test]
    fn without_cab_r_amp_r_merges() {
        let mut chain = stage(false);
        assert!((run(&mut chain, 0.0) - 55.5).abs() < 1e-4);
    }

    /// The blocks around the stage stay in the chain: a gain after the
    /// merge still applies. (L = 0+10+1, R = 0+100, merged at Amp R, then +5.)
    #[test]
    fn wrap_keeps_every_block_without_a_role() {
        let names = ["Gate", "Amp L", "Cab L", "Amp R", "Patch Trim"];
        let adds = [0.0, 10.0, 1.0, 100.0, 5.0];
        let mut boxes: Vec<Option<Box<dyn PluginInstance>>> = adds
            .iter()
            .map(|&a| Some(Box::new(Add(a)) as Box<dyn PluginInstance>))
            .collect();
        wrap(&mut boxes, &roles(&names, true), 8);
        assert!(boxes.iter().all(Option::is_some));
        let mut chain: Vec<Box<dyn PluginInstance>> = boxes.into_iter().flatten().collect();
        assert!((run(&mut chain, 0.0) - (55.5 + 5.0)).abs() < 1e-4);
    }

    /// An empty Amp R is no stage at all — the chain stays plain series.
    #[test]
    fn an_unloaded_amp_r_is_not_a_stage() {
        assert!(
            roles(&["Amp L", "Cab L", "Amp R", "Cab R"], false)
                .iter()
                .all(Option::is_none)
        );
    }
}
