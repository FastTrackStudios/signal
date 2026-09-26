//! What each block of a chain costs: every block built exactly as the rig
//! builds it (`rig::build_block` — NAM, IR, native DSP, hosted plugin), fed
//! a real signal in chain order, each `process_block` timed against the
//! realtime budget of the block size, and its reported latency read.
//!
//! The blocks run serially here even where the live chain runs a time
//! effect in parallel with the dry path: a block costs the same either way,
//! and the signal each one sees stays realistic.

use std::time::Instant;

use crate::rig::{build_block, RigBlock};
use signal_plugin_host::PluginInstance as _;

/// One block's cost and latency.
#[derive(Clone, Debug)]
pub struct BlockCost {
    pub name: String,
    pub kind: String,
    /// Samples of latency the block reports (0 = none).
    pub latency: u32,
    /// Per-block render time, µs.
    pub mean_us: f64,
    pub p99_us: f64,
    pub max_us: f64,
    /// Set when the block would not build.
    pub error: Option<String>,
}

/// Profile `blocks` (the ones that play — pass the chain without bypassed
/// blocks) at `sample_rate`, `frames` per block, over `input` (mono; looped
/// to `seconds`). The first second is warm-up and not counted.
#[must_use]
pub fn profile_chain(
    blocks: &[RigBlock],
    sample_rate: u32,
    frames: usize,
    input: &[f32],
    seconds: f64,
) -> Vec<BlockCost> {
    let mut built: Vec<(RigBlock, Option<Box<dyn signal_plugin_host::PluginInstance>>, Option<String>)> =
        blocks
            .iter()
            .map(|b| match build_block(b, sample_rate) {
                Ok(bb) => {
                    let mut inst = bb.boxed;
                    let err = inst.prepare(f64::from(sample_rate), frames as u32).err().map(|e| format!("{e:?}"));
                    (b.clone(), Some(inst), err)
                }
                Err(e) => (b.clone(), None, Some(e)),
            })
            .collect();
    let total = (seconds * f64::from(sample_rate) / frames as f64) as usize;
    let warm = (f64::from(sample_rate) / frames as f64) as usize;
    let mut times: Vec<Vec<f64>> = vec![Vec::with_capacity(total); built.len()];
    let (mut l, mut r) = (vec![0.0f32; frames], vec![0.0f32; frames]);
    let (mut ol, mut or) = (vec![0.0f32; frames], vec![0.0f32; frames]);
    let events = signal_plugin_host::PluginEvents::default();
    let mut pos = 0usize;
    for k in 0..total {
        for i in 0..frames {
            let s = if input.is_empty() { 0.0 } else { input[(pos + i) % input.len()] };
            l[i] = s;
            r[i] = s;
        }
        pos += frames;
        for (j, (_, inst, err)) in built.iter_mut().enumerate() {
            let Some(inst) = inst.as_mut().filter(|_| err.is_none()) else { continue };
            let t = Instant::now();
            let _ = inst.process_block(&l, &r, &mut ol, &mut or, &events);
            if k >= warm {
                times[j].push(t.elapsed().as_secs_f64() * 1e6);
            }
            std::mem::swap(&mut l, &mut ol);
            std::mem::swap(&mut r, &mut or);
        }
    }
    built
        .iter_mut()
        .zip(times)
        .map(|((b, inst, err), mut t)| {
            t.sort_by(f64::total_cmp);
            let n = t.len().max(1);
            BlockCost {
                name: b.name.clone(),
                kind: format!("{:?}{}", b.block_type, if b.is_nam() { " (NAM)" } else if b.is_cab_ir() { " (IR)" } else { "" }),
                latency: inst.as_mut().map_or(0, |i| i.latency()),
                mean_us: t.iter().sum::<f64>() / n as f64,
                p99_us: t.get((n * 99 / 100).min(n.saturating_sub(1))).copied().unwrap_or(0.0),
                max_us: t.last().copied().unwrap_or(0.0),
                error: err.clone(),
            }
        })
        .collect()
}
