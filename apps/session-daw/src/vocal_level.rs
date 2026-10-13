//! Leveling a vocal: the four lanes — gate, de-breath, ride, de-ess — made
//! for each of its items, and their sum played as the item's volume.
//!
//! What a mix engineer does to a vocal before any compressor: the noise
//! between lines taken down, the breaths tucked under, the esses eased,
//! and every line ridden to where the song wants it — a verse sung 10 dB
//! under the chorus brought up to sit with it. Each is a curve of its own
//! ([`level_dsp::envelope`]), made from one analysis of the item's audio,
//! so any one can be switched off or redrawn without the others; their
//! sum in dB is the take's volume envelope, which plays, rides with the
//! audio through every split and move, and is what the waveform draws.
//!
//! The ride is the one that reaches across the song. Each item is read
//! by itself, but the level every line is ridden to is one level for the
//! track — the middle of where its lines are sung — so a quiet verse is
//! lifted toward the chorus, not only evened against itself. It is read
//! a phrase at a time ([`level_dsp::envelope::phrase_levels`]), so the
//! words inside a line keep their shape. Then the makeup: one gain over
//! the whole track that puts the leveled vocal's sung level where its
//! louder lines were sung: the final amplitude, the choruses keeping
//! their level and the verses brought up to it.
//!
//! The gate and the breath window are set from each recording's own
//! noise floor, not a fixed level: a vocal sung 20 dB under another's
//! gate would be shut.
//!
//! The four lanes are kept with the track, per item (`FTS:level:<item>`),
//! with the settings they came from — editable, and read back as they
//! were made — and where in its recording each was read
//! (`FTS:leveler:from:<item>`), so they stay on the audio through a slip,
//! and a split's new half or a comp's slice is drawn with the lanes of the
//! item it was cut from. The track's dials (`FTS:leveler:stages`, see
//! [`crate::leveler`]) switch each stage on or off and say how far it
//! goes; what plays is the four through them.
//!
//! A two-channel vocal is a dual-capsule recording (front left, rear
//! right; FTS Mic makes it the mic): it is read off its front capsule.

#![cfg(feature = "native")]

use std::collections::HashMap;

use daw::service::{Items as _, ProjectContext, Takes as _, TrackRef, Tracks};
use daw_proto::ItemRef;
use facet::Facet;
use level_dsp::envelope::{self as env, Analysis, Contributions, EnvPoint};

/// How much of a phrase a ride hears, seconds: a line's words, not each
/// syllable.
const PHRASE_S: f64 = 0.5;

/// How far above a recording's noise floor its gate opens, dB.
const GATE_ABOVE_DB: f64 = 10.0;

/// How far a shut gate takes the noise down, dB.
const GATE_FLOOR_DB: f64 = -30.0;

/// How far under the song's level a sound has to be to be a breath (or a
/// soft consonant) to tuck under, dB.
const BREATH_UNDER_DB: f64 = 18.0;

/// How far a breath is taken down, dB.
const BREATH_DB: f64 = 10.0;

/// How far an "ess" is eased, dB.
const ESS_DB: f64 = 4.0;

/// The share of a track's sung phrases below its quiet lines' level: the
/// ride's target is halfway from there to the loud lines' ([`LOUD_SHARE`]),
/// so the range reaches both ends alike. Where the target sits does not
/// change what is heard — the makeup puts the track back at its loud
/// level — only how far the ride's range reaches each way.
const QUIET_SHARE: f64 = 0.1;

/// The share of a track's sung phrases below the level its louder lines
/// (the choruses) are sung at: what the makeup puts the leveled track at,
/// so the loud lines keep their level and the quiet ones come up to them.
const LOUD_SHARE: f64 = 0.9;

/// The most the makeup moves the whole track, dB.
const MAKEUP_MAX_DB: f64 = 24.0;

/// What ends a track's ext key holding one item's lanes.
pub const LANES_KEY: &str = "FTS:level:";

/// The knobs a hand turns.
#[derive(Clone, Copy, Debug)]
pub struct Knobs {
    /// How far each line is brought to the song's level, 0..1.
    pub consistency: f64,
    /// The most a line is raised or lowered, dB.
    pub range_db: f64,
    /// The level to ride to, dBFS; the track's own louder lines' when not
    /// given.
    pub target_db: Option<f64>,
}

impl Default for Knobs {
    fn default() -> Self {
        Self {
            consistency: 0.9,
            range_db: 12.0,
            target_db: None,
        }
    }
}

/// One section's sung level, before and after.
#[derive(Facet, Clone, Debug)]
pub struct SectionLevel {
    pub name: String,
    /// dBFS, the power mean of its sung blocks.
    pub before_db: f64,
    pub after_db: f64,
}

/// What leveling one vocal did, or would.
#[derive(Facet, Clone, Debug, Default)]
pub struct Leveled {
    pub track: String,
    pub items: u32,
    /// The level every line was ridden toward, dBFS (front capsule).
    pub target_db: f64,
    /// Where the makeup put the leveled track's sung level: its louder
    /// lines' level as sung, dBFS.
    pub level_db: f64,
    /// The gain over the whole track that brought it there, dB.
    pub makeup_db: f64,
    /// The loudest section's sung level less the quietest's, dB.
    pub spread_before_db: f64,
    pub spread_after_db: f64,
    pub sections: Vec<SectionLevel>,
    /// The four lanes' points, summed over its items, and the composite's.
    pub gate_points: u32,
    pub breath_points: u32,
    pub ess_spans: u32,
    pub ride_points: u32,
    pub composite_points: u32,
}

/// One playing item, read.
struct Read {
    item: daw_proto::Item,
    analysis: Analysis,
    /// Seconds of the item per second of its analysis (1 / play rate).
    stretch: f64,
    /// Where in its recording it was read from.
    anchor: crate::leveler::Anchor,
}

impl Read {
    /// Project time of an analysis time.
    fn at(&self, t: f64) -> f64 {
        t.mul_add(self.stretch, self.item.position.as_seconds())
    }
}

/// A recording's first channel (its front capsule) at its own rate.
fn front(path: &str) -> Option<(Vec<f64>, f64)> {
    let bytes = std::fs::read(path).ok()?;
    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("wav");
    let audio = daw::standalone::audio_engine::decode_audio_with_extension(&bytes, extension)?;
    let channels = usize::from(audio.channels.max(1));
    let samples = audio
        .samples
        .iter()
        .step_by(channels)
        .map(|s| f64::from(*s))
        .collect();
    Some((samples, f64::from(audio.sample_rate)))
}

/// The items of `track` that play: not muted, on a lane that plays.
pub(crate) fn playing(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &daw_proto::Track,
) -> Vec<daw_proto::Item> {
    daw.get_items(project.clone(), TrackRef::Guid(track.guid.clone()))
        .into_iter()
        .filter(|i| !i.muted)
        .filter(|i| {
            i.fixed_lane
                .is_none_or(|lane| lane < 64 && track.lane_play_mask & (1_u64 << lane) != 0)
                || track.lane_play_mask == 0
        })
        .collect()
}

/// Every playing item of `track`, analysed over the stretch of its
/// recording it plays.
fn read(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &daw_proto::Track,
) -> Vec<Read> {
    let mut sources: HashMap<String, Option<(Vec<f64>, f64)>> = HashMap::new();
    playing(daw, project, track)
        .into_iter()
        .filter_map(|item| {
            let take = daw.get_active_take(project.clone(), ItemRef::Guid(item.guid.clone()))?;
            let path = take.source_file_path?;
            let (samples, rate) = sources
                .entry(path.clone())
                .or_insert_with(|| front(&path))
                .as_ref()?;
            let speed = if take.play_rate > 0.0 {
                take.play_rate
            } else {
                1.0
            };
            let index = |s: f64| {
                num_traits::ToPrimitive::to_usize(&(s * rate).round().max(0.0)).unwrap_or(0)
            };
            let from = index(take.start_offset.as_seconds());
            let to = index(
                item.length
                    .as_seconds()
                    .mul_add(speed, take.start_offset.as_seconds()),
            );
            let slice = samples.get(from.min(samples.len())..to.min(samples.len()))?;
            let analysis = env::analyze(slice, *rate, level_dsp::ClassifyConfig::default());
            Some(Read {
                item,
                analysis,
                stretch: 1.0 / speed,
                anchor: crate::leveler::Anchor {
                    offset: take.start_offset.as_seconds(),
                    path,
                },
            })
        })
        .collect()
}

/// The power mean of `levels` (dB); `None` with none.
fn power_mean(levels: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, n) = levels.fold((0.0, 0_u32), |(s, n), db| {
        (10f64.powf(db / 10.0) + s, n.saturating_add(1))
    });
    (n > 0).then(|| 10.0 * (sum / f64::from(n)).log10())
}

/// The four lanes for one item, ridden to `target_db`.
fn lanes(read: &Read, target_db: f64, knobs: &Knobs) -> Contributions {
    let a = &read.analysis;
    let tolerance = env::GenerationConfig::default().tolerance_db;
    Contributions {
        gate: env::gate_envelope(a, a.silence_db + GATE_ABOVE_DB, GATE_FLOOR_DB, tolerance),
        breath: env::breath_envelope(
            a,
            BREATH_DB,
            target_db - BREATH_UNDER_DB,
            a.silence_db,
            tolerance,
        ),
        sibilance: env::sibilance_spans(a, ESS_DB),
        ride: env::phrase_ride(
            a,
            &env::phrase_levels(a, PHRASE_S),
            target_db,
            knobs.consistency,
            (knobs.range_db, knobs.range_db),
            tolerance,
        ),
        bypass: env::Bypass::default(),
    }
}

/// Each sung block of `reads` — project time and level, dBFS — with the
/// gain `gain_at(read, t)` on it.
fn sung_blocks(reads: &[Read], gain_at: impl Fn(usize, f64) -> f64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for (k, r) in reads.iter().enumerate() {
        for i in (0..r.analysis.blocks()).filter(|i| r.analysis.is_sung(*i)) {
            let t = r.analysis.time_of(i);
            out.push((
                r.at(t),
                r.analysis.rms_db.get(i).copied().unwrap_or(0.0) + gain_at(k, t),
            ));
        }
    }
    out
}

/// The value of a ramped curve at `t`.
fn value_at(points: &[EnvPoint], t: f64) -> f64 {
    let at = points.partition_point(|p| p.t_s < t);
    match (
        at.checked_sub(1).and_then(|i| points.get(i)),
        points.get(at),
    ) {
        (Some(a), Some(b)) if b.t_s > a.t_s => {
            ((t - a.t_s) / (b.t_s - a.t_s)).mul_add(b.db - a.db, a.db)
        }
        (Some(a), _) => a.db,
        (None, Some(b)) => b.db,
        (None, None) => 0.0,
    }
}

/// The track's lanes for one item, as kept, with where in its recording
/// they were read (`anchor`).
fn keep(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
    (item, lanes, digest): (&str, &Contributions, u64),
    anchor: &crate::leveler::Anchor,
) {
    let kept = expression_editor_daw::state::ItemEnvelopes::capture(item, lanes, digest);
    let Ok(text) = facet_json::to_string(&kept) else {
        return;
    };
    let _ = Tracks::set_ext_state(
        daw,
        project.clone(),
        TrackRef::Guid(track.to_owned()),
        &format!("{LANES_KEY}{item}"),
        &text,
    );
    let _ = Tracks::set_ext_state(
        daw,
        project.clone(),
        TrackRef::Guid(track.to_owned()),
        &format!("{}{item}", crate::leveler::ANCHOR_KEY),
        &anchor.to_text(),
    );
}

/// The track ext key that shows the leveler's lanes under a track.
pub const LANE_SHOWN: &str = "FTS:level:lane";

/// Whether `track` shows the vocal leveler's lane.
#[must_use]
pub fn lane_shown(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
) -> bool {
    Tracks::ext_state(daw, project.clone(), TrackRef::Guid(track.to_owned()))
        .iter()
        .any(|e| e.key == LANE_SHOWN && e.value == "1")
}

/// Show or hide the vocal leveler's lane under `track`.
pub fn show_lane(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
    shown: bool,
) {
    let _ = Tracks::set_ext_state(
        daw,
        project.clone(),
        TrackRef::Guid(track.to_owned()),
        LANE_SHOWN,
        if shown { "1" } else { "" },
    );
}

/// Whether any of `track`'s items has lanes kept — not the key that
/// expands the leveler, which shares their prefix.
#[must_use]
pub fn has_lanes(daw: &daw::standalone::Standalone, project: &ProjectContext, track: &str) -> bool {
    Tracks::ext_state(daw, project.clone(), TrackRef::Guid(track.to_owned()))
        .iter()
        .any(|e| e.key.starts_with(LANES_KEY) && e.key != LANE_SHOWN && !e.value.is_empty())
}

/// `track`'s leveler dials, as kept (as made when never turned).
#[must_use]
pub fn stages(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
) -> crate::leveler::Stages {
    Tracks::ext_state(daw, project.clone(), TrackRef::Guid(track.to_owned()))
        .iter()
        .find(|e| e.key == crate::leveler::STAGES_KEY)
        .map(|e| crate::leveler::Stages::parse(&e.value))
        .unwrap_or_default()
}

/// One item's lanes as kept with its track: whose, where in its
/// recording they were read, and how far they reach.
struct Kept {
    item: String,
    anchor: Option<crate::leveler::Anchor>,
    reach: f64,
    lanes: Contributions,
}

/// Every item's lanes kept with a track (`ext`, its ext state). Lanes
/// kept before anchors were are taken to start where their item's take
/// does now — as they were drawn then.
fn kept_all(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    ext: &[daw_proto::TrackExtEntry],
) -> Vec<Kept> {
    let anchors: HashMap<&str, crate::leveler::Anchor> = ext
        .iter()
        .filter_map(|e| {
            let item = e.key.strip_prefix(crate::leveler::ANCHOR_KEY)?;
            Some((item, crate::leveler::Anchor::parse(&e.value)?))
        })
        .collect();
    ext.iter()
        .filter(|e| e.key != LANE_SHOWN)
        .filter_map(|e| {
            let item = e.key.strip_prefix(LANES_KEY)?;
            let lanes =
                facet_json::from_str::<expression_editor_daw::state::ItemEnvelopes>(&e.value)
                    .ok()?
                    .restore();
            let anchor = anchors.get(item).cloned().or_else(|| {
                let take = daw.get_active_take(project.clone(), ItemRef::Guid(item.to_owned()))?;
                Some(crate::leveler::Anchor {
                    offset: take.start_offset.as_seconds(),
                    path: take.source_file_path?,
                })
            });
            Some(Kept {
                item: item.to_owned(),
                anchor,
                reach: crate::leveler::extent(&lanes),
                lanes,
            })
        })
        .collect()
}

/// `track`'s playing items that are leveled, each with its four lanes,
/// the track's dials and their sum — what the item draws on it and the
/// leveler's lanes under it.
///
/// An item's lanes are its own, or — for one with none, a split's new
/// half or a comp's slice — those read over the stretch of the same
/// recording it plays. Either way they are placed by where in the
/// recording they were read, so they stay on the audio through a slip or
/// a cut, as the take's volume envelope does.
#[must_use]
pub fn leveled(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &daw_proto::Track,
) -> Vec<crate::automation::Leveled> {
    let ext = Tracks::ext_state(daw, project.clone(), TrackRef::Guid(track.guid.clone()));
    let dials = ext
        .iter()
        .find(|e| e.key == crate::leveler::STAGES_KEY)
        .map(|e| crate::leveler::Stages::parse(&e.value))
        .unwrap_or_default();
    let kept = kept_all(daw, project, &ext);
    if kept.is_empty() {
        return Vec::new();
    }
    let anchored: Vec<(&crate::leveler::Anchor, f64)> = kept
        .iter()
        .filter_map(|k| Some((k.anchor.as_ref()?, k.reach)))
        .collect();
    let anchored_kept: Vec<&Kept> = kept.iter().filter(|k| k.anchor.is_some()).collect();
    let mut out: Vec<crate::automation::Leveled> = playing(daw, project, track)
        .into_iter()
        .filter_map(|item| {
            let take = daw.get_active_take(project.clone(), ItemRef::Guid(item.guid.clone()))?;
            let speed = if take.play_rate > 0.0 {
                take.play_rate
            } else {
                1.0
            };
            let now = take.start_offset.as_seconds();
            let from = kept.iter().find(|k| k.item == item.guid).or_else(|| {
                let path = take.source_file_path.as_deref()?;
                let at = crate::leveler::covering(&anchored, path, now)?;
                anchored_kept.get(at).copied()
            })?;
            let origin = from.anchor.as_ref().map_or(0.0, |a| a.offset - now);
            Some(crate::automation::Leveled::new(
                (track.guid.as_str(), item.guid.as_str()),
                (
                    item.position.as_seconds(),
                    item.length.as_seconds(),
                    1.0 / speed,
                    origin,
                ),
                from.lanes.clone(),
                dials,
            ))
        })
        .collect();
    out.sort_by(|a, b| a.position.total_cmp(&b.position));
    out
}

/// An item's lanes changed by hand, put back: kept (with where in its
/// recording they start), and their sum through the track's dials written
/// as its volume — one undo step. `false` when the item is gone.
pub fn relevel(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
    leveled: &crate::automation::Leveled,
) -> bool {
    let Some(item) = daw.get_item(project.clone(), ItemRef::Guid(leveled.item.clone())) else {
        return false;
    };
    let take = daw.get_active_take(project.clone(), ItemRef::Guid(leveled.item.clone()));
    let anchor = crate::leveler::Anchor {
        offset: take.as_ref().map_or(0.0, |t| t.start_offset.as_seconds()) + leveled.origin,
        path: take.and_then(|t| t.source_file_path).unwrap_or_default(),
    };
    let label = "Edit the vocal leveling";
    daw::service::Projects::begin_undo_block(daw, project.clone(), label);
    crate::drum_level::write_curve(daw, project, &item, &leveled.item_curve());
    // By hand: no knobs made these, so the digest says so.
    keep(
        daw,
        project,
        track,
        (&leveled.item, &leveled.lanes, 0),
        &anchor,
    );
    daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
    crate::midi::mark_stale([leveled.item.clone()]);
    true
}

/// Set `track`'s leveler dials: kept with the track, and every leveled
/// item's volume written again through them — one undo step. How many
/// items were.
#[must_use]
pub fn set_stages(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
    dials: &crate::leveler::Stages,
) -> usize {
    let label = "Dial the vocal leveler";
    daw::service::Projects::begin_undo_block(daw, project.clone(), label);
    let _ = Tracks::set_ext_state(
        daw,
        project.clone(),
        TrackRef::Guid(track.to_owned()),
        crate::leveler::STAGES_KEY,
        &dials.to_text(),
    );
    let items = Tracks::get(daw, project.clone(), TrackRef::Guid(track.to_owned()))
        .map(|t| leveled(daw, project, &t))
        .unwrap_or_default();
    let mut written = Vec::with_capacity(items.len());
    for l in &items {
        if let Some(item) = daw.get_item(project.clone(), ItemRef::Guid(l.item.clone())) {
            crate::drum_level::write_curve(daw, project, &item, &l.item_curve());
            written.push(l.item.clone());
        }
    }
    daw::service::Projects::end_undo_block(daw, project.clone(), label, None);
    let count = written.len();
    crate::midi::mark_stale(written);
    count
}

/// The lanes kept for `item` on `track`, if any.
#[must_use]
pub fn kept(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &str,
    item: &str,
) -> Option<Contributions> {
    let key = format!("{LANES_KEY}{item}");
    Tracks::ext_state(daw, project.clone(), TrackRef::Guid(track.to_owned()))
        .into_iter()
        .find(|e| e.key == key)
        .and_then(|e| {
            facet_json::from_str::<expression_editor_daw::state::ItemEnvelopes>(&e.value).ok()
        })
        .map(|e| e.restore())
}

/// A digest of what the lanes were made with.
fn digest((target_db, level_db): (f64, f64), knobs: &Knobs) -> u64 {
    [
        target_db,
        level_db,
        knobs.consistency,
        knobs.range_db,
        PHRASE_S,
    ]
    .iter()
    .fold(0xcbf2_9ce4_8422_2325_u64, |h, v| {
        v.to_bits()
            .to_le_bytes()
            .iter()
            .fold(h, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
    })
}

/// Each section's sung level, before `composites` and through them.
fn by_section(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    reads: &[Read],
    composites: &[Vec<EnvPoint>],
) -> Vec<SectionLevel> {
    let before = sung_blocks(reads, |_, _| 0.0);
    let leveled = sung_blocks(reads, |k, t| {
        composites.get(k).map_or(0.0, |c| value_at(c, t))
    });
    let within = |blocks: &[(f64, f64)], (from, to): (f64, f64)| {
        power_mean(
            blocks
                .iter()
                .filter(|(t, _)| *t >= from && *t < to)
                .map(|(_, db)| *db),
        )
    };
    crate::actions::sections(daw, project)
        .iter()
        .filter_map(|r| {
            Some(SectionLevel {
                name: r.name.clone(),
                before_db: within(&before, (r.start, r.end))?,
                after_db: within(&leveled, (r.start, r.end))?,
            })
        })
        .collect()
}

// r[impl level.vocal]
/// Level `track` (see the module); `dry` only says.
///
/// # Errors
///
/// No playing items to level.
pub fn level(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &daw_proto::Track,
    (knobs, dry): (&Knobs, bool),
) -> Result<Leveled, String> {
    let reads = read(daw, project, track);
    if reads.is_empty() {
        return Err(format!("{}: nothing playing to level", track.name));
    }
    // The song's level: where the louder share of its phrases are sung.
    let mut phrases: Vec<f64> = reads
        .iter()
        .flat_map(|r| {
            let levels = env::phrase_levels(&r.analysis, PHRASE_S);
            (0..r.analysis.blocks())
                .filter(|i| r.analysis.is_sung(*i))
                .filter_map(move |i| levels.get(i).copied().flatten())
                .collect::<Vec<_>>()
        })
        .collect();
    phrases.sort_by(f64::total_cmp);
    let share = |of: f64| {
        let at = num_traits::ToPrimitive::to_usize(
            &(of * num_traits::ToPrimitive::to_f64(&phrases.len()).unwrap_or(0.0)),
        )
        .unwrap_or(0);
        phrases
            .get(at.min(phrases.len().saturating_sub(1)))
            .copied()
    };
    let (Some(quiet), Some(loud)) = (share(QUIET_SHARE), share(LOUD_SHARE)) else {
        return Err(format!("{}: nothing sung to level", track.name));
    };
    // Asked for a level, ride to it and finish there.
    let (target_db, level_db) = knobs
        .target_db
        .map_or_else(|| (f64::midpoint(quiet, loud), loud), |t| (t, t));
    let mut made: Vec<Contributions> = reads.iter().map(|r| lanes(r, target_db, knobs)).collect();
    // What plays is the lanes through the track's dials, as they were
    // left: the makeup is measured through them, and written with them.
    let dials = stages(daw, project, &track.guid);
    let summed = |all: &[Contributions]| -> Vec<Vec<EnvPoint>> {
        all.iter()
            .map(|l| env::composite(&dials.apply(l)))
            .collect()
    };
    // The makeup: the leveled track's sung level put at the target.
    let composites = summed(&made);
    let after = power_mean(
        sung_blocks(&reads, |k, t| {
            composites.get(k).map_or(0.0, |c| value_at(c, t))
        })
        .into_iter()
        .map(|(_, db)| db),
    )
    .unwrap_or(level_db);
    let makeup_db = (level_db - after).clamp(-MAKEUP_MAX_DB, MAKEUP_MAX_DB);
    // The makeup rides in the ride: it is how loud the line is. Through
    // the ride's dial, so it lands where it says; none with the ride off.
    let ride = dials.ride.gain();
    let makeup_made = if ride > 1e-3 {
        (makeup_db / ride).clamp(-MAKEUP_MAX_DB, MAKEUP_MAX_DB)
    } else {
        0.0
    };
    for lanes in &mut made {
        if lanes.ride.is_empty() {
            lanes.ride.push(EnvPoint::new(0.0, 0.0));
        }
        for p in &mut lanes.ride {
            p.db += makeup_made;
        }
    }
    let composites = summed(&made);

    let sections = by_section(daw, project, &reads, &composites);
    let spread = |f: fn(&SectionLevel) -> f64| {
        let (lo, hi) = sections
            .iter()
            .map(f)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                (lo.min(v), hi.max(v))
            });
        if hi >= lo { hi - lo } else { 0.0 }
    };
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let mut said = Leveled {
        track: track.name.clone(),
        items: count(reads.len()),
        target_db,
        level_db,
        makeup_db,
        spread_before_db: spread(|s| s.before_db),
        spread_after_db: spread(|s| s.after_db),
        sections,
        gate_points: count(made.iter().map(|l| l.gate.len()).sum()),
        breath_points: count(made.iter().map(|l| l.breath.len()).sum()),
        ess_spans: count(made.iter().map(|l| l.sibilance.len()).sum()),
        ride_points: count(made.iter().map(|l| l.ride.len()).sum()),
        composite_points: count(composites.iter().map(Vec::len).sum()),
    };
    if dry {
        return Ok(said);
    }
    let digest = digest((target_db, level_db), knobs);
    for ((r, lanes), composite) in reads.iter().zip(&made).zip(&composites) {
        // The curves are in the analysis's time — the recording's — and
        // the volume envelope in the item's.
        let curve =
            crate::leveler::on_item(composite, (0.0, r.stretch), r.item.length.as_seconds());
        crate::drum_level::write_curve(daw, project, &r.item, &curve);
        keep(
            daw,
            project,
            &track.guid,
            (&r.item.guid, lanes, digest),
            &r.anchor,
        );
        crate::midi::mark_stale([r.item.guid.clone()]);
    }
    said.items = count(reads.len());
    Ok(said)
}

#[cfg(test)]
mod tests {
    use super::{EnvPoint, power_mean, value_at};

    #[test]
    fn a_curve_is_read_between_its_points() {
        let c = [EnvPoint::new(0.0, 0.0), EnvPoint::new(1.0, -10.0)];
        assert!((value_at(&c, 0.5) + 5.0).abs() < 1e-9);
        assert!((value_at(&c, 2.0) + 10.0).abs() < 1e-9);
        assert!(value_at(&[], 1.0).abs() < 1e-12);
    }

    #[test]
    fn a_power_mean_leans_to_the_loud() {
        let m = power_mean([-20.0, -40.0].into_iter()).expect("a mean");
        assert!(m > -24.0 && m < -22.0, "{m}");
        assert!(power_mean(std::iter::empty()).is_none());
    }
}

static LEVEL: architect::action::ActionMeta = crate::actions::meta(
    "VOCAL_LEVEL",
    "vocal_level",
    "Level the vocals",
    "Level `tracks` (paths, names or GUIDs; every vocal under Vocals when left out) the way an engineer does before any compressor: four lanes made for each playing item from one analysis of it — the gate (noise between lines taken down, set from the recording's own noise floor), de-breath (breaths and soft noises under the song's level tucked down), the ride (each line, a phrase at a time, brought toward one level for the whole track, so a quiet verse sits with the chorus) and de-ess (each \"ess\" eased) — then the makeup that puts the leveled track's sung level where its louder lines were sung (`level_db` says where), so the choruses keep their level and the verses come up to it. Their sum is written as each item's volume envelope (what plays, what the waveform draws, and it rides with the item); the four lanes are kept with the track per item. `consistency` 0–100 (90): how far each line is brought; `range_db` (12): the most a line moves; `target_db` to ride to, and finish at, a level of your own (dBFS, the front capsule). Says each section's sung level before and after. `dry_run` only says. One undo step.",
    "Vocals",
    "",
);

static DIALS: architect::action::ActionMeta = crate::actions::meta(
    "VOCAL_LEVELER",
    "vocal_leveler",
    "The vocal leveler's dials",
    "Each leveled vocal's leveler (`tracks`: paths, names or GUIDs; every leveled vocal when left out): its four stages — De-Ess, De-Breath, Ride, Gate — each on or off and how far it goes (`amount_percent`: 100 as leveled, 0 none, 200 twice as deep), whether they are expanded into a lane each under the track (their sum is drawn on each item either way), and how many items are leveled.",
    "Vocals",
    "",
);

static DIAL: architect::action::ActionMeta = crate::actions::meta(
    "VOCAL_LEVELER_SET",
    "vocal_leveler_set",
    "Dial the vocal leveler",
    "Turn a leveled vocal's stages (`tracks` as VOCAL_LEVELER's): `stage` De-Ess, De-Breath, Ride or Gate (every one when left out) switched `on` or off and/or set to `amount` percent (0–200; 100 as leveled) — each stage's curve in dB scaled by it — and each item's volume written again as the stages' sum (what plays and what the waveform draws). `expanded` shows (or hides) the four as lanes under the track. One undo step per track. Says the dials as they are now.",
    "Vocals",
    "",
);

/// One stage's dial, said.
#[derive(Facet, Clone, Debug)]
pub struct StageDial {
    pub stage: String,
    pub on: bool,
    /// 100 as leveled.
    pub amount_percent: f64,
}

/// A vocal's leveler, said.
#[derive(Facet, Clone, Debug)]
pub struct LevelerState {
    pub track: String,
    /// Its four stages shown a lane each under it.
    pub expanded: bool,
    /// Its playing items that are leveled.
    pub items: u32,
    pub stages: Vec<StageDial>,
}

/// `track`'s leveler, said.
fn state_of(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    track: &daw_proto::Track,
) -> LevelerState {
    let dials = stages(daw, project, &track.guid);
    LevelerState {
        track: track.name.clone(),
        expanded: lane_shown(daw, project, &track.guid),
        items: u32::try_from(leveled(daw, project, track).len()).unwrap_or(u32::MAX),
        stages: crate::leveler::Stage::ALL
            .iter()
            .map(|s| {
                let dial = dials.get(*s);
                StageDial {
                    stage: s.name().to_owned(),
                    on: dial.on,
                    amount_percent: dial.amount * 100.0,
                }
            })
            .collect(),
    }
}

/// Which leveled vocals.
#[derive(Facet)]
struct LevelerArg {
    #[facet(default)]
    tracks: Vec<String>,
}

/// Which leveled vocals, which stage, and how.
#[derive(Facet)]
struct DialArg {
    #[facet(default)]
    tracks: Vec<String>,
    stage: Option<String>,
    on: Option<bool>,
    amount: Option<f64>,
    expanded: Option<bool>,
}

/// The tracks `names` name, or (none named) every leveled vocal.
fn leveled_tracks(
    daw: &daw::standalone::Standalone,
    project: &ProjectContext,
    names: &[String],
) -> Result<Vec<daw_proto::Track>, String> {
    if names.is_empty() {
        return Ok(vocals(daw, project)
            .into_iter()
            .filter(|t| has_lanes(daw, project, &t.guid))
            .collect());
    }
    names
        .iter()
        .map(|t| {
            let guid = crate::actions::resolve(daw, project, t)?;
            Tracks::get(daw, project.clone(), TrackRef::Guid(guid))
                .ok_or_else(|| format!("{t}: no such track"))
        })
        .collect()
}

/// Turn the dials `a` says on each of its tracks.
fn dial(a: &DialArg) -> Result<Vec<LevelerState>, String> {
    let (daw, project) = crate::actions::song()?;
    let which = match &a.stage {
        Some(name) => vec![
            crate::leveler::Stage::parse(name)
                .ok_or_else(|| format!("{name}: a stage is De-Ess, De-Breath, Ride or Gate"))?,
        ],
        None => crate::leveler::Stage::ALL.to_vec(),
    };
    let tracks = leveled_tracks(daw, &project, &a.tracks)?;
    for track in &tracks {
        if a.on.is_some() || a.amount.is_some() {
            let mut dials = stages(daw, &project, &track.guid);
            for stage in &which {
                let was = dials.get(*stage);
                dials.set(
                    *stage,
                    crate::leveler::Dial {
                        on: a.on.unwrap_or(was.on),
                        amount: a.amount.map_or(was.amount, |p| p / 100.0),
                    },
                );
            }
            let _rewritten = set_stages(daw, &project, &track.guid, &dials);
        }
        if let Some(shown) = a.expanded {
            show_lane(daw, &project, &track.guid, shown);
        }
    }
    crate::studio::request_resync();
    Ok(tracks.iter().map(|t| state_of(daw, &project, t)).collect())
}

/// Which vocals, and how hard.
#[derive(Facet)]
struct LevelArg {
    #[facet(default)]
    tracks: Vec<String>,
    consistency: Option<f64>,
    range_db: Option<f64>,
    target_db: Option<f64>,
    dry_run: Option<bool>,
}

/// The tracks a vocal leveling works on when none are named: every
/// recorded track under the Vocals folder.
fn vocals(daw: &daw::standalone::Standalone, project: &ProjectContext) -> Vec<daw_proto::Track> {
    let tracks = Tracks::all(daw, project.clone());
    let by_guid: HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    tracks
        .iter()
        .filter(|t| !t.is_folder)
        .filter(|t| {
            crate::gain_stage::path_of(t, &by_guid)
                .first()
                .is_some_and(|top| top.trim().eq_ignore_ascii_case("vocals"))
        })
        .filter(|t| !playing(daw, project, t).is_empty())
        .cloned()
        .collect()
}

pub(crate) fn register(list: &architect::action::ActionList) {
    list.register(
        &LEVEL,
        architect::action::Effect::Mutating,
        |a: LevelArg| {
            let (daw, project) = crate::actions::song()?;
            let defaults = Knobs::default();
            let knobs = Knobs {
                consistency: a
                    .consistency
                    .map_or(defaults.consistency, |c| (c / 100.0).clamp(0.0, 1.0)),
                range_db: a.range_db.map_or(defaults.range_db, f64::abs),
                target_db: a.target_db,
            };
            let dry = a.dry_run.unwrap_or(false);
            let tracks = if a.tracks.is_empty() {
                vocals(daw, &project)
            } else {
                a.tracks
                    .iter()
                    .map(|t| {
                        let guid = crate::actions::resolve(daw, &project, t)?;
                        Tracks::get(daw, project.clone(), TrackRef::Guid(guid))
                            .ok_or_else(|| format!("{t}: no such track"))
                    })
                    .collect::<Result<Vec<_>, String>>()?
            };
            let label = "Level the vocals";
            if !dry {
                daw::service::Projects::begin_undo_block(daw, project.clone(), label);
            }
            let out: Vec<Leveled> = tracks
                .iter()
                .filter_map(|t| level(daw, &project, t, (&knobs, dry)).ok())
                .collect();
            if !dry {
                daw::service::Projects::end_undo_block(daw, project, label, None);
                crate::studio::request_resync();
            }
            Ok(out)
        },
    );
    list.register(
        &DIALS,
        architect::action::Effect::ReadOnly,
        |a: LevelerArg| {
            let (daw, project) = crate::actions::song()?;
            let tracks = leveled_tracks(daw, &project, &a.tracks)?;
            Ok::<_, String>(
                tracks
                    .iter()
                    .map(|t| state_of(daw, &project, t))
                    .collect::<Vec<_>>(),
            )
        },
    );
    list.register(&DIAL, architect::action::Effect::Mutating, |a: DialArg| {
        dial(&a)
    });
}
