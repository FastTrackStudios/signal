//! frame control surfaces over signal's rigs.
//!
//! [`GuitarSurface`] implements frame's `SurfaceEngine` on top of the guitar
//! rig's own services: it is a *client* of `rig::Rig` / `rig::RigStream`
//! (established over the engine's [`LocalServer`](architect::LocalServer),
//! the same way the watch bridge is), so the rig itself is untouched and the
//! adapter works identically in-process and remotely.
//!
//! Addressing (scoped per rig on the router, e.g. `guitar`):
//!
//! | address | kind | maps to |
//! |---|---|---|
//! | `<block>/<param>` | continuous | `set_block_param(block, param, real)` |
//! | `<block>/on` | toggle | `set_block_bypass(block, !on)` |
//! | `<block>/type` | choice | `set_block_option(block, i)` |
//! | `rig/master_trim` | continuous (dB) | `set_master_trim(db)` |
//!
//! Block ids are the rig's (display-name based, de-duplicated: `Delay 2`).
//! Actions: `tap_tempo`, `tuner`, `next_song`, `prev_song`, `fx`, `boost`,
//! `cycle_boost`, `mute`, `stack/<i>`. Streams: `in_level`, `out_level`
//! (dBFS, L/R), `gain_reduction`, `spectrum` (96 bins), `tempo`.
//! Collections: `songs`, `setlists`, `parts`, `patches`.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use architect::dispatch::CurrentThreadDispatcher;
use architect::{HasDispatcher, Layer, PubSub, Services, layers};
use architect_telemetry::wide;
use signal_guitar_proto::rig::{RigClient, RigEvent, RigStreamClient};
use signal_guitar_proto::{LiveBlock, PatchInfo, PerformanceModel, RigStatus};
use signal_proto::block::BlockType;
use signal_proto::{ParameterRange, Taper, Unit as SigUnit};
use surface_proto::engine::{SurfaceEngine, SurfaceEngineStreamSource};
use surface_proto::{
    ActionInfo, Catalog, CollectionInfo, EditPhase, EntryId, ParamAddr, ParamInfo, ParamValue, Range, Scale,
    StreamFrame, StreamInfo, StreamKind, SurfaceError, Unit,
};

/// Real ranges (taper/unit) for block params, from the DSP (`native::range_of`).
pub type Ranges = Arc<dyn Fn(BlockType, &str) -> Option<ParameterRange> + Send + Sync>;

// ── the pure mapping ─────────────────────────────────────────────────────

fn unit(u: SigUnit) -> Unit {
    match u {
        SigUnit::None => Unit::None,
        SigUnit::Hz => Unit::Hz,
        SigUnit::Decibels => Unit::Db,
        SigUnit::Seconds => Unit::Seconds,
        SigUnit::Milliseconds => Unit::Milliseconds,
        SigUnit::Percent => Unit::Percent,
        SigUnit::Ratio => Unit::Ratio,
        SigUnit::Semitones => Unit::Semitones,
        SigUnit::Degrees => Unit::Degrees,
        SigUnit::Beats => Unit::Beats,
    }
}

/// The engine clamps/normalizes over the block's own `min..max`, so those
/// win; the DSP range contributes only the taper and unit.
fn param_range(block: &LiveBlock, name: &str, min: f32, max: f32, ranges: &Ranges) -> Range {
    let dsp = ranges(block.block_type, name);
    let (scale, steps) = match dsp.map(|r| r.taper) {
        Some(Taper::Logarithmic) => (Scale::Logarithmic, None),
        Some(Taper::Stepped { steps }) => (Scale::Linear, Some(steps)),
        _ => (Scale::Linear, None),
    };
    Range { min: f64::from(min), max: f64::from(max), scale, unit: dsp.map_or(Unit::None, |r| unit(r.unit)), steps }
}

fn title(s: &str) -> String {
    let s = s.replace(['_', '-'], " ");
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str())
}

/// Everything the guitar rig exposes, as a catalog.
#[must_use]
pub fn guitar_catalog(chain: &[LiveBlock], perf: &PerformanceModel, patches: &[PatchInfo], ranges: &Ranges) -> Catalog {
    let mut params = Vec::new();
    for b in chain {
        for p in &b.params {
            let range = param_range(b, &p.name, p.min, p.max, ranges);
            let mut info = ParamInfo::new(format!("{}/{}", b.id, p.name), title(&p.name), range).in_group(b.id.clone());
            // The block's primary param is what its footprint on a stage
            // surface should be (the rig's own chain strip shows the same).
            info.featured = b.param_name.as_deref() == Some(p.name.as_str());
            // The rig doesn't publish defaults on the wire; the value the
            // patch loaded with is the reset target.
            info.default = info.range.to_normalized(f64::from(p.value));
            params.push(info);
        }
        params.push(ParamInfo::new(format!("{}/on", b.id), format!("{} On", b.name), Range::default()).in_group(b.id.clone()).toggle().short(b.name.clone()).featured());
        if !b.options.is_empty() {
            let options: Vec<&str> = b.options.iter().map(String::as_str).collect();
            params.push(ParamInfo::new(format!("{}/type", b.id), format!("{} Type", b.name), Range::default()).in_group(b.id.clone()).choice(&options).featured());
        }
    }
    params.push(ParamInfo::new("rig/master_trim", "Master", Range::linear(-60.0, 12.0, Unit::Db)).in_group("Output").with_default_real(0.0).featured());

    let action = |id: &str, name: &str, group: &str| ActionInfo { id: EntryId::new(id), name: name.into(), group: Some(group.into()) };
    let mut actions = vec![
        action("tap_tempo", "Tap Tempo", "Global"),
        action("tuner", "Tuner", "Global"),
        action("fx", "FX", "Global"),
        action("boost", "Boost", "Global"),
        action("cycle_boost", "Boost Level", "Global"),
        action("mute", "Mute", "Global"),
        action("next_song", "Next Song", "Setlist"),
        action("prev_song", "Previous Song", "Setlist"),
    ];
    for (i, s) in perf.stacks.iter().enumerate() {
        actions.push(action(&format!("stack/{i}"), &s.name, "Stacks"));
    }
    let _ = patches;

    let stream = |id: &str, name: &str, kind: StreamKind, rate_hz: f64| StreamInfo { id: EntryId::new(id), name: name.into(), kind, rate_hz };
    Catalog {
        id: "signal.guitar".into(),
        version: 1,
        params,
        actions,
        streams: vec![
            stream("in_level", "Input", StreamKind::Level { channels: 2 }, 30.0),
            stream("out_level", "Output", StreamKind::Level { channels: 2 }, 30.0),
            stream("gain_reduction", "Compression", StreamKind::GainReduction, 30.0),
            stream("spectrum", "Spectrum", StreamKind::Spectrum { bins: 96 }, 15.0),
            stream("tempo", "Tempo", StreamKind::Scalar { range: Range::linear(40.0, 240.0, Unit::Bpm) }, 4.0),
        ],
        collections: ["songs", "setlists", "parts", "patches"]
            .iter()
            .map(|c| CollectionInfo { id: EntryId::new(*c), name: title(c) })
            .collect(),
    }
}

/// Current values (normalized) for every catalog param.
#[must_use]
pub fn guitar_values(chain: &[LiveBlock], perf: &PerformanceModel, catalog: &Catalog) -> BTreeMap<ParamAddr, f64> {
    let mut out = BTreeMap::new();
    for b in chain {
        for p in &b.params {
            let addr = ParamAddr::new(format!("{}/{}", b.id, p.name));
            if let Some(info) = catalog.param(&addr) {
                out.insert(addr, info.range.to_normalized(f64::from(p.value)));
            }
        }
        out.insert(ParamAddr::new(format!("{}/on", b.id)), if b.bypassed { 0.0 } else { 1.0 });
        if !b.options.is_empty() && b.options.len() > 1 {
            out.insert(ParamAddr::new(format!("{}/type", b.id)), f64::from(b.option) / (b.options.len() - 1) as f64);
        }
    }
    if let Some(info) = catalog.param(&ParamAddr::new("rig/master_trim")) {
        out.insert(info.addr.clone(), info.range.to_normalized(f64::from(perf.master_trim_db)));
    }
    out
}

/// One call on the rig.
#[derive(Debug, Clone, PartialEq)]
pub enum RigCall {
    Param { block: String, param: String, real: f32 },
    Bypass { block: String, bypassed: bool },
    Option { block: String, option: u32 },
    MasterTrim(f32),
    TapTempo,
    Tuner,
    Fx,
    Boost,
    CycleBoost,
    Mute,
    NextSong,
    PrevSong,
    Stack(u32),
    Song(u32),
    Setlist(u32),
    Part(u32),
    Patch(u32),
}

/// A normalized set → the rig call that performs it.
///
/// # Errors
/// Unknown address.
pub fn route_param(catalog: &Catalog, addr: &ParamAddr, value: f64) -> Result<RigCall, SurfaceError> {
    let info = catalog.param(addr).ok_or_else(|| SurfaceError::UnknownParam(addr.0.clone()))?;
    if addr.0 == "rig/master_trim" {
        return Ok(RigCall::MasterTrim(info.range.to_real(value) as f32));
    }
    let (block, param) = addr.0.rsplit_once('/').ok_or_else(|| SurfaceError::UnknownParam(addr.0.clone()))?;
    Ok(match param {
        "on" => RigCall::Bypass { block: block.into(), bypassed: value < 0.5 },
        "type" => {
            let n = match &info.kind {
                surface_proto::ParamKind::Choice { options } => options.len().max(1),
                _ => 1,
            };
            RigCall::Option { block: block.into(), option: (value.clamp(0.0, 1.0) * (n - 1) as f64).round() as u32 }
        }
        _ => RigCall::Param { block: block.into(), param: param.into(), real: info.range.to_real(value) as f32 },
    })
}

/// # Errors
/// Unknown action.
pub fn route_action(action: &EntryId) -> Result<RigCall, SurfaceError> {
    Ok(match action.0.as_str() {
        "tap_tempo" => RigCall::TapTempo,
        "tuner" => RigCall::Tuner,
        "fx" => RigCall::Fx,
        "boost" => RigCall::Boost,
        "cycle_boost" => RigCall::CycleBoost,
        "mute" => RigCall::Mute,
        "next_song" => RigCall::NextSong,
        "prev_song" => RigCall::PrevSong,
        other => match other.strip_prefix("stack/").and_then(|i| i.parse().ok()) {
            Some(i) => RigCall::Stack(i),
            None => return Err(SurfaceError::UnknownAction(other.into())),
        },
    })
}

/// # Errors
/// Unknown collection.
pub fn route_select(collection: &EntryId, index: u32) -> Result<RigCall, SurfaceError> {
    Ok(match collection.0.as_str() {
        "songs" => RigCall::Song(index),
        "setlists" => RigCall::Setlist(index),
        "parts" => RigCall::Part(index),
        "patches" => RigCall::Patch(index),
        other => return Err(SurfaceError::UnknownCollection(other.into())),
    })
}

fn db(peak: f32) -> f64 {
    (20.0 * f64::from(peak.max(1e-6)).log10()).max(-90.0)
}

/// Telemetry frames for a status update.
#[must_use]
pub fn status_frames(s: &RigStatus) -> Vec<StreamFrame> {
    vec![
        StreamFrame { id: EntryId::new("in_level"), values: vec![db(s.input_peak_l), db(s.input_peak_r)] },
        StreamFrame { id: EntryId::new("out_level"), values: vec![db(s.output_peak_l), db(s.output_peak_r)] },
        StreamFrame { id: EntryId::new("gain_reduction"), values: vec![-f64::from(s.comp_gr_db.max(0.0))] },
    ]
}

// ── the backend ──────────────────────────────────────────────────────────

struct State {
    chain: Vec<LiveBlock>,
    perf: PerformanceModel,
    patches: Vec<PatchInfo>,
    catalog: Catalog,
    values: BTreeMap<ParamAddr, f64>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The guitar rig as a frame `SurfaceEngine`. Build with [`GuitarSurface::connect`];
/// mount with `router.merge_router_scoped("guitar", surface.into_router())`.
#[derive(Clone, HasDispatcher)]
#[dispatch(CurrentThreadDispatcher)]
pub struct GuitarSurface {
    state: Arc<Mutex<State>>,
    ranges: Ranges,
    calls: tokio::sync::mpsc::UnboundedSender<RigCall>,
    changes: PubSub<ParamValue>,
    telemetry: PubSub<StreamFrame>,
}

impl GuitarSurface {
    /// Attach to the guitar rig over `local` (the engine's in-process
    /// server): snapshot its state, then follow its events and forward
    /// surface edits to it. Must run inside a tokio runtime.
    ///
    /// # Errors
    /// When the rig's services can't be reached.
    pub async fn connect(local: &architect::LocalServer, ranges: Ranges) -> Result<Self, String> {
        let rig: RigClient = local.establish().await.map_err(|e| format!("rig client: {e}"))?;
        let stream: RigStreamClient = local.establish().await.map_err(|e| format!("rig stream client: {e}"))?;
        let chain = rig.chain().await.map_err(|e| format!("chain: {e:?}"))?;
        let perf = rig.perf().await.map_err(|e| format!("perf: {e:?}"))?;
        let patches = rig.patches().await.map_err(|e| format!("patches: {e:?}"))?;
        let catalog = guitar_catalog(&chain, &perf, &patches, &ranges);
        let values = guitar_values(&chain, &perf, &catalog);
        let (calls, call_rx) = tokio::sync::mpsc::unbounded_channel();
        let this = Self {
            state: Arc::new(Mutex::new(State { chain, perf, patches, catalog, values })),
            ranges,
            calls,
            changes: PubSub::sliding(256),
            telemetry: PubSub::sliding(64),
        };
        architect::platform::spawn(run_calls(rig, call_rx));
        architect::platform::spawn(this.clone().follow(stream));
        Ok(this)
    }

    /// Mirror rig events into the cache, the change feed and telemetry.
    async fn follow(self, stream: RigStreamClient) {
        let (tx, mut rx) = vox::channel::<RigEvent>();
        let call = stream.events(tx);
        let pump = async {
            while let Ok(Some(event)) = rx.recv().await {
                let _ = event.map(|ev| self.on_event(&ev));
            }
        };
        tokio::select! {
            () = pump => {}
            _ = call => {}
        }
    }

    fn on_event(&self, ev: &RigEvent) {
        match ev {
            RigEvent::Status(s) => {
                for f in status_frames(s) {
                    self.telemetry.publish(f);
                }
            }
            RigEvent::Spectrum(bins) => {
                self.telemetry.publish(StreamFrame { id: EntryId::new("spectrum"), values: bins.iter().map(|b| f64::from(*b)).collect() });
            }
            RigEvent::Chain(chain) => self.refresh(Some(chain.clone()), None),
            RigEvent::Perf(perf) => {
                self.telemetry.publish(StreamFrame { id: EntryId::new("tempo"), values: vec![f64::from(perf.tempo_bpm)] });
                self.refresh(None, Some(perf.clone()));
            }
            _ => {}
        }
    }

    /// Rebuild catalog + values; publish what changed.
    fn refresh(&self, chain: Option<Vec<LiveBlock>>, perf: Option<PerformanceModel>) {
        let changed: Vec<ParamValue> = {
            let mut st = lock(&self.state);
            if let Some(c) = chain {
                st.chain = c;
            }
            if let Some(p) = perf {
                st.perf = p;
            }
            let catalog = guitar_catalog(&st.chain, &st.perf, &st.patches, &self.ranges);
            if catalog.params.iter().map(|p| &p.addr).ne(st.catalog.params.iter().map(|p| &p.addr)) {
                st.catalog = Catalog { version: st.catalog.version + 1, ..catalog };
            }
            let values = guitar_values(&st.chain, &st.perf, &st.catalog);
            let changed = values
                .iter()
                .filter(|(a, v)| st.values.get(*a).is_none_or(|old| (old - **v).abs() > 1e-9))
                .map(|(a, v)| ParamValue { addr: a.clone(), value: *v })
                .collect();
            st.values = values;
            changed
        };
        for v in changed {
            self.changes.publish(v);
        }
    }

    fn send(&self, call: RigCall) -> Result<(), SurfaceError> {
        wide::set("surface.call", format!("{call:?}").split([' ', '(']).next().unwrap_or_default().to_string());
        let queued = self.calls.send(call).is_ok();
        wide::set("surface.outcome", if queued { "queued" } else { "rig_gone" });
        Ok(())
    }
}

/// Execute rig calls in order (one task owns the command client).
async fn run_calls(rig: RigClient, mut rx: tokio::sync::mpsc::UnboundedReceiver<RigCall>) {
    while let Some(call) = rx.recv().await {
        let r = match call.clone() {
            RigCall::Param { block, param, real } => rig.set_block_param(block, param, real).await,
            RigCall::Bypass { block, bypassed } => rig.set_block_bypass(block, bypassed).await,
            RigCall::Option { block, option } => rig.set_block_option(block, option).await,
            RigCall::MasterTrim(db) => rig.set_master_trim(db).await,
            RigCall::TapTempo => rig.tap_tempo().await,
            RigCall::Tuner => rig.toggle_tuner().await,
            RigCall::Fx => rig.toggle_fx().await,
            RigCall::Boost => rig.toggle_boost().await,
            RigCall::CycleBoost => rig.cycle_boost().await,
            RigCall::Mute => rig.toggle_main_mute().await,
            RigCall::NextSong => rig.next_song().await,
            RigCall::PrevSong => rig.prev_song().await,
            RigCall::Stack(i) => rig.press_stack(i).await,
            RigCall::Song(i) => rig.select_song(i).await,
            RigCall::Setlist(i) => rig.select_setlist(i).await,
            RigCall::Part(i) => rig.select_part(i).await,
            RigCall::Patch(i) => rig.select_patch(i).await,
        };
        if let Err(e) = r {
            tracing::warn!(?call, error = ?e, "guitar surface: rig call failed");
        }
    }
}

impl SurfaceEngine for GuitarSurface {
    fn catalog(&self) -> Catalog {
        lock(&self.state).catalog.clone()
    }

    fn values(&self) -> Vec<ParamValue> {
        lock(&self.state).values.iter().map(|(addr, value)| ParamValue { addr: addr.clone(), value: *value }).collect()
    }

    fn set_param(&self, addr: ParamAddr, value: f64, _phase: EditPhase) -> Result<(), SurfaceError> {
        wide::set("surface.rig", "guitar");
        wide::set("surface.addr", addr.0.clone());
        let call = {
            let mut st = lock(&self.state);
            let call = route_param(&st.catalog, &addr, value)?;
            // Optimistic: echo immediately; the rig's Chain event confirms.
            st.values.insert(addr, value);
            call
        };
        self.send(call)
    }

    fn invoke(&self, action: EntryId) -> Result<(), SurfaceError> {
        wide::set("surface.rig", "guitar");
        wide::set("surface.action", action.0.clone());
        self.send(route_action(&action)?)
    }

    fn select(&self, collection: EntryId, index: u32) -> Result<(), SurfaceError> {
        wide::set("surface.rig", "guitar");
        wide::set("surface.collection", collection.0.clone());
        self.send(route_select(&collection, index)?)
    }
}

impl SurfaceEngineStreamSource for GuitarSurface {
    fn param_changes_hub(&self) -> &PubSub<ParamValue> {
        &self.changes
    }

    fn telemetry_hub(&self) -> &PubSub<StreamFrame> {
        &self.telemetry
    }
}

impl Services for GuitarSurface {
    fn layers() -> impl Layer<Self> {
        layers![surface_proto::engine::Service, surface_proto::engine::StreamService]
    }
}

#[cfg(test)]
mod tests;
