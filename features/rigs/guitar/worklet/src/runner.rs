//! A patch's chain, run by the host itself — the browser's render loop.
//!
//! Natively the chain runs inside `GuitarRig`'s daw project; the browser has
//! no daw project, only an AudioWorklet asking for 128 frames at a time. So
//! this plays a [`PreparedChain`](signal_sampler::rig::PreparedChain)'s
//! blocks in order on two ping-pong buffers, with bypass, parameter changes
//! and meters — and with the NAM models placed by [`crate::plan`].

use std::sync::Arc;

use signal_plugin_host::{PluginEvents, PluginInstance};
use signal_sampler::RigBlock;
use signal_sampler::amp_blend::{self, Role};
use signal_sampler::rig::prepare_chain_with;

use crate::plan::{self, Model, Place, Plan};
use crate::remote::{Lag, Link, Prefetch, RemoteBlock, RemoteStats, Standby};

/// Parameter changes one slot can queue between quanta.
const MAX_PENDING: usize = 16;

struct Slot {
    id: String,
    block: Box<dyn PluginInstance>,
    bypass: bool,
    pending: Vec<(u32, f64)>,
    /// A model one quantum behind on a worker. While bypassed it still hears
    /// its input when its worker is free (see [`Standby::feed`]), so
    /// switching it on is seamless: no silent first quantum, no cold model.
    standby: Option<Standby>,
}

/// A chain ready to play. See the module docs.
pub struct ChainRunner {
    slots: Vec<Slot>,
    /// Amp R's early post, made where Amp L starts.
    prefetch: Option<(usize, usize, Prefetch)>,
    remote_stats: Vec<Arc<RemoteStats>>,
    plan: Plan,
    quantum: u32,
    a_l: Vec<f32>,
    a_r: Vec<f32>,
    b_l: Vec<f32>,
    b_r: Vec<f32>,
    in_peak: f32,
    out_peak: f32,
    /// The patch's input and output trims, linear.
    in_gain: f32,
    out_gain: f32,
}

/// What a host needs to know to build a planned chain.
pub struct BuildOptions<'a> {
    pub sample_rate: u32,
    /// The host's block size (128 in a browser).
    pub quantum: u32,
    /// Per-quantum µs the render thread may spend on models (the plan's).
    pub budget_us: u32,
    /// Open a link to the worker running the block in slot `i`.
    pub link: &'a mut dyn FnMut(usize, &RigBlock) -> Result<Box<dyn Link>, String>,
}

fn roles(blocks: &[RigBlock]) -> Vec<Option<Role>> {
    let names: Vec<&str> = blocks.iter().map(|b| b.name.as_str()).collect();
    let r_loaded = blocks
        .iter()
        .any(|b| b.name.eq_ignore_ascii_case(amp_blend::AMP_R) && b.is_nam());
    amp_blend::roles(&names, r_loaded)
}

/// Where `blocks`' models run (see [`crate::plan`]). `cost_us` is each
/// model's per-quantum cost, measured or estimated.
#[must_use]
pub fn plan_chain(
    blocks: &[RigBlock],
    cost_us: &dyn Fn(usize, &RigBlock) -> u32,
    budget_us: u32,
) -> Plan {
    let roles = roles(blocks);
    let models: Vec<Model> = blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.is_nam())
        .map(|(slot, b)| Model {
            slot,
            cost_us: cost_us(slot, b),
            parallel: matches!(roles[slot], Some(Role::Swap { .. })),
            pinned: matches!(roles[slot], Some(Role::Tap)),
            active: !b.bypassed,
        })
        .collect();
    plan::plan(&models, budget_us)
}

/// Build the chain as `plan` places it.
///
/// # Errors
///
/// When a block fails to build or a worker link cannot be opened.
pub fn build(
    blocks: &[RigBlock],
    ids: &[String],
    plan: &Plan,
    opts: BuildOptions<'_>,
) -> Result<ChainRunner, String> {
    let roles = roles(blocks);
    let mut remote_stats = Vec::new();
    let mut standbys: Vec<(usize, Standby)> = Vec::new();
    let mut amp_r_prefetch = None;
    let mut link_err = None;
    let wait_budget = opts.budget_us.max(1);
    let prepared = prepare_chain_with(blocks, ids, opts.sample_rate, &mut |slot, block| {
        let Place::Worker(lag) = plan.place(slot) else {
            return None;
        };
        let link = match (opts.link)(slot, block) {
            Ok(link) => link,
            Err(e) => {
                link_err = Some(e);
                return None;
            }
        };
        // A lag-one worker had a whole quantum already; wait only a little
        // more for a late one. A zero-lag one is waited on up to the budget.
        let budget = if lag == Lag::One {
            wait_budget / 4
        } else {
            wait_budget
        };
        let remote = RemoteBlock::new(block.name.clone(), link, lag, budget, opts.quantum);
        remote_stats.push(remote.stats());
        if let Some(s) = remote.standby() {
            standbys.push((slot, s));
        }
        if matches!(roles[slot], Some(Role::Swap { .. })) {
            amp_r_prefetch = remote.prefetch().map(|p| (slot, p));
        }
        Some(Box::new(remote))
    })?;
    if let Some(e) = link_err {
        return Err(e);
    }
    let amp_l = roles.iter().position(|r| matches!(r, Some(Role::Tap)));
    let prefetch = match (amp_l, amp_r_prefetch) {
        (Some(l), Some((r, p))) => Some((l, r, p)),
        _ => None,
    };

    let max = signal_sampler::mixer::FX_PREPARE_BLOCK as usize;
    Ok(ChainRunner {
        // One box per block, in order — so a block's saved bypass carries over.
        slots: prepared
            .into_blocks()
            .into_iter()
            .zip(blocks)
            .enumerate()
            .map(|(slot, ((id, block), def))| Slot {
                id,
                block,
                bypass: def.bypassed,
                pending: Vec::with_capacity(MAX_PENDING),
                standby: standbys
                    .iter()
                    .find(|(s, _)| *s == slot)
                    .map(|(_, h)| h.clone()),
            })
            .collect(),
        prefetch,
        remote_stats,
        plan: plan.clone(),
        quantum: opts.quantum,
        a_l: vec![0.0; max],
        a_r: vec![0.0; max],
        b_l: vec![0.0; max],
        b_r: vec![0.0; max],
        in_peak: 0.0,
        out_peak: 0.0,
        in_gain: 1.0,
        out_gain: 1.0,
    })
}

fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

impl ChainRunner {
    /// Play one block: mono guitar in, stereo out.
    pub fn process(&mut self, input: &[f32], out_l: &mut [f32], out_r: &mut [f32]) {
        let n = input
            .len()
            .min(out_l.len())
            .min(out_r.len())
            .min(self.a_l.len());
        self.in_peak = self.in_peak.max(peak(&input[..n]));
        for i in 0..n {
            self.a_l[i] = input[i] * self.in_gain;
        }
        self.a_r[..n].copy_from_slice(&self.a_l[..n]);
        let prefetch = self.prefetch.as_ref().filter(|(l, r, _)| {
            // A bypassed amp breaks the parallel stage back into series
            // (see `amp_blend`), and Amp R then plays its own input.
            !self.slots[*l].bypass && !self.slots[*r].bypass
        });
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if slot.bypass {
                if let Some(standby) = &slot.standby {
                    standby.feed(&self.a_l[..n], &self.a_r[..n]);
                }
                continue;
            }
            if let Some((l, _, p)) = prefetch
                && *l == i
            {
                p.post(&self.a_l[..n], &self.a_r[..n]);
            }
            let events = PluginEvents {
                params: &slot.pending,
                ..PluginEvents::default()
            };
            if slot
                .block
                .process_block(
                    &self.a_l[..n],
                    &self.a_r[..n],
                    &mut self.b_l[..n],
                    &mut self.b_r[..n],
                    &events,
                )
                .is_ok()
            {
                std::mem::swap(&mut self.a_l, &mut self.b_l);
                std::mem::swap(&mut self.a_r, &mut self.b_r);
            }
            slot.pending.clear();
        }
        for i in 0..n {
            out_l[i] = self.a_l[i] * self.out_gain;
            out_r[i] = self.a_r[i] * self.out_gain;
        }
        self.out_peak = self.out_peak.max(peak(&out_l[..n]).max(peak(&out_r[..n])));
    }

    /// The patch's input and output trims, in dB (as `RigPatch` has them).
    pub fn set_trims(&mut self, input_db: f32, output_db: f32) {
        self.in_gain = 10f32.powf(input_db / 20.0);
        self.out_gain = 10f32.powf(output_db / 20.0);
    }

    /// Queue a parameter change for the next quantum. False when no block
    /// has that id (or its queue is full).
    pub fn set_param(&mut self, block_id: &str, param: u32, value: f64) -> bool {
        let Some(slot) = self.slots.iter_mut().find(|s| s.id == block_id) else {
            return false;
        };
        if let Some(p) = slot.pending.iter_mut().find(|(id, _)| *id == param) {
            p.1 = value;
            return true;
        }
        if slot.pending.len() >= MAX_PENDING {
            return false;
        }
        slot.pending.push((param, value));
        true
    }

    pub fn set_bypass(&mut self, block_id: &str, bypass: bool) -> bool {
        let Some(slot) = self.slots.iter_mut().find(|s| s.id == block_id) else {
            return false;
        };
        slot.bypass = bypass;
        true
    }

    /// The peaks since the last call, in and out (linear).
    pub fn take_peaks(&mut self) -> (f32, f32) {
        let p = (self.in_peak, self.out_peak);
        self.in_peak = 0.0;
        self.out_peak = 0.0;
        p
    }

    /// Quanta any worker was late for, since the chain was built.
    #[must_use]
    pub fn misses(&self) -> u32 {
        self.remote_stats
            .iter()
            .map(|s| s.misses.load(std::sync::atomic::Ordering::Relaxed))
            .sum()
    }

    #[must_use]
    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    /// Latency the chain adds right now, in frames: one quantum for each
    /// lagged model that is switched on.
    #[must_use]
    pub fn added_latency(&self) -> u32 {
        self.slots
            .iter()
            .filter(|s| s.standby.is_some() && !s.bypass)
            .count() as u32
            * self.quantum
    }

    #[must_use]
    pub fn block_ids(&self) -> Vec<&str> {
        self.slots.iter().map(|s| s.id.as_str()).collect()
    }
}
