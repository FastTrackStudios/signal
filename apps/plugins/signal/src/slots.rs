//! The blocks' parameters as host parameters: [`SLOTS`] fixed slots
//! (`b001`…), each one block parameter of the running state, in lane,
//! block and parameter order, named by its mix phase and block
//! (`Tone · Comp · Attack`) and printed and parsed in the block's own
//! words (`-18.0 dB`, `4.0:1`).
//!
//! The same state gives the same slots, so a slot's id is stable for a
//! preset; a different state renames them, and a host that caches
//! parameter info re-reads it after loading one.

use std::sync::atomic::{AtomicPtr, Ordering};
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

/// The running state's slots.
#[derive(Clone, Debug, Default)]
pub struct Table {
    pub slots: Vec<SlotDef>,
}

/// The slot id `b001`…, for slot `i`.
#[must_use]
pub fn slot_id(i: usize) -> String {
    format!("b{:03}", i + 1)
}

fn phase_name(phase: Phase) -> &'static str {
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
fn block_label(id: &str, phase: Option<Phase>) -> String {
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
                        return Self { slots };
                    }
                }
            }
        }
        Self { slots }
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
