//! The headless Electronic Kit backend: pads over a sample space, each pad
//! a one-zone percussion `SampleEngine` on its own daw track.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use architect::dispatch::CurrentThreadDispatcher;
use architect::rig::RigBackend;
use architect::{HasDispatcher, Layer, PubSub, Services, layers};
use signal_ekit_proto::ekit::EkitRig;
use signal_ekit_proto::{EkitEvent, EkitStatus, Pad};
use signal_sampler::{PreloadProfile, SamplerRig};
use signal_space::{Space, knn};

pub const DEFAULT_ROWS: u32 = 4;
pub const DEFAULT_COLS: u32 = 4;

/// The classes a fresh 4×4 grid is seeded with (Atlas "Default" layout —
/// the bottom row is the backbone, upper rows fill out the kit).
const DEFAULT_LAYOUT: &[&str] = &[
    "kick",
    "snare",
    "hat-closed",
    "hat-open", // row 1
    "kick",
    "clap",
    "hat-closed",
    "cymbal", // row 2
    "tom",
    "snare",
    "perc",
    "cymbal", // row 3
    "tom",
    "perc",
    "fx",
    "fx", // row 4
];

struct LoadedSpace {
    space: Space,
    features: Vec<f32>,
}

#[derive(Default)]
struct State {
    pads: Vec<Pad>,
    space_name: String,
    last_error: String,
    /// Per-pad cursor into its similarity list (for stepping).
    cursors: Vec<i32>,
}

struct Inner {
    rig: Mutex<Option<SamplerRig>>,
    space: Mutex<Option<Arc<LoadedSpace>>>,
    state: Mutex<State>,
    events: PubSub<EkitEvent>,
    pump_started: AtomicBool,
    /// Deterministic re-roll counter (no RNG — reproducible kits).
    roll: std::sync::atomic::AtomicU32,
}

/// The Electronic Kit backend handle. Cheap to clone; every clone shares
/// one core.
#[derive(Clone, HasDispatcher)]
#[dispatch(CurrentThreadDispatcher)]
pub struct EkitBackend {
    inner: Arc<Inner>,
}

impl Default for EkitBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl EkitBackend {
    pub fn new() -> Self {
        let pads = (0..DEFAULT_ROWS * DEFAULT_COLS)
            .map(|i| Pad {
                index: i,
                category: DEFAULT_LAYOUT
                    .get(i as usize)
                    .copied()
                    .unwrap_or("perc")
                    .to_string(),
                gain_db: 0.0,
                pan: 0.0,
                pitch: 0.0,
                attack_ms: 0.0,
                release_ms: 0.0,
                ..Default::default()
            })
            .collect::<Vec<_>>();
        let cursors = vec![0; pads.len()];
        let backend = Self {
            inner: Arc::new(Inner {
                rig: Mutex::new(None),
                space: Mutex::new(None),
                state: Mutex::new(State {
                    pads,
                    cursors,
                    ..State::default()
                }),
                events: architect::rig::events_hub(),
                pump_started: AtomicBool::new(false),
                roll: std::sync::atomic::AtomicU32::new(0),
            }),
        };
        backend.spawn_meter_pump("ekit-meter-pump");
        backend
    }

    /// An offline rig (no audio device) for tests + the pad probe: every
    /// other path is identical, only `render_offline` pulls the blocks.
    pub fn new_offline(sample_rate: u32) -> Self {
        let b = Self::new();
        *b.inner.rig.lock().unwrap() = Some(SamplerRig::new_offline(sample_rate));
        b
    }

    /// Trigger `pad` and render ~0.4 s, returning the peak. Offline only.
    pub fn render_hit(&self, pad: u32, velocity: u32) -> f32 {
        // Note-on/off inline: `trigger` takes the same (non-reentrant) rig
        // lock this function holds across the render.
        let rig = self.inner.rig.lock().unwrap();
        let Some(rig) = rig.as_ref() else { return 0.0 };
        let note = crate::BASE_NOTE + pad as u8;
        let mut buf = vec![0.0f32; 512 * 2];
        // The sample cache warms on a background queue; render (and wait) a
        // little before the hit or the first strike lands on empty buffers.
        for _ in 0..30 {
            buf.iter_mut().for_each(|s| *s = 0.0);
            let _ = rig.render_offline(&mut buf);
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
        let id = format!("pad{pad}");
        rig.note_on_instrument(&id, note, velocity.min(127) as u8);
        let mut peak = 0.0f32;
        for _ in 0..40 {
            buf.iter_mut().for_each(|s| *s = 0.0);
            if rig.render_offline(&mut buf).is_err() {
                break;
            }
            for &s in buf.iter() {
                peak = peak.max(s.abs());
            }
        }
        rig.note_off_instrument(&id, note, 0);
        peak
    }

    pub fn router(&self) -> architect::LayerRouter {
        self.clone().into_router()
    }

    fn publish(&self) {
        // running first (rig lock), THEN state — never both at once.
        let running = self.inner.rig.lock().map(|r| r.is_some()).unwrap_or(false);
        let (pads, status) = {
            let s = self.inner.state.lock().unwrap();
            (s.pads.clone(), self.status_locked(&s, running))
        };
        self.inner.events.publish(EkitEvent::Status(status));
        self.inner.events.publish(EkitEvent::Pads(pads));
    }

    fn status_locked(&self, s: &State, running: bool) -> EkitStatus {
        EkitStatus {
            running,
            space: s.space_name.clone(),
            rows: DEFAULT_ROWS,
            cols: DEFAULT_COLS,
            last_error: s.last_error.clone(),
            base_note: crate::BASE_NOTE as u32,
        }
    }

    /// Open audio (idempotent).
    fn open_blocking(&self) {
        let mut open_error: Option<String> = None;
        {
            let mut rig = self.inner.rig.lock().unwrap();
            if rig.is_some() {
                return;
            }
            match SamplerRig::new() {
                Ok(r) => {
                    r.set_preload_profile(PreloadProfile::DrumKit);
                    *rig = Some(r);
                }
                Err(e) => {
                    tracing::error!("ekit: audio open failed: {e}");
                    open_error = Some(e.to_string());
                }
            }
        }
        if let Some(e) = open_error {
            self.inner.state.lock().unwrap().last_error = e;
        }
    }

    /// Locate + load a built space by name (shared, depth-bounded discovery).
    fn load_space(&self, name: &str) -> Option<Arc<LoadedSpace>> {
        if let Some(cur) = self.inner.space.lock().unwrap().as_ref() {
            if cur.space.name == name {
                return Some(cur.clone());
            }
        }
        let (_, space, features) = signal_space::find_space(name)?;
        let loaded = Arc::new(LoadedSpace { space, features });
        *self.inner.space.lock().unwrap() = Some(loaded.clone());
        Some(loaded)
    }

    /// Every item index in the space whose class matches `category`.
    fn candidates(space: &LoadedSpace, category: &str) -> Vec<usize> {
        space
            .space
            .items
            .iter()
            .enumerate()
            .filter(|(_, it)| it.class == category)
            .map(|(i, _)| i)
            .collect()
    }

    /// Install `item_idx` on `pad`, building a one-zone percussion engine
    /// for the resolved audio file.
    fn install(&self, pad_index: u32, item_idx: usize) {
        let Some(loaded) = self.inner.space.lock().unwrap().clone() else {
            return;
        };
        let Some(item) = loaded.space.items.get(item_idx) else {
            return;
        };
        let Some(path) = resolve_audio(&loaded.space, item_idx) else {
            tracing::warn!(path = %item.path, "ekit: no audio for item");
            return;
        };
        let (class, display) = (item.class.clone(), item.path.clone());
        // Lock discipline: NEVER hold `rig` and `state` at once — the meter
        // pump takes state→rig, so the reverse order here would deadlock.
        let pad_state = self
            .inner
            .state
            .lock()
            .unwrap()
            .pads
            .get(pad_index as usize)
            .cloned()
            .unwrap_or_default();
        let mut err: Option<String> = None;
        let installed = {
            let rig = self.inner.rig.lock().unwrap();
            let Some(rig) = rig.as_ref() else { return };
            let id = format!("pad{pad_index}");
            if let Err(e) = install_pad_engine(rig, &id, &path, &pad_state) {
                tracing::warn!("ekit: pad {pad_index} install failed: {e}");
                err = Some(e.to_string());
                None
            } else {
                Some(())
            }
        };
        if let Some(e) = err {
            self.inner.state.lock().unwrap().last_error = e;
            return;
        }
        if installed.is_none() {
            return;
        }
        let mut s = self.inner.state.lock().unwrap();
        if let Some(p) = s.pads.get_mut(pad_index as usize) {
            p.item_idx = item_idx as u32;
            p.path = display;
            p.space = loaded.space.name.clone();
            // Dropping a sample of a different class re-assigns the pad's
            // category (Atlas rule).
            if !p.category.is_empty() && p.category != class {
                p.category = class;
            }
            if !p.params_locked {
                p.gain_db = 0.0;
                p.pan = 0.0;
                p.pitch = 0.0;
            }
        }
    }

    /// Deterministic "random" pick from a candidate list.
    fn pick(&self, candidates: &[usize], salt: u32) -> Option<usize> {
        if candidates.is_empty() {
            return None;
        }
        let n = self
            .inner
            .roll
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut x = n
            .wrapping_mul(2654435761)
            .wrapping_add(salt.wrapping_mul(40503));
        x ^= x >> 13;
        x = x.wrapping_mul(1274126177);
        x ^= x >> 16;
        Some(candidates[(x as usize) % candidates.len()])
    }
}

/// Resolve a space item to a playable wav.
///
/// Sample items ARE a file. Piece items are directories holding every
/// round-robin / velocity / mic variant, so pick a representative strike:
/// the loudest velocity layer (`…VL8…` beats `…VL1…`), preferring shallow
/// paths so a close mic wins over a room/overhead subfolder. Picking the
/// middle file instead lands on whisper-quiet layers.
fn resolve_audio(space: &Space, idx: usize) -> Option<PathBuf> {
    let item = space.items.get(idx)?;
    let direct = std::path::Path::new(&space.root).join(&item.path);
    if direct.is_file() {
        return Some(direct);
    }
    let mut wavs: Vec<PathBuf> = Vec::new();
    let mut stack = vec![direct];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case("wav"))
            {
                wavs.push(p);
            }
        }
    }
    wavs.sort();
    wavs.into_iter().max_by_key(|p| {
        let name = p.to_string_lossy().to_lowercase();
        (velocity_rank(&name), -(p.components().count() as i64))
    })
}

/// Highest `vl<N>` / `v<N>` token in a path — the hardest sampled hit.
fn velocity_rank(name: &str) -> i64 {
    let mut best = 0i64;
    for tok in name.split(|c: char| !c.is_ascii_alphanumeric()) {
        for prefix in ["vl", "v"] {
            if let Some(rest) = tok.strip_prefix(prefix) {
                if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
                    best = best.max(rest.parse().unwrap_or(0));
                }
            }
        }
    }
    best
}

/// Build (or replace) the one-zone percussion engine backing a pad.
///
/// Bank-level (`load_instrument`) rather than the per-track `add_instrument`
/// so the same path works on an offline rig (tests, the pad probe) and a
/// live one. Pad gain / pan / pitch ride in the zone itself, so a param
/// change is just a cheap re-install of a one-zone spec.
fn install_pad_engine(
    rig: &SamplerRig,
    id: &str,
    path: &std::path::Path,
    pad: &Pad,
) -> Result<(), Box<dyn std::error::Error>> {
    // Every pad is its own instrument in one bank, so each zone must answer
    // ONLY its own note — a full-range zone would make one hit fire all 16.
    let note = crate::BASE_NOTE + pad.index as u8;
    let dir = path.parent().ok_or("sample has no parent dir")?;
    let file = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("bad sample name")?;
    // A minimal percussion library: one zone, whole keyboard, natural pitch.
    // `category drum` puts the engine in percussion mode, so the pad plays
    // the sample unpitched on whatever note it receives.
    let styx = format!(
        r#"
name "ekit {id}"
category drum
instrument perc
sections ( {{ id main, label Main, lowest_note "C-1", highest_note "G9" }} )
mics ( {{ id default, label Default, kind close }} )
articulations ( {{ id hit, label Hit, kind @Short, rr 1 }} )
zones (
    {{ file "{file}", key_min {note}, key_max {note}, root_key {note}, vel_min 0, vel_max 127, articulation hit, mic default, gain_db {gain}, pan {pan}, tune_cents {tune} }}
)
"#,
        note = note,
        gain = pad.gain_db,
        pan = pad.pan,
        tune = pad.pitch * 100.0,
    );
    let spec_path = std::env::temp_dir().join(format!("fts-ekit-{id}.styx"));
    std::fs::write(&spec_path, styx)?;
    rig.unload_instrument(id);
    rig.load_instrument(id.to_string(), &spec_path, Some(dir), "main", "default")?;
    // The pad grid drives GM drum channel 10; without this the bank only
    // hears channel 0 (a silent-pad trap we already paid for once).
    // No `set_midi_channel`: the bank maps a channel to exactly ONE
    // instrument, so 16 pads on one channel would collapse to the last
    // registered. The rig addresses each pad by instrument id instead.
    // Without an explicit preload the voice starts on an empty cache and
    // renders silence (the loose-wav trap; see loose_wav_level example).
    let _ = rig.preload_instrument(id);
    Ok(())
}

impl EkitRig for EkitBackend {
    fn start(&self) {
        let b = self.clone();
        let _ = std::thread::Builder::new()
            .name("ekit-open".into())
            .spawn(move || {
                b.open_blocking();
                b.publish();
            });
    }

    fn stop(&self) {
        *self.inner.rig.lock().unwrap() = None;
        self.publish();
    }

    fn status(&self) -> EkitStatus {
        let running = self.inner.rig.lock().map(|r| r.is_some()).unwrap_or(false);
        let s = self.inner.state.lock().unwrap();
        self.status_locked(&s, running)
    }

    fn pads(&self) -> Vec<Pad> {
        self.inner.state.lock().unwrap().pads.clone()
    }

    fn set_space(&self, space: String) {
        if self.load_space(&space).is_none() {
            let mut s = self.inner.state.lock().unwrap();
            s.last_error = format!("no space {space:?}");
            return;
        }
        {
            let mut s = self.inner.state.lock().unwrap();
            s.space_name = space;
            s.last_error.clear();
        }
        self.new_kit();
    }

    fn trigger(&self, pad: u32, velocity: u32) {
        let rig = self.inner.rig.lock().unwrap();
        let Some(rig) = rig.as_ref() else { return };
        let note = crate::BASE_NOTE + pad as u8;
        let id = format!("pad{pad}");
        if velocity == 0 {
            rig.note_off_instrument(&id, note, 0);
        } else {
            rig.note_on_instrument(&id, note, velocity.min(127) as u8);
        }
        if velocity > 0 {
            self.inner.events.publish(EkitEvent::Hit(pad));
        }
    }

    fn load_item(&self, pad: u32, item_idx: u32) {
        self.install(pad, item_idx as usize);
        self.publish();
    }

    fn randomize_pad(&self, pad: u32) {
        let Some(space) = self.inner.space.lock().unwrap().clone() else {
            return;
        };
        let category = {
            let s = self.inner.state.lock().unwrap();
            s.pads
                .get(pad as usize)
                .map(|p| p.category.clone())
                .unwrap_or_default()
        };
        let candidates = Self::candidates(&space, &category);
        if let Some(idx) = self.pick(&candidates, pad) {
            self.install(pad, idx);
            self.publish();
        }
    }

    fn step_similar(&self, pad: u32, delta: i32) {
        let Some(space) = self.inner.space.lock().unwrap().clone() else {
            return;
        };
        let (current, category, cursor) = {
            let s = self.inner.state.lock().unwrap();
            let p = s.pads.get(pad as usize);
            (
                p.map(|p| p.item_idx as usize).unwrap_or(0),
                p.map(|p| p.category.clone()).unwrap_or_default(),
                s.cursors.get(pad as usize).copied().unwrap_or(0),
            )
        };
        let hits = knn::similar(&space.features, space.space.dim, current, 32, |i| {
            space.space.items[i].class == category
        });
        if hits.is_empty() {
            return;
        }
        let next = (cursor + delta).rem_euclid(hits.len() as i32);
        {
            let mut s = self.inner.state.lock().unwrap();
            if let Some(c) = s.cursors.get_mut(pad as usize) {
                *c = next;
            }
        }
        self.install(pad, hits[next as usize].0);
        self.publish();
    }

    fn new_kit(&self) {
        let Some(space) = self.inner.space.lock().unwrap().clone() else {
            return;
        };
        let pads = self.inner.state.lock().unwrap().pads.clone();
        for p in pads {
            if p.locked {
                continue;
            }
            let candidates = Self::candidates(&space, &p.category);
            if let Some(idx) = self.pick(&candidates, p.index) {
                self.install(p.index, idx);
            }
        }
        self.publish();
    }

    fn morph_kit(&self, delta: i32) {
        let pads = self.inner.state.lock().unwrap().pads.clone();
        for p in pads {
            if !p.locked && !p.path.is_empty() {
                self.step_similar(p.index, delta);
            }
        }
        self.publish();
    }

    fn set_category(&self, pad: u32, category: String) {
        {
            let mut s = self.inner.state.lock().unwrap();
            if let Some(p) = s.pads.get_mut(pad as usize) {
                p.category = category;
            }
        }
        self.publish();
    }

    fn set_locked(&self, pad: u32, locked: bool) {
        {
            let mut s = self.inner.state.lock().unwrap();
            if let Some(p) = s.pads.get_mut(pad as usize) {
                p.locked = locked;
            }
        }
        self.publish();
    }

    fn set_params_locked(&self, pad: u32, locked: bool) {
        {
            let mut s = self.inner.state.lock().unwrap();
            if let Some(p) = s.pads.get_mut(pad as usize) {
                p.params_locked = locked;
            }
        }
        self.publish();
    }

    fn set_pad_param(&self, pad: u32, param: String, value: f32) {
        {
            let mut s = self.inner.state.lock().unwrap();
            let Some(p) = s.pads.get_mut(pad as usize) else {
                return;
            };
            match param.as_str() {
                "gain_db" => p.gain_db = value.clamp(-60.0, 12.0),
                "pan" => p.pan = value.clamp(-1.0, 1.0),
                "pitch" => p.pitch = value.clamp(-24.0, 24.0),
                "attack_ms" => p.attack_ms = value.max(0.0),
                "release_ms" => p.release_ms = value.max(0.0),
                "reverse" => p.reverse = value >= 0.5,
                "choke_group" => p.choke_group = value.clamp(0.0, 5.0) as u32,
                other => tracing::debug!(param = other, "ekit: unknown pad param"),
            }
        }
        self.apply_pad_mix(pad, true);
        self.publish();
    }

    fn set_muted(&self, pad: u32, muted: bool) {
        {
            let mut s = self.inner.state.lock().unwrap();
            if let Some(p) = s.pads.get_mut(pad as usize) {
                p.muted = muted;
            }
        }
        self.apply_pad_mix(pad, false);
        self.publish();
    }

    fn set_soloed(&self, pad: u32, soloed: bool) {
        {
            let mut s = self.inner.state.lock().unwrap();
            if let Some(p) = s.pads.get_mut(pad as usize) {
                p.soloed = soloed;
            }
        }
        for i in 0..self.inner.state.lock().unwrap().pads.len() as u32 {
            self.apply_pad_mix(i, false);
        }
        self.publish();
    }

    fn midi_ports(&self) -> Vec<String> {
        SamplerRig::midi_input_ports()
    }

    fn set_midi_port(&self, name: String) {
        let rig = self.inner.rig.lock().unwrap();
        if let Some(rig) = rig.as_ref() {
            let _ = rig.attach_midi(midicore::selector_for(Some(&name)));
        }
    }
}

impl EkitBackend {
    /// Apply a pad's mixer state. Mute/solo are bank-level; gain / pan /
    /// pitch live in the pad's zone, so they re-install the one-zone engine
    /// (cheap — one file, already in the sample cache).
    fn apply_pad_mix(&self, pad: u32, reinstall: bool) {
        let (p, any_solo) = {
            let s = self.inner.state.lock().unwrap();
            (
                s.pads.get(pad as usize).cloned(),
                s.pads.iter().any(|p| p.soloed),
            )
        };
        let Some(p) = p else { return };
        let id = format!("pad{pad}");
        let space = self.inner.space.lock().unwrap().clone();
        let rig = self.inner.rig.lock().unwrap();
        let Some(rig) = rig.as_ref() else { return };
        rig.set_muted(&id, p.muted || (any_solo && !p.soloed));
        if reinstall && !p.path.is_empty() {
            if let Some(space) = space {
                if let Some(path) = resolve_audio(&space.space, p.item_idx as usize) {
                    if let Err(e) = install_pad_engine(rig, &id, &path, &p) {
                        tracing::warn!("ekit: pad {pad} param re-install failed: {e}");
                    }
                }
            }
        }
    }
}

impl signal_ekit_proto::ekit::EkitRigStreamSource for EkitBackend {
    fn events_hub(&self) -> &PubSub<EkitEvent> {
        &self.inner.events
    }
}

impl RigBackend for EkitBackend {
    type Event = EkitEvent;
    type Tick = ();

    fn events_hub(&self) -> &PubSub<EkitEvent> {
        &self.inner.events
    }
    fn is_running(&self) -> bool {
        self.inner.rig.lock().map(|r| r.is_some()).unwrap_or(false)
    }
    fn pump_started(&self) -> &AtomicBool {
        &self.inner.pump_started
    }
    fn midi_ports(&self) -> Vec<String> {
        SamplerRig::midi_input_ports()
    }
    fn on_running_edge(&self, _running: bool) {
        self.publish();
    }
    fn on_running_tick(&self) {
        // Meter pump: pull the per-pad peaks off the rig and publish.
        let meters = {
            let rig = self.inner.rig.lock().unwrap();
            rig.as_ref().map(|r| r.meters_bank())
        }; // rig lock dropped before touching state
        let pads = {
            let mut s = self.inner.state.lock().unwrap();
            for (i, p) in s.pads.iter_mut().enumerate() {
                p.peak = meters
                    .as_ref()
                    .and_then(|m| m.cell(i))
                    .map(|c| c.peak(0).max(c.peak(1)))
                    .unwrap_or(0.0);
            }
            s.pads.clone()
        };
        self.inner.events.publish(EkitEvent::Pads(pads));
    }
}

impl signal_rigs_proto::rig_core::RigCore for EkitBackend {
    fn start(&self) {
        EkitRig::start(self);
    }
    fn stop(&self) {
        EkitRig::stop(self);
    }
    fn running(&self) -> bool {
        <Self as RigBackend>::is_running(self)
    }
    fn presets(&self) -> Vec<signal_rigs_proto::RigPresetInfo> {
        Vec::new()
    }
    fn load_preset(&self, _index: u32) {}
    fn midi_ports(&self) -> Vec<String> {
        SamplerRig::midi_input_ports()
    }
    fn set_midi_port(&self, name: String) {
        EkitRig::set_midi_port(self, name);
    }
    fn midi_recent(&self) -> Vec<String> {
        Vec::new()
    }
}

impl Services for EkitBackend {
    fn layers() -> impl Layer<Self> {
        layers![
            signal_ekit_proto::ekit::Service,
            signal_ekit_proto::ekit::StreamService
        ]
    }
}
