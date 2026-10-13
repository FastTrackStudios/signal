//! The hit detector: a kit's hits found once and written down as hit
//! markers on its recordings, and the actions that read and correct them.
//!
//! Spec: `docs/spec/session/drum-editing.md` (`r[hits.*]`).
//!
//! A detection run takes every recorded take of the kit — each piece's
//! close mics and trigger, recorded together — decodes each recording
//! whole at its own rate, and runs `drum_hits::detect_kit` on it: the
//! triggers calibrate the mics, a dead trigger is caught and its piece
//! read from its mic alone. What it finds is written as take markers
//! (`daw_proto::HitMark`) on every recording of the piece — its mics and
//! its trigger — at the attack's source position, one hit id across them.
//! A slip cuts the recording into many items; they all show the same
//! hits, and nothing is detected again.
//!
//! A run replaces only the `Auto` hits in its range. A hit placed or
//! moved by hand (`Manual`) or checked (`Approved`) is never touched, and
//! a new hit within [`NEAR`] of one is not added.
//!
//! The work runs on the caller's thread — an MCP call's, never the
//! window's — and spreads across cores inside `drum_hits`.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use architect::action::{ActionList, ActionMeta, Effect};
use daw::service::{Items as _, Projects, Takes as _, Tracks};
use daw_proto::{HitMark, ItemRef, MarkOrigin, ProjectContext, TakeMarker, TakeRef, TrackRef};
use facet::Facet;

use crate::actions::{meta, song};

/// A hand-placed hit this near a detected one keeps the detected one out,
/// seconds.
const NEAR: f64 = 0.003;
/// Two recordings whose first samples sound this near together are one
/// take, seconds.
const SAME_TAKE: f64 = 0.002;
/// Below this confidence a hit is one to look at.
const UNSURE: f64 = 0.6;

// ── settings ──────────────────────────────────────────────────────────

/// Which of a piece's inputs its hits come from.
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Trust {
    /// The trigger where it works, calibrating the mic; else the mic.
    Auto,
    /// The trigger, even where it disagrees with the mic.
    Trigger,
    /// The mic alone, whatever the trigger says.
    Mic,
}

/// The detector's settings: sticky, as the drum-edit settings are.
#[derive(Facet, Clone, Debug, PartialEq)]
pub struct HitSettings {
    pub trust: Trust,
    /// A fixed gate threshold for every mic, dBFS, instead of fitting
    /// each to its trigger. `None`: fitted.
    pub threshold_db: Option<f64>,
    /// The gate's crest, dB: with a fixed threshold, and for a mic with
    /// no trigger.
    pub crest_db: f64,
    /// After a hit, how long until the next, ms.
    pub retrig_ms: f64,
    /// How far under its loudest a trigger spike still counts, dB.
    pub trigger_under_db: f64,
}

impl Default for HitSettings {
    fn default() -> Self {
        Self {
            trust: Trust::Auto,
            threshold_db: None,
            crest_db: 5.0,
            retrig_ms: 40.0,
            trigger_under_db: 24.0,
        }
    }
}

/// Some settings: each given replaces the one there.
#[derive(Facet, Clone, Debug, Default, PartialEq)]
pub struct HitPatch {
    pub trust: Option<Trust>,
    /// A fixed threshold, dBFS; 0 or above goes back to fitting each mic
    /// to its trigger.
    pub threshold_db: Option<f64>,
    pub crest_db: Option<f64>,
    pub retrig_ms: Option<f64>,
    pub trigger_under_db: Option<f64>,
}

impl HitSettings {
    /// These, with `patch`'s in place of theirs.
    #[must_use]
    pub fn with(&self, patch: &HitPatch) -> Self {
        Self {
            trust: patch.trust.unwrap_or(self.trust),
            threshold_db: match patch.threshold_db {
                Some(db) if db < 0.0 => Some(db),
                Some(_) => None,
                None => self.threshold_db,
            },
            crest_db: patch.crest_db.unwrap_or(self.crest_db).max(0.0),
            retrig_ms: patch.retrig_ms.unwrap_or(self.retrig_ms).max(0.0),
            trigger_under_db: patch
                .trigger_under_db
                .unwrap_or(self.trigger_under_db)
                .abs(),
        }
    }

    fn detector(&self) -> drum_hits::Settings {
        let retrig_secs = self.retrig_ms / 1_000.0;
        drum_hits::Settings {
            retrig_secs,
            trigger_under_db: self.trigger_under_db,
            gate: self.threshold_db.map(|threshold_db| drum_hits::Gate {
                threshold_db,
                crest_db: self.crest_db,
                retrig_secs,
            }),
            crest_db: self.crest_db,
            trust: match self.trust {
                Trust::Auto => drum_hits::Trust::Auto,
                Trust::Trigger => drum_hits::Trust::Trigger,
                Trust::Mic => drum_hits::Trust::Mic,
            },
        }
    }
}

fn store() -> &'static std::sync::Mutex<HitSettings> {
    static STORE: std::sync::OnceLock<std::sync::Mutex<HitSettings>> = std::sync::OnceLock::new();
    STORE.get_or_init(|| std::sync::Mutex::new(load().unwrap_or_default()))
}

/// The settings as they stand.
#[must_use]
pub fn settings() -> HitSettings {
    store()
        .lock()
        .map_or_else(|e| e.into_inner().clone(), |s| s.clone())
}

/// Change some, keep them (and save them), and say what they are now.
pub fn set_settings(patch: &HitPatch) -> HitSettings {
    let now = {
        let mut held = store()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *held = held.with(patch);
        held.clone()
    };
    save(&now);
    now
}

fn settings_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("fts").join("hit-detect.json"))
}

fn load() -> Option<HitSettings> {
    let text = std::fs::read(settings_path()?).ok()?;
    facet_json::from_slice(&text).ok()
}

fn save(settings: &HitSettings) {
    let Some(path) = settings_path() else { return };
    if let Some(dir) = path.parent()
        && std::fs::create_dir_all(dir).is_err()
    {
        return;
    }
    match facet_json::to_vec(settings) {
        Ok(bytes) => {
            if std::fs::write(&path, bytes).is_err() {
                tracing::warn!("hit detect: the settings could not be saved");
            }
        }
        Err(error) => {
            tracing::warn!(error = %error, "hit detect: the settings would not serialize");
        }
    }
}

// ── the kit ───────────────────────────────────────────────────────────

/// A piece of the kit and its tracks.
#[derive(Clone, Debug)]
pub struct KitPiece {
    pub name: String,
    pub trigger: String,
    /// Its close mics, the one detection reads first (`In`, `Top`).
    pub mics: Vec<String>,
}

impl KitPiece {
    /// Every track whose recordings carry the piece's hits.
    pub fn tracks(&self) -> impl Iterator<Item = &String> {
        self.mics.iter().chain(std::iter::once(&self.trigger))
    }
}

/// The kit: each piece with a trigger, and the close mics that belong to
/// no piece (hi-hat, ride) — bleed to refuse, nothing to detect.
#[must_use]
pub fn kit(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
) -> (Vec<KitPiece>, Vec<String>) {
    let tracks = Tracks::all(daw, project.clone());
    let has_items = |guid: &str| {
        !daw.get_items(project.clone(), TrackRef::Guid(guid.to_owned()))
            .is_empty()
    };
    let pieces: Vec<KitPiece> = crate::drum_reference::pieces(daw, project)
        .into_iter()
        .filter_map(|(name, trigger)| {
            let parent = tracks
                .iter()
                .find(|t| t.guid == trigger)?
                .parent_guid
                .clone();
            let mut mics: Vec<&daw_proto::Track> = tracks
                .iter()
                .filter(|t| {
                    let lower = t.name.trim().to_ascii_lowercase();
                    t.parent_guid == parent
                        && t.guid != trigger
                        && !t.is_folder
                        && !dynamic_template::apply::is_trigger(&t.name)
                        && !lower.ends_with("sample")
                        && !dynamic_template::apply::is_one_note(&t.name)
                        && has_items(&t.guid)
                })
                .collect();
            // The mic nearest the head first: what detection reads.
            mics.sort_by_key(|t| {
                let lower = t.name.trim().to_ascii_lowercase();
                !(lower.starts_with("in")
                    || lower.starts_with("top")
                    || lower == name.to_ascii_lowercase())
            });
            (!mics.is_empty()).then(|| KitPiece {
                name,
                trigger,
                mics: mics.into_iter().map(|t| t.guid.clone()).collect(),
            })
        })
        .collect();
    let taken: BTreeSet<&String> = pieces.iter().flat_map(KitPiece::tracks).collect();
    let by_guid: HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let extras = tracks
        .iter()
        .filter(|t| {
            let lower = t.name.to_ascii_lowercase();
            !t.is_folder
                && !taken.contains(&t.guid)
                && (lower.contains("hat") || lower.contains("ride"))
                && crate::gain_stage::path_of(t, &by_guid)
                    .first()
                    .is_some_and(|p| p.trim().eq_ignore_ascii_case("drums"))
                && has_items(&t.guid)
        })
        .map(|t| t.guid.clone())
        .collect();
    (pieces, extras)
}

// ── recordings and takes ──────────────────────────────────────────────

/// A recording on a track: its file, where its first sample sounds as
/// recorded, and an item that plays it.
#[derive(Clone, Debug)]
struct Recording {
    path: String,
    origin: f64,
    /// Which recording pass it is from: what tells two takes recorded
    /// from the same place apart (see [`pass_of`]).
    pass: String,
    item: String,
    /// Something the song plays is cut from it (a playing or `EDIT`
    /// lane's item), not only a take kept on a lane.
    in_use: bool,
}

/// A recording's pass: the stamp a DAW names a recording pass's files
/// with (REAPER's `…-260907_1321-01.wav`), else its length — files
/// recorded together are the same length, and two passes from the same
/// place almost never are.
fn pass_of(path: &str, length: Option<f64>) -> String {
    let stem = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    // `YYMMDD_HHMM`, then any `-NN` take counters.
    let parts: Vec<&str> = stem.rsplit('-').collect();
    let stamp = parts.iter().position(|p| {
        let b = p.as_bytes();
        b.len() == 11
            && b.get(6) == Some(&b'_')
            && b.iter()
                .enumerate()
                .all(|(i, c)| i == 6 || c.is_ascii_digit())
    });
    if let Some(at) = stamp {
        let mut kept: Vec<&str> = parts.iter().take(at.saturating_add(1)).copied().collect();
        kept.reverse();
        return kept.join("-");
    }
    length.map_or_else(String::new, |l| format!("{l:.3}s"))
}

/// A track's recordings, each placed where most of its items put it — a
/// slipped slice moves, the takes it was cut from do not.
fn recordings(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
) -> Vec<Recording> {
    let mut seen: BTreeMap<String, Vec<(i64, String)>> = BTreeMap::new();
    let mut lengths: BTreeMap<String, Option<f64>> = BTreeMap::new();
    let mut used: BTreeSet<String> = BTreeSet::new();
    let this = Tracks::get(daw, project.clone(), TrackRef::Guid(track.to_owned()));
    for item in daw.get_items(project.clone(), TrackRef::Guid(track.to_owned())) {
        let Some(take) = daw.get_active_take(project.clone(), ItemRef::Guid(item.guid.clone()))
        else {
            continue;
        };
        let Some(path) = take.source_file_path else {
            continue;
        };
        let rate = if take.play_rate > 0.0 {
            take.play_rate
        } else {
            1.0
        };
        let origin = item.position.as_seconds() - take.start_offset.as_seconds() / rate;
        let ms = num_traits::ToPrimitive::to_i64(&(origin * 1_000.0).round()).unwrap_or(0);
        lengths
            .entry(path.clone())
            .or_insert_with(|| take.source_length.map(|d| d.as_seconds()));
        if this
            .as_ref()
            .is_some_and(|t| crate::drum_edit::is_edited(t, &item))
        {
            used.insert(path.clone());
        }
        seen.entry(path).or_default().push((ms, item.guid));
    }
    seen.into_iter()
        .filter_map(|(path, placed)| {
            let mut counts: BTreeMap<i64, usize> = BTreeMap::new();
            for (ms, _) in &placed {
                let n = counts.entry(*ms).or_default();
                *n = n.saturating_add(1);
            }
            let (ms, _) = counts.into_iter().max_by_key(|(_, n)| *n)?;
            let item = placed.into_iter().find(|(m, _)| *m == ms)?.1;
            let pass = pass_of(&path, lengths.get(&path).copied().flatten());
            Some(Recording {
                in_use: used.contains(&path),
                path,
                origin: num_traits::ToPrimitive::to_f64(&ms).unwrap_or(0.0) / 1_000.0,
                pass,
                item,
            })
        })
        .collect()
}

/// One recorded take of the kit: each track's recording in it.
#[derive(Clone, Debug)]
struct KitTake {
    origin: f64,
    pass: String,
    by_track: BTreeMap<String, Recording>,
}

impl KitTake {
    /// Whether the song plays anything cut from it.
    fn in_use(&self) -> bool {
        self.by_track.values().any(|r| r.in_use)
    }
}

/// The kit's recordings gathered into takes: those whose first samples
/// sound together.
fn takes(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    tracks: &[&String],
) -> Vec<KitTake> {
    let mut all: Vec<(String, Recording)> = tracks
        .iter()
        .flat_map(|t| {
            recordings(daw, project, t)
                .into_iter()
                .map(|r| ((*t).clone(), r))
        })
        .collect();
    all.sort_by(|a, b| a.1.origin.total_cmp(&b.1.origin));
    let mut out: Vec<KitTake> = Vec::new();
    for (track, recording) in all {
        // Passes recorded from the same place are told apart by pass.
        let same = out
            .iter_mut()
            .rev()
            .take_while(|t| (recording.origin - t.origin).abs() <= SAME_TAKE)
            .find(|t| t.pass == recording.pass);
        match same {
            Some(take) => {
                take.by_track.entry(track).or_insert(recording);
            }
            None => out.push(KitTake {
                origin: recording.origin,
                pass: recording.pass.clone(),
                by_track: BTreeMap::from([(track, recording)]),
            }),
        }
    }
    out
}

/// One recorded take of the kit, as listed: where it starts (project
/// seconds), its pass, whether the song plays it, and each track's
/// recording in it (track GUID, file).
pub type TakeListing = (f64, String, bool, Vec<(String, String)>);

/// The kit's recorded takes.
#[must_use]
pub fn kit_takes(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Vec<TakeListing> {
    let (pieces, extras) = kit(daw, project);
    let tracks: Vec<&String> = pieces
        .iter()
        .flat_map(KitPiece::tracks)
        .chain(&extras)
        .collect();
    takes(daw, project, &tracks)
        .into_iter()
        .map(|t| {
            let used = t.in_use();
            let files = t
                .by_track
                .into_iter()
                .map(|(track, r)| (track, r.path))
                .collect();
            (t.origin, t.pass, used, files)
        })
        .collect()
}

/// A recording read whole: mono (its channels averaged) at its own rate.
fn decode(path: &str) -> Option<(Vec<f32>, f64)> {
    let bytes = std::fs::read(path).ok()?;
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("wav");
    let audio = daw::standalone::audio_engine::decode_audio_with_extension(&bytes, extension)?;
    let channels = usize::from(audio.channels.max(1));
    let scale = 1.0 / num_traits::ToPrimitive::to_f32(&channels).unwrap_or(1.0);
    let mono = audio
        .samples
        .chunks(channels)
        .map(|c| c.iter().sum::<f32>() * scale)
        .collect();
    Some((mono, f64::from(audio.sample_rate)))
}

/// Every one of `paths` read whole, in parallel; those that would not
/// decode left out.
fn decode_all(paths: &BTreeSet<String>) -> HashMap<String, (Vec<f32>, f64)> {
    std::thread::scope(|scope| {
        // Every read started before any is waited on: joining as they are
        // spawned would read them one at a time.
        let mut reads = Vec::with_capacity(paths.len());
        for p in paths {
            reads.push(scope.spawn(move || decode(p).map(|a| (p.clone(), a))));
        }
        reads
            .into_iter()
            .filter_map(|r| r.join().ok().flatten())
            .collect()
    })
}

// ── a run ─────────────────────────────────────────────────────────────

/// What a run did for one piece, over every take.
#[derive(Facet, Clone, Debug, Default)]
pub struct PieceReport {
    pub piece: String,
    /// How its hits were found: `trigger+mic` where its trigger worked,
    /// `mic alone` where it did not, by take.
    pub method: String,
    /// Its trigger's verdict, where it was not believed.
    pub trigger: String,
    /// Its mic's fitted gate in the take with the most hits, and how
    /// well it agreed with the trigger there.
    pub gate: String,
    pub hits: u32,
    /// Heard by trigger and mic.
    pub both: u32,
    pub trigger_only: u32,
    pub mic_only: u32,
    pub ghosts: u32,
    pub flams: u32,
    /// Under 0.6 confidence: the ones to look at.
    pub unsure: u32,
    /// Hand-placed or checked hits it left as they were.
    pub kept: u32,
}

/// What a run did.
#[derive(Facet, Clone, Debug, Default)]
pub struct DetectReport {
    /// The run's id, on every hit it placed.
    pub run: String,
    pub takes: u32,
    pub seconds: f64,
    pub settings: String,
    pub pieces: Vec<PieceReport>,
    /// Takes left out, and why.
    pub skipped: Vec<String>,
}

/// One hit of a run, before it is written.
struct Found {
    id: String,
    at: f64,
    mark: HitMark,
}

/// What detecting one take gave: its rate, and each piece in it (by
/// index into the kit) with what was found.
type TakeFound = (f64, Vec<(usize, drum_hits::Found)>);

/// Read one take's recordings whole and detect on them. `Ok(None)` when
/// nothing of the kit is in it.
///
/// # Errors
/// Its recordings are at different rates.
fn detect_take(
    take: &KitTake,
    pieces: &[KitPiece],
    extras: &[String],
    settings: &HitSettings,
) -> Result<Option<TakeFound>, String> {
    let primary = |p: &KitPiece| p.mics.iter().find_map(|m| take.by_track.get(m));
    let mut paths: BTreeSet<String> = BTreeSet::new();
    for p in pieces {
        if let Some(mic) = primary(p) {
            paths.insert(mic.path.clone());
            if let Some(t) = take.by_track.get(&p.trigger) {
                paths.insert(t.path.clone());
            }
        }
    }
    paths.extend(
        extras
            .iter()
            .filter_map(|e| take.by_track.get(e))
            .map(|r| r.path.clone()),
    );
    let read = decode_all(&paths);
    let rates: BTreeSet<u64> = read.values().map(|(_, r)| r.to_bits()).collect();
    let Some(rate) = read.values().map(|(_, r)| *r).next() else {
        return Ok(None);
    };
    if rates.len() > 1 {
        return Err(format!(
            "the take at {:.2} s: its recordings are at different rates",
            take.origin
        ));
    }
    let signal = |r: &Recording| read.get(&r.path).map(|(s, _)| s.as_slice());
    let present: Vec<(usize, drum_hits::Piece<'_>)> = pieces
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            Some((
                i,
                drum_hits::Piece {
                    mic: signal(primary(p)?)?,
                    trigger: take.by_track.get(&p.trigger).and_then(signal),
                },
            ))
        })
        .collect();
    if present.is_empty() {
        return Ok(None);
    }
    let extra: Vec<&[f32]> = extras
        .iter()
        .filter_map(|e| take.by_track.get(e).and_then(signal))
        .collect();
    let kit_in: Vec<drum_hits::Piece<'_>> = present.iter().map(|(_, p)| *p).collect();
    let found = drum_hits::detect_kit(&kit_in, &extra, rate, &settings.detector());
    Ok(Some((
        rate,
        present.into_iter().map(|(i, _)| i).zip(found).collect(),
    )))
}

/// A piece's hits in one take as markers to write, those inside `span`
/// (seconds into the take's recordings).
fn take_hits(
    piece: &str,
    f: &drum_hits::Found,
    (rate, origin): (f64, f64),
    run: &str,
    span: (f64, f64),
) -> Vec<Found> {
    let at_take = |id: &str| format!("{id}@{:.0}", origin * 1_000.0);
    let ids: Vec<String> = (0..f.hits.len())
        .map(|n| format!("{run}.{}.{n:04}", slug(piece)))
        .collect();
    f.hits
        .iter()
        .zip(&ids)
        .map(|(h, id)| Found {
            id: at_take(id),
            at: drum_hits::seconds(h.at, rate),
            mark: HitMark {
                piece: piece.to_owned(),
                origin: MarkOrigin::Auto,
                run: run.to_owned(),
                peak_db: drum_hits::db(f64::from(h.peak)),
                confidence: h.confidence,
                from_trigger: h.from_trigger,
                from_mic: h.from_mic,
                ghost: h.ghost,
                bleed: f.method == drum_hits::Method::MicAlone && h.confidence < UNSURE,
                flam_with: h.flam_with.and_then(|n| ids.get(n)).map(|o| at_take(o)),
                note: String::new(),
            },
        })
        .filter(|h| h.at >= span.0 && h.at <= span.1)
        .collect()
}

/// What a run has learned of one piece so far, over the takes.
#[derive(Default)]
struct Seen {
    /// Its fitted gate in the take with the most hits: those hits, and
    /// the gate said.
    best: (usize, String),
    /// How many takes each method found it in.
    methods: BTreeMap<String, u32>,
}

/// Fold one take's result for a piece into its report.
fn note_take(line: &mut PieceReport, seen: &mut Seen, f: &drum_hits::Found, origin: f64) {
    let method = match f.method {
        drum_hits::Method::Fused => "trigger+mic",
        drum_hits::Method::MicAlone => "mic alone",
    };
    let n = seen.methods.entry(method.to_owned()).or_default();
    *n = n.saturating_add(1);
    match &f.trigger {
        Some(drum_hits::Health::Dead(why)) => {
            line.trigger = format!("dead in the take at {origin:.2} s: {why}");
        }
        Some(drum_hits::Health::MicSilent(why)) => {
            line.trigger = format!("mic silent in the take at {origin:.2} s: {why}");
        }
        _ => {}
    }
    if let Some(c) = f.calibration
        && f.hits.len() > seen.best.0
    {
        seen.best = (
            f.hits.len(),
            format!(
                "{:.0} dBFS / {:.0} dB crest, agrees with the trigger at F {:.2}",
                c.gate.threshold_db,
                c.gate.crest_db,
                c.agreement.f1()
            ),
        );
    }
}

/// A run over the takes the song plays (every take with `all_takes`),
/// in `span` (project seconds, as recorded), on `only` pieces (all when
/// empty).
///
/// # Errors
/// No kit to detect on.
pub fn detect(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    span: (f64, f64),
    only: &[String],
    settings: &HitSettings,
    all_takes: bool,
) -> Result<DetectReport, String> {
    let started = std::time::Instant::now();
    let _job = crate::jobs::start("Finding the drum hits", 0);
    let (all_pieces, extras) = kit(daw, project);
    let pieces: Vec<KitPiece> = all_pieces
        .into_iter()
        .filter(|p| only.is_empty() || only.iter().any(|o| o.eq_ignore_ascii_case(&p.name)))
        .collect();
    if pieces.is_empty() {
        return Err("no kit: no piece with a trigger and a close mic".to_owned());
    }
    let tracks: Vec<&String> = pieces
        .iter()
        .flat_map(KitPiece::tracks)
        .chain(&extras)
        .collect();
    let run: String = uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(8)
        .collect();
    let mut report = DetectReport {
        run: run.clone(),
        settings: format!("{settings:?}"),
        pieces: pieces
            .iter()
            .map(|p| PieceReport {
                piece: p.name.clone(),
                ..PieceReport::default()
            })
            .collect(),
        ..DetectReport::default()
    };
    let mut seen: Vec<Seen> = pieces.iter().map(|_| Seen::default()).collect();
    // Each take's hits of each piece, before they are written.
    let mut placed: Vec<(KitTake, usize, Vec<Found>)> = Vec::new();

    for take in takes(daw, project, &tracks)
        .into_iter()
        .filter(|t| all_takes || t.in_use())
    {
        let found = match detect_take(&take, &pieces, &extras, settings) {
            Ok(Some(found)) => found,
            Ok(None) => continue,
            Err(why) => {
                report.skipped.push(why);
                continue;
            }
        };
        report.takes = report.takes.saturating_add(1);
        let (rate, per_piece) = found;
        let take_span = (span.0 - take.origin, span.1 - take.origin);
        for (i, f) in per_piece {
            let (Some(piece), Some(line), Some(s)) =
                (pieces.get(i), report.pieces.get_mut(i), seen.get_mut(i))
            else {
                continue;
            };
            note_take(line, s, &f, take.origin);
            placed.push((
                take.clone(),
                i,
                take_hits(&piece.name, &f, (rate, take.origin), &run, take_span),
            ));
        }
    }
    check_against_midi(daw, project, &mut placed);

    // On every recording of the piece in its take.
    let mut written: BTreeMap<String, (String, Vec<Found>)> = BTreeMap::new();
    for (take, i, hits) in &placed {
        let (Some(piece), Some(line)) = (pieces.get(*i), report.pieces.get_mut(*i)) else {
            continue;
        };
        tally(line, hits);
        for r in piece.tracks().filter_map(|t| take.by_track.get(t)) {
            let entry = written
                .entry(r.path.clone())
                .or_insert_with(|| (r.item.clone(), Vec::new()));
            entry.1.extend(hits.iter().map(|h| Found {
                id: h.id.clone(),
                at: h.at,
                mark: h.mark.clone(),
            }));
        }
    }

    finish(&mut report, seen);
    let names: BTreeSet<String> = pieces.iter().map(|p| p.name.clone()).collect();
    Projects::begin_undo_block(daw, project.clone(), "Detect drum hits");
    for (item, hits) in written.into_values() {
        for (piece, n) in write(daw, project, &item, hits, &names, span) {
            if let Some(line) = report.pieces.iter_mut().find(|l| l.piece == piece) {
                line.kept = line.kept.max(n);
            }
        }
    }
    Projects::end_undo_block(daw, project.clone(), "Detect drum hits", None);
    report.seconds = started.elapsed().as_secs_f64();
    Ok(report)
}

/// What was learned of each piece over the takes, into its report line.
fn finish(report: &mut DetectReport, seen: Vec<Seen>) {
    for (line, s) in report.pieces.iter_mut().zip(seen) {
        line.gate = s.best.1;
        line.method = s
            .methods
            .into_iter()
            .map(|(k, n)| format!("{k} in {n}"))
            .collect::<Vec<_>>()
            .join(", ");
    }
}

/// How near a written note a hit is to be that note's, seconds.
const ON_NOTE: f64 = 0.03;
/// What a mic-alone hit on one of its piece's written notes is trusted to.
const ON_NOTE_CONFIDENCE: f64 = 0.8;

/// Hold the hits up to the song's drum MIDI, where it has one: each note
/// number mapped to the piece whose hits its notes land on (learned from
/// these hits, so a piece with a dead trigger is mapped too), and a hit
/// its trigger did not hear but that lands on one of its piece's notes
/// trusted more, and said to.
///
/// The MIDI is a guide, not the truth — a drummer plays other fills — so
/// a hit with no note is left as it is, never removed.
fn check_against_midi(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    placed: &mut [(KitTake, usize, Vec<Found>)],
) {
    let Some(track) = crate::drum_reference::find_track(daw, project) else {
        return;
    };
    let notes = crate::drum_reference::notes(daw, project, &track);
    if notes.is_empty() {
        return;
    }
    let mut by_piece: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for (take, _, hits) in placed.iter() {
        for h in hits {
            by_piece
                .entry(h.mark.piece.clone())
                .or_default()
                .push(take.origin + h.at);
        }
    }
    let named: Vec<(String, Vec<f64>)> = by_piece
        .into_iter()
        .map(|(piece, mut times)| {
            times.sort_by(f64::total_cmp);
            (piece, times)
        })
        .collect();
    let map = crate::drum_reference::learn_map(&notes, &named, ON_NOTE);
    let pitches = |piece: &str| -> BTreeSet<u8> {
        map.iter()
            .filter(|m| m.piece.as_deref() == Some(piece))
            .map(|m| m.pitch)
            .collect()
    };
    for (take, _, hits) in placed.iter_mut() {
        for h in hits.iter_mut().filter(|h| !h.mark.from_trigger) {
            let own = pitches(&h.mark.piece);
            let time = take.origin + h.at;
            if notes
                .iter()
                .any(|n| own.contains(&n.pitch) && (n.time - time).abs() <= ON_NOTE)
            {
                h.mark.confidence = h.mark.confidence.max(ON_NOTE_CONFIDENCE);
                h.mark.bleed = false;
                "on a written note".clone_into(&mut h.mark.note);
            }
        }
    }
}

/// A piece's name as an id part.
fn slug(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Count a piece's hits into its report.
fn tally(line: &mut PieceReport, hits: &[Found]) {
    let add = |n: &mut u32, yes: bool| {
        if yes {
            *n = n.saturating_add(1);
        }
    };
    for h in hits {
        let m = &h.mark;
        add(&mut line.hits, true);
        add(&mut line.both, m.from_trigger && m.from_mic);
        add(&mut line.trigger_only, m.from_trigger && !m.from_mic);
        add(&mut line.mic_only, !m.from_trigger && m.from_mic);
        add(&mut line.ghosts, m.ghost);
        add(&mut line.flams, m.flam_with.is_some());
        add(&mut line.unsure, m.confidence < UNSURE);
    }
}

/// Write a run's `hits` onto the recording `item` plays: its `Auto` hits
/// of `pieces` inside `span` replaced, everything else kept, and no new
/// hit within [`NEAR`] of a kept one of its piece. How many hits of each
/// piece were kept.
fn write(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    item: &str,
    hits: Vec<Found>,
    pieces: &BTreeSet<String>,
    span: (f64, f64),
) -> BTreeMap<String, u32> {
    let at = ItemRef::Guid(item.to_owned());
    let origin = recording_origin(daw, project, item).unwrap_or(0.0);
    let (from, to) = (span.0 - origin, span.1 - origin);
    let existing = daw.get_take_markers(project.clone(), at.clone(), TakeRef::Active);
    let mut kept: BTreeMap<String, u32> = BTreeMap::new();
    let mut out: Vec<TakeMarker> = existing
        .into_iter()
        .filter(|m| match &m.hit {
            Some(h) if h.origin == MarkOrigin::Auto && pieces.contains(&h.piece) => {
                m.source_position_seconds < from || m.source_position_seconds > to
            }
            Some(h) => {
                if pieces.contains(&h.piece) {
                    let n = kept.entry(h.piece.clone()).or_default();
                    *n = n.saturating_add(1);
                }
                true
            }
            None => true,
        })
        .collect();
    let held: Vec<(String, f64)> = out
        .iter()
        .filter_map(|m| {
            m.hit
                .as_ref()
                .map(|h| (h.piece.clone(), m.source_position_seconds))
        })
        .collect();
    out.extend(
        hits.into_iter()
            .filter(|h| {
                !held
                    .iter()
                    .any(|(piece, t)| *piece == h.mark.piece && (t - h.at).abs() < NEAR)
            })
            .map(|h| TakeMarker {
                id: h.id,
                source_position_seconds: h.at,
                hit: Some(h.mark),
                ..TakeMarker::default()
            }),
    );
    if let Err(e) = daw.set_take_markers(project.clone(), at, TakeRef::Active, out) {
        tracing::warn!(hits.write_error = %e, "the hits could not be written");
    }
    kept
}

/// Where the first sample of the recording `item` plays sounds, as the
/// item places it.
fn recording_origin(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    item: &str,
) -> Option<f64> {
    let i = daw.get_item(project.clone(), ItemRef::Guid(item.to_owned()))?;
    let take = daw.get_active_take(project.clone(), ItemRef::Guid(item.to_owned()))?;
    let rate = if take.play_rate > 0.0 {
        take.play_rate
    } else {
        1.0
    };
    Some(i.position.as_seconds() - take.start_offset.as_seconds() / rate)
}

// ── reading them back ─────────────────────────────────────────────────

/// One hit, as the list gives it.
#[derive(Facet, Clone, Debug)]
pub struct HitRow {
    pub id: String,
    pub piece: String,
    /// When it was played, project seconds, as recorded.
    pub time: f64,
    /// Where the song plays it now (an edit has moved it), project
    /// seconds; none when no slice plays it.
    pub now: Option<f64>,
    /// Bar and beat where it plays now (as recorded when it plays
    /// nowhere).
    pub bar: i32,
    pub beat: f64,
    pub peak_db: f64,
    pub confidence: f64,
    /// `both`, `trigger` or `mic`; empty for a hand-placed one.
    pub heard: String,
    /// `ghost`, `flam`, `bleed`, `unsure`.
    pub flags: Vec<String>,
    /// `auto`, `manual` or `approved`.
    pub origin: String,
    pub note: String,
}

/// Every hit of `pieces` (all when empty), read off each piece's first
/// mic's recordings, in time order.
#[must_use]
pub fn hits(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    only: &[String],
) -> Vec<HitRow> {
    let (pieces, _) = kit(daw, project);
    let now: HashMap<String, f64> = playback(daw, project)
        .into_iter()
        .map(|p| (p.id, p.time))
        .collect();
    let mut rows: BTreeMap<String, HitRow> = BTreeMap::new();
    for piece in pieces
        .iter()
        .filter(|p| only.is_empty() || only.iter().any(|o| o.eq_ignore_ascii_case(&p.name)))
    {
        let Some(mic) = piece.mics.first() else {
            continue;
        };
        for r in recordings(daw, project, mic) {
            for m in daw.get_take_markers(
                project.clone(),
                ItemRef::Guid(r.item.clone()),
                TakeRef::Active,
            ) {
                let Some(h) = m.hit else { continue };
                if h.piece != piece.name {
                    continue;
                }
                let time = r.origin + m.source_position_seconds;
                let plays = now.get(&m.id).copied();
                let (bar, beat, frac) =
                    crate::song_bars::musical(daw, project, plays.unwrap_or(time));
                let mut flags = Vec::new();
                for (yes, flag) in [
                    (h.ghost, "ghost"),
                    (h.flam_with.is_some(), "flam"),
                    (h.bleed, "bleed"),
                    (
                        h.origin == MarkOrigin::Auto && h.confidence < UNSURE,
                        "unsure",
                    ),
                ] {
                    if yes {
                        flags.push(flag.to_owned());
                    }
                }
                rows.insert(
                    m.id.clone(),
                    HitRow {
                        id: m.id,
                        piece: h.piece,
                        time,
                        now: plays,
                        bar,
                        beat: f64::from(beat) + frac,
                        peak_db: h.peak_db,
                        confidence: h.confidence,
                        heard: match (h.from_trigger, h.from_mic) {
                            (true, true) => "both",
                            (true, false) => "trigger",
                            (false, true) => "mic",
                            (false, false) => "",
                        }
                        .to_owned(),
                        flags,
                        origin: origin_name(h.origin).to_owned(),
                        note: h.note,
                    },
                );
            }
        }
    }
    let mut out: Vec<HitRow> = rows.into_values().collect();
    out.sort_by(|a, b| a.at().total_cmp(&b.at()));
    out
}

impl HitRow {
    /// Where it is now: where the song plays it, else as recorded.
    #[must_use]
    pub fn at(&self) -> f64 {
        self.now.unwrap_or(self.time)
    }
}

const fn origin_name(o: MarkOrigin) -> &'static str {
    match o {
        MarkOrigin::Auto => "auto",
        MarkOrigin::Manual => "manual",
        MarkOrigin::Approved => "approved",
    }
}

// ── correcting them ───────────────────────────────────────────────────

/// Every recording of every piece's tracks, with the item that plays it.
fn kit_recordings(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Vec<Recording> {
    let (pieces, _) = kit(daw, project);
    let mut seen: BTreeMap<String, Recording> = BTreeMap::new();
    for p in &pieces {
        for t in p.tracks() {
            for r in recordings(daw, project, t) {
                seen.entry(r.path.clone()).or_insert(r);
            }
        }
    }
    seen.into_values().collect()
}

/// Change the markers of every hit in `ids`, on every recording that
/// carries one, with `change` (return `false` to remove it). How many
/// markers were changed.
fn edit_hits(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    ids: &BTreeSet<String>,
    label: &str,
    mut change: impl FnMut(&mut TakeMarker, f64) -> bool,
) -> u32 {
    let mut n = 0_u32;
    Projects::begin_undo_block(daw, project.clone(), label);
    for r in kit_recordings(daw, project) {
        let at = ItemRef::Guid(r.item.clone());
        let markers = daw.get_take_markers(project.clone(), at.clone(), TakeRef::Active);
        if !markers.iter().any(|m| ids.contains(&m.id)) {
            continue;
        }
        let out: Vec<TakeMarker> = markers
            .into_iter()
            .filter_map(|mut m| {
                if !ids.contains(&m.id) {
                    return Some(m);
                }
                n = n.saturating_add(1);
                change(&mut m, r.origin).then_some(m)
            })
            .collect();
        if let Err(e) = daw.set_take_markers(project.clone(), at, TakeRef::Active, out) {
            tracing::warn!(hits.write_error = %e, "a hit edit could not be written");
        }
    }
    Projects::end_undo_block(daw, project.clone(), label, None);
    n
}

/// When what `piece` plays at `now` (project seconds, after any edit)
/// was recorded: the slice of its first mic playing then, read back to
/// its recording.
#[must_use]
pub fn recorded_at(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    piece: &str,
    now: f64,
) -> Option<f64> {
    let (pieces, _) = kit(daw, project);
    let mic = pieces
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(piece))?
        .mics
        .first()?
        .clone();
    let track = Tracks::get(daw, project.clone(), TrackRef::Guid(mic.clone()))?;
    let item = daw
        .get_items(project.clone(), TrackRef::Guid(mic.clone()))
        .into_iter()
        .filter(|i| !i.muted && crate::drum_edit::is_edited(&track, i))
        .find(|i| {
            let at = i.position.as_seconds();
            now >= at && now < at + i.length.as_seconds()
        })?;
    let take = daw.get_active_take(project.clone(), ItemRef::Guid(item.guid.clone()))?;
    let path = take.source_file_path?;
    let rate = if take.play_rate > 0.0 {
        take.play_rate
    } else {
        1.0
    };
    let source = (now - item.position.as_seconds()).mul_add(rate, take.start_offset.as_seconds());
    let origin = recordings(daw, project, &mic)
        .into_iter()
        .find(|r| r.path == path)?
        .origin;
    Some(origin + source / rate)
}

/// Place a hit of `piece` by hand at `time` (project seconds, as
/// recorded), on every recording of the piece that covers it — snapped
/// to the attack start on its first mic when `snap`. Its id.
///
/// # Errors
/// No such piece, or no take of it covers `time`.
pub fn add_hit(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    piece: &str,
    time: f64,
    (snap, note): (bool, String),
) -> Result<String, String> {
    let (pieces, _) = kit(daw, project);
    let p = pieces
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(piece))
        .ok_or_else(|| {
            format!(
                "no piece {piece}: one of {:?}",
                pieces.iter().map(|p| &p.name).collect::<Vec<_>>()
            )
        })?;
    let tracks: Vec<&String> = p.tracks().collect();
    let take = take_at(takes(daw, project, &tracks), p, time)
        .ok_or_else(|| format!("no take of {} covers {time:.3} s", p.name))?;
    let mut at = time - take.origin;
    if snap
        && let Some(mic) = p.mics.iter().find_map(|m| take.by_track.get(m))
        && let Some((samples, rate)) = decode(&mic.path)
    {
        let found = drum_hits::attack_start(&samples, rate, drum_hits::samples(at, rate));
        at = drum_hits::seconds(found, rate);
    }
    let id = format!(
        "m.{}",
        uuid::Uuid::new_v4()
            .simple()
            .to_string()
            .chars()
            .take(8)
            .collect::<String>()
    );
    Projects::begin_undo_block(daw, project.clone(), "Place a drum hit");
    for track in p.tracks() {
        if let Some(r) = take.by_track.get(track) {
            let item = ItemRef::Guid(r.item.clone());
            let mut markers = daw.get_take_markers(project.clone(), item.clone(), TakeRef::Active);
            markers.push(TakeMarker {
                id: id.clone(),
                source_position_seconds: at,
                hit: Some(HitMark {
                    piece: p.name.clone(),
                    origin: MarkOrigin::Manual,
                    confidence: 1.0,
                    note: note.clone(),
                    ..HitMark::default()
                }),
                ..TakeMarker::default()
            });
            if let Err(e) = daw.set_take_markers(project.clone(), item, TakeRef::Active, markers) {
                tracing::warn!(hits.write_error = %e, "a placed hit could not be written");
            }
        }
    }
    Projects::end_undo_block(daw, project.clone(), "Place a drum hit", None);
    Ok(id)
}

/// The take of `piece` that `time` (project seconds) is in: one the song
/// plays when there is one — a kit keeps many passes recorded from the
/// same place, and the one heard is the one meant — else the latest to
/// start before it.
fn take_at(all: Vec<KitTake>, piece: &KitPiece, time: f64) -> Option<KitTake> {
    let mut fits: Vec<KitTake> = all
        .into_iter()
        .filter(|t| t.origin <= time && piece.mics.iter().any(|m| t.by_track.contains_key(m)))
        .collect();
    if fits.iter().any(KitTake::in_use) {
        fits.retain(KitTake::in_use);
    }
    fits.into_iter()
        .max_by(|a, b| a.origin.total_cmp(&b.origin))
}

// ── looking closer ────────────────────────────────────────────────────

/// One track's audio around a moment.
#[derive(Facet, Clone, Debug)]
pub struct TraceRow {
    pub track: String,
    /// `mic`, `trigger`, or `other` (another piece's mic, the hat, the
    /// ride).
    pub role: String,
    /// The loudest sample of each slice, dBFS, from `from` to `to`.
    pub peaks_db: Vec<f64>,
}

/// What the detector saw around a moment.
#[derive(Facet, Clone, Debug)]
pub struct Explain {
    pub piece: String,
    /// Project seconds, as recorded.
    pub from: f64,
    pub to: f64,
    /// Seconds a slice.
    pub step: f64,
    pub traces: Vec<TraceRow>,
    /// The piece's hits in the window.
    pub hits: Vec<HitRow>,
    /// Where the attack starts, by the detector, nearest the middle —
    /// project seconds, with its peak (dBFS) on the first mic.
    pub attack: Option<(f64, f64)>,
}

/// `points` peaks (dBFS, a tenth of a dB) of `samples` from `from`
/// seconds into them, `step` seconds a peak.
fn peaks(samples: &[f32], rate: f64, from: f64, (step, points): (f64, usize)) -> Vec<f64> {
    (0..points)
        .map(|k| {
            let a = step.mul_add(num_traits::ToPrimitive::to_f64(&k).unwrap_or(0.0), from);
            let lo = drum_hits::samples(a, rate).min(samples.len());
            let hi = drum_hits::samples(a + step, rate)
                .max(lo.saturating_add(1))
                .min(samples.len());
            let peak = samples
                .get(lo..hi)
                .map_or(0.0, |s| s.iter().fold(0.0_f32, |m, x| m.max(x.abs())));
            (drum_hits::db(f64::from(peak)) * 10.0).round() / 10.0
        })
        .collect()
}

/// The audio of `piece`'s mics and trigger, and of every other close mic
/// of its take, from `time - half` to `time + half`, as `points` peaks
/// each, with the piece's hits there.
///
/// # Errors
/// No such piece, or no take of it covers `time`.
pub fn explain(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    piece: &str,
    time: f64,
    (half, points): (f64, usize),
) -> Result<Explain, String> {
    let (pieces, extras) = kit(daw, project);
    let p = pieces
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(piece))
        .ok_or_else(|| format!("no piece {piece}"))?;
    let kit_tracks: Vec<&String> = pieces
        .iter()
        .flat_map(KitPiece::tracks)
        .chain(&extras)
        .collect();
    let take = take_at(takes(daw, project, &kit_tracks), p, time)
        .ok_or_else(|| format!("no take of {} covers {time:.3} s", p.name))?;
    let names: HashMap<String, String> = Tracks::all(daw, project.clone())
        .into_iter()
        .map(|t| (t.guid, t.name))
        .collect();
    let role = |guid: &String| {
        if p.mics.contains(guid) {
            "mic"
        } else if *guid == p.trigger {
            "trigger"
        } else {
            "other"
        }
    };
    // Every mic of the take and this piece's trigger: the other pieces'
    // triggers hear only their own drums.
    let others: BTreeSet<&String> = pieces
        .iter()
        .filter(|o| o.name != p.name)
        .map(|o| &o.trigger)
        .collect();
    let paths: BTreeSet<String> = take
        .by_track
        .iter()
        .filter(|(t, _)| !others.contains(t))
        .map(|(_, r)| r.path.clone())
        .collect();
    let read = decode_all(&paths);
    let points = points.clamp(8, 2_000);
    let (from, to) = (time - half.abs(), time + half.abs());
    let step = (to - from) / num_traits::ToPrimitive::to_f64(&points).unwrap_or(1.0);
    let mut traces: Vec<TraceRow> = take
        .by_track
        .iter()
        .filter_map(|(guid, r)| {
            let (samples, rate) = read.get(&r.path)?;
            let peaks_db = peaks(samples, *rate, from - take.origin, (step, points));
            Some(TraceRow {
                track: names.get(guid).cloned().unwrap_or_else(|| guid.clone()),
                role: role(guid).to_owned(),
                peaks_db,
            })
        })
        .collect();
    traces.sort_by_key(|t| match t.role.as_str() {
        "mic" => 0,
        "trigger" => 1,
        _ => 2,
    });
    let attack = p
        .mics
        .iter()
        .find_map(|m| take.by_track.get(m))
        .and_then(|r| read.get(&r.path))
        .map(|(samples, rate)| {
            let at = drum_hits::attack_start(
                samples,
                *rate,
                drum_hits::samples(time - take.origin, *rate),
            );
            (
                take.origin + drum_hits::seconds(at, *rate),
                drum_hits::db(f64::from(drum_hits::attack_peak(samples, *rate, at))),
            )
        });
    let hits = hits(daw, project, std::slice::from_ref(&p.name))
        .into_iter()
        .filter(|h| h.time >= from && h.time <= to)
        .collect();
    Ok(Explain {
        piece: p.name.clone(),
        from,
        to,
        step,
        traces,
        hits,
        attack,
    })
}

// ── where the hits are heard now ──────────────────────────────────────

/// One hit where the song plays it now: on its piece's first mic, on the
/// lane an edit works on (`EDIT`, else the playing lane).
#[derive(Clone, Debug)]
pub struct Played {
    pub id: String,
    pub piece: String,
    /// Project seconds, as it plays now.
    pub time: f64,
    pub peak_db: f64,
    pub mark: HitMark,
}

/// What the arrangement draws over the kit in Edit mode: every hit where
/// it plays now, by piece and who placed it, and the zones that are not
/// turned down.
#[must_use]
pub fn overlay(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
) -> crate::hit_overlay::Overlay {
    use crate::hit_overlay::{Edited, Mark, Shade, Who};
    let (kit, _) = kit(daw, project);
    // Where each hit was played, and which a hand placed: what has been
    // done to it, by how far it plays from there.
    let recorded = recorded_times(daw, project, &kit);
    let by_hand = placed(daw, project);
    let mut pieces: Vec<String> = kit.iter().map(|p| p.name.clone()).collect();
    let mut tracks: Vec<Vec<String>> = kit
        .iter()
        .map(|p| p.tracks().filter(|t| !t.is_empty()).cloned().collect())
        .collect();
    let marks = playback(daw, project)
        .into_iter()
        .map(|p| {
            let piece = pieces
                .iter()
                .position(|n| *n == p.piece)
                .unwrap_or_else(|| {
                    pieces.push(p.piece.clone());
                    tracks.push(Vec::new());
                    pieces.len().saturating_sub(1)
                });
            let auto = p.mark.origin == MarkOrigin::Auto;
            Mark {
                id: p.id.clone(),
                piece,
                time: p.time,
                who: match p.mark.origin {
                    MarkOrigin::Auto => Who::Auto,
                    MarkOrigin::Manual => Who::Manual,
                    MarkOrigin::Approved => Who::Approved,
                },
                ghost: p.mark.ghost,
                doubt: auto && (p.mark.bleed || p.mark.confidence < UNSURE),
                edited: match recorded.get(&p.id) {
                    Some(was) if (p.time - was).abs() > MOVED => {
                        if by_hand.contains(&p.id) {
                            Edited::ByHand
                        } else {
                            Edited::Quantized
                        }
                    }
                    _ => Edited::AsPlayed,
                },
            }
        })
        .collect();
    let zones = zones(daw, project)
        .into_iter()
        .filter(|z| z.origin != ZoneOrigin::Rejected)
        .map(|z| Shade {
            start: z.start,
            end: z.end,
            found: z.origin == ZoneOrigin::Auto,
        })
        .collect();
    let stretches = stretches(daw, project, &kit);
    crate::hit_overlay::Overlay {
        pieces,
        tracks,
        marks,
        zones,
        stretches,
    }
}

/// Played this near its recorded speed, a stretch is not drawn.
const AT_SPEED: f64 = 0.005;

/// Each piece's stretched stretches where they play now — its first mic's
/// slices, between each pair of stretch markers played slower or faster
/// than recorded — in time order.
fn stretches(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    kit: &[KitPiece],
) -> Vec<crate::hit_overlay::Stretch> {
    let mut out = Vec::new();
    for (piece, kp) in kit.iter().enumerate() {
        let Some(mic) = kp.mics.first() else {
            continue;
        };
        let Some(track) = Tracks::get(daw, project.clone(), TrackRef::Guid(mic.clone())) else {
            continue;
        };
        for item in daw.get_items(project.clone(), TrackRef::Guid(mic.clone())) {
            if item.muted || !crate::drum_edit::is_edited(&track, &item) {
                continue;
            }
            let markers = daw_proto::StretchMarkers::get_stretch_markers(
                daw,
                project.clone(),
                ItemRef::Guid(item.guid.clone()),
                TakeRef::Active,
            );
            let at = item.position.as_seconds();
            for pair in markers.windows(2) {
                let [a, b] = pair else { continue };
                let played = b.position - a.position;
                if played <= 0.0 {
                    continue;
                }
                let rate = (b.source_position - a.source_position) / played;
                if (rate - 1.0).abs() > AT_SPEED {
                    out.push(crate::hit_overlay::Stretch {
                        piece,
                        start: at + a.position,
                        end: at + b.position,
                        rate,
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    out
}

/// Where in its take `source` plays, through `markers` (by position): `None`
/// with fewer than two, or outside them.
fn stretched(markers: &[daw_proto::StretchMarker], source: f64) -> Option<f64> {
    markers.windows(2).find_map(|pair| {
        let [a, b] = pair else { return None };
        let span = b.source_position - a.source_position;
        (source >= a.source_position && source <= b.source_position && span > 0.0).then(|| {
            ((source - a.source_position) / span).mul_add(b.position - a.position, a.position)
        })
    })
}

/// Every hit of the kit where it plays now — a hit cut into two slices
/// that both play it is here twice, one cut away is not here at all.
#[must_use]
pub fn playback(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Vec<Played> {
    let (pieces, _) = kit(daw, project);
    let mut out = Vec::new();
    for piece in &pieces {
        let Some(mic) = piece.mics.first() else {
            continue;
        };
        let Some(track) = Tracks::get(daw, project.clone(), TrackRef::Guid(mic.clone())) else {
            continue;
        };
        let mut by_source: HashMap<String, Vec<TakeMarker>> = HashMap::new();
        for item in daw.get_items(project.clone(), TrackRef::Guid(mic.clone())) {
            if item.muted || !crate::drum_edit::is_edited(&track, &item) {
                continue;
            }
            let Some(take) = daw.get_active_take(project.clone(), ItemRef::Guid(item.guid.clone()))
            else {
                continue;
            };
            let Some(path) = take.source_file_path.clone() else {
                continue;
            };
            let rate = if take.play_rate > 0.0 {
                take.play_rate
            } else {
                1.0
            };
            let from = take.start_offset.as_seconds();
            let to = item.length.as_seconds().mul_add(rate, from);
            // A slice drawn out over a gap plays less of its source than
            // its length says: where its stretch markers say.
            let stretch = daw_proto::StretchMarkers::get_stretch_markers(
                daw,
                project.clone(),
                ItemRef::Guid(item.guid.clone()),
                TakeRef::Active,
            );
            let (from, to) = match (stretch.first(), stretch.last()) {
                (Some(first), Some(last)) if stretch.len() > 1 => {
                    (first.source_position, last.source_position)
                }
                _ => (from, to),
            };
            let at = |source: f64| {
                stretched(&stretch, source).map_or_else(
                    || item.position.as_seconds() + (source - from) / rate,
                    |t| item.position.as_seconds() + t,
                )
            };
            let markers = by_source.entry(path).or_insert_with(|| {
                daw.get_take_markers(
                    project.clone(),
                    ItemRef::Guid(item.guid.clone()),
                    TakeRef::Active,
                )
            });
            for m in markers.iter() {
                let Some(h) = &m.hit else { continue };
                if h.piece != piece.name
                    || m.source_position_seconds < from
                    || m.source_position_seconds >= to
                {
                    continue;
                }
                out.push(Played {
                    id: m.id.clone(),
                    piece: h.piece.clone(),
                    time: at(m.source_position_seconds),
                    peak_db: h.peak_db,
                    mark: h.clone(),
                });
            }
        }
    }
    out.sort_by(|a, b| a.time.total_cmp(&b.time));
    out
}

/// How far from where it was played a hit must play to have been moved,
/// seconds.
const MOVED: f64 = 0.001;

/// Where the kit's hits are kept as placed by hand: on its folder.
const PLACED_KEY: &str = "fts.hits_placed";

/// Each hit's time as recorded, by id: the first mic of each piece.
fn recorded_times(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    kit: &[KitPiece],
) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    for piece in kit {
        let Some(mic) = piece.mics.first() else {
            continue;
        };
        for r in recordings(daw, project, mic) {
            for m in daw.get_take_markers(
                project.clone(),
                ItemRef::Guid(r.item.clone()),
                TakeRef::Active,
            ) {
                if m.hit.as_ref().is_some_and(|h| h.piece == piece.name) {
                    out.insert(m.id, r.origin + m.source_position_seconds);
                }
            }
        }
    }
    out
}

/// The hits a hand has placed (`HITS_PLACE`: a flag dragged).
#[must_use]
pub fn placed(daw: &daw::standalone::Standalone, project: &ProjectContext) -> HashSet<String> {
    let Some(folder) = crate::track_regions::kit_folder(daw, project) else {
        return HashSet::new();
    };
    Tracks::ext_state(daw, project.clone(), TrackRef::Guid(folder))
        .into_iter()
        .find(|e| e.key == PLACED_KEY)
        .map(|e| {
            e.value
                .split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Remember that a hand placed hit `id`: kept on the kit's folder, so it
/// is saved with the song and undone with the edit.
pub fn mark_placed(daw: &daw::standalone::Standalone, project: &ProjectContext, id: &str) {
    let Some(folder) = crate::track_regions::kit_folder(daw, project) else {
        return;
    };
    let mut ids: Vec<String> = placed(daw, project).into_iter().collect();
    if ids.iter().any(|i| i == id) {
        return;
    }
    ids.push(id.to_owned());
    ids.sort();
    if let Err(e) = Tracks::set_ext_state(
        daw,
        project.clone(),
        TrackRef::Guid(folder),
        PLACED_KEY,
        &ids.join(","),
    ) {
        tracing::warn!(hits.error = %e, "a placed hit could not be remembered");
    }
}

/// Forget which hits a hand placed: the edit started over.
pub fn forget_placed(daw: &daw::standalone::Standalone, project: &ProjectContext) {
    let Some(folder) = crate::track_regions::kit_folder(daw, project) else {
        return;
    };
    if let Err(e) =
        Tracks::set_ext_state(daw, project.clone(), TrackRef::Guid(folder), PLACED_KEY, "")
    {
        tracing::warn!(hits.error = %e, "the placed hits could not be forgotten");
    }
}

/// The kit as the slip edit reads it, from the hit markers.
///
/// Each piece's hits where they play now, each against its piece's
/// loudest. `None` when the song has no hit markers (the slip then
/// detects, as before).
///
/// Left out: a detected hit flagged as bleed and not approved — not a
/// stroke to cut on until someone says it is — the quieter stroke of a
/// flam, which moves with the louder, and an unchecked ghost note, which
/// rides with the groove around it rather than being put on a line of
/// its own (a ghost a hand placed or approved is cut on).
#[must_use]
pub fn kit_from_markers(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
) -> Option<Vec<crate::actions::KitPiece>> {
    let played = playback(daw, project);
    if played.is_empty() {
        return None;
    }
    // The quieter stroke of a flam after its main one is no cut of its
    // own: it rides in its main stroke's slice. Cut on, it is quantized
    // on its own and the flam's spacing goes — a grace note 33 ms after
    // the stroke pulled to the next 1/32, 47 ms after it. A grace before
    // its main stroke stays a cut: not cut on, it plays in the slice
    // before, and again in the main's slice reaching back (Unbreakable,
    // bar 166). The two are one gesture, moved by the louder.
    let peaks: HashMap<&str, (f64, f64)> = played
        .iter()
        .map(|p| (p.id.as_str(), (p.peak_db, p.time)))
        .collect();
    let grace = |p: &Played| {
        p.mark
            .flam_with
            .as_deref()
            .and_then(|other| peaks.get(other))
            .is_some_and(|(peak, time)| *peak > p.peak_db && *time < p.time)
    };
    let mut by_piece: BTreeMap<String, Vec<&Played>> = BTreeMap::new();
    for p in played.iter().filter(|p| {
        let checked = p.mark.origin != MarkOrigin::Auto;
        !(grace(p) || (!checked && (p.mark.bleed || p.mark.ghost)))
    }) {
        by_piece.entry(p.piece.clone()).or_default().push(p);
    }
    Some(
        by_piece
            .into_iter()
            .map(|(name, hits)| {
                let loudest = hits
                    .iter()
                    .map(|h| h.peak_db)
                    .fold(f64::NEG_INFINITY, f64::max);
                let hits = hits
                    .into_iter()
                    .map(|h| crate::drum_edit::KitHit {
                        time: h.time,
                        relative_db: h.peak_db - loudest,
                    })
                    .collect();
                (name.clone(), crate::drum_edit::Piece::of(&name), hits)
            })
            .collect(),
    )
}

/// How an edit left the hits, told from the markers: what a listener
/// would catch, counted.
#[derive(Facet, Clone, Debug, Default)]
pub struct Check {
    /// Hits in the span after the edit.
    pub hits: u32,
    /// Hits that played before and play nowhere now: cut away (a bleed
    /// nobody checked is not counted).
    pub lost: u32,
    /// Hits that play twice now: a double attack at a seam.
    pub doubled: u32,
    /// How far the moved hits now are from their targets, ms.
    pub off_median_ms: Option<f64>,
    pub off_worst_ms: Option<f64>,
    /// Moved hits more than 2 ms from their target.
    pub off_target: u32,
    /// Hits outside the span that play somewhere else now: the edit
    /// reached past its span.
    pub strayed: u32,
    /// Silences an edited kit track's slices leave between them: the
    /// drums cut out.
    pub gaps: u32,
    /// None lost, none doubled, none off target, nothing outside the span
    /// moved, no gap.
    pub ok: bool,
    /// The first few problems: what and where (project seconds).
    pub problems: Vec<String>,
}

/// Judge an edit of `span` by its hits: `before` is [`playback`] taken
/// before it, `targets` where its moved hits were sent.
#[must_use]
pub fn check(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    before: &[Played],
    targets: &[f64],
    span: (f64, f64),
) -> Check {
    let after = playback(daw, project);
    let inside = |t: f64| t >= span.0 && t <= span.1;
    let mut count: HashMap<&str, Vec<f64>> = HashMap::new();
    for p in &after {
        count.entry(p.id.as_str()).or_default().push(p.time);
    }
    let mut out = Check {
        hits: u32::try_from(after.iter().filter(|p| inside(p.time)).count()).unwrap_or(u32::MAX),
        ..Check::default()
    };
    for (id, times) in &count {
        if times.len() > 1 && times.iter().any(|t| inside(*t)) {
            out.doubled = out.doubled.saturating_add(1);
            if out.problems.len() < 20 {
                out.problems.push(format!("{id} plays twice: {times:.3?}"));
            }
        }
    }
    // A bleed nobody checked is another drum heard through this one's mic:
    // cut away with its slice, nothing is missed (as `audit` counts).
    let stroke = |b: &&Played| !(b.mark.bleed && b.mark.origin == MarkOrigin::Auto);
    for b in before.iter().filter(|b| inside(b.time)).filter(stroke) {
        if !count.contains_key(b.id.as_str()) {
            out.lost = out.lost.saturating_add(1);
            if out.problems.len() < 20 {
                out.problems.push(format!(
                    "{} ({}) at {:.3} plays nowhere now",
                    b.id, b.piece, b.time
                ));
            }
        }
    }
    let mut offs: Vec<f64> = Vec::new();
    for t in targets {
        let Some((d, near)) = after
            .iter()
            .map(|p| ((p.time - t).abs(), p))
            .filter(|(d, _)| *d < 0.010)
            .min_by(|a, b| a.0.total_cmp(&b.0))
        else {
            continue;
        };
        if d > 0.002 && out.problems.len() < 20 {
            out.problems.push(format!(
                "sent to {t:.4}, the nearest hit ({} {}) plays at {:.4}",
                near.piece, near.id, near.time
            ));
        }
        offs.push(d * 1_000.0);
    }
    offs.sort_by(f64::total_cmp);
    out.off_target = u32::try_from(offs.iter().filter(|d| **d > 2.0).count()).unwrap_or(u32::MAX);
    // Outside the span nothing moves: a gesture at its edge may take a hit
    // just past it along, nothing further.
    let near = |t: f64| t >= span.0 - STRAY_MARGIN && t <= span.1 + STRAY_MARGIN;
    for b in before.iter().filter(|b| !near(b.time)) {
        let now = count.get(b.id.as_str());
        if now.is_some_and(|times| !times.iter().any(|t| (t - b.time).abs() < 0.001)) {
            out.strayed = out.strayed.saturating_add(1);
            if out.problems.len() < 20 {
                out.problems.push(format!(
                    "{} ({}) at {:.3}, outside the span, plays at {:.3?} now",
                    b.id,
                    b.piece,
                    b.time,
                    now.map(Vec::as_slice).unwrap_or_default()
                ));
            }
        }
    }
    for (track, from, to) in gaps(daw, project, span) {
        out.gaps = out.gaps.saturating_add(1);
        if out.problems.len() < 20 {
            out.problems.push(format!(
                "{track}: silent from {from:.4} to {to:.4} ({:.1} ms)",
                (to - from) * 1_000.0
            ));
        }
    }
    out.off_median_ms = offs.get(offs.len() / 2).copied();
    out.off_worst_ms = offs.last().copied();
    out.ok = out.lost == 0
        && out.doubled == 0
        && out.off_target == 0
        && out.strayed == 0
        && out.gaps == 0;
    out
}

/// The shortest silence between two slices that counts as a gap,
/// seconds: under it is a seam's rounding.
const GAP: f64 = 0.000_5;

/// Where an edited kit track — one with an EDIT lane — plays nothing
/// between two of its slices in `span`: `(track, from, to)`.
#[must_use]
pub fn gaps(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    span: (f64, f64),
) -> Vec<(String, f64, f64)> {
    use daw::service::{Items as _, TrackRef, Tracks};
    let mut out = Vec::new();
    for track in Tracks::all(daw, project.clone()) {
        if !track
            .lane_names
            .iter()
            .any(|n| n == crate::drum_lanes::EDIT)
        {
            continue;
        }
        let mut slices: Vec<(f64, f64)> = daw
            .get_items(project.clone(), TrackRef::Guid(track.guid.clone()))
            .iter()
            .filter(|i| !i.muted && daw_ui::studio::project::on_playing_lane(&track, i))
            .map(|i| {
                let at = i.position.as_seconds();
                (at, at + i.length.as_seconds())
            })
            .filter(|(at, end)| *end > span.0 && *at < span.1)
            .collect();
        slices.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut covered = f64::NEG_INFINITY;
        for (at, end) in slices {
            if covered.is_finite() && at - covered > GAP {
                out.push((track.name.clone(), covered, at));
            }
            covered = covered.max(end);
        }
    }
    out
}

/// One piece's hits under the edit as it stands.
#[derive(Facet, Clone, Debug, Default)]
pub struct AuditPiece {
    pub piece: String,
    pub hits: u32,
    /// Played exactly once.
    pub playing: u32,
    /// Played nowhere: cut away by the edit.
    pub lost: u32,
    /// Of them, strokes (not ghosts, not bleed): what a listener misses.
    pub lost_strokes: u32,
    /// Played twice: a double attack at a seam.
    pub doubled: u32,
    /// The lost strokes and the doubled hits, where (first 40).
    pub problems: Vec<String>,
}

/// How far past a span's edge an edit may take a hit along, seconds: a
/// gesture's width.
const STRAY_MARGIN: f64 = 0.05;

/// How the edit as it stands plays every hit, in `span` (as recorded).
#[must_use]
pub fn audit(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    span: (f64, f64),
) -> Vec<AuditPiece> {
    let rows = hits(daw, project, &[]);
    let mut plays: HashMap<String, usize> = HashMap::new();
    for p in playback(daw, project) {
        let n = plays.entry(p.id).or_default();
        *n = n.saturating_add(1);
    }
    let mut out: BTreeMap<String, AuditPiece> = BTreeMap::new();
    for h in rows.iter().filter(|h| h.time >= span.0 && h.time <= span.1) {
        let line = out.entry(h.piece.clone()).or_insert_with(|| AuditPiece {
            piece: h.piece.clone(),
            ..AuditPiece::default()
        });
        line.hits = line.hits.saturating_add(1);
        let flagged = h.flags.iter().any(|f| f == "ghost" || f == "bleed");
        match plays.get(&h.id).copied().unwrap_or(0) {
            0 => {
                line.lost = line.lost.saturating_add(1);
                if !flagged {
                    line.lost_strokes = line.lost_strokes.saturating_add(1);
                    if line.problems.len() < 40 {
                        line.problems.push(format!(
                            "lost: {} at {:.3} s (bar {} beat {:.2}), {:.1} dBFS",
                            h.id, h.time, h.bar, h.beat, h.peak_db
                        ));
                    }
                }
            }
            1 => line.playing = line.playing.saturating_add(1),
            n => {
                line.doubled = line.doubled.saturating_add(1);
                if line.problems.len() < 40 {
                    line.problems.push(format!(
                        "plays {n} times: {} recorded at {:.3} s (bar {} beat {:.2})",
                        h.id, h.time, h.bar, h.beat
                    ));
                }
            }
        }
    }
    out.into_values().collect()
}

// ── zones ─────────────────────────────────────────────────────────────

/// The ruler lane zones are kept on: the Drums lane, where the song's
/// drum notes go.
pub const ZONE_LANE: u32 = session::ruler_lanes::InstrumentLane::Drums.lane_index();
/// What that lane is called.
pub const ZONE_LANE_NAME: &str = session::ruler_lanes::InstrumentLane::Drums.display_name();

/// What a zone does.
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ZoneKind {
    /// The drummer leaves the groove: left as played unless asked.
    Fill,
    /// Not to be quantized, for any reason.
    Exception,
}

/// Who put a zone there, or what was decided about it.
#[derive(Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ZoneOrigin {
    /// Found, not yet looked at.
    Auto,
    Manual,
    Approved,
    /// Found and turned down: kept, so finding again does not bring it
    /// back, and holding nothing.
    Rejected,
}

/// A zone, as the list gives it.
#[derive(Facet, Clone, Debug)]
pub struct Zone {
    pub guid: String,
    pub start: f64,
    pub end: f64,
    pub kind: ZoneKind,
    pub origin: ZoneOrigin,
    /// Why it was found, or anything said about it.
    pub note: String,
    /// A fill quantized on a grid of its own (a whole note's division):
    /// its hits put on it. `None`: left as played.
    pub grid: Option<u32>,
}

/// A zone's name on the ruler: `Fill`, `Fill (auto)`, `Fill (approved)`,
/// `No quantize (rejected)` — what it is, then who (nothing: placed by
/// hand), then anything said about it.
fn zone_name(kind: ZoneKind, origin: ZoneOrigin, note: &str) -> String {
    let what = match kind {
        ZoneKind::Fill => "Fill",
        ZoneKind::Exception => "No quantize",
    };
    let who = match origin {
        ZoneOrigin::Auto => " (auto)",
        ZoneOrigin::Rejected => " (rejected)",
        ZoneOrigin::Approved => " (approved)",
        ZoneOrigin::Manual => "",
    };
    if note.is_empty() {
        format!("{what}{who}")
    } else {
        format!("{what}{who}: {note}")
    }
}

/// A region's name read back as a zone's kind, origin and note.
fn parse_zone(name: &str) -> Option<(ZoneKind, ZoneOrigin, String)> {
    let (head, note) = name.split_once(": ").unwrap_or((name, ""));
    let (kind, rest) = if let Some(r) = head.strip_prefix("Fill") {
        (ZoneKind::Fill, r)
    } else if let Some(r) = head.strip_prefix("No quantize") {
        (ZoneKind::Exception, r)
    } else {
        return None;
    };
    let origin = match rest.trim() {
        "(auto)" => ZoneOrigin::Auto,
        "(rejected)" => ZoneOrigin::Rejected,
        "(approved)" => ZoneOrigin::Approved,
        _ => ZoneOrigin::Manual,
    };
    Some((kind, origin, note.to_owned()))
}

/// The song's zones, in time order.
#[must_use]
pub fn zones(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Vec<Zone> {
    let mut out: Vec<Zone> = daw::service::Regions::all(daw, project.clone())
        .into_iter()
        .filter(|r| r.lane == Some(ZONE_LANE))
        .filter_map(|r| {
            let (kind, origin, note) = parse_zone(&r.name)?;
            Some(Zone {
                guid: r.guid.clone().unwrap_or_default(),
                start: r.time_range.start_seconds(),
                end: r.time_range.end_seconds(),
                kind,
                origin,
                note,
                grid: None,
            })
        })
        .collect();
    // And the kit's own: its folder's regions (and its tracks') that say
    // how a stretch is edited.
    out.extend(
        crate::track_regions::kit_marks(daw, project)
            .into_iter()
            .filter_map(|(track, r)| {
                let kind = match r.kind {
                    crate::track_regions::RegionKind::Fill => ZoneKind::Fill,
                    crate::track_regions::RegionKind::NoQuantize => ZoneKind::Exception,
                    crate::track_regions::RegionKind::Grid
                    | crate::track_regions::RegionKind::Note => return None,
                };
                Some(Zone {
                    guid: format!("{TRACK_ZONE}{track}:{}", r.id),
                    start: r.start,
                    end: r.end,
                    kind,
                    origin: zone_origin(r.origin),
                    note: r.note,
                    grid: r.division.filter(|_| kind == ZoneKind::Fill),
                })
            }),
    );
    out.sort_by(|a, b| a.start.total_cmp(&b.start));
    out
}

/// A zone kept as a region of a kit track's own: `track:<track>:<id>`.
const TRACK_ZONE: &str = "track:";

/// The track and region id of a zone kept on a track.
fn track_zone(guid: &str) -> Option<(&str, &str)> {
    guid.strip_prefix(TRACK_ZONE)?.rsplit_once(':')
}

const fn zone_origin(origin: crate::track_regions::Origin) -> ZoneOrigin {
    match origin {
        crate::track_regions::Origin::Manual => ZoneOrigin::Manual,
        crate::track_regions::Origin::Auto => ZoneOrigin::Auto,
        crate::track_regions::Origin::Approved => ZoneOrigin::Approved,
        crate::track_regions::Origin::Rejected => ZoneOrigin::Rejected,
    }
}

const fn region_origin(origin: ZoneOrigin) -> crate::track_regions::Origin {
    match origin {
        ZoneOrigin::Manual => crate::track_regions::Origin::Manual,
        ZoneOrigin::Auto => crate::track_regions::Origin::Auto,
        ZoneOrigin::Approved => crate::track_regions::Origin::Approved,
        ZoneOrigin::Rejected => crate::track_regions::Origin::Rejected,
    }
}

const fn region_kind(kind: ZoneKind) -> crate::track_regions::RegionKind {
    match kind {
        ZoneKind::Fill => crate::track_regions::RegionKind::Fill,
        ZoneKind::Exception => crate::track_regions::RegionKind::NoQuantize,
    }
}

/// Remove the zone `guid`, wherever it is kept.
fn remove_zone(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    guid: &str,
) -> Result<(), String> {
    if let Some((track, id)) = track_zone(guid) {
        return crate::track_regions::change(daw, project, (track, id), |_| None);
    }
    let id = zone_region(daw, project, guid).ok_or_else(|| format!("no zone {guid}"))?;
    daw::service::Regions::remove(daw, project.clone(), id).map_err(|e| e.to_string())
}

/// Whether a zone that holds hits (anything but a rejected one) holds
/// `time`, a beat being `beat_length` seconds there.
///
/// As a found fill counts its hits: each belongs to the slot it is
/// nearest the start of, so a landing pushed a little early is still the
/// next bar's — quantized — and not the fill's.
#[must_use]
pub fn zone_holds(zones: &[Zone], time: f64, beat_length: f64) -> bool {
    let shift = beat_length / crate::drum_edit::SLOTS / 2.0;
    zones.iter().any(|z| {
        z.origin != ZoneOrigin::Rejected && time >= z.start - shift && time < z.end - shift
    })
}

/// Put a zone on the kit: a region of its folder's own (the song's ruler
/// when there is no kit folder). Its guid.
pub(crate) fn add_zone(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    (start, end): (f64, f64),
    (kind, origin, note): (ZoneKind, ZoneOrigin, &str),
) -> Result<String, String> {
    if let Some(folder) = crate::track_regions::kit_folder(daw, project) {
        let id = crate::track_regions::add(
            daw,
            project,
            &folder,
            crate::track_regions::TrackRegion {
                id: crate::track_regions::fresh_id(),
                start,
                end,
                kind: region_kind(kind),
                division: None,
                origin: region_origin(origin),
                note: note.to_owned(),
            },
        )?;
        return Ok(format!("{TRACK_ZONE}{folder}:{id}"));
    }
    let id = daw::service::Regions::add(
        daw,
        project.clone(),
        start,
        end,
        &zone_name(kind, origin, note),
    )
    .map_err(|e| e.to_string())?;
    daw::service::Regions::set_lane(daw, project.clone(), id, Some(ZONE_LANE))
        .map_err(|e| e.to_string())?;
    let color = match (kind, origin) {
        (_, ZoneOrigin::Rejected) => 0x0055_5555,
        (ZoneKind::Fill, ZoneOrigin::Auto) => 0x00B0_8030,
        (ZoneKind::Fill, _) => 0x00E0_A030,
        (ZoneKind::Exception, _) => 0x00C0_4040,
    };
    daw::service::Regions::set_color(daw, project.clone(), id, color).map_err(|e| e.to_string())?;
    Ok(zone_guid(daw, project, id).unwrap_or_default())
}

/// The guid of the ruler region `id`.
fn zone_guid(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    id: u32,
) -> Option<String> {
    daw::service::Regions::all(daw, project.clone())
        .into_iter()
        .find(|r| r.id == Some(id))
        .and_then(|r| r.guid)
}

/// The region id of the zone `guid`.
fn zone_region(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    guid: &str,
) -> Option<u32> {
    daw::service::Regions::all(daw, project.clone())
        .into_iter()
        .find(|r| r.guid.as_deref() == Some(guid))
        .and_then(|r| r.id)
}

/// Propose the fills the hits show as `auto` zones, in `span`: the auto
/// ones there replaced, none laid over a zone already decided (manual,
/// approved, rejected). How many were proposed.
///
/// # Errors
/// A region would not be written.
pub fn find_zones(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    fills: &[crate::drum_edit::Fill],
    span: (f64, f64),
) -> Result<u32, String> {
    let now = zones(daw, project);
    Projects::begin_undo_block(daw, project.clone(), "Find the fills");
    daw::service::Projects::set_ruler_lane_name(daw, project.clone(), ZONE_LANE, ZONE_LANE_NAME);
    for z in now
        .iter()
        .filter(|z| z.origin == ZoneOrigin::Auto && z.start < span.1 && z.end > span.0)
    {
        remove_zone(daw, project, &z.guid)?;
    }
    let decided: Vec<&Zone> = now
        .iter()
        .filter(|z| z.origin != ZoneOrigin::Auto)
        .collect();
    let mut n = 0_u32;
    for f in fills.iter().filter(|f| f.from < span.1 && f.to > span.0) {
        if decided.iter().any(|z| z.start < f.to && z.end > f.from) {
            continue;
        }
        add_zone(
            daw,
            project,
            (f.from, f.to),
            (ZoneKind::Fill, ZoneOrigin::Auto, &f.why.join(", ")),
        )?;
        n = n.saturating_add(1);
    }
    Projects::end_undo_block(daw, project.clone(), "Find the fills", None);
    Ok(n)
}

// ── the actions ───────────────────────────────────────────────────────

static DETECT: ActionMeta = meta(
    "HITS_DETECT",
    "hits_detect",
    "Detect drum hits",
    "Find every drum hit, once, and write it down as a hit marker on each of its piece's recordings (its mics and trigger, one id across them): the triggers fit each close mic's gate, a dead trigger is caught and its piece read from its mic alone with the other mics' bleed refused. Runs on the takes the song plays (comp or EDIT lanes; `all_takes` for every take kept). Replaces only unchecked detected hits (`auto`) in `from`/`to` (project seconds; the whole song when left out) on `pieces` (all when left out) — hits placed by hand or approved are kept. Settings (HITS_SETTINGS) can be given for this run alone. Says per piece how it was found, the trigger's verdict, the fitted gate, and the counts: both/trigger-only/mic-only/ghosts/flams/unsure (the ones to look at).",
    "Drums",
    "",
);
static SETTINGS: ActionMeta = meta(
    "HITS_SETTINGS",
    "hits_settings",
    "Hit detector settings",
    "The hit detector's settings, sticky: `trust` (Auto: the trigger where it works; Trigger; Mic), `threshold_db` (a fixed gate for every mic, dBFS; 0 or above: fit each to its trigger), `crest_db`, `retrig_ms`, `trigger_under_db`. Any given are changed and kept; none given, just read.",
    "Drums",
    "",
);
static LIST: ActionMeta = meta(
    "HITS_LIST",
    "hits_list",
    "Drum hits found",
    "The hit markers: each hit's id, piece, `time` (project seconds, as recorded), `now` (where the song plays it now, after any edit; none when no slice plays it), bar and beat (where it plays now), peak (dBFS), confidence, what heard it (both, trigger, mic), flags (ghost, flam, bleed, unsure) and who placed it (auto, manual, approved). `pieces`, `from`/`to` (where it plays now), `flag` (only hits with it), `origin`, `heard` (both, trigger, mic) narrow it; paged by `offset`/`limit`.",
    "Drums",
    "",
);
static EXPLAIN: ActionMeta = meta(
    "HITS_EXPLAIN",
    "hits_explain",
    "Look at a drum hit",
    "What the detector saw around a hit — `id`, or `time` as recorded (a hit's `time`, not its `now`) with `piece`: the peaks (dBFS, `points` of them, 120 unless asked) of the piece's mics, its trigger and every other close mic of the take, `window_ms` either side (40), lined up; the attack start nearest `time`; and the piece's hits there. For deciding whether a hit is real, bleed, a ghost or misplaced.",
    "Drums",
    "",
);
static ADD: ActionMeta = meta(
    "HITS_ADD",
    "hits_add",
    "Place a drum hit",
    "Place a hit of `piece` by hand at `time` (project seconds, as recorded; with `now`, where it plays now — after any edit) on every recording of the piece — snapped to the attack start on its first mic unless `snap` is false. Hand-placed hits are never replaced by a detection run. Says its id.",
    "Drums",
    "",
);
static REMOVE: ActionMeta = meta(
    "HITS_REMOVE",
    "hits_remove",
    "Remove drum hits",
    "Remove the hits `ids` from every recording that carries them. One undo step.",
    "Drums",
    "",
);
static MOVE: ActionMeta = meta(
    "HITS_MOVE",
    "hits_move",
    "Move a drum hit",
    "Move the hit `id` to `time` (project seconds, as recorded) or by `by_ms`, on every recording that carries it; it becomes hand-placed (`manual`).",
    "Drums",
    "",
);
static MARK: ActionMeta = meta(
    "HITS_MARK",
    "hits_mark",
    "Mark drum hits",
    "Set what is known about the hits `ids`: `approve` (true: checked — kept by every run; false: back to auto), `ghost`, `bleed`, `note`. One undo step.",
    "Drums",
    "",
);

static AUDIT: ActionMeta = meta(
    "HITS_AUDIT",
    "hits_audit",
    "Audit the drum edit",
    "How the drum edit as it stands plays every hit (in `from`/`to`, as recorded): per piece, how many play once, how many play nowhere (cut away by the edit — and of them the strokes, not ghosts or bleed: what a listener misses), how many play twice (a double attack at a seam), and where. Read-only; run it after any edit.",
    "Drums",
    "",
);
static ZONES_LIST: ActionMeta = meta(
    "ZONES_LIST",
    "zones_list",
    "Edit zones",
    "The song's edit zones, on the ruler's Drums lane: fills (left as played by a quantize unless asked) and no-quantize exceptions — each one's guid, start and end (project seconds), kind, and who put it there (Auto: found, not yet looked at; Manual; Approved; Rejected: found and turned down, holding nothing).",
    "Drums",
    "",
);
static ZONES_ADD: ActionMeta = meta(
    "ZONES_ADD",
    "zones_add",
    "Add an edit zone",
    "Put a zone from `start` to `end` (project seconds): `kind` Fill or Exception (no quantize), with an optional `note`. Placed by hand, so finding fills again never replaces it.",
    "Drums",
    "",
);
static ZONES_EDIT: ActionMeta = meta(
    "ZONES_EDIT",
    "zones_edit",
    "Edit an edit zone",
    "Change the zone `guid`: `origin` (Approved to keep a found one, Rejected to turn it down — kept so finding again does not bring it back), `kind`, `start`/`end`, `note`, `grid` (a fill quantized on a grid of its own — `1/16`, `1/8T` — its hits put on it whatever the quantize skips; `none`: left as played again); or `remove` it.",
    "Drums",
    "",
);

#[derive(Facet, Default)]
struct SpanOnly {
    from: Option<f64>,
    to: Option<f64>,
}

#[derive(Facet)]
struct ZoneAddArg {
    start: f64,
    end: f64,
    kind: ZoneKind,
    note: Option<String>,
}

#[derive(Facet)]
struct ZoneEditArg {
    guid: String,
    origin: Option<ZoneOrigin>,
    kind: Option<ZoneKind>,
    start: Option<f64>,
    end: Option<f64>,
    note: Option<String>,
    remove: Option<bool>,
    /// A fill quantized on this grid (`1/16`, `1/8T`); `none` as played.
    grid: Option<String>,
}

#[derive(Facet, Default)]
struct DetectArg {
    from: Option<f64>,
    /// Every recorded take, not only those the song plays.
    all_takes: Option<bool>,
    to: Option<f64>,
    #[facet(default)]
    pieces: Vec<String>,
    #[facet(flatten)]
    settings: HitPatch,
}

#[derive(Facet, Default)]
struct ListArg {
    #[facet(default)]
    pieces: Vec<String>,
    from: Option<f64>,
    to: Option<f64>,
    flag: Option<String>,
    origin: Option<String>,
    /// Only hits heard so: `both`, `trigger` or `mic`.
    heard: Option<String>,
    offset: Option<u32>,
    limit: Option<u32>,
}

#[derive(Facet)]
struct ListPage {
    total: u32,
    offset: u32,
    hits: Vec<HitRow>,
}

#[derive(Facet)]
struct ExplainArg {
    id: Option<String>,
    piece: Option<String>,
    time: Option<f64>,
    window_ms: Option<f64>,
    points: Option<u32>,
}

#[derive(Facet)]
struct AddArg {
    piece: String,
    time: f64,
    /// `time` is where it plays now, after any edit (as a click on the
    /// arrangement says it), not as recorded.
    now: Option<bool>,
    snap: Option<bool>,
    note: Option<String>,
}

#[derive(Facet)]
struct IdsArg {
    ids: Vec<String>,
}

#[derive(Facet)]
struct MoveArg {
    id: String,
    time: Option<f64>,
    by_ms: Option<f64>,
}

#[derive(Facet)]
struct MarkArg {
    ids: Vec<String>,
    approve: Option<bool>,
    ghost: Option<bool>,
    bleed: Option<bool>,
    note: Option<String>,
}

/// Register the hit detector's actions.
pub fn register(list: &ActionList) {
    register_reads(list);
    register_edits(list);
    register_zones(list);
}

/// Detecting, and reading what was found.
fn register_reads(list: &ActionList) {
    list.register(&SETTINGS, Effect::Mutating, |a: HitPatch| {
        Ok(set_settings(&a))
    });
    list.register(&DETECT, Effect::Mutating, |a: DetectArg| {
        let (daw, project) = song()?;
        let settings = settings().with(&a.settings);
        detect(
            daw,
            &project,
            (
                a.from.unwrap_or(f64::NEG_INFINITY),
                a.to.unwrap_or(f64::INFINITY),
            ),
            &a.pieces,
            &settings,
            a.all_takes.unwrap_or(false),
        )
    });
    list.register(&LIST, Effect::ReadOnly, |a: ListArg| {
        let (daw, project) = song()?;
        let (from, to) = (
            a.from.unwrap_or(f64::NEG_INFINITY),
            a.to.unwrap_or(f64::INFINITY),
        );
        let all: Vec<HitRow> = hits(daw, &project, &a.pieces)
            .into_iter()
            .filter(|h| h.at() >= from && h.at() <= to)
            .filter(|h| {
                a.flag
                    .as_ref()
                    .is_none_or(|f| h.flags.iter().any(|x| x.eq_ignore_ascii_case(f)))
            })
            .filter(|h| {
                a.origin
                    .as_ref()
                    .is_none_or(|o| h.origin.eq_ignore_ascii_case(o))
            })
            .filter(|h| {
                a.heard
                    .as_ref()
                    .is_none_or(|w| h.heard.eq_ignore_ascii_case(w))
            })
            .collect();
        let offset = a.offset.unwrap_or(0);
        let limit = a.limit.unwrap_or(100).min(500);
        Ok(ListPage {
            total: u32::try_from(all.len()).unwrap_or(u32::MAX),
            offset,
            hits: all
                .into_iter()
                .skip(usize::try_from(offset).unwrap_or(usize::MAX))
                .take(usize::try_from(limit).unwrap_or(usize::MAX))
                .collect(),
        })
    });
    list.register(&EXPLAIN, Effect::ReadOnly, |a: ExplainArg| {
        let (daw, project) = song()?;
        let (piece, time) = match (&a.id, &a.piece, a.time) {
            (Some(id), _, _) => hits(daw, &project, &[])
                .into_iter()
                .find(|h| &h.id == id)
                .map(|h| (h.piece, h.time))
                .ok_or_else(|| format!("no hit {id}"))?,
            (None, Some(piece), Some(time)) => (piece.clone(), time),
            _ => return Err("give a hit's `id`, or `piece` and `time`".to_owned()),
        };
        explain(
            daw,
            &project,
            &piece,
            time,
            (
                a.window_ms.unwrap_or(40.0) / 1_000.0,
                usize::try_from(a.points.unwrap_or(120)).unwrap_or(120),
            ),
        )
    });
}

/// The song's edit zones.
fn register_zones(list: &ActionList) {
    list.register(&AUDIT, Effect::ReadOnly, |a: SpanOnly| {
        let (daw, project) = song()?;
        Ok(audit(
            daw,
            &project,
            (
                a.from.unwrap_or(f64::NEG_INFINITY),
                a.to.unwrap_or(f64::INFINITY),
            ),
        ))
    });
    list.register(&ZONES_LIST, Effect::ReadOnly, |(): ()| {
        let (daw, project) = song()?;
        Ok(zones(daw, &project))
    });
    list.register(&ZONES_ADD, Effect::Mutating, |a: ZoneAddArg| {
        let (daw, project) = song()?;
        if a.end <= a.start {
            return Err("a zone ends after it starts".to_owned());
        }
        Projects::begin_undo_block(daw, project.clone(), "Add an edit zone");
        daw::service::Projects::set_ruler_lane_name(
            daw,
            project.clone(),
            ZONE_LANE,
            ZONE_LANE_NAME,
        );
        let added = add_zone(
            daw,
            &project,
            (a.start, a.end),
            (
                a.kind,
                ZoneOrigin::Manual,
                a.note.as_deref().unwrap_or_default(),
            ),
        );
        Projects::end_undo_block(daw, project.clone(), "Add an edit zone", None);
        added?;
        Ok(zones(daw, &project)
            .into_iter()
            .find(|z| (z.start - a.start).abs() < 1e-6 && (z.end - a.end).abs() < 1e-6))
    });
    list.register(&ZONES_EDIT, Effect::Mutating, |a: ZoneEditArg| {
        let (daw, project) = song()?;
        let zone = zones(daw, &project)
            .into_iter()
            .find(|z| z.guid == a.guid)
            .ok_or_else(|| format!("no zone {}", a.guid))?;
        // A grid asked of a zone on the ruler: it is moved onto the kit's
        // folder, where a region keeps one.
        let grid = match a.grid.as_deref().map(str::trim) {
            None => None,
            Some("" | "none" | "as played") => Some(None),
            Some(text) => Some(Some(crate::track_regions::parse_grid(text)?)),
        };
        if grid.is_some() && track_zone(&a.guid).is_none() {
            remove_zone(daw, &project, &a.guid)?;
            let moved = add_zone(
                daw,
                &project,
                (a.start.unwrap_or(zone.start), a.end.unwrap_or(zone.end)),
                (
                    a.kind.unwrap_or(zone.kind),
                    a.origin.unwrap_or(zone.origin),
                    a.note.as_deref().unwrap_or(&zone.note),
                ),
            )?;
            if let Some((track, id)) = track_zone(&moved) {
                crate::track_regions::change(daw, &project, (track, id), |mut r| {
                    r.division = grid.flatten();
                    Some(r)
                })?;
            }
            return Ok(zones(daw, &project).into_iter().find(|z| z.guid == moved));
        }
        // Kept on a kit track: changed there.
        if let Some((track, id)) = track_zone(&a.guid) {
            if a.remove == Some(true) {
                remove_zone(daw, &project, &a.guid)?;
                return Ok(None);
            }
            let (start, end) = (a.start.unwrap_or(zone.start), a.end.unwrap_or(zone.end));
            crate::track_regions::change(daw, &project, (track, id), |mut r| {
                r.start = start;
                r.end = end;
                r.kind = region_kind(a.kind.unwrap_or(zone.kind));
                r.origin = region_origin(a.origin.unwrap_or(zone.origin));
                if let Some(note) = &a.note {
                    r.note.clone_from(note);
                }
                if let Some(grid) = grid {
                    r.division = grid;
                }
                Some(r)
            })?;
            return Ok(zones(daw, &project).into_iter().find(|z| z.guid == a.guid));
        }
        let id =
            zone_region(daw, &project, &a.guid).ok_or_else(|| format!("no zone {}", a.guid))?;
        let failed = |e: daw_proto::DawError| e.to_string();
        if a.remove == Some(true) {
            daw::service::Regions::remove(daw, project, id).map_err(failed)?;
            return Ok(None);
        }
        let (start, end) = (a.start.unwrap_or(zone.start), a.end.unwrap_or(zone.end));
        if (start, end) != (zone.start, zone.end) {
            daw::service::Regions::set_bounds(daw, project.clone(), id, start, end)
                .map_err(failed)?;
        }
        let name = zone_name(
            a.kind.unwrap_or(zone.kind),
            a.origin.unwrap_or(zone.origin),
            a.note.as_deref().unwrap_or(&zone.note),
        );
        daw::service::Regions::rename(daw, project.clone(), id, &name).map_err(failed)?;
        Ok(zones(daw, &project).into_iter().find(|z| z.guid == a.guid))
    });
}

/// Correcting what was found.
fn register_edits(list: &ActionList) {
    list.register(&ADD, Effect::Mutating, |a: AddArg| {
        let (daw, project) = song()?;
        let time = if a.now.unwrap_or(false) {
            recorded_at(daw, &project, &a.piece, a.time)
                .ok_or_else(|| format!("nothing of {} plays at {:.3} s", a.piece, a.time))?
        } else {
            a.time
        };
        add_hit(
            daw,
            &project,
            &a.piece,
            time,
            (a.snap.unwrap_or(true), a.note.unwrap_or_default()),
        )
    });
    list.register(&REMOVE, Effect::Mutating, |a: IdsArg| {
        let (daw, project) = song()?;
        let ids: BTreeSet<String> = a.ids.into_iter().collect();
        Ok(edit_hits(
            daw,
            &project,
            &ids,
            "Remove drum hits",
            |_, _| false,
        ))
    });
    list.register(&MOVE, Effect::Mutating, |a: MoveArg| {
        let (daw, project) = song()?;
        let which = BTreeSet::from([a.id.clone()]);
        let n = edit_hits(daw, &project, &which, "Move a drum hit", |m, origin| {
            if let Some(time) = a.time {
                m.source_position_seconds = time - origin;
            } else if let Some(ms) = a.by_ms {
                m.source_position_seconds += ms / 1_000.0;
            }
            if let Some(h) = m.hit.as_mut() {
                h.origin = MarkOrigin::Manual;
            }
            true
        });
        if n == 0 {
            return Err(format!("no hit {}", a.id));
        }
        Ok(n)
    });
    list.register(&MARK, Effect::Mutating, |a: MarkArg| {
        let (daw, project) = song()?;
        let ids: BTreeSet<String> = a.ids.iter().cloned().collect();
        Ok(edit_hits(daw, &project, &ids, "Mark drum hits", |m, _| {
            if let Some(h) = m.hit.as_mut() {
                match a.approve {
                    Some(true) => h.origin = MarkOrigin::Approved,
                    Some(false) if h.origin == MarkOrigin::Approved => h.origin = MarkOrigin::Auto,
                    _ => {}
                }
                if let Some(g) = a.ghost {
                    h.ghost = g;
                }
                if let Some(b) = a.bleed {
                    h.bleed = b;
                }
                if let Some(n) = &a.note {
                    h.note.clone_from(n);
                }
            }
            true
        }))
    });
}
