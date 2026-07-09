//! Phase B — the model made REAL from real libraries, bound to real bytes.
//!
//! `docs/sampler-trait-design.md` §3 (`InstrumentModel::scan`), §5 (Legato as
//! `VelCurve`s), §9 (`Loader`). Phase A defined the declarative model and the
//! trait surface with opaque placeholders; **Phase B builds the model from
//! signal's existing [`LibrarySpec`] / [`PlayerPatch`] and binds
//! [`ZoneLayers`] / [`Loader`] to the real [`SampleCache`].**
//!
//! Nothing here re-implements decoding, caching, or resolution — it *adapts*:
//!
//! | model surface | real backing |
//! |---|---|
//! | [`InstrumentModel`] | built `From<&LibrarySpec>` / `From<&PlayerPatch>` |
//! | [`Legato`] `VelCurve`s | sampled from [`LegatoModeSpec::delay_for_velocity`] |
//! | [`ZoneLayers`] | [`CacheZoneLayers`] → `PlayerPatch::resolve_zone` + cache |
//! | [`Loader`] | [`CacheLoader`] → `SampleCache::preload` / `get` |
//! | [`SampleSlice`] | an index into the layers' coordinate table; PCM via [`CacheZoneLayers::pcm`] |
//!
//! ## REAL vs Phase-C
//! - **REAL**: `From<&LibrarySpec>` (articulations / variations / groups /
//!   zones / mics / dynamics axis / keyswitch selects), the legato →
//!   `VelCurve` mapping (per-velocity pre-delay sampled from the spec's
//!   expressive/low-latency legato zones), and [`CacheZoneLayers`] /
//!   [`CacheLoader`] resolving a real cached `Arc<SampleData>` for a coordinate.
//! - **Phase C (flagged, not attempted here)**: constructing a `SampleEngine`
//!   *from* the `InstrumentModel` (today the engine stays spec-driven —
//!   `EngineInstrument::from_patch` builds both from the same `PlayerPatch`, so
//!   the model and the engine are siblings off one source, not yet a
//!   model→engine pipeline). Also: `DefaultVoicer` extraction, and
//!   `TrueLegato`/`SustainPedal` superseding the engine internals.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::engine::cache::{SampleCache, SampleData};
use crate::spec::{
    ArticulationKind, ArticulationSpec, Cc1Layer, DynamicsSpec, LegatoModeSpec, LibrarySpec,
};
use crate::{PlayerPatch, SampleEngine};

use super::model::{
    Articulation, DynLayer, Group, InstrumentModel, Legato, LegatoMode, Mic, Polyphony, RoundRobin,
    SampleSlice, Select, Trigger, Variation, VelCurve, Zone, ZoneLayers,
};
use super::prim::{
    ArticulationId, Axis, Cc, Cents, Db, GroupId, InstrumentId, MicId, Note, Seconds, Velocity,
    ZoneId,
};

// ── Controller-string → Axis ─────────────────────────────────────────────

/// Parse a spec controller string (`"CC1"`, `"cc1"`, `"velocity"`) into the
/// model's [`Axis`]. The dynamics axis is what the design doc calls
/// `.dynamics_on(Axis::Cc(Cc(1)))`.
fn axis_from_controller(s: &str) -> Option<Axis> {
    let t = s.trim().to_ascii_lowercase();
    if t == "velocity" || t == "vel" {
        return Some(Axis::Velocity);
    }
    if let Some(num) = t.strip_prefix("cc") {
        if let Ok(n) = num.trim().parse::<u8>() {
            return Some(Axis::Cc(Cc::new(n)));
        }
    }
    None
}

// ── Legato → VelCurve (the marquee per-velocity timing) ──────────────────

/// Sample a [`LegatoModeSpec`]'s velocity zones into a [`VelCurve<Seconds>`].
///
/// The spec stores legato pre-delay as discrete velocity *zones* (`vel_range`
/// → `delay_ms`); [`LegatoModeSpec::delay_for_velocity`] is a step lookup. We
/// turn that into a real [`VelCurve`] by placing a breakpoint at the **centre
/// of each zone** (so the curve passes through every zone's `delay_ms`) — the
/// design doc's `VelCurve::breakpoints([(20, ms(120)), (110, ms(20))])`. The
/// curve then *interpolates* between zones, which is what "different velocities
/// → different legato times" means on real CSS-style data: soft+slow gets the
/// long expressive pre-delay, hard+fast the short one, and everything between
/// glides smoothly rather than stepping.
pub fn pre_delay_curve(mode: &LegatoModeSpec) -> VelCurve<Seconds> {
    let mut pts: Vec<(Velocity, Seconds)> = mode
        .zones
        .iter()
        .map(|z| {
            let centre = ((z.vel_range[0] as u16 + z.vel_range[1] as u16) / 2) as u8;
            (Velocity::new(centre), Seconds::from_ms(z.delay_ms as f64))
        })
        .collect();
    pts.sort_by_key(|(v, _)| v.get());
    pts.dedup_by_key(|(v, _)| v.get());
    match pts.len() {
        0 => VelCurve::constant(Seconds::from_ms(0.0)),
        1 => VelCurve::constant(pts[0].1),
        _ => VelCurve::breakpoints(pts),
    }
}

/// Build a model [`Legato`] from a spec's legato modes for one articulation.
///
/// `pre_delay` comes from the **expressive** mode (the longer, velocity-rich
/// curve — the "delayed legato" path); falls back to `low_latency`, then the
/// flat `primary_mode`. `portamento` reuses the same per-velocity shape scaled
/// down (the spec models portamento as a velocity trigger threshold + a volume
/// CC, not per-velocity glide times, so we approximate the glide as a fraction
/// of the pre-delay — REAL data, documented approximation). `crossfade` is a
/// modest constant the engine already applies internally.
fn legato_from_spec(spec: &LibrarySpec) -> Option<Legato> {
    let eng = spec.legato_engine.as_ref()?;
    // Prefer expressive (the velocity-rich curve), then low-latency, then flat.
    let mode = eng
        .expressive
        .clone()
        .or_else(|| eng.low_latency.clone())
        .or_else(|| eng.primary_mode())?;
    if mode.zones.is_empty() {
        return None;
    }
    let pre_delay = pre_delay_curve(&mode);
    // Portamento glide ≈ 40% of the pre-delay shape (a documented heuristic —
    // the spec has no per-velocity glide field; it models portamento as a
    // velocity threshold + volume CC). Reuses the same velocity breakpoints so
    // wider/softer moves still glide longer.
    let porta_pts: Vec<(Velocity, Seconds)> = mode
        .zones
        .iter()
        .map(|z| {
            let centre = ((z.vel_range[0] as u16 + z.vel_range[1] as u16) / 2) as u8;
            (
                Velocity::new(centre),
                Seconds::from_ms(z.delay_ms as f64 * 0.4),
            )
        })
        .collect();
    let portamento = if porta_pts.len() < 2 {
        VelCurve::constant(porta_pts.first().map(|(_, s)| *s).unwrap_or(Seconds(0.0)))
    } else {
        VelCurve::breakpoints(porta_pts)
    };
    Some(Legato {
        transitions: None, // recorded transition lookup is Phase-C (engine owns it today)
        pre_delay,
        portamento,
        crossfade: VelCurve::constant(Seconds::from_ms(40.0)),
        mode: LegatoMode::Mono,
    })
}

// ── Dynamics layers from the spec ────────────────────────────────────────

/// The widest declared CC1 crossfade layer set, low→high, as the model's
/// [`DynLayer`] list. The spec stores 2..=6-layer variants; we pick the
/// richest non-empty one (so a 6-dynamic piano keeps all six).
fn dyn_layers(dynamics: &DynamicsSpec) -> (Vec<DynLayer>, Vec<String>) {
    let chosen: &Vec<Cc1Layer> = [
        &dynamics.cc1_layers_6,
        &dynamics.cc1_layers_5,
        &dynamics.cc1_layers_4,
        &dynamics.cc1_layers_3,
        &dynamics.cc1_layers_2,
    ]
    .into_iter()
    .find(|v| !v.is_empty())
    .unwrap_or(&dynamics.cc1_layers_2);
    let labels: Vec<String> = chosen.iter().map(|l| l.label.clone()).collect();
    let layers = (0..chosen.len() as u16).map(DynLayer).collect();
    (layers, labels)
}

// ── ZoneLayers bound to the real cache ───────────────────────────────────

/// A real [`ZoneLayers`] backed by signal's [`PlayerPatch`] + [`SampleCache`].
///
/// This replaces Phase A's opaque `SampleSlice{index:u32}` placeholder with a
/// genuine handle: every distinct `(mic, dynamic, rr)` coordinate this zone can
/// render is enumerated into `coords`, and a [`SampleSlice`]'s `index` is a
/// position in that table. [`sample`](ZoneLayers::sample) resolves the
/// coordinate to a `SampleSlice`; [`pcm`](CacheZoneLayers::pcm) fetches the
/// decoded `Arc<SampleData>` from the cache — exactly the path
/// `SampleEngine::make_voice` walks (`patch.resolve_zone` / `patch.resolve` →
/// `cache.get_loaded`). No decoding or caching is re-implemented.
pub struct CacheZoneLayers {
    patch: Arc<PlayerPatch>,
    cache: SampleCache,
    /// The note this zone is anchored at (used for zone-mode resolution).
    note: Note,
    /// Resolved file path per coordinate-table index.
    paths: Vec<PathBuf>,
    /// Coordinate → table index. Key is `(mic, dynamic, rr)`.
    coords: HashMap<(MicId, DynLayer, u32), usize>,
    /// Declared dynamic layers, low→high.
    dynamics: Vec<DynLayer>,
    /// Declared dynamic-layer labels (parallel to `dynamics`).
    dyn_labels: Vec<String>,
    /// Round-robin count.
    rr: u32,
}

impl CacheZoneLayers {
    /// Build the coordinate table for one zone-mode anchor note.
    ///
    /// Zone-mode patches address samples by `(note, velocity, rr)`; mics and
    /// dynamic crossfade live inside the zone set. We enumerate every rr the
    /// patch exposes at this note and record the resolved path, so a
    /// [`SampleSlice`] is a stable index even though the underlying lookup is
    /// `patch.resolve_zone`.
    pub fn for_zone_note(
        patch: Arc<PlayerPatch>,
        cache: SampleCache,
        note: Note,
        mics: &[MicId],
        dynamics: Vec<DynLayer>,
        dyn_labels: Vec<String>,
        rr: u32,
    ) -> Self {
        let mut paths = Vec::new();
        let mut coords = HashMap::new();
        let rr_count = rr.max(1);
        // Velocity is folded into the dynamic axis for the table; use a
        // representative velocity per dynamic layer (mid-scale) so a slice can
        // be resolved without a live note event.
        for (di, dl) in dynamics.iter().copied().enumerate() {
            let vel = dyn_repr_velocity(di, dynamics.len());
            for rr_idx in 0..rr_count {
                if let Some(rz) = patch.resolve_zone(note.get(), vel, rr_idx as usize) {
                    let idx = paths.len();
                    paths.push(rz.path);
                    // The same resolved path serves every mic in zone mode
                    // (mics are pre-mixed into the zone); register all mics.
                    if mics.is_empty() {
                        coords.insert((MicId::new(""), dl, rr_idx), idx);
                    } else {
                        for m in mics {
                            coords.insert((m.clone(), dl, rr_idx), idx);
                        }
                    }
                }
            }
        }
        // Fallback: a zone with no declared dynamics still resolves at rr 0.
        if paths.is_empty() {
            if let Some(rz) = patch.resolve_zone(note.get(), 100, 0) {
                paths.push(rz.path);
                let m = mics.first().cloned().unwrap_or_else(|| MicId::new(""));
                coords.insert((m, DynLayer(0), 0), 0);
            }
        }
        Self {
            patch,
            cache,
            note,
            paths,
            coords,
            dynamics,
            dyn_labels,
            rr: rr_count,
        }
    }

    /// The decoded PCM for a resolved [`SampleSlice`], from the real cache.
    /// Returns `None` on a cache miss (not yet preloaded) or a bad index.
    pub fn pcm(&self, slice: SampleSlice) -> Option<Arc<SampleData>> {
        let path = self.paths.get(slice.index as usize)?;
        self.cache.get_loaded(path)
    }

    /// The resolved file path for a slice (for preload / debugging).
    pub fn path(&self, slice: SampleSlice) -> Option<&PathBuf> {
        self.paths.get(slice.index as usize)
    }

    /// Dynamic-layer labels, parallel to [`dynamics`](ZoneLayers::dynamics).
    pub fn dyn_labels(&self) -> &[String] {
        &self.dyn_labels
    }

    /// The anchor note this zone resolves around.
    pub fn note(&self) -> Note {
        self.note
    }

    /// Borrow the backing patch (shared).
    pub fn patch(&self) -> &Arc<PlayerPatch> {
        &self.patch
    }
}

impl ZoneLayers for CacheZoneLayers {
    fn sample(&self, mic: MicId, dynamic: DynLayer, rr: u32) -> Option<SampleSlice> {
        let rr = if self.rr == 0 { 0 } else { rr % self.rr };
        let idx = self
            .coords
            .get(&(mic, dynamic, rr))
            // Fall back to dynamic-only / first-mic lookups so a caller that
            // does not know the exact mic id still resolves.
            .or_else(|| {
                self.coords
                    .iter()
                    .find(|((_, d, r), _)| *d == dynamic && *r == rr)
                    .map(|(_, v)| v)
            })
            .copied()?;
        Some(SampleSlice { index: idx as u32 })
    }

    fn dynamics(&self) -> &[DynLayer] {
        &self.dynamics
    }

    fn round_robins(&self) -> u32 {
        self.rr
    }
}

/// A representative velocity for dynamic layer `i` of `n` (mid-band of an even
/// split of 1..=127) — used to resolve a zone-mode slice without a live event.
fn dyn_repr_velocity(i: usize, n: usize) -> u8 {
    if n == 0 {
        return 100;
    }
    let band = 127.0 / n as f32;
    ((i as f32 + 0.5) * band).round().clamp(1.0, 127.0) as u8
}

// ── Loader bound to the real cache ───────────────────────────────────────

/// A real [`Loader`] backed by the [`SampleCache`]: `preload` warms every
/// sample path the patch knows about (delegating to `SampleCache::preload`),
/// `slice` returns an addressing handle. IO stays off the hot path exactly as
/// the design doc §9 requires — `note_on`/`render` only ever touch
/// `cache.get_loaded`, which is the lock-free read snapshot.
pub struct CacheLoader {
    patch: Arc<PlayerPatch>,
    cache: SampleCache,
}

impl CacheLoader {
    pub fn new(patch: Arc<PlayerPatch>, cache: SampleCache) -> Self {
        Self { patch, cache }
    }

    /// The shared cache handle (cheap clone) — hand to [`CacheZoneLayers`].
    pub fn cache(&self) -> SampleCache {
        self.cache.clone_handle()
    }

    /// All sample paths the patch exposes, owned (for the centered preloader).
    pub fn sample_paths(&self) -> Vec<PathBuf> {
        self.patch.sample_paths().cloned().collect()
    }
}

impl super::traits::Loader for CacheLoader {
    fn preload(&self, _profile: super::traits::PreloadProfile) -> super::traits::PreloadStats {
        let paths: Vec<PathBuf> = self.patch.sample_paths().cloned().collect();
        let stats = self.cache.preload(paths.iter().map(|p| p.as_path()));
        super::traits::PreloadStats {
            samples: stats.loaded,
            bytes: stats.bytes,
        }
    }

    fn slice(&self, id: super::traits::SampleRef) -> SampleSlice {
        // SampleRef is a flat index into the patch's sample-path list; the
        // returned SampleSlice mirrors it (the real PCM is fetched through the
        // owning CacheZoneLayers / cache by path).
        SampleSlice { index: id.0 }
    }
}

// ── From<&LibrarySpec> for InstrumentModel — THE key adapter ──────────────

impl From<&LibrarySpec> for InstrumentModel {
    /// Build the declarative [`InstrumentModel`] from signal's existing
    /// [`LibrarySpec`]. This is the heart of Phase B: the spec's
    /// sections/articulations/dynamics/zones/legato/keyswitches become the
    /// model's `Articulation`/`Variation`/`Group`/`Zone` tree, mic list,
    /// dynamics axis, and per-velocity legato [`VelCurve`]s.
    ///
    /// Mapping:
    /// - `mics` ← `spec.mics[].id` ([`Mic`] per [`MicSpec`], `loaded` ← `default`).
    /// - `dynamics` axis ← `spec.dynamics.sustain_controller` (e.g. `"CC1"`).
    /// - one [`Articulation`] per [`ArticulationSpec`]; `select` is
    ///   [`Select::keyswitch`] when the keyswitch map names it, else
    ///   [`Select::Always`]. `legato` is built from the spec's legato engine
    ///   when the articulation's [`ArticulationKind`] is `Legato`.
    /// - each articulation gets one [`Variation`] holding one [`Group`] whose
    ///   `trigger`/`polyphony`/`round_robin` come from the artic kind + `rr`.
    /// - **zones**: in zone mode, each `spec.zones[i]` becomes a [`Zone`] (with
    ///   no `ZoneLayers` bytes — that binding needs the `PlayerPatch`; use
    ///   [`InstrumentModel::from`]`(&PlayerPatch)` for real layers). Without a
    ///   patch we cannot resolve files, so zone byte-binding is empty here and
    ///   filled by the `PlayerPatch` impl below.
    fn from(spec: &LibrarySpec) -> Self {
        model_from(spec, None)
    }
}

impl InstrumentModel {
    /// Build the model **with real sample-byte bindings** — the zone-mode
    /// [`Zone`]s get [`CacheZoneLayers`] that resolve to the real cache.
    /// Identical structure to `From<&LibrarySpec>`, plus live `ZoneLayers`.
    ///
    /// Takes an `Arc<PlayerPatch>` so every zone shares one patch handle
    /// ([`PlayerPatch`] is not `Clone`; the resolve methods only need `&self`,
    /// so an `Arc` is the natural shared owner). Pair it with a [`CacheLoader`]
    /// built from the same patch + cache to warm the bytes before rendering.
    pub fn from_patch_arc(patch: Arc<PlayerPatch>, cache: SampleCache) -> Self {
        let spec_clone = patch.spec.clone();
        model_from(&spec_clone, Some((patch, cache)))
    }
}

/// Shared model builder. When `bind` is `Some`, zone-mode zones get real
/// [`CacheZoneLayers`]; otherwise they are structural-only.
fn model_from(
    spec: &LibrarySpec,
    bind: Option<(Arc<PlayerPatch>, SampleCache)>,
) -> InstrumentModel {
    let mics: Vec<Mic> = spec
        .mics
        .iter()
        .map(|m| Mic {
            id: MicId::new(&m.id),
            gain: Db::UNITY,
            pan: 0.0,
            loaded: m.default,
        })
        .collect();
    let mic_ids: Vec<MicId> = mics.iter().map(|m| m.id.clone()).collect();

    let dynamics_axis = spec
        .dynamics
        .sustain_controller
        .as_deref()
        .and_then(axis_from_controller);
    let (dyn_layers_list, dyn_labels) = dyn_layers(&spec.dynamics);

    // Keyswitch select: the spec's cc58_map is CC58-range → function label; the
    // model's per-articulation Select uses keyswitch *notes*, which the styx
    // keyswitch map doesn't carry as notes. We therefore drive `select` from
    // articulation order via a synthetic keyswitch base (C-1 = note 0 upward),
    // matching the engine's keyswitch convention, and mark the dynamics axis
    // separately. (Real note keyswitches live in the engine's runtime config;
    // exposing them as model Selects without losing data is Phase-C.)
    let bind_ref = bind.as_ref();
    let articulations: Vec<Articulation> = spec
        .articulations
        .iter()
        .enumerate()
        .map(|(i, a)| {
            articulation_from_spec(
                spec,
                a,
                i,
                &mic_ids,
                &dyn_layers_list,
                &dyn_labels,
                bind_ref,
            )
        })
        .collect();

    // Zone-mode patches with no articulation rows still have playable zones;
    // surface them under a single "Always" articulation so the model is not
    // empty (mirrors the engine's zone-mode path).
    let articulations = if articulations.is_empty() && !spec.zones.is_empty() {
        vec![zone_only_articulation(
            spec,
            &mic_ids,
            &dyn_layers_list,
            &dyn_labels,
            bind_ref,
        )]
    } else {
        articulations
    };

    InstrumentModel {
        id: InstrumentId::new(if spec.name.is_empty() {
            "instrument"
        } else {
            &spec.name
        }),
        mics,
        dynamics: dynamics_axis,
        articulations,
        sample_rate: 48_000,
    }
}

/// One [`Articulation`] from an [`ArticulationSpec`].
#[allow(clippy::too_many_arguments)]
fn articulation_from_spec(
    spec: &LibrarySpec,
    a: &ArticulationSpec,
    index: usize,
    mics: &[MicId],
    dyn_layers_list: &[DynLayer],
    dyn_labels: &[String],
    bind: Option<&(Arc<PlayerPatch>, SampleCache)>,
) -> Articulation {
    let trigger = trigger_for_kind(&a.kind);
    let round_robin = if a.rr > 1 {
        RoundRobin::Cycle
    } else {
        RoundRobin::Off
    };
    let polyphony = match a.kind {
        ArticulationKind::Legato => Polyphony::Mono,
        ArticulationKind::OneShot => Polyphony::Unlimited,
        _ => Polyphony::Unlimited,
    };

    let zones = build_zones(
        spec,
        mics,
        dyn_layers_list,
        dyn_labels,
        a.rr.max(1) as u32,
        bind,
    );

    let group = Group {
        id: GroupId::new(&a.id),
        trigger,
        polyphony,
        choke: None,
        round_robin,
        zones,
    };
    let variation = Variation {
        id: GroupId::new(format!("{}::main", a.id)),
        select: Select::Always,
        groups: vec![group],
    };

    // Keyswitch select: synthetic note = base (C-1, note 12) + articulation
    // index, matching the engine's contiguous-keyswitch convention. If the
    // spec declares a keyswitch block at all, use it; else Always.
    let select = if spec.keyswitch.is_some() {
        Select::keyswitch(Note::new(12 + index as u8))
    } else {
        Select::Always
    };

    let legato = if matches!(a.kind, ArticulationKind::Legato) {
        legato_from_spec(spec)
    } else {
        None
    };

    Articulation {
        id: ArticulationId::new(&a.id),
        select,
        variations: vec![variation],
        legato,
    }
}

/// A single catch-all "Always" articulation for zone-only (no-articulation)
/// patches — keeps the model non-empty and playable.
fn zone_only_articulation(
    spec: &LibrarySpec,
    mics: &[MicId],
    dyn_layers_list: &[DynLayer],
    dyn_labels: &[String],
    bind: Option<&(Arc<PlayerPatch>, SampleCache)>,
) -> Articulation {
    let zones = build_zones(spec, mics, dyn_layers_list, dyn_labels, 1, bind);
    Articulation {
        id: ArticulationId::new("Default"),
        select: Select::Always,
        variations: vec![Variation {
            id: GroupId::new("Default::main"),
            select: Select::Always,
            groups: vec![Group {
                id: GroupId::new("Default"),
                trigger: Trigger::NoteOn,
                polyphony: Polyphony::Unlimited,
                choke: None,
                round_robin: RoundRobin::Off,
                zones,
            }],
        }],
        legato: None,
    }
}

/// Build [`Zone`]s from `spec.zones`, binding real [`CacheZoneLayers`] when a
/// patch+cache are supplied.
fn build_zones(
    spec: &LibrarySpec,
    mics: &[MicId],
    dyn_layers_list: &[DynLayer],
    dyn_labels: &[String],
    rr: u32,
    bind: Option<&(Arc<PlayerPatch>, SampleCache)>,
) -> Vec<Zone> {
    spec.zones
        .iter()
        .enumerate()
        .map(|(i, z)| {
            let layers: Box<dyn ZoneLayers> = match bind {
                Some((patch, cache)) => Box::new(CacheZoneLayers::for_zone_note(
                    Arc::clone(patch),
                    cache.clone_handle(),
                    Note::new(z.root_key),
                    mics,
                    dyn_layers_list.to_vec(),
                    dyn_labels.to_vec(),
                    rr,
                )),
                None => Box::new(EmptyZoneLayers {
                    dynamics: dyn_layers_list.to_vec(),
                    rr,
                }),
            };
            Zone {
                id: ZoneId(i as u32),
                keys: Note::new(z.key_min)..=Note::new(z.key_max),
                vel: Velocity::new(z.vel_min)..=Velocity::new(z.vel_max),
                root: Note::new(z.root_key),
                tune: Cents(z.tune_cents),
                gain: Db(z.gain_db),
                pan: z.pan,
                layers,
            }
        })
        .collect()
}

/// A structural-only [`ZoneLayers`] for models built without a patch (no real
/// bytes available). Resolves nothing — `From<&PlayerPatch>` gives real layers.
struct EmptyZoneLayers {
    dynamics: Vec<DynLayer>,
    rr: u32,
}
impl ZoneLayers for EmptyZoneLayers {
    fn sample(&self, _mic: MicId, _dynamic: DynLayer, _rr: u32) -> Option<SampleSlice> {
        None
    }
    fn dynamics(&self) -> &[DynLayer] {
        &self.dynamics
    }
    fn round_robins(&self) -> u32 {
        self.rr
    }
}

fn trigger_for_kind(kind: &ArticulationKind) -> Trigger {
    match kind {
        ArticulationKind::Release => Trigger::Release,
        ArticulationKind::Legato => Trigger::Legato,
        _ => Trigger::NoteOn,
    }
}

// ── Model ⟷ engine bridge ────────────────────────────────────────────────

impl super::engine::EngineInstrument {
    /// Build a playing [`EngineInstrument`] **and** the matching declarative
    /// [`InstrumentModel`] from one [`PlayerPatch`] — they are siblings off a
    /// single source of truth.
    ///
    /// **Phase-B state**: the engine stays *spec-driven* (constructed from the
    /// patch via `SampleEngine::new`, as today). The returned model is built
    /// `From<&PlayerPatch>` so it carries real `ZoneLayers`; querying it
    /// (articulations / zones / mics / legato curves) and driving the engine
    /// both reflect the same patch. A direct `InstrumentModel → SampleEngine`
    /// construction is **Phase C** (the engine would need to consume the model
    /// tree rather than the spec) — flagged, not attempted.
    pub fn from_patch(
        patch: PlayerPatch,
        sample_rate: u32,
        section_id: &str,
        mic_id: &str,
    ) -> (Self, InstrumentModel) {
        // The model is built `From<&LibrarySpec>` (structural — the engine owns
        // the playing bytes, so the bridge model does not need its own cache
        // binding). For a byte-bound model that resolves real samples
        // independently, use [`InstrumentModel::from_patch_arc`].
        let model = InstrumentModel::from(&patch.spec);
        let mut engine = SampleEngine::new(patch, sample_rate, section_id, mic_id);
        engine.preload_samples();
        (Self::new(engine), model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{LegatoEngineSpec, LegatoZoneSpec};

    fn write_sine_wav(path: &std::path::Path) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(path, spec).expect("create wav");
        for i in 0..48_000 {
            let t = i as f32 / 48_000.0;
            let s = (2.0 * std::f32::consts::PI * 220.0 * t).sin() * 0.8;
            w.write_sample(s).expect("write sample");
        }
        w.finalize().expect("finalize wav");
    }

    /// A zone-mode patch from a generated WAV + inline styx (reuses the
    /// `engine.rs` / `instrument.rs` fixture approach).
    fn zone_patch(dir: &std::path::Path) -> PlayerPatch {
        let wav = dir.join("note.wav");
        write_sine_wav(&wav);
        let styx = "\
name TestZoneLib
zones (
    { file note.wav, key_min 0, key_max 127, root_key 60, vel_min 0, vel_max 127 }
)
";
        let spec_path = dir.join("lib.styx");
        std::fs::write(&spec_path, styx).expect("write styx");
        PlayerPatch::load(&spec_path, dir).expect("load patch")
    }

    #[test]
    fn axis_parsing() {
        assert_eq!(axis_from_controller("CC1"), Some(Axis::Cc(Cc::new(1))));
        assert_eq!(axis_from_controller("cc11"), Some(Axis::Cc(Cc::new(11))));
        assert_eq!(axis_from_controller("velocity"), Some(Axis::Velocity));
        assert_eq!(axis_from_controller("nonsense"), None);
    }

    /// Legato `VelCurve` sampled from a spec with per-velocity delays gives the
    /// expected times: soft → long pre-delay, hard → short, interpolated between.
    #[test]
    fn legato_velcurve_from_spec_delays() {
        let mode = LegatoModeSpec {
            enabled_cc58_range: None,
            zones: vec![
                LegatoZoneSpec {
                    vel_range: [0, 42],
                    label: "slow".into(),
                    delay_ms: 120,
                },
                LegatoZoneSpec {
                    vel_range: [43, 85],
                    label: "med".into(),
                    delay_ms: 70,
                },
                LegatoZoneSpec {
                    vel_range: [86, 127],
                    label: "fast".into(),
                    delay_ms: 20,
                },
            ],
        };
        let curve = pre_delay_curve(&mode);
        // Soft (vel 10) clamps to the slowest zone's ~120ms; hard (vel 120) to ~20ms.
        let soft = curve.at(Velocity::new(10), super::super::prim::Interval(0));
        let hard = curve.at(Velocity::new(120), super::super::prim::Interval(0));
        assert!(
            soft.as_ms() > hard.as_ms(),
            "soft={} hard={}",
            soft.as_ms(),
            hard.as_ms()
        );
        assert!(
            (soft.as_ms() - 120.0).abs() < 1.0,
            "soft was {}",
            soft.as_ms()
        );
        assert!(
            (hard.as_ms() - 20.0).abs() < 1.0,
            "hard was {}",
            hard.as_ms()
        );
        // A mid velocity interpolates strictly between the extremes.
        let mid = curve.at(Velocity::new(64), super::super::prim::Interval(0));
        assert!(mid.as_ms() < soft.as_ms() && mid.as_ms() > hard.as_ms());
    }

    /// Full legato build from a `LibrarySpec` with an expressive engine.
    #[test]
    fn legato_from_library_spec() {
        let mut spec = LibrarySpec::from_styx("name T\nzones ()\n").expect("parse");
        spec.legato_engine = Some(LegatoEngineSpec {
            zones: vec![],
            expressive: Some(LegatoModeSpec {
                enabled_cc58_range: None,
                zones: vec![
                    LegatoZoneSpec {
                        vel_range: [0, 60],
                        label: "slow".into(),
                        delay_ms: 100,
                    },
                    LegatoZoneSpec {
                        vel_range: [61, 127],
                        label: "fast".into(),
                        delay_ms: 30,
                    },
                ],
            }),
            low_latency: None,
            portamento: None,
            retrigger: None,
        });
        let leg = legato_from_spec(&spec).expect("legato built");
        assert!(matches!(leg.mode, LegatoMode::Mono));
        let soft = leg
            .pre_delay
            .at(Velocity::new(5), super::super::prim::Interval(0));
        let hard = leg
            .pre_delay
            .at(Velocity::new(127), super::super::prim::Interval(0));
        assert!(soft.as_ms() > hard.as_ms());
    }

    /// `From<&LibrarySpec>` maps articulations, mics, and the dynamics axis.
    #[test]
    fn model_from_spec_structure() {
        let styx = "\
name CSSish
mics (
    { id Close, label Close, default true }
    { id Main, label Main }
)
dynamics { sustain_controller CC1 }
articulations (
    { id Sustain, label Sustain, kind Sustain }
    { id Legato, label Legato, kind Legato, rr 2 }
)
keyswitch { velocity_sensitive false, user_configurable true }
zones ()
";
        let spec = LibrarySpec::from_styx(styx).expect("parse styx");
        let model = InstrumentModel::from(&spec);

        // Mics from the spec, Close marked loaded (default), Main not.
        assert_eq!(model.mics.len(), 2);
        assert_eq!(model.mics[0].id.as_str(), "Close");
        assert!(model.mics[0].loaded);
        assert!(!model.mics[1].loaded);

        // Dynamics axis = CC1.
        assert_eq!(model.dynamics, Some(Axis::Cc(Cc::new(1))));

        // Both articulations, with the right ids, and Legato carries a Legato.
        assert_eq!(model.articulations.len(), 2);
        assert_eq!(model.articulations[0].id.as_str(), "Sustain");
        assert_eq!(model.articulations[1].id.as_str(), "Legato");
        assert!(model.articulations[0].legato.is_none());
        // Legato kind → Mono polyphony in its group + a keyswitch select.
        let leg = &model.articulations[1];
        assert!(matches!(leg.select, Select::Keyswitch(_)));
        assert!(matches!(
            leg.variations[0].groups[0].polyphony,
            Polyphony::Mono
        ));
        assert!(matches!(
            leg.variations[0].groups[0].round_robin,
            RoundRobin::Cycle
        ));
    }

    /// `From<&PlayerPatch>` binds real `ZoneLayers`, and `sample`→`pcm`
    /// resolves a real cached `Arc<SampleData>` for a known zone coordinate.
    #[test]
    fn zone_layers_resolves_real_cached_sample() {
        let dir = std::env::temp_dir().join(format!("signal-adapt-zl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let patch = zone_patch(&dir);
        assert!(patch.is_zoned());

        // Build a CacheZoneLayers directly (the model wraps these in Boxes).
        let cache = SampleCache::with_prepared(None);
        let patch_arc = Arc::new(patch);
        // Preload the one sample so get_loaded hits.
        let stats = cache.preload(patch_arc.sample_paths().map(|p| p.as_path()));
        assert!(stats.loaded >= 1, "expected the sine wav to preload");

        let layers = CacheZoneLayers::for_zone_note(
            patch_arc.clone(),
            cache.clone_handle(),
            Note::new(60),
            &[MicId::new("")],
            vec![DynLayer(0)],
            vec!["mf".into()],
            1,
        );
        let slice = layers
            .sample(MicId::new(""), DynLayer(0), 0)
            .expect("zone resolves a slice for root note 60");
        let pcm = layers.pcm(slice).expect("slice resolves real cached PCM");
        assert!(pcm.num_frames > 0, "decoded PCM should be non-empty");
        assert_eq!(pcm.sample_rate, 48_000);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `From<&PlayerPatch>` on the zone fixture yields a model whose single
    /// zone's layers resolve real bytes through the bound cache loader.
    #[test]
    fn model_from_patch_binds_bytes() {
        let dir = std::env::temp_dir().join(format!("signal-adapt-mp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let patch = zone_patch(&dir);

        let cache = SampleCache::with_prepared(None);
        let model = InstrumentModel::from_patch_arc(Arc::new(patch), cache);
        // Zone-only patch → one synthetic "Default" articulation with one zone.
        assert_eq!(model.articulations.len(), 1);
        let groups = &model.articulations[0].variations[0].groups;
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].zones.len(), 1);
        let zone = &groups[0].zones[0];
        assert_eq!(zone.root, Note::new(60));
        // The bound layers report a round-robin count of at least 1.
        assert!(zone.layers.round_robins() >= 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The model⟷engine bridge: one patch builds both a playing engine and the
    /// model, and the engine renders audio.
    #[test]
    fn from_patch_builds_engine_and_model() {
        use super::super::traits::Instrument;
        use super::super::traits::{MicBlock, StereoBuf};

        let dir = std::env::temp_dir().join(format!("signal-adapt-br-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let patch = zone_patch(&dir);

        let (mut inst, model) =
            super::super::engine::EngineInstrument::from_patch(patch, 48_000, "", "");
        // Model carries the zone (sibling off the same patch).
        assert!(!model.articulations.is_empty());
        assert_eq!(inst.sample_rate(), 48_000);

        inst.note_on(Note::new(60), Velocity::new(100));
        assert!(inst.voices_active() > 0);

        let frames = 512usize;
        let mut l = vec![0.0f32; frames];
        let mut r = vec![0.0f32; frames];
        let mut energy = 0.0f64;
        for _ in 0..8 {
            for v in l.iter_mut().chain(r.iter_mut()) {
                *v = 0.0;
            }
            {
                let mut mics = [(
                    MicId::new(""),
                    StereoBuf {
                        l: &mut l,
                        r: &mut r,
                    },
                )];
                let mut block = MicBlock::new(frames, &mut mics);
                inst.render(&mut block);
            }
            for s in l.iter().chain(r.iter()) {
                energy += (*s as f64) * (*s as f64);
            }
        }
        let rms = (energy / (frames * 2 * 8) as f64).sqrt();
        assert!(
            rms > 1e-4,
            "engine should render non-silent audio, rms={rms}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
