//! A block's stored params, as its processor is set to them — the one
//! conversion both the build and a live write go through.
//!
//! A [`RigBlock`] stores its params as strings (`{ name "time", value "350" }`)
//! in the backend's own units. The built-in effects (`fx-blocks`) are set
//! from them at build time by `set_named(name, value)`, which is `set(id,
//! value)` for the param's id — the same `set` a param event delivers to
//! `process_block`. So a live write is the build's write exactly when it
//! carries the same `(id, value)` pairs, in the same order, as plain values:
//!
//! - [`native_writes`] is what each built-in effect's builder applies: every
//!   numeric param, in stored order, each parsed by [`stored_value`] (as
//!   `f32`, widened — the type a knob write arrives in, so a knob's value and
//!   the same value read back from the definition set the block
//!   identically).
//! - [`block_delta`] compares two versions of one block and says whether the
//!   difference can be written to the running block ([`BlockWrite`]) or needs
//!   a new one ([`BlockDelta::Structural`]).
//! - A live write goes to the block as a param event (`process_block` with no
//!   frames, on the control thread): `(id, plain value)`, no normalisation,
//!   no range clamp — none of which the build does either.
//!
//! [`STRUCTURAL`] lists the params that cannot be written live; everything
//! else a built-in effect exposes can.

use std::collections::HashMap;
use std::sync::OnceLock;

use signal_proto::block::BlockType;

use crate::rig::RigBlock;

/// A stored param's value, as the processor is set to it.
///
/// Parsed as `f32` and widened: a knob write arrives as `f32`, and the
/// definition records it with `f32`'s shortest round-tripping text, so a
/// value written live and the same value built from the definition are the
/// same `f64` bit for bit.
#[must_use]
pub fn stored_value(text: &str) -> Option<f64> {
    text.trim().parse::<f32>().ok().map(f64::from)
}

/// Whether `block_type`'s built-in DSP is an `fx-blocks` effect, set by
/// name from its stored params.
fn is_fx(block_type: BlockType) -> bool {
    matches!(
        block_type,
        BlockType::Chorus
            | BlockType::Flanger
            | BlockType::Vibrato
            | BlockType::Pitch
            | BlockType::Trem
            | BlockType::Gate
            | BlockType::Volume
            | BlockType::Boost
            | BlockType::Compressor
            | BlockType::Eq
            | BlockType::Reverb
            | BlockType::Delay
            | BlockType::Phaser
            | BlockType::Rotary
            | BlockType::Drive
    )
}

/// What a built-in effect's builder sets it to: every numeric param the
/// block stores, in stored order, as `(param, plain value)` — the effect
/// ignores a name it does not have. `None` for a block that is not a
/// built-in effect set this way (a NAM, IR, plugin or sample block, or a
/// synth-side native whose params configure it at construction).
///
/// Every param, not a list per effect: a list is a second copy of the
/// effect's params that falls behind it. The EQ's listed only its bands'
/// six fields, so a preset's slopes, dynamics and output gain were dropped
/// at build — and a knob on one of them set what the build ignored.
#[must_use]
pub fn native_writes(block: &RigBlock) -> Option<Vec<(String, f64)>> {
    if !block.is_native() || !is_fx(block.block_type) {
        return None;
    }
    Some(
        block
            .params
            .iter()
            .filter_map(|p| stored_value(&p.value).map(|v| (p.name.clone(), v)))
            .collect(),
    )
}

/// Params that cannot be written to a running block, by block type: a
/// change to one builds the block's chain again (gaplessly). `"*"` is every
/// param of the type.
///
/// - a string, not a number (a reverb's impulse file) — nothing to send;
/// - wiring rather than a knob (a compressor's meter channel) — the effect
///   takes it at build only;
/// - a new engine (a reverb's algorithm, variant or size selector): a new
///   delay network allocated, not work for the renderer's lock.
///
/// Anything else a built-in effect's param list names is written live —
/// [`live_param_ids`] is that list.
pub const STRUCTURAL: &[(BlockType, &str)] = &[
    (BlockType::Reverb, "ir_path"),
    (BlockType::Compressor, "meter"),
    // A new engine: the reverb builds a new algorithm (its delay network,
    // allocated) — not work to do under the renderer's lock.
    (BlockType::Reverb, "algorithm"),
    (BlockType::Reverb, "algo_b"),
    (BlockType::Reverb, "r2_algorithm"),
    (BlockType::Reverb, "variant"),
    (BlockType::Reverb, "r2_variant"),
    (BlockType::Reverb, "size_sel"),
    (BlockType::Reverb, "r2_size_sel"),
    // Where it lands depends on the params set around it in the build —
    // the second engine's two names (`*_b`, `r2_*`) for one value, the
    // later overruling the earlier; decay, size and voice through the
    // voice pairing and size selector — so no write of it alone reproduces
    // the build (the reverb's `set` would need an order-free form; that is
    // the processor's to give).
    (BlockType::Reverb, "decay"),
    (BlockType::Reverb, "voice"),
    (BlockType::Reverb, "decay_b"),
    (BlockType::Reverb, "mix_b"),
    (BlockType::Reverb, "pan_b"),
    (BlockType::Reverb, "r2_decay"),
    (BlockType::Reverb, "r2_size"),
    (BlockType::Reverb, "r2_voice"),
    // Reconfigure an engine (~50–75 µs): too long a hold of the renderer's
    // lock, which it spins on for tens of microseconds at most.
    (BlockType::Pitch, "live"),
    (BlockType::Delay, "style_b"),
];

/// Params whose effect depends on where they fall among the block's other
/// params — written alone to a running block they would not land where the
/// build does, so a change to one re-sends the block's whole list in the
/// build's order:
///
/// - aliases the build lets a later param overrule: the reverb's second
///   engine is both `*_b` and `r2_*`; a delay's `tap_div` is both taps'
///   `tap_div_l` / `tap_div_r`;
/// - settings the reverb derives from others set before or after them
///   (`decay` and `size` through the size selector and voice).
///
/// Found by the sweep in `tests/live_params.rs`, which writes every param of
/// every built-in effect alone over a non-default block and checks it
/// against a build.
pub const ORDER_SENSITIVE: &[(BlockType, &str)] = &[
    (BlockType::Delay, "tap_div"),
    (BlockType::Delay, "tap_div_l"),
    (BlockType::Delay, "tap_div_r"),
];

fn order_sensitive(block_type: BlockType, param: &str) -> bool {
    ORDER_SENSITIVE
        .iter()
        .any(|(t, p)| *t == block_type && p.eq_ignore_ascii_case(param))
}

fn structural(block_type: BlockType, param: &str) -> bool {
    STRUCTURAL
        .iter()
        .any(|(t, p)| *t == block_type && (*p == "*" || p.eq_ignore_ascii_case(param)))
}

/// A built-in effect type's live params: name (lowercase) → id, from the
/// effect's own param list. Built once per type, off any lock.
#[must_use]
pub fn live_param_ids(block_type: BlockType) -> Option<&'static HashMap<String, u32>> {
    static CACHE: OnceLock<std::sync::Mutex<HashMap<BlockType, &'static HashMap<String, u32>>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut cache = cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(ids) = cache.get(&block_type) {
        return Some(ids);
    }
    if !is_fx(block_type) {
        return None;
    }
    let mut inst = crate::native::build_native(&RigBlock::of_type(block_type), 48_000)?;
    let mut ids: HashMap<String, u32> = inst
        .params()
        .into_iter()
        .map(|p| (p.name.to_ascii_lowercase(), p.id))
        .collect();
    if block_type == BlockType::Reverb {
        // The second engine: `r2_<param>` is `<param>`'s id + 100 for every
        // param (`NativeReverb::set_named`), mirrored or not in its list.
        let base: Vec<(String, u32)> = ids
            .iter()
            .filter(|(n, id)| !n.starts_with("r2_") && **id < 100)
            .map(|(n, id)| (format!("r2_{n}"), id + 100))
            .collect();
        for (n, id) in base {
            ids.entry(n).or_insert(id);
        }
    }
    let ids: &'static HashMap<String, u32> = Box::leak(Box::new(ids));
    cache.insert(block_type, ids);
    Some(ids)
}

/// Whether `param` of a `block_type` block can be written live and land
/// where a build from the same stored value does — what a reload may
/// write instead of building.
#[must_use]
pub fn is_live(block_type: BlockType, param: &str) -> bool {
    !structural(block_type, param) && is_known(block_type, param)
}

/// Whether a `block_type` built-in effect has a param `param` to write — a
/// knob's write goes to the block whatever [`STRUCTURAL`] says (a knob is
/// live by nature; the table is about what a reload may skip building).
#[must_use]
pub fn is_known(block_type: BlockType, param: &str) -> bool {
    live_param_ids(block_type).is_some_and(|ids| ids.contains_key(&param.to_ascii_lowercase()))
}

/// What a running block was last set to, in the ids it takes params by: its
/// build's values, then every write since. Kept by the rig beside each
/// resident block (the one place writes reach blocks), and compared with what
/// the block is due to play by `ProfileRig::reconcile`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockState {
    /// A built-in effect's params: id → plain value.
    pub params: HashMap<u32, f64>,
    /// A NAM block's trims (dB).
    pub nam_in: Option<f32>,
    pub nam_out: Option<f32>,
}

impl BlockState {
    /// What building `block` sets it to — the build's own conversion.
    #[must_use]
    pub fn built(block: &RigBlock) -> Self {
        let mut st = Self::default();
        if let Some(writes) = native_writes(block) {
            if let Some(w) = ResolvedWrite::resolve(block.block_type, &BlockWrite::Params(writes)) {
                st.note(&w);
            }
        } else if block.is_nam() {
            st.nam_in = Some(block.input_trim_db);
            st.nam_out = Some(block.output_trim_db);
        }
        st
    }

    /// Record `w` as written.
    pub fn note(&mut self, w: &ResolvedWrite) {
        match w {
            ResolvedWrite::Events(events) => {
                for (id, v) in events {
                    self.params.insert(*id, *v);
                }
            }
            ResolvedWrite::Nam { input_db, output_db } => {
                if input_db.is_some() {
                    self.nam_in = *input_db;
                }
                if output_db.is_some() {
                    self.nam_out = *output_db;
                }
            }
        }
    }

    /// The write that takes a block from `self` to `due` (`None`: nothing
    /// differs).
    #[must_use]
    pub fn diff_to(&self, due: &Self) -> Vec<ResolvedWrite> {
        let mut out = Vec::new();
        let mut events: Vec<(u32, f64)> = due
            .params
            .iter()
            .filter(|(id, v)| self.params.get(id).is_none_or(|have| have.to_bits() != v.to_bits()))
            .map(|(id, v)| (*id, *v))
            .collect();
        events.sort_by_key(|(id, _)| *id);
        if !events.is_empty() {
            out.push(ResolvedWrite::Events(events));
        }
        let differs = |a: Option<f32>, b: Option<f32>| b.is_some() && a.map(f32::to_bits) != b.map(f32::to_bits);
        let (i, o) = (differs(self.nam_in, due.nam_in), differs(self.nam_out, due.nam_out));
        if i || o {
            out.push(ResolvedWrite::Nam {
                input_db: due.nam_in.filter(|_| i),
                output_db: due.nam_out.filter(|_| o),
            });
        }
        out
    }
}

/// A difference to write to a running block.
#[derive(Clone, Debug, PartialEq)]
pub enum BlockWrite {
    /// Param events, `(param, plain value)`, in order.
    Params(Vec<(String, f64)>),
    /// A NAM block's trims (dB) — what the build sets from the block's
    /// `input_trim_db` / `output_trim_db` (a drive knob's compensation
    /// lands here). `None` leaves one as it is.
    Nam { input_db: Option<f32>, output_db: Option<f32> },
}

/// How a block differs from the version of it that is running.
#[derive(Clone, Debug, PartialEq)]
pub enum BlockDelta {
    /// Builds the same.
    Same,
    /// Differs only in what can be written to it as it runs.
    Live(BlockWrite),
    /// Needs building again.
    Structural,
}

/// Everything about a block except what can change while it runs: its
/// params, trims, bypass and UUID. Equal shapes build the same processor
/// in the same place in the chain.
fn same_shape(a: &RigBlock, b: &RigBlock) -> bool {
    a.block_type == b.block_type
        && a.nam == b.nam
        && a.ir == b.ir
        && a.plugin == b.plugin
        && a.state_b64 == b.state_b64
        && a.sample == b.sample
        && a.samples_root == b.samples_root
        && a.sample_section == b.sample_section
        && a.sample_mic == b.sample_mic
        && a.name == b.name
        && a.module == b.module
}

fn params_text(b: &RigBlock) -> Vec<(&str, &str)> {
    b.params.iter().map(|p| (p.name.as_str(), p.value.as_str())).collect()
}

/// How `new` differs from `old` (the block running), for the build.
///
/// A block's bypass is never a difference here: it rides the block's gate,
/// which activation sets from the patch.
#[must_use]
pub fn block_delta(old: &RigBlock, new: &RigBlock) -> BlockDelta {
    if !same_shape(old, new) {
        return BlockDelta::Structural;
    }
    if new.is_nam() {
        // A NAM block's params are not read by its build (the drive knob
        // reaches it as the trims the definition computes from it).
        let input = (old.input_trim_db != new.input_trim_db).then_some(new.input_trim_db);
        let output = (old.output_trim_db != new.output_trim_db).then_some(new.output_trim_db);
        return if input.is_none() && output.is_none() {
            BlockDelta::Same
        } else {
            BlockDelta::Live(BlockWrite::Nam { input_db: input, output_db: output })
        };
    }
    if new.is_cab_ir() || new.is_plugin() {
        // Params are not read by these builds; a plugin's state is shape.
        return BlockDelta::Same;
    }
    match (native_writes(old), native_writes(new)) {
        (Some(a), Some(b)) => {
            // Structural params are compared as stored (a string param such
            // as an IR path never reaches `native_writes`).
            let text_changed = |name: &str| old.param_str(name) != new.param_str(name);
            if STRUCTURAL
                .iter()
                .any(|(t, p)| *t == new.block_type && text_changed(p))
            {
                return BlockDelta::Structural;
            }
            // A name the effect does not know is a no-op in its build (its
            // `set_named` ignores it) and so is nothing to write; structural
            // params were compared above.
            let Some(ids) = live_param_ids(new.block_type) else {
                return BlockDelta::Structural;
            };
            let known = |w: Vec<(String, f64)>| -> Vec<(String, f64)> {
                w.into_iter()
                    .filter(|(n, _)| {
                        !structural(new.block_type, n) && ids.contains_key(&n.to_ascii_lowercase())
                    })
                    .collect()
            };
            let (a, b) = (known(a), known(b));
            if a == b {
                return BlockDelta::Same;
            }
            // A param set and then no longer set keeps its old value live,
            // but the default in a build: rebuild.
            let names = |w: &[(String, f64)]| w.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
            if names(&a) != names(&b) {
                return BlockDelta::Structural;
            }
            let changed: Vec<(String, f64)> = a
                .iter()
                .zip(&b)
                .filter(|(x, y)| x.1 != y.1)
                .map(|(_, y)| y.clone())
                .collect();
            // A param whose effect depends on where it falls among the
            // others (an alias the build lets a later param overrule, a
            // setting derived from the mode set before or after it): the
            // whole list, in the build's order, so the running block ends
            // where the build does. Otherwise only what changed —
            // re-sending an unchanged param is not free (a delay re-derives
            // its times) and a running block should hear only the edit.
            if changed.iter().any(|(n, _)| order_sensitive(new.block_type, n)) {
                BlockDelta::Live(BlockWrite::Params(b))
            } else {
                BlockDelta::Live(BlockWrite::Params(changed))
            }
        }
        _ => {
            if params_text(old) == params_text(new) {
                BlockDelta::Same
            } else {
                BlockDelta::Structural
            }
        }
    }
}

/// A [`BlockWrite`] resolved for its block's type: param names to the
/// effect's ids. Resolve before taking any lock the renderer takes, so the
/// hold is the writes alone.
#[derive(Clone, Debug)]
pub enum ResolvedWrite {
    Events(Vec<(u32, f64)>),
    Nam { input_db: Option<f32>, output_db: Option<f32> },
}

impl ResolvedWrite {
    /// `None` when nothing in `w` reaches a `block_type` block.
    #[must_use]
    pub fn resolve(block_type: BlockType, w: &BlockWrite) -> Option<Self> {
        match w {
            BlockWrite::Params(params) => {
                let ids = live_param_ids(block_type)?;
                let events: Vec<(u32, f64)> = params
                    .iter()
                    .filter_map(|(n, v)| ids.get(&n.to_ascii_lowercase()).map(|&id| (id, *v)))
                    .collect();
                (!events.is_empty()).then_some(Self::Events(events))
            }
            BlockWrite::Nam { input_db, output_db } => Some(Self::Nam {
                input_db: *input_db,
                output_db: *output_db,
            }),
        }
    }

    /// Write to the block (control thread). A built-in effect takes its
    /// params as events on a block of no frames — its own `set` for each, and
    /// whatever it derives after (a delay's tempo-synced times), exactly as a
    /// param write between two blocks; the gate, time-stage and blend
    /// wrappers pass them through as they do any event. A NAM block takes its
    /// trims.
    pub fn apply(&self, inst: &mut dyn signal_plugin_host::PluginInstance) {
        match self {
            Self::Events(events) => {
                let ev = signal_plugin_host::PluginEvents {
                    params: events,
                    ..signal_plugin_host::PluginEvents::default()
                };
                let _ = inst.process_block(&[], &[], &mut [], &mut [], &ev);
            }
            #[cfg(not(target_arch = "wasm32"))]
            Self::Nam { input_db, output_db } => {
                if let Some(nam) = inst
                    .as_any_mut()
                    .and_then(|a| a.downcast_mut::<crate::nam::NamProcessor>())
                {
                    if let Some(v) = input_db {
                        nam.input_gain_db = *v;
                    }
                    if let Some(v) = output_db {
                        nam.output_gain_db = *v;
                    }
                }
            }
            #[cfg(target_arch = "wasm32")]
            Self::Nam { .. } => {}
        }
    }
}
