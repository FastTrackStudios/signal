//! The keys rig as a frame `SurfaceEngine`.
//!
//! [`KeysSurface`] mirrors [`GuitarSurface`](crate::GuitarSurface): a vox
//! client of the rig's own `keys::KeysRig` / `KeysRigStream` services over
//! the engine's [`LocalServer`](architect::LocalServer), so the rig is
//! untouched. It caches a [`KeysSnapshot`] (mixer, rig/engine/layer Global
//! Controls, loaded modules' macros, perform model), re-fetched whenever the
//! rig publishes a `Mixer` / `Tree` / `Perform` / `Library` event; the diff
//! feeds `param_changes`.
//!
//! Addressing (scoped `keys` on the router). Engines, layers and module
//! slots are addressed by the rig's own ids — the engine/layer names its
//! setters take ("Keys", "Keys A") and the module slot letter ("A".."D") —
//! never by display position. Macro ids are the rig's stable ids with the
//! level prefix (`r.`/`e.`/`l.`) dropped.
//!
//! | address | kind | maps to |
//! |---|---|---|
//! | `rig/<key>` | Global Control (featured) | `set_rig_global("r.<key>", real)` |
//! | `master/gain_db` | dB (featured) | `set_master_gain(db)` |
//! | `perform/mode` | choice | `set_perform_mode(i)` |
//! | `<engine>/<key>` | Global Control | `set_engine_global(engine, "e.<key>", real)` |
//! | `<engine>/gain_db` · `<engine>/mute` | dB · toggle | `set_engine_gain` · `set_engine_mute` |
//! | `<engine>/<layer>/<key>` | Global Control | `set_layer_global(layer, "l.<key>", real)` |
//! | `<engine>/<layer>/gain_db` · `mute` · `solo` · `exclude_global` | dB · toggles | `set_layer_gain` · `_mute` · `_solo` · `_exclude_global` |
//! | `<engine>/<layer>/<slot>/<macro id>` | module macro | `set_layer_macro(layer, module, id, real)` |
//! | `<engine>/<layer>/<slot>/gain_db` · `on` | dB · toggle | `set_module_gain` · `set_module_enabled` |
//!
//! Module params exist only for modules with a loaded source. A Global
//! Control whose modules disagree is a *bipolar offset* on the rig (−1..1,
//! centre = the patch's own values); the catalog follows (range −1..1,
//! `bipolar`, default centre) and its version bumps when that flips.
//!
//! Actions: `start`, `stop`, `stack/<i>` (named after the profile's stacks).
//! Collections: `presets` (`select` → `load_preset`). Streams: `master_level`
//! (dBFS), one `<path>/level` per engine, layer and loaded module meter
//! (dBFS, floor −90), `voices`.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use architect::dispatch::CurrentThreadDispatcher;
use architect::{HasDispatcher, Layer, PubSub, Services, layers};
use architect_telemetry::wide;
use signal_keys_proto::keys::{KeysRigClient, KeysRigStreamClient};
use signal_keys_proto::{KeysEngineDetail, KeysEvent, KeysMacro, KeysMixer, KeysPerform, KeysStatus};
use surface_proto::engine::{SurfaceEngine, SurfaceEngineStreamSource};
use surface_proto::{
    ActionInfo, Catalog, CollectionInfo, EditPhase, EntryId, ParamAddr, ParamInfo, ParamKind, ParamValue, Range, Scale,
    StreamFrame, StreamInfo, StreamKind, SurfaceError, Unit,
};

use crate::{db, lock, title};

/// The rig's fader bounds (`MIN_FADER_DB` / `MAX_FADER_DB` in the backend —
/// every gain setter clamps to them).
pub const FADER_DB: (f64, f64) = (-60.0, 6.0);

const PERFORM_MODES: [&str; 3] = ["Preset", "Profile", "Setlist"];

// ── the snapshot ─────────────────────────────────────────────────────────

/// Everything the surface reads from the rig — one fetch of its getters.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KeysSnapshot {
    pub mixer: KeysMixer,
    pub perform: KeysPerform,
    /// `rig_macros()` — the `r.` Global Controls.
    pub rig_macros: Vec<KeysMacro>,
    /// `engine_detail(engine)` by engine name.
    pub engines: BTreeMap<String, KeysEngineDetail>,
    /// The `l.` Global Controls (`layer_detail(..).layer_macros`) by layer name.
    pub layer_macros: BTreeMap<String, Vec<KeysMacro>>,
    /// A loaded module's macros (`layer_detail(layer, module).macros`).
    pub module_macros: BTreeMap<(String, u32), Vec<KeysMacro>>,
}

impl KeysSnapshot {
    /// Read the rig's current state through its getters.
    ///
    /// # Errors
    /// Any getter call failing.
    pub async fn fetch(rig: &KeysRigClient) -> Result<Self, String> {
        let err = |what: &str| {
            let what = what.to_owned();
            move |e| format!("{what}: {e:?}")
        };
        let mixer = rig.mixer().await.map_err(err("mixer"))?;
        let perform = rig.perform().await.map_err(err("perform"))?;
        let rig_macros = rig.rig_macros().await.map_err(err("rig_macros"))?;
        let mut snap = Self { mixer, perform, rig_macros, ..Self::default() };
        for e in &snap.mixer.engines {
            let detail = rig.engine_detail(e.name.clone()).await.map_err(err("engine_detail"))?;
            snap.engines.insert(e.name.clone(), detail);
            for l in &e.layers {
                let detail = rig.layer_detail(l.name.clone(), 0).await.map_err(err("layer_detail"))?;
                snap.layer_macros.insert(l.name.clone(), detail.layer_macros);
                for m in l.modules.iter().filter(|m| !m.patch.is_empty()) {
                    let macros = if m.index == detail.module {
                        detail.macros.clone()
                    } else {
                        rig.layer_detail(l.name.clone(), m.index).await.map_err(err("layer_detail"))?.macros
                    };
                    snap.module_macros.insert((l.name.clone(), m.index), macros);
                }
            }
        }
        Ok(snap)
    }
}

// ── the pure mapping ─────────────────────────────────────────────────────

/// The rig's display-unit strings → frame units.
#[must_use]
pub fn unit(u: &str) -> Unit {
    match u {
        "" => Unit::None,
        "dB" | "db" => Unit::Db,
        "Hz" | "hz" => Unit::Hz,
        "ms" => Unit::Milliseconds,
        "s" => Unit::Seconds,
        "%" => Unit::Percent,
        "st" => Unit::Semitones,
        "c" | "ct" | "cents" => Unit::Cents,
        "°" | "deg" => Unit::Degrees,
        "bpm" | "BPM" => Unit::Bpm,
        other => Unit::Custom { suffix: other.to_owned() },
    }
}

/// A macro's range: its own min/max, tapered by what it measures.
///
/// Log for positive frequency/time spans; whole-number steps for the
/// enumerated ones (algorithm, division, shape, voices). A bipolar (offset)
/// Global Control is a plain −1..1.
#[must_use]
pub fn macro_range(m: &KeysMacro) -> Range {
    let (min, max) = (f64::from(m.min), f64::from(m.max));
    if m.bipolar {
        return Range::linear(min, max, Unit::None);
    }
    let unit = unit(&m.unit);
    let log = min > 0.0 && matches!(unit, Unit::Hz | Unit::Milliseconds | Unit::Seconds);
    let key = m.id.rsplit('.').next().unwrap_or_default();
    let stepped = matches!(key, "algo" | "div" | "shape" | "unison" | "voices") && max > min;
    Range {
        min,
        max,
        scale: if log { Scale::Logarithmic } else { Scale::Linear },
        unit,
        steps: stepped.then(|| (max - min) as u32 + 1),
    }
}

/// What one surface param drives on the rig.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    RigGlobal(String),
    EngineGlobal { engine: String, id: String },
    LayerGlobal { layer: String, id: String },
    ModuleMacro { layer: String, module: u32, id: String },
    MasterGain,
    EngineGain(String),
    EngineMute(String),
    LayerGain(String),
    LayerMute(String),
    LayerSolo(String),
    LayerExcludeGlobal(String),
    ModuleGain { layer: String, module: u32 },
    ModuleOn { layer: String, module: u32 },
    PerformMode,
}

/// The catalog plus how each param routes, and the current values.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KeysModel {
    pub catalog: Catalog,
    pub targets: BTreeMap<ParamAddr, Target>,
    pub values: BTreeMap<ParamAddr, f64>,
    /// Status meter `(kind, name)` → its stream id.
    pub meters: BTreeMap<(String, String), EntryId>,
}

fn strip_level(id: &str) -> &str {
    id.strip_prefix("r.").or_else(|| id.strip_prefix("e.")).or_else(|| id.strip_prefix("l.")).unwrap_or(id)
}

struct Builder {
    params: Vec<ParamInfo>,
    targets: BTreeMap<ParamAddr, Target>,
    values: BTreeMap<ParamAddr, f64>,
}

impl Builder {
    fn push(&mut self, info: ParamInfo, target: Target, real: f64) {
        self.values.insert(info.addr.clone(), info.range.to_normalized(real));
        self.targets.insert(info.addr.clone(), target);
        self.params.push(info);
    }

    fn gain(&mut self, addr: String, name: String, group: &str, target: Target, db: f32) {
        let info = ParamInfo::new(addr, name, Range::linear(FADER_DB.0, FADER_DB.1, Unit::Db))
            .in_group(group)
            .short("Gain")
            .with_default_real(0.0);
        self.push(info, target, f64::from(db));
    }

    fn toggle(&mut self, addr: String, name: String, group: &str, target: Target, on: bool) {
        let info = ParamInfo::new(addr, name, Range::default()).in_group(group).toggle();
        self.push(info, target, if on { 1.0 } else { 0.0 });
    }

    /// A macro at `prefix/<key>`; the default is the value it has now (the
    /// rig doesn't publish defaults), or centre for an offset.
    fn macros(&mut self, prefix: &str, group: &str, macros: &[KeysMacro], featured: bool, target: impl Fn(&KeysMacro) -> Target) {
        for m in macros {
            let range = macro_range(m);
            let mut info = ParamInfo::new(format!("{prefix}/{}", strip_level(&m.id)), format!("{} {}", m.group, m.name), range)
                .in_group(group)
                .short(m.name.clone());
            if m.bipolar {
                info = info.bipolar();
                info.default = 0.5;
            } else {
                info.default = info.range.to_normalized(f64::from(m.value));
                if m.id.ends_with(".bypass") && info.range.steps.is_none() && (m.max - m.min - 1.0).abs() < f32::EPSILON {
                    info = info.toggle();
                }
            }
            info.featured = featured;
            self.push(info, target(m), f64::from(m.value));
        }
    }
}

/// Everything the keys rig exposes, as a surface model.
#[must_use]
pub fn keys_model(snap: &KeysSnapshot) -> KeysModel {
    let mut b = Builder { params: Vec::new(), targets: BTreeMap::new(), values: BTreeMap::new() };
    let mut meters = BTreeMap::new();
    let mut streams = vec![StreamInfo {
        id: EntryId::new("master_level"),
        name: "Master".into(),
        kind: StreamKind::Level { channels: 1 },
        rate_hz: 30.0,
    }];
    let mut meter = |kind: &str, name: &str, path: &str, label: String| {
        let id = EntryId::new(format!("{path}/level"));
        meters.insert((kind.to_owned(), name.to_owned()), id.clone());
        streams.push(StreamInfo { id, name: label, kind: StreamKind::Level { channels: 1 }, rate_hz: 30.0 });
    };

    // Rig level: the Global Controls over the whole profile + the master.
    b.macros("rig", "Rig", &snap.rig_macros, true, |m| Target::RigGlobal(m.id.clone()));
    let master = ParamInfo::new("master/gain_db", "Master", Range::linear(FADER_DB.0, FADER_DB.1, Unit::Db))
        .in_group("Master")
        .with_default_real(0.0)
        .featured();
    b.push(master, Target::MasterGain, f64::from(snap.mixer.master_db));
    let mode = ParamInfo::new("perform/mode", "Perform Mode", Range::default()).in_group("Perform").choice(&PERFORM_MODES);
    let mode_n = (f64::from(snap.perform.perform_mode) / (PERFORM_MODES.len() - 1) as f64).clamp(0.0, 1.0);
    b.push(mode, Target::PerformMode, mode_n);

    for e in &snap.mixer.engines {
        let en = &e.name;
        b.gain(format!("{en}/gain_db"), format!("{en} Gain"), en, Target::EngineGain(en.clone()), e.gain_db);
        b.toggle(format!("{en}/mute"), format!("{en} Mute"), en, Target::EngineMute(en.clone()), e.muted);
        if let Some(d) = snap.engines.get(en) {
            b.macros(en, en, &d.macros, false, |m| Target::EngineGlobal { engine: en.clone(), id: m.id.clone() });
        }
        meter("engine", en, en, en.clone());
        for l in &e.layers {
            let ln = &l.name;
            let path = format!("{en}/{ln}");
            let group = path.clone();
            b.gain(format!("{path}/gain_db"), format!("{ln} Gain"), &group, Target::LayerGain(ln.clone()), l.gain_db);
            b.toggle(format!("{path}/mute"), format!("{ln} Mute"), &group, Target::LayerMute(ln.clone()), l.muted);
            b.toggle(format!("{path}/solo"), format!("{ln} Solo"), &group, Target::LayerSolo(ln.clone()), l.soloed);
            b.toggle(
                format!("{path}/exclude_global"),
                format!("{ln} Exclude Globals"),
                &group,
                Target::LayerExcludeGlobal(ln.clone()),
                l.exclude_global,
            );
            if let Some(ms) = snap.layer_macros.get(ln) {
                b.macros(&path, &group, ms, false, |m| Target::LayerGlobal { layer: ln.clone(), id: m.id.clone() });
            }
            meter("layer", ln, &path, ln.clone());
            for m in l.modules.iter().filter(|m| !m.patch.is_empty()) {
                let mpath = format!("{path}/{}", m.slot);
                let module = m.index;
                b.gain(format!("{mpath}/gain_db"), format!("{ln} {} Gain", m.slot), &mpath, Target::ModuleGain { layer: ln.clone(), module }, m.gain_db);
                b.toggle(format!("{mpath}/on"), format!("{ln} {} On", m.slot), &mpath, Target::ModuleOn { layer: ln.clone(), module }, m.enabled);
                if let Some(ms) = snap.module_macros.get(&(ln.clone(), module)) {
                    b.macros(&mpath, &mpath, ms, false, |x| Target::ModuleMacro { layer: ln.clone(), module, id: x.id.clone() });
                }
                meter("module", &format!("{ln} {}", m.slot), &mpath, format!("{ln} {}", m.slot));
            }
        }
    }
    streams.push(StreamInfo {
        id: EntryId::new("voices"),
        name: "Voices".into(),
        kind: StreamKind::Scalar { range: Range::linear(0.0, 64.0, Unit::None) },
        rate_hz: 30.0,
    });

    let action = |id: String, name: &str, group: &str| ActionInfo { id: EntryId::new(id), name: name.into(), group: Some(group.into()) };
    let mut actions = vec![action("start".into(), "Start", "Rig"), action("stop".into(), "Stop", "Rig")];
    for (i, s) in snap.perform.stacks.iter().enumerate() {
        actions.push(action(format!("stack/{i}"), &s.name, "Stacks"));
    }

    KeysModel {
        catalog: Catalog {
            id: "signal.keys".into(),
            version: 1,
            params: b.params,
            actions,
            streams,
            collections: vec![CollectionInfo { id: EntryId::new("presets"), name: title("presets") }],
        },
        targets: b.targets,
        values: b.values,
        meters,
    }
}

/// One call on the keys rig.
#[derive(Debug, Clone, PartialEq)]
pub enum KeysCall {
    RigGlobal { id: String, value: f32 },
    EngineGlobal { engine: String, id: String, value: f32 },
    LayerGlobal { layer: String, id: String, value: f32 },
    LayerMacro { layer: String, module: u32, id: String, value: f32 },
    MasterGain(f32),
    EngineGain { engine: String, db: f32 },
    EngineMute { engine: String, muted: bool },
    LayerGain { layer: String, db: f32 },
    LayerMute { layer: String, muted: bool },
    LayerSolo { layer: String, soloed: bool },
    LayerExcludeGlobal { layer: String, excluded: bool },
    ModuleGain { layer: String, module: u32, db: f32 },
    ModuleEnabled { layer: String, module: u32, on: bool },
    PerformMode(u32),
    Start,
    Stop,
    Stack(u32),
    Preset(u32),
}

/// A normalized set → the rig call that performs it.
///
/// # Errors
/// Unknown address.
pub fn route_param(model: &KeysModel, addr: &ParamAddr, value: f64) -> Result<KeysCall, SurfaceError> {
    let unknown = || SurfaceError::UnknownParam(addr.0.clone());
    let info = model.catalog.param(addr).ok_or_else(unknown)?;
    let target = model.targets.get(addr).ok_or_else(unknown)?;
    let real = info.range.to_real(value) as f32;
    let on = value >= 0.5;
    Ok(match target.clone() {
        Target::RigGlobal(id) => KeysCall::RigGlobal { id, value: real },
        Target::EngineGlobal { engine, id } => KeysCall::EngineGlobal { engine, id, value: real },
        Target::LayerGlobal { layer, id } => KeysCall::LayerGlobal { layer, id, value: real },
        Target::ModuleMacro { layer, module, id } => KeysCall::LayerMacro { layer, module, id, value: real },
        Target::MasterGain => KeysCall::MasterGain(real),
        Target::EngineGain(engine) => KeysCall::EngineGain { engine, db: real },
        Target::EngineMute(engine) => KeysCall::EngineMute { engine, muted: on },
        Target::LayerGain(layer) => KeysCall::LayerGain { layer, db: real },
        Target::LayerMute(layer) => KeysCall::LayerMute { layer, muted: on },
        Target::LayerSolo(layer) => KeysCall::LayerSolo { layer, soloed: on },
        Target::LayerExcludeGlobal(layer) => KeysCall::LayerExcludeGlobal { layer, excluded: on },
        Target::ModuleGain { layer, module } => KeysCall::ModuleGain { layer, module, db: real },
        Target::ModuleOn { layer, module } => KeysCall::ModuleEnabled { layer, module, on },
        Target::PerformMode => {
            let n = match &info.kind {
                ParamKind::Choice { options } => options.len().max(1),
                _ => 1,
            };
            KeysCall::PerformMode((value.clamp(0.0, 1.0) * (n - 1) as f64).round() as u32)
        }
    })
}

/// # Errors
/// Unknown action.
pub fn route_action(action: &EntryId) -> Result<KeysCall, SurfaceError> {
    Ok(match action.0.as_str() {
        "start" => KeysCall::Start,
        "stop" => KeysCall::Stop,
        other => match other.strip_prefix("stack/").and_then(|i| i.parse().ok()) {
            Some(i) => KeysCall::Stack(i),
            None => return Err(SurfaceError::UnknownAction(other.into())),
        },
    })
}

/// # Errors
/// Unknown collection.
pub fn route_select(collection: &EntryId, index: u32) -> Result<KeysCall, SurfaceError> {
    match collection.0.as_str() {
        "presets" => Ok(KeysCall::Preset(index)),
        other => Err(SurfaceError::UnknownCollection(other.into())),
    }
}

/// Telemetry frames for a status update: the master, each known meter, voices.
#[must_use]
pub fn status_frames(s: &KeysStatus, meters: &BTreeMap<(String, String), EntryId>) -> Vec<StreamFrame> {
    let mut out = vec![StreamFrame { id: EntryId::new("master_level"), values: vec![db(s.master_peak)] }];
    for m in &s.meters {
        if let Some(id) = meters.get(&(m.kind.clone(), m.name.clone())) {
            out.push(StreamFrame { id: id.clone(), values: vec![db(m.peak)] });
        }
    }
    out.push(StreamFrame { id: EntryId::new("voices"), values: vec![f64::from(s.voices)] });
    out
}

/// Fold a freshly built model into the live one.
///
/// Params that kept their shape keep their original default (the reset
/// target is what they were first seen at), and the version bumps only when
/// the shape changed.
#[must_use]
pub fn merge_model(old: &KeysModel, mut new: KeysModel) -> KeysModel {
    for p in &mut new.catalog.params {
        if let Some(prev) = old.catalog.param(&p.addr)
            && prev.range == p.range
            && prev.kind == p.kind
        {
            p.default = prev.default;
        }
    }
    new.catalog.version = old.catalog.version;
    if new.catalog != old.catalog {
        new.catalog.version += 1;
    }
    new
}

// ── the backend ──────────────────────────────────────────────────────────

/// The keys rig as a frame `SurfaceEngine`. Build with [`KeysSurface::connect`];
/// mount with `router.merge_router_scoped("keys", surface.into_router())`.
#[derive(Clone, HasDispatcher)]
#[dispatch(CurrentThreadDispatcher)]
pub struct KeysSurface {
    model: Arc<Mutex<KeysModel>>,
    calls: tokio::sync::mpsc::UnboundedSender<KeysCall>,
    changes: PubSub<ParamValue>,
    telemetry: PubSub<StreamFrame>,
}

impl KeysSurface {
    /// Attach to the keys rig over `local` (the engine's in-process server):
    /// snapshot it, then follow its events and forward surface edits to it.
    /// Must run inside a tokio runtime.
    ///
    /// # Errors
    /// When the rig's services can't be reached.
    pub async fn connect(local: &architect::LocalServer) -> Result<Self, String> {
        let rig: KeysRigClient = local.establish().await.map_err(|e| format!("keys client: {e}"))?;
        let reader: KeysRigClient = local.establish().await.map_err(|e| format!("keys reader client: {e}"))?;
        let stream: KeysRigStreamClient = local.establish().await.map_err(|e| format!("keys stream client: {e}"))?;
        let model = keys_model(&KeysSnapshot::fetch(&reader).await?);
        let (calls, call_rx) = tokio::sync::mpsc::unbounded_channel();
        let this = Self { model: Arc::new(Mutex::new(model)), calls, changes: PubSub::sliding(256), telemetry: PubSub::sliding(64) };
        let (dirty, dirty_rx) = tokio::sync::watch::channel(());
        architect::platform::spawn(run_calls(rig, call_rx));
        architect::platform::spawn(this.clone().refresher(reader, dirty_rx));
        architect::platform::spawn(this.clone().follow(stream, dirty));
        Ok(this)
    }

    /// Mirror rig events: meters straight to telemetry; anything structural
    /// marks the snapshot dirty (the refresher coalesces bursts).
    async fn follow(self, stream: KeysRigStreamClient, dirty: tokio::sync::watch::Sender<()>) {
        let (tx, mut rx) = vox::channel::<KeysEvent>();
        let call = stream.events(tx);
        let pump = async {
            while let Ok(Some(event)) = rx.recv().await {
                let _ = event.map(|ev| self.on_event(&ev, &dirty));
            }
        };
        tokio::select! {
            () = pump => {}
            _ = call => {}
        }
    }

    fn on_event(&self, ev: &KeysEvent, dirty: &tokio::sync::watch::Sender<()>) {
        match ev {
            KeysEvent::Status(s) => {
                let frames = status_frames(s, &lock(&self.model).meters);
                for f in frames {
                    self.telemetry.publish(f);
                }
            }
            KeysEvent::Mixer(_) | KeysEvent::Tree(_) | KeysEvent::Perform(_) | KeysEvent::Library(_) => {
                dirty.send_replace(());
            }
            KeysEvent::Midi(_) => {}
        }
    }

    /// Re-read the rig whenever it changed; publish what moved.
    async fn refresher(self, reader: KeysRigClient, mut dirty: tokio::sync::watch::Receiver<()>) {
        while dirty.changed().await.is_ok() {
            dirty.mark_unchanged();
            match KeysSnapshot::fetch(&reader).await {
                Ok(snap) => self.apply(keys_model(&snap)),
                Err(error) => tracing::warn!(%error, "keys surface: rig snapshot failed"),
            }
        }
    }

    fn apply(&self, fresh: KeysModel) {
        let changed: Vec<ParamValue> = {
            let mut st = lock(&self.model);
            let next = merge_model(&st, fresh);
            let changed = next
                .values
                .iter()
                .filter(|(a, v)| st.values.get(*a).is_none_or(|old| (old - **v).abs() > 1e-9))
                .map(|(a, v)| ParamValue { addr: a.clone(), value: *v })
                .collect();
            *st = next;
            changed
        };
        for v in changed {
            self.changes.publish(v);
        }
    }

    fn send(&self, call: KeysCall) {
        wide::set("surface.call", format!("{call:?}").split([' ', '(']).next().unwrap_or_default().to_string());
        let queued = self.calls.send(call).is_ok();
        wide::set("surface.outcome", if queued { "queued" } else { "rig_gone" });
    }
}

/// Execute rig calls in order (one task owns the command client).
async fn run_calls(rig: KeysRigClient, mut rx: tokio::sync::mpsc::UnboundedReceiver<KeysCall>) {
    while let Some(call) = rx.recv().await {
        let r = match call.clone() {
            KeysCall::RigGlobal { id, value } => rig.set_rig_global(id, value).await,
            KeysCall::EngineGlobal { engine, id, value } => rig.set_engine_global(engine, id, value).await,
            KeysCall::LayerGlobal { layer, id, value } => rig.set_layer_global(layer, id, value).await,
            KeysCall::LayerMacro { layer, module, id, value } => rig.set_layer_macro(layer, module, id, value).await,
            KeysCall::MasterGain(db) => rig.set_master_gain(db).await,
            KeysCall::EngineGain { engine, db } => rig.set_engine_gain(engine, db).await,
            KeysCall::EngineMute { engine, muted } => rig.set_engine_mute(engine, muted).await,
            KeysCall::LayerGain { layer, db } => rig.set_layer_gain(layer, db).await,
            KeysCall::LayerMute { layer, muted } => rig.set_layer_mute(layer, muted).await,
            KeysCall::LayerSolo { layer, soloed } => rig.set_layer_solo(layer, soloed).await,
            KeysCall::LayerExcludeGlobal { layer, excluded } => rig.set_layer_exclude_global(layer, excluded).await,
            KeysCall::ModuleGain { layer, module, db } => rig.set_module_gain(layer, module, db).await,
            KeysCall::ModuleEnabled { layer, module, on } => rig.set_module_enabled(layer, module, on).await,
            KeysCall::PerformMode(m) => rig.set_perform_mode(m).await,
            KeysCall::Start => rig.start().await,
            KeysCall::Stop => rig.stop().await,
            KeysCall::Stack(i) => rig.press_stack(i).await,
            KeysCall::Preset(i) => rig.load_preset(i).await,
        };
        if let Err(e) = r {
            tracing::warn!(?call, error = ?e, "keys surface: rig call failed");
        }
    }
}

impl SurfaceEngine for KeysSurface {
    fn catalog(&self) -> Catalog {
        lock(&self.model).catalog.clone()
    }

    fn values(&self) -> Vec<ParamValue> {
        lock(&self.model).values.iter().map(|(addr, value)| ParamValue { addr: addr.clone(), value: *value }).collect()
    }

    fn set_param(&self, addr: ParamAddr, value: f64, _phase: EditPhase) -> Result<(), SurfaceError> {
        wide::set("surface.rig", "keys");
        wide::set("surface.addr", addr.0.clone());
        let call = {
            let mut st = lock(&self.model);
            let call = route_param(&st, &addr, value)?;
            // Optimistic: echo immediately; the rig's next event confirms.
            st.values.insert(addr, value);
            call
        };
        self.send(call);
        Ok(())
    }

    fn invoke(&self, action: EntryId) -> Result<(), SurfaceError> {
        wide::set("surface.rig", "keys");
        wide::set("surface.action", action.0.clone());
        self.send(route_action(&action)?);
        Ok(())
    }

    fn select(&self, collection: EntryId, index: u32) -> Result<(), SurfaceError> {
        wide::set("surface.rig", "keys");
        wide::set("surface.collection", collection.0.clone());
        self.send(route_select(&collection, index)?);
        Ok(())
    }
}

impl SurfaceEngineStreamSource for KeysSurface {
    fn param_changes_hub(&self) -> &PubSub<ParamValue> {
        &self.changes
    }

    fn telemetry_hub(&self) -> &PubSub<StreamFrame> {
        &self.telemetry
    }
}

impl Services for KeysSurface {
    fn layers() -> impl Layer<Self> {
        layers![surface_proto::engine::Service, surface_proto::engine::StreamService]
    }
}

#[cfg(test)]
mod tests;
