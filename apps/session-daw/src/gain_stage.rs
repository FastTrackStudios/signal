//! Gain staging: every recorded track brought to one reference level,
//! without touching its audio.
//!
//! A track's level is read from its takes' peaks — its strong hits, the
//! 90th percentile of the 100 ms windows within [`HIT_RANGE_DB`] of its
//! loudest — and the difference to its role's [`target`] goes on every
//! item as item volume. Nothing is rendered or written into the audio: an
//! item's volume set back to 1 is the track as it was recorded.
//!
//! The targets are a sampled kit's: GGD Modern & Massive 2, the loudest
//! velocity layer of each piece's mics, measured. A kit staged to it sits
//! at that library's balance with every fader at unity, so a staged kit
//! is processed the way that kit is — and every song's the same way.
//!
//! A guitar DI is never staged: it feeds amps and reamps at the level it
//! was recorded at, and stays there. A bass DI is.
//!
//! And the kit is laid out: the staged tracks' faders at unity, the toms
//! with hits spread across the stereo field in order — the first 80%
//! left, the last 80% right — and an `L` / `R` pair hard to its side.

use daw::service::{ItemRef, Items, Peaks, ProjectContext, TakeRef, TrackRef, Tracks};

/// What a recorded track is, for its level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    KickIn,
    KickOut,
    KickSub,
    SnareTop,
    SnareBottom,
    Tom,
    HiHat,
    Ride,
    Overheads,
    Room,
    BassDi,
}

/// The level a role's strong hits are brought to, dBFS peak.
///
/// GGD Modern & Massive 2, the loudest velocity layer, close mics and
/// rooms averaged over three drums of each piece and their round robins
/// (kick In 1 0.0, In 2 −6.2, Sub −4.8; snare Top −3.3, Bottom −13.2; toms
/// −7.2 to −7.7; hats −26.0; ride −21.3; the snare in the overheads
/// −10.6, in the rooms −8.3 to −9.2). A bass DI has no kit to follow:
/// −10, a DI's strong notes with room above them.
#[must_use]
pub const fn target(role: Role) -> f64 {
    match role {
        Role::KickIn => 0.0,
        Role::KickOut => -6.2,
        Role::KickSub => -4.8,
        Role::SnareTop => -3.3,
        Role::SnareBottom => -13.2,
        Role::Tom => -7.5,
        Role::HiHat => -26.0,
        Role::Ride => -21.3,
        Role::Overheads => -10.6,
        Role::Room => -8.5,
        Role::BassDi => -10.0,
    }
}

/// How far under a track's loudest a window still counts as a strong hit.
///
/// Under it is bleed and ring, and the soft notes. 12, not more: a tom
/// hit a dozen times a song is mostly the kit in its mic, and at 24 dB
/// that bleed outnumbered the hits (unbreakable's T4 read −35 dBFS).
pub const HIT_RANGE_DB: f64 = 12.0;

/// How far a track is moved at most: past it, the reading is wrong (a
/// track of nothing but bleed), not the track.
const MOST_DB: f64 = 24.0;

/// The role of a track by where the organizer filed it: `path` its
/// folders from the top, then its own name (`Drums`, `Kick`, `In`).
/// `None` for anything not staged — a guitar DI above all, and a trigger.
#[must_use]
pub fn role(path: &[&str]) -> Option<Role> {
    let lower: Vec<String> = path.iter().map(|p| p.trim().to_lowercase()).collect();
    let name = lower.last()?;
    let words: Vec<&str> = name
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .filter(|w| !w.is_empty())
        .collect();
    let has = |w: &str| words.contains(&w);
    let under = |what: &str| lower.iter().any(|p| p.contains(what));
    // A trigger is a sampler's source; the references, a rehearsal's
    // `Kick - Song` among them, are for listening against.
    // Process holds the kit's parallel chains, not mics.
    if under(&dynamic_template::apply::PROCESS_FOLDER.to_lowercase())
        || has("trig")
        || has("trigger")
        || under(&dynamic_template::apply::REFERENCES_FOLDER.to_lowercase())
    {
        return None;
    }
    if has("di") {
        return (under("bass") && !under("guitar")).then_some(Role::BassDi);
    }
    if !under("drum") && !under("kit") {
        return None;
    }
    let tom = words.iter().any(|w| {
        *w == "tom" || *w == "toms" || w.strip_prefix('t').is_some_and(|n| n.parse::<u8>().is_ok())
    });
    Some(if under("kick") || has("kick") {
        if has("sub") {
            Role::KickSub
        } else if has("out") {
            Role::KickOut
        } else {
            Role::KickIn
        }
    } else if under("snare") || has("snare") {
        if has("bottom") || has("bot") {
            Role::SnareBottom
        } else {
            Role::SnareTop
        }
    } else if tom || under("tom") {
        Role::Tom
    } else if has("hat") || has("hats") || has("hh") || name.contains("hi hat") {
        Role::HiHat
    } else if has("ride") {
        Role::Ride
    } else if has("room") || has("rooms") || under("room") {
        Role::Room
    } else if has("oh") || has("ohs") || name.contains("overhead") || under("cymbal") {
        Role::Overheads
    } else {
        return None;
    })
}

/// A track's strong-hit level, dBFS, from its peaks.
///
/// The peaks are per-point absolute maxima, `step` seconds each; the
/// level is the 90th percentile of its 100 ms windows' peaks within
/// [`HIT_RANGE_DB`] of the loudest. `None` for silence.
#[must_use]
pub fn level(points: &[f64], step: f64) -> Option<f64> {
    if step <= 0.0 {
        return None;
    }
    let per = usize::try_from(std::time::Duration::from_millis(100).as_nanos())
        .ok()
        .zip(usize::try_from(std::time::Duration::from_secs_f64(step).as_nanos()).ok())
        .and_then(|(window, point)| window.checked_div(point))
        .unwrap_or(1)
        .max(1);
    let windows: Vec<f64> = points
        .chunks(per)
        .map(|w| w.iter().fold(0.0_f64, |p, x| p.max(x.abs())))
        .collect();
    let loudest = windows.iter().copied().fold(0.0_f64, f64::max);
    if loudest <= 0.0 {
        return None;
    }
    let floor = loudest * 10.0_f64.powf(-HIT_RANGE_DB / 20.0);
    let mut hits: Vec<f64> = windows.into_iter().filter(|w| *w >= floor).collect();
    hits.sort_by(f64::total_cmp);
    // Nearest rank, over the gaps between hits: ten hits read the ninth,
    // not the one loudest.
    let at = hits.len().saturating_sub(1).saturating_mul(9) / 10;
    hits.get(at.min(hits.len().saturating_sub(1)))
        .map(|peak| 20.0 * peak.log10())
}

/// Where `count` toms sit, in order: spread evenly from 80% left to 80%
/// right — one, in the middle.
#[must_use]
pub fn tom_pans(count: usize) -> Vec<f64> {
    const WIDE: f64 = 0.8;
    match count {
        0 => Vec::new(),
        1 => vec![0.0],
        n => {
            let steps = u32::try_from(n.saturating_sub(1)).unwrap_or(u32::MAX);
            (0..steps.saturating_add(1))
                .map(|i| -WIDE + 2.0 * WIDE * f64::from(i) / f64::from(steps))
                .collect()
        }
    }
}

/// A side named in a track's name: `OH L` is −1, `Room R` 1.
fn side(name: &str) -> Option<f64> {
    let last = name.split_whitespace().last()?.to_lowercase();
    match last.as_str() {
        "l" | "left" => Some(-1.0),
        "r" | "right" => Some(1.0),
        _ => None,
    }
}

/// A track's folders from the top, then its name.
pub(crate) fn path_of<'a>(
    track: &'a daw_proto::Track,
    by_guid: &std::collections::HashMap<&str, &'a daw_proto::Track>,
) -> Vec<&'a str> {
    let mut path = vec![track.name.as_str()];
    let mut at = track
        .parent_guid
        .as_deref()
        .and_then(|p| by_guid.get(p).copied());
    while let Some(parent) = at {
        path.push(parent.name.as_str());
        at = parent
            .parent_guid
            .as_deref()
            .and_then(|p| by_guid.get(p).copied());
    }
    path.reverse();
    path
}

/// What staging did to one track.
#[derive(Clone, Debug, PartialEq)]
pub struct Staged {
    pub name: String,
    pub role: Role,
    /// Its strong-hit level before, dBFS.
    pub level: f64,
    /// The item volume put on, dB.
    pub gain_db: f64,
}

/// What [`plan`] decided, and [`stage`] / [`stage_live`] did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub staged: Vec<Staged>,
    /// Tracks with a role but no level (nothing recorded on them).
    pub silent: Vec<String>,
    /// The toms panned, in order, and where.
    pub toms: Vec<(String, f64)>,
}

/// One track's part of a [`Plan`].
#[derive(Clone, Debug, PartialEq)]
pub struct Move {
    pub track: String,
    /// The item volume every one of its items gets.
    pub volume: f64,
    /// Its pan, when it is set.
    pub pan: Option<f64>,
    pub items: Vec<String>,
}

/// Everything staging changes, decided before any of it is done.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub moves: Vec<Move>,
    pub report: Report,
}

/// Decide the staging of `tracks`: `items` every item of the project,
/// `wave` an item's peaks ([`crate::midi::Wave`], read from its active
/// take), asked only of the items on a playing lane.
#[must_use]
pub fn plan(
    tracks: &[daw_proto::Track],
    items: &[daw_proto::Item],
    mut wave: impl FnMut(&daw_proto::Item) -> Option<crate::midi::Wave>,
) -> Plan {
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let mut out = Plan::default();
    let mut toms: Vec<usize> = Vec::new();
    for track in tracks.iter().filter(|t| !t.is_folder) {
        let Some(role) = role(&path_of(track, &by_guid)) else {
            continue;
        };
        let own: Vec<&daw_proto::Item> = items
            .iter()
            .filter(|i| i.track_guid == track.guid)
            .collect();
        // Read from what plays: a fixed-lane track's other lanes hold
        // other takes (unbreakable's, another song's), at their own
        // levels. The gain goes on every item, so a lane switched to is
        // staged alike.
        let mut points: Vec<f64> = Vec::new();
        let mut step = 0.0;
        for item in own
            .iter()
            .filter(|item| daw_ui::studio::project::on_playing_lane(track, item))
        {
            if let Some(read) = wave(item) {
                step = read.step;
                points.extend(
                    read.points
                        .iter()
                        .map(|(max, min)| f64::from(max.abs().max(min.abs()))),
                );
            }
        }
        let Some(level) = level(&points, step) else {
            out.report.silent.push(track.name.clone());
            continue;
        };
        let gain_db = (target(role) - level).clamp(-MOST_DB, MOST_DB);
        if role == Role::Tom {
            toms.push(out.moves.len());
        }
        out.moves.push(Move {
            track: track.guid.clone(),
            volume: 10.0_f64.powf(gain_db / 20.0),
            pan: side(&track.name),
            items: own.iter().map(|i| i.guid.clone()).collect(),
        });
        out.report.staged.push(Staged {
            name: track.name.clone(),
            role,
            level,
            gain_db,
        });
    }
    for (at, pan) in toms.iter().zip(tom_pans(toms.len())) {
        if let Some(each) = out.moves.get_mut(*at) {
            each.pan = Some(pan);
            let name = out
                .report
                .staged
                .get(*at)
                .map(|s| s.name.clone())
                .unwrap_or_default();
            out.report.toms.push((name, pan));
        }
    }
    out
}

/// Stage `project` on an engine in hand (prepare's): [`plan`], then the
/// item volumes, the faders at unity and the pans.
///
/// # Errors
///
/// A backend call failed.
pub fn stage<D>(daw: &D, project: &ProjectContext) -> eyre::Result<Report>
where
    D: Tracks + Items + Peaks,
{
    let tracks = Tracks::all(daw, project.clone());
    let items = daw.get_all_items(project.clone());
    let planned = plan(&tracks, &items, |item| {
        let data = daw.take_peaks(
            project.clone(),
            ItemRef::Guid(item.guid.clone()),
            TakeRef::Active,
            crate::midi::ANALYSIS_BLOCK,
        );
        crate::midi::Wave::from_peaks(&data)
    });
    for each in &planned.moves {
        for item in &each.items {
            Items::set_volume(
                daw,
                project.clone(),
                ItemRef::Guid(item.clone()),
                each.volume,
            )?;
        }
        let at = TrackRef::Guid(each.track.clone());
        Tracks::set_volume(daw, project.clone(), at.clone(), 1.0)?;
        if let Some(pan) = each.pan {
            Tracks::set_pan(daw, project.clone(), at, pan)?;
        }
    }
    Ok(planned.report)
}

/// Stage the session through the facade — whatever this window drives,
/// the engine or REAPER: [`plan`], then the same changes as [`stage`].
///
/// # Errors
///
/// A facade call failed.
pub async fn stage_live(project: &daw_control::Project) -> eyre::Result<Report> {
    let tracks = project.tracks().all().await?;
    let items = project.items().all().await?;
    let mut waves: std::collections::HashMap<String, crate::midi::Wave> =
        std::collections::HashMap::new();
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    for item in &items {
        // Only what [`plan`] will ask for: a staged track's playing lane.
        let Some(track) = by_guid.get(item.track_guid.as_str()) else {
            continue;
        };
        if role(&path_of(track, &by_guid)).is_none()
            || !daw_ui::studio::project::on_playing_lane(track, item)
        {
            continue;
        }
        let Ok(Some(handle)) = project.items().by_guid(&item.guid).await else {
            continue;
        };
        if let Ok(data) = handle
            .active_take()
            .peaks(crate::midi::ANALYSIS_BLOCK)
            .await
            && let Some(wave) = crate::midi::Wave::from_peaks(&data)
        {
            waves.insert(item.guid.clone(), wave);
        }
    }
    let planned = plan(&tracks, &items, |item| waves.remove(&item.guid));
    for each in &planned.moves {
        for guid in &each.items {
            if let Some(item) = project.items().by_guid(guid).await? {
                item.set_volume(each.volume).await?;
            }
        }
        if let Some(track) = project.tracks().by_guid(&each.track).await? {
            track.set_volume(1.0).await?;
            if let Some(pan) = each.pan {
                track.set_pan(pan).await?;
            }
        }
    }
    Ok(planned.report)
}

/// Undo staging through the facade: every item of a track staging would
/// stage back at an item volume of 1 — the track as it was recorded.
/// Faders and pans stay where they are. Returns the tracks cleared.
///
/// # Errors
///
/// A facade call failed.
pub async fn clear_live(project: &daw_control::Project) -> eyre::Result<Vec<String>> {
    let tracks = project.tracks().all().await?;
    let items = project.items().all().await?;
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let mut cleared = Vec::new();
    for track in tracks.iter().filter(|t| !t.is_folder) {
        if role(&path_of(track, &by_guid)).is_none() {
            continue;
        }
        for item in items.iter().filter(|i| i.track_guid == track.guid) {
            if (item.volume - 1.0).abs() > 1e-9
                && let Some(handle) = project.items().by_guid(&item.guid).await?
            {
                handle.set_volume(1.0).await?;
            }
        }
        cleared.push(track.name.clone());
    }
    Ok(cleared)
}

/// One staged track as it is heard: rendered alone through the engine,
/// item volume and all, its strong hits read as staging reads them.
#[derive(Clone, Debug, PartialEq)]
pub struct Heard {
    pub name: String,
    pub role: Role,
    pub target: f64,
    /// Its strong-hit level as rendered, dBFS.
    pub level: f64,
}

/// Render `project_guid` from `from` to `to` seconds with only `solo`
/// soloed (none: the whole mix), handing each 50 ms block's interleaved
/// stereo samples to `each`. The folders above what is soloed are
/// unmuted for it — a muted folder silences what is in it, soloed or not
/// (unbreakable's Drums is muted in its RPP) — and muted again after.
#[cfg(feature = "native")]
fn render_blocks(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
    solo: &[String],
    from: f64,
    to: f64,
    mut each: impl FnMut(&[f32], f64),
) {
    const CHUNK: usize = 2_400;
    const CHUNK_FRAMES: u64 = 2_400;
    let project = ProjectContext::Project(project_guid.to_owned());
    let tracks = Tracks::all(daw, project.clone());
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let mut unmuted: Vec<String> = Vec::new();
    for guid in solo {
        let mut at = by_guid.get(guid.as_str()).copied();
        while let Some(track) = at {
            if track.muted && !unmuted.contains(&track.guid) {
                unmuted.push(track.guid.clone());
            }
            at = track
                .parent_guid
                .as_deref()
                .and_then(|p| by_guid.get(p).copied());
        }
    }
    for guid in &unmuted {
        let _ = Tracks::set_muted(daw, project.clone(), TrackRef::Guid(guid.clone()), false);
    }
    for guid in solo {
        let _ = Tracks::set_soloed(daw, project.clone(), TrackRef::Guid(guid.clone()), true);
    }
    let renderer =
        daw::standalone::audio_engine::render::ProjectRenderer::new(daw, project_guid, RENDER_RATE);
    let frame = |seconds: f64| {
        let nanos =
            std::time::Duration::try_from_secs_f64(seconds.max(0.0)).map_or(0, |d| d.as_nanos());
        u64::try_from(nanos.saturating_mul(u128::from(RENDER_RATE)) / 1_000_000_000).unwrap_or(0)
    };
    // What the bus itself puts out: the master fader taken back off (it
    // is unbreakable's −2.4 dB, and the kit is set before it).
    // `each` is handed the gain that undoes it.
    let unmaster = Tracks::master(daw, project.clone())
        .map(|m| m.volume)
        .filter(|v| *v > 1e-6)
        .map_or(1.0, |v| 1.0 / v);
    let (mut at, end) = (frame(from), frame(to));
    while at < end {
        each(&renderer.render_block(at, CHUNK).samples, unmaster);
        at = at.saturating_add(CHUNK_FRAMES);
    }
    for guid in solo {
        let _ = Tracks::set_soloed(daw, project.clone(), TrackRef::Guid(guid.clone()), false);
    }
    for guid in &unmuted {
        let _ = Tracks::set_muted(daw, project.clone(), TrackRef::Guid(guid.clone()), true);
    }
}

/// The rate staging renders at.
#[cfg(feature = "native")]
const RENDER_RATE: u32 = 48_000;

/// [`render_blocks`]' peaks, one per 50 ms block.
#[cfg(feature = "native")]
fn render_peaks(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
    solo: &[String],
    from: f64,
    to: f64,
) -> Vec<f64> {
    let mut peaks = Vec::new();
    render_blocks(daw, project_guid, solo, from, to, |block, gain| {
        let peak = block.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
        peaks.push(f64::from(peak) * gain);
    });
    peaks
}

/// The loudest short-term loudness, LUFS.
///
/// BS.1770: K-weighted, 3 s windows, a new one every 100 ms. `sums` are
/// 50 ms blocks: each block's K-weighted sum of squares over both
/// channels, and its frame count.
#[must_use]
pub fn short_term_max(sums: &[(f64, usize)]) -> Option<f64> {
    // 100 ms steps of two 50 ms blocks; 3 s is 30 steps.
    const PER_STEP: usize = 2;
    const WINDOW: usize = 30;
    let steps: Vec<(f64, usize)> = sums
        .chunks(PER_STEP)
        .map(|c| {
            c.iter().fold((0.0, 0_usize), |(s, n), (a, b)| {
                (s + a, n.saturating_add(*b))
            })
        })
        .collect();
    steps
        .windows(WINDOW.min(steps.len()).max(1))
        .filter_map(|w| {
            let (energy, frames) = w.iter().fold((0.0, 0_usize), |(s, n), (a, b)| {
                (s + a, n.saturating_add(*b))
            });
            let frames = f64::from(u32::try_from(frames).ok()?);
            (energy > 0.0 && frames > 0.0)
                .then(|| 10.0_f64.mul_add((energy / frames).log10(), -0.691))
        })
        .reduce(f64::max)
}

/// The loudest short-term loudness of `solo` rendered over `from..to`,
/// LUFS ([`short_term_max`]).
#[cfg(feature = "native")]
fn loudest_short_term(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
    solo: &[String],
    from: f64,
    to: f64,
) -> Option<f64> {
    let mut filters = [
        audiocore_dsp::loudness::KWeightingFilter::new(),
        audiocore_dsp::loudness::KWeightingFilter::new(),
    ];
    for filter in &mut filters {
        filter.update(f64::from(RENDER_RATE));
    }
    let mut sums: Vec<(f64, usize)> = Vec::new();
    render_blocks(daw, project_guid, solo, from, to, |block, gain| {
        let mut energy = 0.0;
        for frame in block.chunks_exact(2) {
            for (filter, sample) in filters.iter_mut().zip(frame) {
                let weighted = filter.tick(f64::from(*sample) * gain);
                energy += weighted * weighted;
            }
        }
        sums.push((energy, block.len() / 2));
    });
    short_term_max(&sums)
}

/// Where the drum kit is put by its folder's fader, LUFS.
///
/// Its loudest moment at −23 LUFS short-term. The kit is the session's
/// reference: everything
/// else is set in relation to it. Every mic stays at its staged level, so
/// each one's own processing still sees the reference kit's levels.
pub const KIT_LUFS: f64 = -23.0;

/// The kit's level, by its folder's fader.
///
/// Render the top-level `Drums` folder alone over `from..to` and set its
/// fader so its loudest short-term loudness is
/// [`KIT_LUFS`], and unmuted. Returns that loudness at a unity fader, LUFS, and the
/// fader set, dB. `None` with no Drums folder or nothing heard in it.
#[cfg(feature = "native")]
#[must_use]
pub fn kit_headroom(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
    from: f64,
    to: f64,
) -> Option<(f64, f64)> {
    let project = ProjectContext::Project(project_guid.to_owned());
    let drums = Tracks::all(daw, project.clone()).into_iter().find(|t| {
        t.parent_guid.is_none() && t.is_folder && t.name.trim().eq_ignore_ascii_case("drums")
    })?;
    let at = TrackRef::Guid(drums.guid.clone());
    Tracks::set_volume(daw, project.clone(), at.clone(), 1.0).ok()?;
    let loudest = loudest_short_term(
        daw,
        project_guid,
        std::slice::from_ref(&drums.guid),
        from,
        to,
    )?;
    let fader = (KIT_LUFS - loudest).clamp(-MOST_DB, MOST_DB);
    Tracks::set_volume(
        daw,
        project.clone(),
        at.clone(),
        10.0_f64.powf(fader / 20.0),
    )
    .ok()?;
    // A kit set to be heard is heard: unbreakable's Drums came muted from
    // its RPP, and the staged kit was silent behind it.
    Tracks::set_muted(daw, project, at, false).ok()?;
    Some((loudest, fader))
}

/// The check that staging landed where it meant to.
///
/// The staged tracks as heard over `from..to` seconds (see [`Heard`]),
/// then the drum folder's loudest short-term loudness, LUFS, and the
/// whole mix's peak, dBFS.
#[cfg(feature = "native")]
#[must_use]
pub fn heard_levels(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
    from: f64,
    to: f64,
) -> (Vec<Heard>, Option<f64>, Option<f64>) {
    let project = ProjectContext::Project(project_guid.to_owned());
    let tracks = Tracks::all(daw, project);
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    let peak_db = |peaks: &[f64]| {
        let loudest = peaks.iter().copied().fold(0.0_f64, f64::max);
        (loudest > 0.0).then(|| 20.0 * loudest.log10())
    };
    let mut heard = Vec::new();
    for track in tracks.iter().filter(|t| !t.is_folder) {
        let Some(role) = role(&path_of(track, &by_guid)) else {
            continue;
        };
        // Two 50 ms chunks make the 100 ms windows `level` reads.
        let peaks = render_peaks(
            daw,
            project_guid,
            std::slice::from_ref(&track.guid),
            from,
            to,
        );
        if let Some(level) = level(&peaks, 0.05) {
            heard.push(Heard {
                name: track.name.clone(),
                role,
                target: target(role),
                level,
            });
        }
    }
    let drums: Vec<String> = tracks
        .iter()
        .filter(|t| {
            t.parent_guid.is_none() && t.is_folder && t.name.trim().eq_ignore_ascii_case("drums")
        })
        .map(|t| t.guid.clone())
        .collect();
    let kit = (!drums.is_empty())
        .then(|| loudest_short_term(daw, project_guid, &drums, from, to))
        .flatten();
    let mix = peak_db(&render_peaks(daw, project_guid, &[], from, to));
    (heard, kit, mix)
}

#[cfg(test)]
mod tests {
    /// The loudest 3 s, not the average: a quiet minute then three loud
    /// seconds reads the loud ones. A mean square of 0.01 summed over two
    /// channels is −20.691 LUFS before K-weighting.
    #[test]
    fn short_term_is_the_loudest_three_seconds() {
        let block = |ms: f64| (ms * 2_400.0, 2_400_usize);
        let mut sums = vec![block(1e-6); 1_200];
        sums.extend(vec![block(0.01); 60]);
        let loudest = super::short_term_max(&sums).expect("a loudness");
        assert!((loudest - -20.691).abs() < 0.01, "{loudest}");
        assert_eq!(super::short_term_max(&[]), None);
    }

    use super::{Role, level, role, tom_pans};

    /// A track's role is where it was filed: the kit by piece and mic, a
    /// bass DI staged, a guitar DI and a trigger never.
    #[test]
    fn a_track_is_staged_by_where_it_was_filed() {
        assert_eq!(role(&["Drums", "Kick", "In"]), Some(Role::KickIn));
        assert_eq!(role(&["Drums", "Kick", "Out"]), Some(Role::KickOut));
        assert_eq!(role(&["Drums", "Kick", "Sub"]), Some(Role::KickSub));
        assert_eq!(role(&["Drums", "Snare", "Top"]), Some(Role::SnareTop));
        assert_eq!(role(&["Drums", "Snare", "Bottom"]), Some(Role::SnareBottom));
        assert_eq!(role(&["Drums", "Toms", "T3"]), Some(Role::Tom));
        assert_eq!(role(&["Drums", "Cymbals", "OHs"]), Some(Role::Overheads));
        assert_eq!(role(&["Drums", "Cymbals", "Hi Hat"]), Some(Role::HiHat));
        assert_eq!(role(&["Drums", "Cymbals", "Ride"]), Some(Role::Ride));
        assert_eq!(role(&["Drums", "Rooms", "Rooms"]), Some(Role::Room));
        assert_eq!(role(&["Bass", "Bass DI"]), Some(Role::BassDi));
        assert_eq!(
            role(&["Guitars", "Clean", "L", "DI"]),
            None,
            "a guitar DI never"
        );
        assert_eq!(role(&["Drums", "Kick", "Trig"]), None, "a trigger never");
        assert_eq!(
            role(&["Drums", "Process", "FX", "Room Sim"]),
            None,
            "a parallel chain is no mic"
        );
        assert_eq!(role(&["Vocals", "Lead Vocal"]), None);
        assert_eq!(
            role(&["REFERENCES", "REHEARSAL", "Drums", "Kick - Unbreakable"]),
            None,
            "a rehearsal's kick is no mic"
        );
    }

    /// The strong hits, not the loudest one nor the bleed: two hits at
    /// −6 dBFS and one at 0 in a bed of bleed at −40 read −6.
    #[test]
    fn the_level_is_the_strong_hits() {
        let step = 0.02;
        let bleed = 10.0_f64.powi(-2);
        let mut points = vec![bleed; 500];
        let hit = 10.0_f64.powf(-6.0 / 20.0);
        for at in [50, 100, 150, 200, 250, 300, 350, 400, 450] {
            points[at] = hit;
        }
        points[475] = 1.0;
        let read = level(&points, step).expect("a level");
        assert!((read - -6.0).abs() < 0.1, "{read}");
        assert_eq!(level(&[0.0; 10], step), None, "silence");
    }

    /// The plan: a staged track's gain on all its items, read from its
    /// playing lane only; a silent tom left out of the spread; a guitar
    /// DI untouched.
    #[test]
    fn the_plan_stages_what_plays_and_spreads_the_toms_heard() {
        let folder = |guid: &str, name: &str, parent: Option<&str>| daw_proto::Track {
            guid: guid.into(),
            name: name.into(),
            is_folder: true,
            parent_guid: parent.map(Into::into),
            ..daw_proto::Track::default()
        };
        let track = |guid: &str, name: &str, parent: &str| daw_proto::Track {
            guid: guid.into(),
            name: name.into(),
            parent_guid: Some(parent.into()),
            ..daw_proto::Track::default()
        };
        let mut kick_in = track("in", "In", "kick");
        kick_in.lane_count = 2;
        kick_in.lane_play_mask = 0b1;
        let tracks = vec![
            folder("drums", "Drums", None),
            folder("kick", "Kick", Some("drums")),
            kick_in,
            folder("toms", "Toms", Some("drums")),
            track("t1", "T1", "toms"),
            track("t2", "T2", "toms"),
            track("t3", "T3", "toms"),
            folder("gtr", "Guitars", None),
            track("di", "DI", "gtr"),
        ];
        let item = |guid: &str, on: &str, lane: Option<u32>| daw_proto::Item {
            guid: guid.into(),
            track_guid: on.into(),
            fixed_lane: lane,
            ..daw_proto::Item::default()
        };
        let items = vec![
            item("in-play", "in", Some(0)),
            item("in-other", "in", Some(1)),
            item("t1-a", "t1", None),
            item("t3-a", "t3", None),
            item("di-a", "di", None),
        ];
        let at = |db: f32| crate::midi::Wave {
            step: 0.02,
            points: vec![(10.0_f32.powf(db / 20.0), 0.0); 50],
        };
        let mut asked = Vec::new();
        let planned = super::plan(&tracks, &items, |item| {
            asked.push(item.guid.clone());
            match item.guid.as_str() {
                "in-play" => Some(at(-12.0)),
                "in-other" => Some(at(-1.0)),
                "t1-a" | "t3-a" => Some(at(-20.0)),
                _ => None,
            }
        });
        assert!(
            !asked.contains(&"in-other".to_string()),
            "the other lane is not read"
        );
        assert!(
            !asked.contains(&"di-a".to_string()),
            "a guitar DI is not read"
        );
        let kick = &planned.moves[0];
        assert_eq!(
            kick.items,
            ["in-play", "in-other"],
            "but every item gets the gain"
        );
        assert!(
            20.0_f64.mul_add(kick.volume.log10(), -12.0).abs() < 0.1,
            "-12 to 0: +12 dB"
        );
        assert_eq!(planned.report.silent, ["T2"]);
        let toms: Vec<(String, f64)> = planned.report.toms.clone();
        assert_eq!(toms, [("T1".to_string(), -0.8), ("T3".to_string(), 0.8)]);
    }

    #[test]
    fn the_toms_spread_eighty_left_to_eighty_right() {
        let pans = tom_pans(4);
        assert_eq!(pans.len(), 4);
        assert!((pans[0] + 0.8).abs() < 1e-9 && (pans[3] - 0.8).abs() < 1e-9);
        assert!(pans[1] < 0.0 && pans[2] > 0.0);
        assert_eq!(tom_pans(1), [0.0]);
    }
}
