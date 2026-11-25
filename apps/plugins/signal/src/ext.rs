//! FastTrackStudio's CLAP extensions, Signal's side.
//!
//! - **`com.fasttrackstudio.signal/1`** (the host drives Signal): the chain
//!   as `signal.chain/1` (daw-clap-ext `docs/signal-chain.md`), its
//!   revision, the structural actions, the session's every port, and the
//!   live meters through `mod_source_*`.
//! - **`com.fasttrackstudio.host-events/1`** (where the host's events
//!   arrive): the transport, and each piece's hits.
//! - **`com.fasttrackstudio.host-actions/1`** (Signal calls the host), when
//!   the host offers it: files by reference, the subscriptions, and a
//!   `SIGNAL_CHAIN_CHANGED` after every change. Without it Signal reads
//!   files from disk and behaves exactly as before.

use std::ffi::{CStr, c_char};
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use daw_clap_ext::plugin::{HostActionsClient, SignalPlugin, SignalPluginLookup};
use daw_clap_ext::{ExtError, ModSourceInfo};
use nice_plug::prelude::Param as _;
use nice_plug::wrapper::clap::{clap_extension_state, clap_host, clap_plugin};
use serde_json::{Value, json};
use signal_core::block::BlockKind;
use signal_core::graph::{self, KeyVia};
use signal_core::host::{AssetRef, HostError, SignalHost};
use signal_core::param::{Unit, ValueText};
use signal_core::session::{PortName, SessionHit, SessionId, SessionLink};
use signal_core::state::{BlockSpec, Instance, LaneSpec, Phase, PhaseTrim, SignalState};

use crate::plugin::{FtsSignal, Shared};
use crate::slots::{self, MAX_BLOCKS, MAX_METERS, MeterKind, Table};

/// What the extensions keep, beside the plugin's own shared state.
pub struct Ext {
    /// Increases on every change to what `chain` returns.
    pub revision: AtomicU64,
    /// The instance's own `clap_plugin` and its host (`clap_init`).
    pub plugin: AtomicPtr<clap_plugin>,
    pub host: AtomicPtr<clap_host>,
    /// The host's `host-actions/1`, when it offers it. Main thread.
    pub actions: Mutex<Option<HostActionsClient>>,
    /// Subscriptions to the host's topics, by topic.
    pub subscriptions: Mutex<Vec<(String, u64)>>,
    /// The last build's blocks: `(lane, block, backend, status)`.
    pub report: Mutex<Vec<(String, String, String, &'static str)>>,
    /// Live values, by the running table's meter index (f32 bits).
    pub meters: Box<[AtomicU32]>,
    /// Per table block: what keys it (0 none, 1 hits, 2 midi, 3 self,
    /// 4 placed).
    pub via: Box<[AtomicU8]>,
    /// The host's transport, from its `transport` events: where it plays.
    pub transport_frame: AtomicU64,
    pub transport_playing: AtomicBool,
    /// Whether the host has `SIGNAL_CHAIN_CHANGED` (asked once): a host
    /// that polls `revision()` has none to run.
    pub chain_changed: std::sync::OnceLock<bool>,
    /// Each piece's hits as the host placed them (its `hits:<piece>`
    /// events), kept to hand every new graph.
    pub planned: Mutex<Vec<(u16, Vec<(u64, f32)>)>>,
    /// Offline blocks: their running analyses and captures, and the host's
    /// `host-automation/1`.
    pub offline: Mutex<crate::offline::Offline>,
}

impl Default for Ext {
    fn default() -> Self {
        Self {
            revision: AtomicU64::new(1),
            plugin: AtomicPtr::new(std::ptr::null_mut()),
            host: AtomicPtr::new(std::ptr::null_mut()),
            actions: Mutex::new(None),
            subscriptions: Mutex::new(Vec::new()),
            report: Mutex::new(Vec::new()),
            meters: (0..MAX_METERS).map(|_| AtomicU32::new(0)).collect(),
            via: (0..MAX_BLOCKS).map(|_| AtomicU8::new(0)).collect(),
            transport_frame: AtomicU64::new(0),
            transport_playing: AtomicBool::new(false),
            chain_changed: std::sync::OnceLock::new(),
            planned: Mutex::new(Vec::new()),
            offline: Mutex::new(crate::offline::Offline::default()),
        }
    }
}

// ── Every instance in the process, for SIGNAL_SESSION ──────────────────

static INSTANCES: Mutex<Vec<Weak<Shared>>> = Mutex::new(Vec::new());

pub fn register(shared: &Arc<Shared>) {
    let mut all = INSTANCES.lock().unwrap_or_else(PoisonError::into_inner);
    all.retain(|w| w.strong_count() > 0);
    all.push(Arc::downgrade(shared));
}

/// Every instance in `shared`'s session (itself alone when it's in none).
pub fn session_of_instance(shared: &Arc<Shared>) -> Vec<Arc<Shared>> {
    let state = shared.state();
    match session_of(&state).map(str::to_owned) {
        None => vec![shared.clone()],
        Some(s) => instances()
            .into_iter()
            .filter(|o| session_of(&o.state()) == Some(s.as_str()))
            .collect(),
    }
}

fn instances() -> Vec<Arc<Shared>> {
    INSTANCES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter_map(Weak::upgrade)
        .collect()
}

// ── The host's files ────────────────────────────────────────────────────

/// Files through the host's `host-actions/1` (`load_file`), the disk when
/// the host doesn't resolve one. Main thread only, as the extension is.
pub struct HostFiles(pub HostActionsClient);

impl SignalHost for HostFiles {
    fn load_file(&self, asset: &AssetRef) -> Result<Vec<u8>, HostError> {
        match self.0.load_file(&asset.path, asset.blake3.as_deref()) {
            Ok(bytes) => Ok(bytes),
            Err(_) => crate::plugin::DiskHost.load_file(asset),
        }
    }
}

// ── After a build ───────────────────────────────────────────────────────

/// What follows a build or a structural change: the revision, the host's
/// rescan of the parameters' info and text, and (on the main thread) the
/// host told, and subscribed to the pieces this instance listens to.
pub fn after_build(shared: &Shared, report: &graph::BuildReport, on_main: bool) {
    *shared
        .ext
        .report
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = report
        .blocks
        .iter()
        .map(|b| {
            let status = match &b.status {
                signal_core::block::BlockStatus::Active => "ok",
                signal_core::block::BlockStatus::Fallback { .. } => "fallback",
                signal_core::block::BlockStatus::Bypassed { .. } => "unavailable",
            };
            (b.lane.clone(), b.block.clone(), b.backend.clone(), status)
        })
        .collect();
    let revision = shared.ext.revision.fetch_add(1, Ordering::AcqRel) + 1;
    let plugin = shared.ext.plugin.load(Ordering::Acquire);
    if !plugin.is_null() {
        // SAFETY: the instance's own pointer, alive as long as it is.
        unsafe { nice_plug::wrapper::clap::clap_request_rescan::<FtsSignal>(plugin, true, false) };
    }
    if !on_main {
        return;
    }
    let client = *shared
        .ext
        .actions
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(client) = client {
        let state = shared.state();
        let port = port_of(&state);
        // Only where the host has the action (Studio polls revision()).
        let has = *shared.ext.chain_changed.get_or_init(|| {
            client
                .list()
                .is_ok_and(|list| list.contains("\"SIGNAL_CHAIN_CHANGED\""))
        });
        if has {
            let _ = client.run(
                "SIGNAL_CHAIN_CHANGED",
                &json!({ "port": port, "revision": revision }).to_string(),
            );
        }
        subscribe(shared, &client, &state);
    }
}

/// Drop this instance's subscriptions (each given back to the host), so
/// the next [`after_build`] makes them again. Activation calls it: a host
/// may forget a subscription made while it loaded the instance, before
/// the instance was one of its own (Studio's FX_RELOAD loads and restores
/// a new instance, then puts it on the chain, and its pump drops the
/// subscriptions of a plugin it does not run) — and a subscription the
/// host forgot is a piece's placed hits never sent: its gate falls back
/// to its own input. Main thread.
pub fn renew_subscriptions(shared: &Shared) {
    let client = *shared
        .ext
        .actions
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let held: Vec<(String, u64)> = core::mem::take(
        &mut *shared
            .ext
            .subscriptions
            .lock()
            .unwrap_or_else(PoisonError::into_inner),
    );
    if let Some(client) = client {
        for (_, id) in held {
            client.unsubscribe(id);
        }
    }
}

/// Subscribe to the transport, and to the hits of each piece a block here
/// is keyed to.
fn subscribe(shared: &Shared, client: &HostActionsClient, state: &SignalState) {
    let mut subs = shared
        .ext
        .subscriptions
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let mut topics = vec!["transport".to_owned()];
    // A user's edit of this FX's lanes, when the host writes them for us.
    if shared
        .ext
        .offline
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .automation
        .is_some()
    {
        topics.push(daw_clap_ext::sys::FTS_AUTOMATION_TOPIC.to_owned());
    }
    for lane in graph::lanes_for(&state.preset, port_of(state)) {
        for key in lane.blocks.iter().filter_map(|b| b.key.as_deref()) {
            topics.push(format!("hits:{}", piece_of(key)));
        }
    }
    topics.sort();
    topics.dedup();
    for topic in topics {
        if !subs.iter().any(|(t, _)| *t == topic) {
            if let Some(id) = client.subscribe(&topic) {
                subs.push((topic, id));
            }
        }
    }
}

/// A trigger lane's piece as the host names it: `Drums/Kick/Sum/Trig` →
/// `Drums/Kick`, `Drums/Toms/Tom 1/T1 Trig` → `Drums/Toms/Tom 1`.
#[must_use]
pub fn piece_of(trigger: &str) -> String {
    trigger
        .strip_suffix("/Sum/Trig")
        .map(str::to_owned)
        .or_else(|| {
            trigger
                .rsplit_once('/')
                .filter(|(_, last)| last.ends_with("Trig"))
                .map(|(p, _)| p.to_owned())
        })
        .unwrap_or_else(|| trigger.to_owned())
}

fn port_of(state: &SignalState) -> Option<&str> {
    match &state.instance {
        Instance::MultiChannel => None,
        Instance::Port(link) => Some(link.port.0.as_str()),
    }
}

fn session_of(state: &SignalState) -> Option<&str> {
    match &state.instance {
        Instance::MultiChannel => None,
        Instance::Port(link) => Some(link.session.0.as_str()),
    }
}

// ── The audio side's meters ─────────────────────────────────────────────

/// Keep the running graph's meters and keys for the host. Audio thread:
/// atomics only.
pub fn store_meters(ext: &Ext, table: &Table, graph: &graph::Graph) {
    for (i, m) in table.meters.iter().enumerate() {
        let v = match m.kind {
            MeterKind::Source { block, index } => graph
                .block(m.lane, block)
                .and_then(|b| b.mod_source(index))
                .unwrap_or(0.0),
            MeterKind::Phase(_) => f32::NAN,
        };
        if let (Some(slot), false) = (ext.meters.get(i), v.is_nan()) {
            slot.store(v.to_bits(), Ordering::Relaxed);
        }
    }
    graph.each_phase_level(|lane, phase, db| {
        for (i, m) in table.meters.iter().enumerate() {
            if m.lane == lane && matches!(m.kind, MeterKind::Phase(p) if p == phase) {
                if let Some(slot) = ext.meters.get(i) {
                    slot.store(db.to_bits(), Ordering::Relaxed);
                }
            }
        }
    });
    for (i, b) in table.blocks.iter().enumerate().filter(|(_, b)| b.keyed) {
        let code = match graph.key_via(b.lane, b.block) {
            None => 0,
            Some(KeyVia::Hits) => 1,
            Some(KeyVia::Midi) => 2,
            Some(KeyVia::Own) => 3,
            Some(KeyVia::Placed) => 4,
        };
        if let Some(slot) = ext.via.get(i) {
            slot.store(code, Ordering::Relaxed);
        }
    }
}

// ── signal.chain/1 ──────────────────────────────────────────────────────

/// `signal.chain/1`'s kind for a block.
fn kind_name(kind: &BlockKind) -> String {
    match kind {
        BlockKind::Signal(name) => match name.as_str() {
            "drive" => "saturate".into(),
            "drum" | "sample" | "sampler" => "sampler".into(),
            other => other.into(),
        },
        BlockKind::Plugin(_) => "clap".into(),
    }
}

/// The block kind a `kind` in an action names.
fn kind_from(name: &str) -> BlockKind {
    BlockKind::Signal(
        match name {
            "saturate" => "drive",
            "sampler" => "drum",
            other => other,
        }
        .into(),
    )
}

fn phase_from(id: &str) -> Option<Phase> {
    Phase::ALL.into_iter().find(|p| p.slug() == id)
}

/// How a meter is drawn (`signal.chain/1` additions): its kind and range.
fn meter_shape(m: &slots::MeterDef) -> (&'static str, f32, f32) {
    match m.kind {
        MeterKind::Phase(_) => ("level", -60.0, 0.0),
        MeterKind::Source { .. } if m.id.ends_with("gain_reduction") => ("reduction", 0.0, 24.0),
        // A gate's opening, 0 shut, 1 open.
        MeterKind::Source { .. } if m.unit == "%" => ("state", 0.0, 1.0),
        // A dynamic band's applied gain, cut or boost (dyn_eq).
        MeterKind::Source { .. } if m.id.ends_with("_gain") => ("gain", -24.0, 24.0),
        MeterKind::Source { .. } => ("level", -60.0, 0.0),
    }
}

/// The algorithm a block runs, when its family has algorithms: the one it
/// names, else the family's default.
fn algorithm_of(spec: &BlockSpec) -> Option<String> {
    let BlockKind::Signal(family) = &spec.kind else {
        return None;
    };
    if plugin_processors::algorithms::family(family).is_empty() {
        return None;
    }
    spec.algorithm
        .clone()
        .or_else(|| plugin_processors::algorithms::find(family, None).map(|a| a.id.to_owned()))
}

/// A parameter's CLAP id (`clap_param_info.id`), as `signal.chain/1` writes
/// it: the decimal `uint32` the host's params extension reports, for the
/// parameter nice-plug knows as `id`.
fn clap_id(id: &str) -> String {
    nice_plug::wrapper::clap::param_clap_id(id).to_string()
}

fn param_json(spec: &signal_core::param::ParamSpec, value: f32, clap_id: Option<String>) -> Value {
    let choices: Vec<&str> = match spec.unit {
        Unit::Choice(names) => names.to_vec(),
        Unit::Toggle => vec!["Off", "On"],
        _ => Vec::new(),
    };
    json!({
        "id": spec.id,
        "clap_id": clap_id,
        "name": spec.name,
        "value": value,
        "normalized": spec.normalize(value),
        "text": ValueText::of(spec, value).as_str(),
        "unit": slots::unit_name(spec.unit),
        "min": spec.min,
        "max": spec.max,
        "default": spec.default,
        "choices": choices,
    })
}

/// One lane's phases, as `signal.chain/1` has them.
fn lane_phases(shared: &Shared, table: &Table, l: usize, spec: &LaneSpec) -> Vec<Value> {
    let report = shared
        .ext
        .report
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let mut phases: Vec<(String, Vec<Value>)> = Vec::new();
    for (bi, b) in table.blocks.iter().enumerate().filter(|(_, b)| b.lane == l) {
        let Some(bspec) = spec.blocks.get(b.block) else {
            continue;
        };
        let params: Vec<Value> = b
            .specs
            .iter()
            .enumerate()
            .map(|(pi, ps)| {
                let slot = table
                    .slots
                    .iter()
                    .position(|s| s.lane == l && s.block == b.block && s.param == pi);
                let value = slot
                    .and_then(|i| shared.params.slots.get(i))
                    .map(|p| ps.denormalize(p.unmodulated_normalized_value()))
                    .or_else(|| bspec.params.get(ps.id).copied())
                    .unwrap_or(ps.default);
                param_json(ps, value, slot.map(|i| clap_id(&slots::slot_id(i))))
            })
            .collect();
        let meters: Vec<Value> = table
            .meters
            .iter()
            .enumerate()
            .filter(|(_, m)| m.lane == l && matches!(m.kind, MeterKind::Source { block, .. } if block == b.block))
            .map(|(i, m)| {
                let (kind, min, max) = meter_shape(m);
                json!({ "id": m.id.rsplit('.').next().unwrap_or(&m.id), "name": m.name, "unit": m.unit, "index": i, "kind": kind, "min": min, "max": max })
            })
            .collect();
        let (backend, status) = report
            .iter()
            .find(|(lane, block, ..)| *lane == spec.name && *block == bspec.id)
            .map_or(("linked".to_owned(), "ok"), |(_, _, be, st)| {
                (be.clone(), *st)
            });
        let via = match shared
            .ext
            .via
            .get(bi)
            .map_or(0, |v| v.load(Ordering::Relaxed))
        {
            2 => "midi",
            3 => "self",
            4 => "placed",
            _ => "hits",
        };
        let ids = |param: &str| {
            b.specs.iter().position(|ps| ps.id == param).and_then(|pi| {
                table
                    .slots
                    .iter()
                    .position(|s| s.lane == l && s.block == b.block && s.param == pi)
                    .map(|i| clap_id(&slots::slot_id(i)))
            })
        };
        let (mode, analysis) = crate::offline::chain_fields(shared, &spec.name, bspec, &ids);
        let block = json!({
            "id": b.id,
            "kind": kind_name(&bspec.kind),
            "mode": mode,
            "analysis": analysis,
            "name": slots::block_label(&bspec.id, b.phase),
            "bypass": bspec.bypass,
            "backend": backend,
            "status": status,
            "algorithm": algorithm_of(bspec),
            // What it was keyed to, what keys it now, and why, when it fell back.
            "key": bspec.key.as_ref().map(|k| json!({
                "from": k,
                "via": via,
                "fallback": (via == "self").then_some("no hits for 10 s"),
            })),
            "params": params,
            "meters": meters,
        });
        let phase = b.phase.map_or("source", Phase::slug).to_owned();
        match phases.iter_mut().find(|(p, _)| *p == phase) {
            Some((_, blocks)) => blocks.push(block),
            None => phases.push((phase, vec![block])),
        }
    }
    phases
        .into_iter()
        .map(|(id, blocks)| {
            let phase = phase_from(&id);
            let meters: Vec<Value> = table
                .meters
                .iter()
                .enumerate()
                .filter(|(_, m)| m.lane == l && phase.is_some_and(|p| matches!(m.kind, MeterKind::Phase(q) if q == p)))
                .map(|(i, m)| json!({ "id": "level", "name": m.name, "unit": m.unit, "index": i, "kind": "level", "min": -60.0, "max": 0.0 }))
                .collect();
            json!({
                "id": id,
                "name": phase.map_or("Source", slots::phase_name),
                "bypass": phase.is_some_and(|p| spec.bypass_phases.contains(&p)),
                "makeup_db": phase.and_then(|p| spec.trims.iter().find(|t| t.phase == p)).map_or(0.0, |t| t.gain_db),
                "blocks": blocks,
                "meters": meters,
            })
        })
        .collect()
}

/// This instance's chain, `signal.chain/1`.
pub fn chain(shared: &Shared) -> Value {
    let state = shared.state();
    let table = shared.params.shown.get();
    let port = port_of(&state);
    let lanes = graph::lanes_for(&state.preset, port);
    let per_track = port.is_some();
    let block_of = |lane: &LaneSpec, piece: &str| -> Option<String> {
        let _ = piece;
        lane.blocks
            .iter()
            .find(|b| b.kind == BlockKind::Signal("drum".into()))
            .map(|b| slots::chain_id(b.phase, &b.id))
    };
    let midi_plays: Vec<Value> = lanes
        .iter()
        .flat_map(|lane| {
            lane.midi_hits.iter().map(move |m| {
                json!({ "note": m.note, "piece": piece_of(&m.piece), "block": block_of(lane, &m.piece) })
            })
        })
        .collect();
    let any_midi = lanes.iter().any(|l| !l.midi_hits.is_empty());
    let macros: Vec<Value> = state
        .preset
        .macros
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let targets: Vec<String> = m
                .targets
                .iter()
                .map(|t| {
                    let phase = state
                        .preset
                        .lanes
                        .iter()
                        .find(|l| l.name == t.lane)
                        .and_then(|l| l.blocks.iter().find(|b| b.id == t.block))
                        .and_then(|b| b.phase);
                    format!("{}.{}", slots::chain_id(phase, &t.block), t.param)
                })
                .collect();
            json!({ "id": format!("macro_{}", i + 1), "name": m.name, "clap_id": clap_id(&format!("macro_{}", i + 1)), "targets": targets })
        })
        .collect();
    let mut out = json!({
        "schema": "signal.chain/1",
        "revision": shared.ext.revision.load(Ordering::Acquire),
        "values_revision": shared.params.values_revision.load(Ordering::Acquire),
        "session": session_of(&state),
        "port": port,
        "mode": if per_track { "per-track" } else { "kit" },
        "midi": if any_midi { json!({ "plays": midi_plays, "publishes_hits": true }) } else { Value::Null },
        "phases": [],
        "macros": macros,
    });
    if per_track || lanes.len() == 1 {
        if let Some(spec) = lanes.first() {
            out["phases"] = Value::Array(lane_phases(shared, &table, 0, spec));
        }
    } else {
        // The kit mode: one instance, every lane, each its own strip.
        out["lanes"] = Value::Array(
            lanes
                .iter()
                .enumerate()
                .map(|(l, spec)| json!({ "lane": spec.name, "phases": lane_phases(shared, &table, l, spec) }))
                .collect(),
        );
    }
    out
}

// ── The actions ─────────────────────────────────────────────────────────

const ACTIONS: &[(&str, &str, &str)] = &[
    (
        "SIGNAL_CHAIN",
        "This instance's chain, signal.chain/1.",
        "{}",
    ),
    (
        "SIGNAL_SET",
        "Bypass a block or a phase, set a phase's make-up, or a block's algorithm or key: {\"block\"|\"phase\", \"bypass\"?, \"makeup_db\"?, \"algorithm\"?, \"key\"?}. `key` is the lane the block listens to (a trigger lane, `Drums/Snare/Sum/Trig`: its hits, placed or detected, and its audio when it runs in this instance), null to unkey it. Answers the new chain.",
        r#"{"type":"object","properties":{"block":{"type":"string"},"phase":{"type":"string"},"bypass":{"type":"boolean"},"makeup_db":{"type":"number"},"algorithm":{"type":"string"},"key":{"type":["string","null"]}}}"#,
    ),
    (
        "SIGNAL_BLOCK_ADD",
        "Add a block of `kind` to `phase`, after the block `after` (else at the phase's end), keyed by the lane `key` when given (a trigger lane: `Drums/Snare/Sum/Trig`). Answers the new chain.",
        r#"{"type":"object","required":["phase","kind"],"properties":{"phase":{"type":"string"},"kind":{"type":"string"},"after":{"type":"string"},"algorithm":{"type":"string"},"key":{"type":"string"}}}"#,
    ),
    (
        "SIGNAL_BLOCK_REMOVE",
        "Remove the block `block`. Answers the new chain.",
        r#"{"type":"object","required":["block"],"properties":{"block":{"type":"string"}}}"#,
    ),
    (
        "SIGNAL_BLOCK_MOVE",
        "Move the block `block` to `phase` (else its own), after the block `after` (else at the phase's end). Answers the new chain.",
        r#"{"type":"object","required":["block"],"properties":{"block":{"type":"string"},"phase":{"type":"string"},"after":{"type":"string"}}}"#,
    ),
    (
        "SIGNAL_SESSION",
        "Every port of this instance's session in this process: {\"session\", \"ports\": [signal.chain/1]}.",
        "{}",
    ),
    (
        "SIGNAL_ANALYZE",
        "Analyse an offline block (level, align) over the audio arriving at it: `block` its chain id; `audio` {path (a WAV of that audio), sample_rate, start_s} as the host renders it offline (no playback), or none to capture it while the song plays through once (analysed when it stops, or on `finish`: true); `reference` {path, sample_rate} for an align search, or `values` to set an alignment found elsewhere; `from`/`to` for part of the song; `consistency`, `range_db`, `target_db` for level; `source_revision`; `replace_user_edits`. Answers a job: the lanes land on the next main-thread call (SIGNAL_ANALYSIS), written as the host's automation of this FX's parameters through host-automation/1 in one undo step, or played by Signal where the host has no such extension.",
        r#"{"type":"object","properties":{"block":{"type":"string"},"lane":{"type":"string"},"audio":{"type":"object"},"reference":{"type":"object"},"values":{"type":"object"},"from":{"type":"number"},"to":{"type":"number"},"finish":{"type":"boolean"},"source_revision":{"type":"string"},"replace_user_edits":{"type":"boolean"},"consistency":{"type":"number"},"range_db":{"type":"number"},"target_db":{"type":"number"}},"required":["block"]}"#,
    ),
    (
        "SIGNAL_ANALYSIS",
        "Every offline block's analysis (or `block`'s), its lanes with their points, after landing whatever finished: {blocks: [{block, lane, kind, job, analysis}], finished: [what each finished job made and wrote]}.",
        r#"{"type":"object","properties":{"block":{"type":"string"},"lane":{"type":"string"}}}"#,
    ),
    (
        "SIGNAL_ANALYSIS_CLEAR",
        "Forget `block`'s analysis: its lanes cleared in the host, its parameters its own again.",
        r#"{"type":"object","properties":{"block":{"type":"string"},"lane":{"type":"string"}},"required":["block"]}"#,
    ),
    (
        "SIGNAL_ANALYSIS_STALE",
        "The audio arriving changed (an edit upstream): every offline block (or `block`) stale over `from`..`to` (its whole range when not given), unless `source_revision` is what it was made from.",
        r#"{"type":"object","properties":{"block":{"type":"string"},"from":{"type":"number"},"to":{"type":"number"},"source_revision":{"type":"string"}}}"#,
    ),
    (
        "SIGNAL_ALGORITHMS",
        "A family's algorithms: {\"kind\"} → [{\"id\", \"name\"}].",
        r#"{"type":"object","required":["kind"],"properties":{"kind":{"type":"string"}}}"#,
    ),
];

fn actions_json() -> String {
    Value::Array(
        ACTIONS
            .iter()
            .map(|(id, description, schema)| {
                json!({ "id": id, "description": description, "args_schema": serde_json::from_str::<Value>(schema).unwrap_or(Value::Null) })
            })
            .collect(),
    )
    .to_string()
}

fn arg<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

/// The lane this instance runs, by index in the preset, and a block of it
/// by its chain id (`tone.comp`, or just `comp`).
/// The lanes this instance runs (their preset indices), narrowed to the
/// action's `"lane"` when it names one. In the kit mode (more than one
/// lane) the action must name it: block ids are unique within a lane only.
fn lanes_of(state: &SignalState, lane: Option<&str>) -> Result<Vec<usize>, ExtError> {
    let port = port_of(state).map(str::to_owned);
    let names: Vec<String> = graph::lanes_for(&state.preset, port.as_deref())
        .iter()
        .map(|l| l.name.clone())
        .collect();
    let picked: Vec<&String> = match lane {
        Some(want) => names.iter().filter(|n| n.as_str() == want).collect(),
        None if names.len() > 1 => {
            return Err(ExtError::Failed(
                "the kit mode: name the lane (\"lane\": \"<path>\")".into(),
            ));
        }
        None => names.iter().collect(),
    };
    if picked.is_empty() {
        return Err(ExtError::NotFound(format!(
            "lane {}",
            lane.unwrap_or("(none)")
        )));
    }
    Ok(state
        .preset
        .lanes
        .iter()
        .enumerate()
        .filter(|(_, l)| picked.contains(&&l.name))
        .map(|(i, _)| i)
        .collect())
}

/// A block, by its chain id (`tone.comp`, or just `comp`), in the action's
/// lane.
pub(crate) fn find_block(
    state: &SignalState,
    id: &str,
    lane: Option<&str>,
) -> Result<(usize, usize), ExtError> {
    for li in lanes_of(state, lane)? {
        if let Some(bi) = state.preset.lanes[li]
            .blocks
            .iter()
            .position(|b| slots::chain_id(b.phase, &b.id) == id || b.id == id)
        {
            return Ok((li, bi));
        }
    }
    Err(ExtError::NotFound(format!("block {id}")))
}

fn first_lane(state: &SignalState, lane: Option<&str>) -> Result<usize, ExtError> {
    lanes_of(state, lane)?
        .first()
        .copied()
        .ok_or_else(|| ExtError::NotFound("a lane".into()))
}

/// Where a block of `phase` goes in `lane`: after `after`, else after the
/// last block of its phase or an earlier one (phase order kept).
fn insert_at(lane: &LaneSpec, phase: Phase, after: Option<&str>) -> usize {
    if let Some(i) = after.and_then(|a| {
        lane.blocks
            .iter()
            .position(|b| slots::chain_id(b.phase, &b.id) == a || b.id == a)
    }) {
        return i + 1;
    }
    // After the sources (a sampler, phase-less) at least: a block before
    // the sampler would process silence.
    lane.blocks
        .iter()
        .rposition(|b| b.phase.is_some_and(|p| p <= phase))
        .map_or_else(
            || lane.blocks.iter().take_while(|b| b.phase.is_none()).count(),
            |i| i + 1,
        )
}

/// An id no block in `lane` has: `kind`, `kind_2`, …
fn fresh_id(lane: &LaneSpec, kind: &str) -> String {
    let taken = |id: &str| lane.blocks.iter().any(|b| b.id == id);
    if !taken(kind) {
        return kind.to_owned();
    }
    (2..)
        .map(|n| format!("{kind}_{n}"))
        .find(|id| !taken(id))
        .unwrap_or_else(|| kind.to_owned())
}

/// Change the state, build it, and answer the new chain.
fn edit(
    shared: &Shared,
    on_main: bool,
    f: impl FnOnce(&mut SignalState) -> Result<(), ExtError>,
) -> Result<String, ExtError> {
    let mut state = shared.state();
    f(&mut state)?;
    let text = String::from_utf8(state.to_blob()).map_err(|e| ExtError::Failed(e.to_string()))?;
    shared.load_state_text(text, on_main);
    Ok(chain(shared).to_string())
}

/// Run action `id`. `on_main`: called on the host's main thread (signal/1),
/// so the host may be called back; an editor's call is not.
pub fn run(shared: &Shared, id: &str, args: &Value, on_main: bool) -> Result<String, ExtError> {
    match id {
        "SIGNAL_CHAIN" => Ok(chain(shared).to_string()),
        "SIGNAL_ANALYZE" => crate::offline::analyze(shared, args, on_main),
        "SIGNAL_ANALYSIS" => crate::offline::analysis(shared, args, on_main),
        "SIGNAL_ANALYSIS_CLEAR" => crate::offline::clear(shared, args, on_main),
        "SIGNAL_ANALYSIS_STALE" => crate::offline::stale(shared, args, on_main),
        "SIGNAL_SET" => edit(shared, on_main, |state| {
            if let Some(block) = arg(args, "block") {
                let (l, b) = find_block(state, block, arg(args, "lane"))?;
                let spec = &mut state.preset.lanes[l].blocks[b];
                if let Some(bypass) = args.get("bypass").and_then(Value::as_bool) {
                    spec.bypass = bypass;
                }
                if let Some(a) = arg(args, "algorithm") {
                    spec.algorithm = Some(a.to_owned());
                }
                // A string keys it, null unkeys it.
                if let Some(k) = args.get("key") {
                    spec.key = k.as_str().map(str::to_owned);
                }
                return Ok(());
            }
            let phase_id = arg(args, "phase")
                .ok_or_else(|| ExtError::Failed("SIGNAL_SET wants a block or a phase".into()))?;
            let phase = phase_from(phase_id)
                .ok_or_else(|| ExtError::NotFound(format!("phase {phase_id}")))?;
            let l = first_lane(state, arg(args, "lane"))?;
            let lane = &mut state.preset.lanes[l];
            if let Some(bypass) = args.get("bypass").and_then(Value::as_bool) {
                lane.bypass_phases.retain(|p| *p != phase);
                if bypass {
                    lane.bypass_phases.push(phase);
                }
            }
            if let Some(db) = args.get("makeup_db").and_then(Value::as_f64) {
                let gain_db = db as f32;
                match lane.trims.iter_mut().find(|t| t.phase == phase) {
                    Some(t) => t.gain_db = gain_db,
                    None => lane.trims.push(PhaseTrim { phase, gain_db }),
                }
            }
            Ok(())
        }),
        "SIGNAL_BLOCK_ADD" => edit(shared, on_main, |state| {
            let phase_id = arg(args, "phase").ok_or_else(|| ExtError::Failed("a phase".into()))?;
            let phase = phase_from(phase_id)
                .ok_or_else(|| ExtError::NotFound(format!("phase {phase_id}")))?;
            let kind = arg(args, "kind").ok_or_else(|| ExtError::Failed("a kind".into()))?;
            let BlockKind::Signal(name) = kind_from(kind) else {
                return Err(ExtError::Unsupported);
            };
            if !signal_core::blocks::exists(&name) {
                return Err(ExtError::NotFound(format!("block kind {kind}")));
            }
            let l = first_lane(state, arg(args, "lane"))?;
            let lane = &mut state.preset.lanes[l];
            // `first`: at the head of its phase (an offline block reading the
            // raw track: the vocal leveler).
            let at = if args.get("first").and_then(Value::as_bool) == Some(true) {
                lane.blocks
                    .iter()
                    .position(|b| b.phase.is_some_and(|p| p >= phase))
                    .unwrap_or(lane.blocks.len())
            } else {
                insert_at(lane, phase, arg(args, "after"))
            };
            let mut block = BlockSpec::signal(fresh_id(lane, kind), name).phase(phase);
            if let Some(a) = arg(args, "algorithm") {
                block = block.algorithm(a);
            }
            if let Some(k) = arg(args, "key") {
                block = block.key(k);
            }
            lane.blocks.insert(at, block);
            Ok(())
        }),
        "SIGNAL_BLOCK_REMOVE" => edit(shared, on_main, |state| {
            let block = arg(args, "block").ok_or_else(|| ExtError::Failed("a block".into()))?;
            let (l, b) = find_block(state, block, arg(args, "lane"))?;
            state.preset.lanes[l].blocks.remove(b);
            Ok(())
        }),
        "SIGNAL_BLOCK_MOVE" => edit(shared, on_main, |state| {
            let block = arg(args, "block").ok_or_else(|| ExtError::Failed("a block".into()))?;
            let (l, b) = find_block(state, block, arg(args, "lane"))?;
            let lane = &mut state.preset.lanes[l];
            let mut spec = lane.blocks.remove(b);
            if let Some(p) = arg(args, "phase") {
                spec.phase =
                    Some(phase_from(p).ok_or_else(|| ExtError::NotFound(format!("phase {p}")))?);
            }
            let at = insert_at(lane, spec.phase.unwrap_or(Phase::Tone), arg(args, "after"))
                .min(lane.blocks.len());
            lane.blocks.insert(at, spec);
            Ok(())
        }),
        "SIGNAL_SESSION" => {
            let state = shared.state();
            let session = session_of(&state).map(str::to_owned);
            let ports: Vec<Value> = match &session {
                None => vec![chain(shared)],
                Some(s) => instances()
                    .iter()
                    .filter(|other| session_of(&other.state()) == Some(s.as_str()))
                    .map(|other| chain(other))
                    .collect(),
            };
            Ok(json!({ "session": session, "ports": ports }).to_string())
        }
        "SIGNAL_ALGORITHMS" => {
            let kind = arg(args, "kind").ok_or_else(|| ExtError::Failed("a kind".into()))?;
            let BlockKind::Signal(family) = kind_from(kind) else {
                return Err(ExtError::Unsupported);
            };
            let algos: Vec<Value> = plugin_processors::algorithms::family(&family)
                .iter()
                .map(|a| json!({ "id": a.id, "name": a.name }))
                .collect();
            Ok(Value::Array(algos).to_string())
        }
        other => Err(ExtError::NotFound(other.into())),
    }
}

// ── signal/1 ────────────────────────────────────────────────────────────

impl SignalPlugin for Shared {
    fn list_actions(&self) -> String {
        actions_json()
    }

    fn run_action(&self, id: &str, args_json: &str) -> Result<String, ExtError> {
        let args: Value = serde_json::from_str(args_json).unwrap_or(Value::Null);
        // A finished analysis lands on the main thread, before anything else.
        if id != "SIGNAL_ANALYSIS" {
            crate::offline::drain(self);
            let _ = crate::offline::poll(self, true);
        }
        run(self, id, &args, true)
    }

    fn chain(&self) -> String {
        crate::offline::drain(self);
        let _ = crate::offline::poll(self, true);
        chain(self).to_string()
    }

    fn revision(&self) -> u64 {
        self.ext.revision.load(Ordering::Acquire)
    }

    fn param_stable_id(&self, param_id: u32) -> Option<String> {
        let table = self.params.shown.get();
        let i = (0..slots::SLOTS)
            .find(|i| nice_plug::wrapper::clap::param_clap_id(&slots::slot_id(*i)) == param_id)?;
        let s = table.slots.get(i)?;
        let phase = table
            .blocks
            .iter()
            .find(|b| b.lane == s.lane && b.block == s.block)
            .and_then(|b| b.phase);
        Some(format!(
            "{}.{}",
            slots::chain_id(phase, &s.block_id),
            s.spec.id
        ))
    }

    fn mod_sources(&self) -> Vec<ModSourceInfo> {
        self.params
            .shown
            .get()
            .meters
            .iter()
            .map(|m| ModSourceInfo {
                id: m.id.clone(),
                name: m.name.clone(),
                unit: m.unit.to_owned(),
            })
            .collect()
    }

    fn mod_source_value(&self, index: usize) -> f64 {
        // Audio thread: one atomic load.
        self.ext.meters.get(index).map_or(0.0, |v| {
            f64::from(f32::from_bits(v.load(Ordering::Relaxed)))
        })
    }

    fn set_session(&self, session: Option<(&str, &str)>) -> bool {
        edit(self, true, |state| {
            state.instance = match session {
                Some((s, p)) => Instance::Port(SessionLink {
                    session: SessionId(s.to_owned()),
                    port: PortName(p.to_owned()),
                }),
                None => Instance::MultiChannel,
            };
            Ok(())
        })
        .is_ok()
    }
}

/// How the extension finds an instance's [`Shared`] from the host's
/// pointer, and keeps the pointer for the instance's own calls.
pub struct Lookup;

impl SignalPluginLookup for Lookup {
    unsafe fn lookup(plugin: *const clap_plugin) -> Option<Arc<dyn SignalPlugin>> {
        let shared = unsafe { shared_of(plugin) }?;
        Some(shared as Arc<dyn SignalPlugin>)
    }
}

/// # Safety
///
/// `plugin` is nice-plug's `clap_plugin` for [`FtsSignal`].
pub unsafe fn shared_of(plugin: *const clap_plugin) -> Option<Arc<Shared>> {
    let state = unsafe { clap_extension_state::<FtsSignal>(plugin) }?;
    let shared = state.downcast::<Shared>().ok()?;
    shared
        .ext
        .plugin
        .store(plugin.cast_mut(), Ordering::Release);
    Some(shared)
}

/// `init()`: the instance's pointers, and the host's `host-actions/1`.
pub fn init(shared: &Shared, plugin: *const clap_plugin, host: *const clap_host) {
    shared
        .ext
        .plugin
        .store(plugin.cast_mut(), Ordering::Release);
    shared.ext.host.store(host.cast_mut(), Ordering::Release);
    // SAFETY: the host's own pointer, on its main thread (init).
    let client = unsafe { HostActionsClient::query(host) };
    // SAFETY: as above.
    shared
        .ext
        .offline
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .automation = unsafe { daw_clap_ext::automation::HostAutomationClient::query(host) };
    *shared
        .ext
        .actions
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = client;
}

// ── host-events/1 ───────────────────────────────────────────────────────

/// The host's events: the transport (`{"playing", "frame"}`) and a piece's
/// hits (`{"frame", "velocity"}`, the host's hit markers: published to the
/// session as notes, so gates key on them).
pub unsafe extern "C" fn on_event(
    plugin: *const clap_plugin,
    _subscription: u64,
    topic: *const c_char,
    event_json: *const c_char,
) {
    let Some(shared) = (unsafe { shared_of(plugin) }) else {
        return;
    };
    let (Some(topic), Some(body)) = (cstr(topic), cstr(event_json)) else {
        return;
    };
    let Ok(event) = serde_json::from_str::<Value>(body) else {
        return;
    };
    if topic == daw_clap_ext::sys::FTS_AUTOMATION_TOPIC {
        crate::offline::user_edited(&shared, &event);
        return;
    }
    if topic == "transport" {
        if let Some(frame) = event.get("frame").and_then(Value::as_u64) {
            shared.ext.transport_frame.store(frame, Ordering::Release);
        }
        let playing = event
            .get("playing")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        shared
            .ext
            .transport_playing
            .store(playing, Ordering::Release);
        // A capture finishes when the song stops; a finished analysis lands.
        crate::offline::drain(&shared);
        let _ = crate::offline::poll(&shared, true);
        return;
    }
    let Some(piece) = topic.strip_prefix("hits:") else {
        return;
    };
    // Studio's: the piece's every hit, after each edit step:
    // `{piece, track, note, count, hits: [{time, velocity}]}`, time in
    // seconds of the song. An empty list is a piece with no hit to play,
    // known: its gates stay shut. One with `error` names no piece of the
    // song's kit, and says nothing of its hits.
    if event.get("error").is_some_and(|e| !e.is_null()) {
        tracing::warn!(topic, "signal: the host names no piece for these hits");
        return;
    }
    let Some(list) = event.get("hits").and_then(Value::as_array) else {
        return;
    };
    let rate = f64::from(shared.sample_rate());
    let raw: Vec<(f64, f64)> = list
        .iter()
        .filter_map(|h| {
            Some((
                h.get("time")?.as_f64()?,
                h.get("velocity").and_then(Value::as_f64).unwrap_or(1.0),
            ))
        })
        .collect();
    let scale = if raw.iter().any(|(_, v)| *v > 1.0) {
        127.0
    } else {
        1.0
    };
    let mut hits: Vec<(u64, f32)> = raw
        .iter()
        .filter(|(t, _)| *t >= 0.0)
        .map(|(t, v)| {
            (
                (t * rate).round() as u64,
                ((v / scale) as f32).clamp(0.0, 1.0),
            )
        })
        .collect();
    hits.sort_by_key(|h| h.0);
    let state = shared.state();
    for index in planned_pieces(&state.preset, piece) {
        {
            let mut planned = shared
                .ext
                .planned
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            planned.retain(|(p, _)| *p != index);
            planned.push((index, hits.clone()));
        }
        shared.send_planned(index, hits.clone());
    }
}

/// The lanes of `preset` a `hits:<piece>` event places hits for, by
/// index (the graph's id of a remote key's piece): the piece's trigger
/// lanes, and any lane of the piece a block keys on (a trigger track
/// played by notes, with no detector of its own).
#[must_use]
pub fn planned_pieces(preset: &signal_core::state::Preset, piece: &str) -> Vec<u16> {
    let keyed = |name: &str| {
        preset
            .lanes
            .iter()
            .any(|l| l.blocks.iter().any(|b| b.key.as_deref() == Some(name)))
    };
    preset
        .lanes
        .iter()
        .enumerate()
        .filter(|(_, l)| piece_of(&l.name) == piece && (l.trigger.is_some() || keyed(&l.name)))
        .filter_map(|(i, _)| u16::try_from(i).ok())
        .collect()
}

fn cstr<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        return None;
    }
    // SAFETY: the host's NUL-terminated string.
    unsafe { CStr::from_ptr(p) }.to_str().ok()
}

pub const HOST_EVENTS: daw_clap_ext::sys::fts_plugin_host_events =
    daw_clap_ext::sys::fts_plugin_host_events {
        on_event: Some(on_event),
    };

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placed_hits_reach_a_key_on_a_lane_without_a_detector() {
        let mut preset = signal_core::kit::drum_kit(&signal_core::kit::KitPieces::default());
        let trig = "Drums/Toms/Tom 4/T4 Trig";
        let at = preset
            .lanes
            .iter()
            .position(|l| l.name == trig)
            .expect("the tom's trigger");
        let want = vec![u16::try_from(at).expect("an id")];
        assert_eq!(planned_pieces(&preset, "Drums/Toms/Tom 4"), want);
        // Played by notes, no detector: the tom's gate still keys on it.
        preset.lanes[at].trigger = None;
        assert_eq!(planned_pieces(&preset, "Drums/Toms/Tom 4"), want);
        assert!(planned_pieces(&preset, "Drums/Toms/Tom 9").is_empty());
    }

    #[test]
    fn a_block_added_to_a_sampler_port_goes_after_the_sampler() {
        let lane = LaneSpec {
            blocks: vec![BlockSpec::signal("drum", "drum")],
            ..LaneSpec::default()
        };
        assert_eq!(insert_at(&lane, Phase::Rescue, None), 1);
        let lane = LaneSpec {
            blocks: vec![
                BlockSpec::signal("drum", "drum"),
                BlockSpec::signal("eq", "eq").phase(Phase::Tone),
            ],
            ..LaneSpec::default()
        };
        assert_eq!(insert_at(&lane, Phase::Rescue, None), 1);
        assert_eq!(insert_at(&lane, Phase::Polish, None), 2);
    }
}
