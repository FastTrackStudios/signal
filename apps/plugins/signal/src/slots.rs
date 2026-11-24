//! The blocks' parameters as host parameters: [`SLOTS`] fixed slots
//! (`b001`…), each one block parameter of the running state, in lane,
//! block and parameter order, named by its mix phase and block
//! (`Tone · Comp · Attack`) and printed and parsed in the block's own
//! words (`-18.0 dB`, `4.0:1`).
//!
//! The same state gives the same slots, so a slot's id is stable for a
//! preset; a different state renames them, and a host that caches
//! parameter info re-reads it after loading one.

use std::sync::atomic::{AtomicPtr, AtomicU32, Ordering};
use std::sync::{Arc, PoisonError, RwLock};

use signal_core::graph::Graph;
use signal_core::param::ParamSpec;
use signal_core::state::Phase;

/// How many block parameters an instance shows: a kit track's chain is
/// under 80 (a gate, three compressors' worth, an EQ's six bands).
pub const SLOTS: usize = 256;

/// The parametric EQ's bands a slot shows: six, enough for a Tone EQ.
const EQ_BANDS_SHOWN: usize = 6;

/// One slot: which block parameter, its spec, its name.
#[derive(Clone, Debug)]
pub struct SlotDef {
    pub lane: usize,
    pub block: usize,
    pub param: usize,
    /// Where it lives in the preset, to write a dialled-in value back.
    pub lane_name: String,
    pub block_id: String,
    pub spec: ParamSpec,
    pub name: String,
}

/// One block of the running graph: where it is, its parameters.
#[derive(Clone, Debug)]
pub struct BlockDef {
    pub lane: usize,
    pub block: usize,
    pub id: String,
    pub phase: Option<Phase>,
    pub specs: &'static [ParamSpec],
    /// Keyed by another lane (its `via` is live).
    pub keyed: bool,
}

/// A live value the host reads (`signal/1` `mod_source_value`): a block's
/// own source (a compressor's gain reduction, a gate's opening), or a
/// phase's output level.
#[derive(Clone, Debug)]
pub struct MeterDef {
    pub id: String,
    pub name: String,
    pub unit: &'static str,
    pub lane: usize,
    pub kind: MeterKind,
}

#[derive(Clone, Copy, Debug)]
pub enum MeterKind {
    /// Source `index` of block `block`.
    Source { block: usize, index: usize },
    /// Phase `phase`'s output, dB (K-weighted, over the last piece).
    Phase(Phase),
}

/// The most meters, and blocks, an instance shows: the shared atomics are
/// this many, made once.
pub const MAX_METERS: usize = 512;
pub const MAX_BLOCKS: usize = 256;

/// The running state's slots, and its lanes' phase make-ups.
#[derive(Debug, Default)]
pub struct Table {
    pub slots: Vec<SlotDef>,
    pub blocks: Vec<BlockDef>,
    pub meters: Vec<MeterDef>,
    /// Each lane's name, and per lane and phase (`Phase::ALL` order) its
    /// make-up in dB as f32 bits (NaN: the lane has no such phase). The
    /// audio thread writes them as it calibrates; a save reads them.
    pub lanes: Vec<String>,
    pub trims: Vec<AtomicU32>,
}

fn phase_index(phase: Phase) -> usize {
    Phase::ALL.iter().position(|p| *p == phase).unwrap_or(0)
}

/// The slot id `b001`…, for slot `i`.
#[must_use]
pub fn slot_id(i: usize) -> String {
    format!("b{:03}", i + 1)
}

/// A block's id in `signal.chain/1`: `<phase>.<block>` (`tone.comp`),
/// `source.<block>` for one outside the phases (a sampler).
#[must_use]
pub fn chain_id(phase: Option<Phase>, block: &str) -> String {
    format!("{}.{block}", phase.map_or("source", Phase::slug))
}

/// A unit as `signal.chain/1` writes it.
#[must_use]
pub fn unit_name(unit: signal_core::param::Unit) -> &'static str {
    use signal_core::param::Unit;
    match unit {
        Unit::Db => "dB",
        Unit::Hz => "Hz",
        Unit::Ms => "ms",
        Unit::Percent => "%",
        Unit::Plain(_) | Unit::Choice(_) | Unit::Toggle => "",
    }
}

#[must_use]
pub fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Rescue => "Rescue",
        Phase::Tone => "Tone",
        Phase::Polish => "Polish",
        Phase::Relational => "Relational",
        Phase::Depth => "Depth",
        Phase::Creative => "Creative",
    }
}

/// A block id as a label: `rescue_eq` → `EQ` (under Rescue), `band_pass`
/// → `Band Pass`, `comp` → `Comp`.
#[must_use]
pub fn block_label(id: &str, phase: Option<Phase>) -> String {
    let id = match phase {
        Some(Phase::Rescue) => id.strip_prefix("rescue_").unwrap_or(id),
        _ => id,
    };
    id.split('_')
        .map(|w| match w {
            "eq" => "EQ".to_owned(),
            w => {
                let mut c = w.chars();
                c.next()
                    .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether a block parameter gets a slot: everything but the parametric
/// EQ's bands past the sixth.
fn shown(spec: &ParamSpec) -> bool {
    spec.id
        .strip_prefix("band.")
        .and_then(|rest| rest.split('.').next())
        .and_then(|n| n.parse::<usize>().ok())
        .is_none_or(|n| n <= EQ_BANDS_SHOWN)
}

impl Table {
    /// The slots of `graph`'s blocks. With more than one lane (the
    /// multi-channel mode), each name leads with its lane's last part.
    #[must_use]
    pub fn of(graph: &Graph) -> Self {
        let many = graph.lane_count() > 1;
        let mut slots = Vec::new();
        for lane in 0..graph.lane_count() {
            let lane_name = graph.lane_name(lane).unwrap_or("").to_owned();
            let lane_label = lane_name.rsplit('/').next().unwrap_or("").to_owned();
            for block in 0..graph.block_count(lane) {
                let Some(b) = graph.block(lane, block) else {
                    continue;
                };
                let phase = graph.block_phase(lane, block);
                let label = block_label(&b.id, phase);
                for (param, spec) in b
                    .params()
                    .specs()
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| shown(s))
                {
                    let mut parts: Vec<&str> = Vec::new();
                    if many {
                        parts.push(&lane_label);
                    }
                    if let Some(p) = phase {
                        parts.push(phase_name(p));
                    }
                    parts.push(&label);
                    parts.push(spec.name);
                    slots.push(SlotDef {
                        lane,
                        block,
                        param,
                        lane_name: lane_name.clone(),
                        block_id: b.id.clone(),
                        spec: *spec,
                        name: parts.join(" · "),
                    });
                    if slots.len() == SLOTS {
                        tracing::warn!(
                            slots = SLOTS,
                            "more block parameters than slots: the rest aren't shown"
                        );
                        return Self::with_trims(slots, graph);
                    }
                }
            }
        }
        Self::with_trims(slots, graph)
    }

    fn with_trims(slots: Vec<SlotDef>, graph: &Graph) -> Self {
        let lanes: Vec<String> = (0..graph.lane_count())
            .map(|l| graph.lane_name(l).unwrap_or("").to_owned())
            .collect();
        let trims: Vec<AtomicU32> = (0..lanes.len() * Phase::ALL.len())
            .map(|_| AtomicU32::new(f32::NAN.to_bits()))
            .collect();
        let mut blocks = Vec::new();
        let mut meters = Vec::new();
        for lane in 0..graph.lane_count() {
            let mut phases: Vec<Phase> = Vec::new();
            for block in 0..graph.block_count(lane) {
                let Some(b) = graph.block(lane, block) else {
                    continue;
                };
                let phase = graph.block_phase(lane, block);
                let id = chain_id(phase, &b.id);
                blocks.push(BlockDef {
                    lane,
                    block,
                    id: id.clone(),
                    phase,
                    specs: b.params().specs(),
                    keyed: graph.key_via(lane, block).is_some(),
                });
                for (index, src) in b.mod_sources().iter().enumerate() {
                    meters.push(MeterDef {
                        id: format!("{id}.{}", src.id),
                        name: format!("{} {}", block_label(&b.id, phase), src.name),
                        unit: unit_name(src.unit),
                        lane,
                        kind: MeterKind::Source { block, index },
                    });
                }
                if let Some(p) = phase.filter(|p| !phases.contains(p)) {
                    phases.push(p);
                }
            }
            for p in phases {
                meters.push(MeterDef {
                    id: format!("{}.level", p.slug()),
                    name: format!("{} level", phase_name(p)),
                    unit: "dB",
                    lane,
                    kind: MeterKind::Phase(p),
                });
            }
        }
        meters.truncate(MAX_METERS);
        blocks.truncate(MAX_BLOCKS);
        let table = Self {
            slots,
            blocks,
            meters,
            lanes,
            trims,
        };
        table.store_trims(graph);
        table
    }

    /// Keep `graph`'s phase make-ups. Allocates nothing: the audio thread
    /// calls it as it calibrates.
    pub fn store_trims(&self, graph: &Graph) {
        graph.each_trim(|lane, phase, db| {
            if let Some(t) = self.trims.get(lane * Phase::ALL.len() + phase_index(phase)) {
                t.store(db.to_bits(), Ordering::Release);
            }
        });
    }

    /// Lane `lane`'s make-ups: `(phase, dB)`.
    #[must_use]
    pub fn trims_of(&self, lane: usize) -> Vec<(Phase, f32)> {
        Phase::ALL
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let v = f32::from_bits(
                    self.trims
                        .get(lane * Phase::ALL.len() + i)?
                        .load(Ordering::Acquire),
                );
                (!v.is_nan()).then_some((*p, v))
            })
            .collect()
    }

    /// The normalized value of each slot in `graph` now.
    #[must_use]
    pub fn values(&self, graph: &Graph) -> Vec<f32> {
        self.slots
            .iter()
            .map(|s| {
                graph
                    .block(s.lane, s.block)
                    .and_then(|b| b.params().base(s.param))
                    .map_or(0.0, |v| s.spec.normalize(v))
            })
            .collect()
    }
}

/// The table the main thread reads (names, text), shared.
#[derive(Default)]
pub struct Shown(RwLock<Arc<Table>>);

impl Shown {
    pub fn get(&self) -> Arc<Table> {
        self.0
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn set(&self, table: Arc<Table>) {
        *self.0.write().unwrap_or_else(PoisonError::into_inner) = table;
    }
}

/// A new table for the audio thread, and the old one back for the main
/// thread to drop: no lock, no free on the audio thread.
#[derive(Default)]
pub struct Mailbox {
    pending: AtomicPtr<Arc<Table>>,
    retired: AtomicPtr<Arc<Table>>,
}

impl Mailbox {
    /// Main thread.
    pub fn post(&self, table: Arc<Table>) {
        self.collect();
        let old = self
            .pending
            .swap(Box::into_raw(Box::new(table)), Ordering::AcqRel);
        if !old.is_null() {
            // SAFETY: never taken: ours, by swap.
            drop(unsafe { Box::from_raw(old) });
        }
    }

    /// Main thread: drop what the audio side handed back.
    pub fn collect(&self) {
        let old = self.retired.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if !old.is_null() {
            // SAFETY: the audio side gave it up.
            drop(unsafe { Box::from_raw(old) });
        }
    }

    /// Audio thread: the new table, when there's room to hand the old back.
    pub fn take(&self) -> Option<Box<Arc<Table>>> {
        if !self.retired.load(Ordering::Acquire).is_null() {
            return None;
        }
        let p = self.pending.swap(std::ptr::null_mut(), Ordering::AcqRel);
        // SAFETY: posted by `post`, now ours.
        (!p.is_null()).then(|| unsafe { Box::from_raw(p) })
    }

    /// Audio thread.
    pub fn retire(&self, old: Box<Arc<Table>>) {
        let prev = self.retired.swap(Box::into_raw(old), Ordering::AcqRel);
        debug_assert!(prev.is_null(), "take() only runs with room to retire");
    }
}

impl Drop for Mailbox {
    fn drop(&mut self) {
        for p in [&self.pending, &self.retired] {
            let old = p.swap(std::ptr::null_mut(), Ordering::AcqRel);
            if !old.is_null() {
                // SAFETY: ours at drop.
                drop(unsafe { Box::from_raw(old) });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_read_by_phase_and_block() {
        assert_eq!(block_label("rescue_eq", Some(Phase::Rescue)), "EQ");
        assert_eq!(block_label("band_pass", Some(Phase::Tone)), "Band Pass");
        assert_eq!(block_label("comp", Some(Phase::Tone)), "Comp");
        assert_eq!(slot_id(0), "b001");
    }
}
