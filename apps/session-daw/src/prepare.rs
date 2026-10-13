//! Prepare a multitrack session for playing: organize it, build the song
//! from its chart, and generate the click and guide — on the engine this
//! window opened, before the view reads it.
//!
//! Each step is the shared one, not a window-only copy:
//! `dynamic_template::apply::organize` (the `--apply-buses` pass, run live),
//! `session::keyflow::from_chart::build_from_chart`, and
//! `session::guide::Guide::generate`. Each is optional; they run in that
//! order because the guide reads the song the chart stamps, and files its
//! tracks into the folder the organizer builds.

use std::path::PathBuf;

use daw::service::ProjectContext;

/// What to do to a freshly opened session.
#[derive(Debug, Default, Clone)]
pub struct Prepare {
    pub organize: bool,
    pub chart: Option<PathBuf>,
    pub guide: bool,
}

impl Prepare {
    /// Everything a song gets on its first open: organized, built from the
    /// chart beside it, and given its guide. What a setlist's songs get,
    /// and what `session-desktop --engine` gives a song.
    #[must_use]
    #[cfg(not(target_arch = "wasm32"))]
    pub fn for_song(path: &std::path::Path) -> Self {
        Self::for_song_in(crate::folder::disk(), path)
    }

    /// [`Self::for_song`] for a song in `folder`.
    #[must_use]
    pub fn for_song_in(folder: &dyn crate::folder::Folder, path: &std::path::Path) -> Self {
        // `FTS_SESSION_PREPARE=0`: the multitrack as it is — a template
        // already laid out (the golden session) opens as the scenes render
        // it rather than organized a second time.
        if std::env::var("FTS_SESSION_PREPARE").as_deref() == Ok("0") {
            return Self {
                organize: false,
                chart: None,
                guide: false,
            };
        }
        let chart = path
            .parent()
            .and_then(|dir| folder.only_with_extension(dir, "kf"));
        Self {
            organize: true,
            chart,
            guide: true,
        }
    }

    /// From `FTS_BLITZ_ORGANIZE=1`, `FTS_BLITZ_CHART=<file.kf>` and
    /// `FTS_BLITZ_GUIDE=1`.
    #[must_use]
    pub fn from_env() -> Self {
        let on = |key: &str| std::env::var(key).is_ok_and(|v| !v.is_empty() && v != "0");
        Self {
            organize: on("FTS_BLITZ_ORGANIZE"),
            chart: std::env::var_os("FTS_BLITZ_CHART").map(PathBuf::from),
            guide: on("FTS_BLITZ_GUIDE"),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.organize && self.chart.is_none() && !self.guide
    }

    /// Run the requested steps against `opened`. Stops at the first step
    /// that fails — a guide built for a song the chart never stamped is
    /// worse than no guide.
    ///
    /// # Errors
    ///
    /// The step that failed, and why.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run(&self, opened: &crate::open::Opened) -> eyre::Result<()> {
        self.run_in(crate::folder::disk(), opened)
    }

    /// [`Self::run`], reading the chart and lyrics from `folder`.
    ///
    /// # Errors
    ///
    /// The step that failed, and why.
    pub fn run_in(
        &self,
        folder: &dyn crate::folder::Folder,
        opened: &crate::open::Opened,
    ) -> eyre::Result<()> {
        // This window's runtime when it has one; otherwise whatever
        // runtime the caller runs on (`session-desktop --engine`).
        #[cfg(feature = "native")]
        let _entered = crate::open::runtime().map(|runtime| runtime.enter());
        let chart = match &self.chart {
            Some(path) => Some(
                folder
                    .read_to_string(path)
                    .map_err(|e| eyre::eyre!("chart {}: {e}", path.display()))?,
            ),
            None => None,
        };
        steps(
            &opened.daw,
            &opened.project_guid,
            self.organize,
            chart.as_deref(),
            self.guide,
        )?;
        // The song's synced lyrics, when a `.lrc` sits beside its chart:
        // onto the LINES track, from where the song starts.
        if let (Some(path), Some(text)) = (&self.chart, chart.as_deref())
            && let Some(lrc) = path
                .parent()
                .and_then(|dir| folder.only_with_extension(dir, "lrc"))
        {
            let lyrics = folder
                .read_to_string(&lrc)
                .map_err(|e| eyre::eyre!("lyrics {}: {e}", lrc.display()))?;
            let lines = stamp_lyrics(&opened.daw, &opened.project_guid, text, &lyrics)?;
            tracing::info!(lyrics.lines = lines, "prepare: lyrics stamped");
        }
        Ok(())
    }
}

/// The one `.lrc` beside a chart, if there is exactly one.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn lrc_beside(chart: &std::path::Path) -> Option<PathBuf> {
    crate::folder::Folder::only_with_extension(&crate::folder::Disk, chart.parent()?, "lrc")
}

/// Stamp the synced lyrics `lrc` (an `.lrc`'s text) onto the song's LINES track: the
/// recording's 0 at the song's start (SONGSTART, from the chart), the
/// last line held to its end. Returns how many lines went on.
///
/// # Errors
///
/// The chart does not lay out, or a line's item could not be made.
pub fn stamp_lyrics(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
    chart: &str,
    lrc: &str,
) -> eyre::Result<usize> {
    let layout = session::setlist::chart_import::chart_to_layout(chart)
        .map_err(|e| eyre::eyre!("chart: {e:?}"))?;
    let text = lrc;
    let mut lyrics =
        session::lyrics::Lyrics::from_lrc(text, layout.song_start_seconds, layout.song_end_seconds);
    let project = ProjectContext::Project(project_guid.to_owned());
    // Pinned to the song where the `.lrc` says (see `lyrics::Anchor`): its
    // section's downbeat, from the regions the chart just laid out, and
    // the beats from there at the song's tempo.
    if let Some(anchor) = session::lyrics::Anchor::in_lrc(text) {
        use daw::service::Regions as _;
        let downbeat = daw
            .all(project.clone())
            .into_iter()
            .filter(|r| anchor.names(&r.name))
            .map(|r| r.time_range.start_seconds())
            .reduce(f64::min);
        let placed = downbeat.is_some_and(|downbeat| {
            let at = anchor
                .beats
                .mul_add(60.0 / layout.tempo_bpm.max(1.0), downbeat);
            lyrics.align(
                &anchor.line,
                at,
                anchor.drop_before,
                layout.song_end_seconds,
            )
        });
        if !placed {
            tracing::warn!(
                lyrics.anchor_section = %anchor.section,
                lyrics.anchor_found = downbeat.is_some(),
                "prepare: the lyrics' anchor matched nothing; they stay where the recording has them"
            );
        }
    }
    session::lyrics::stamp_lines(daw, project, &lyrics).map_err(|e| eyre::eyre!("lyrics: {e}"))
}

/// The song's name, "Title - Artist": the project's name less a working
/// copy's suffix, title-cased (`unbreakable.current` is "Unbreakable"),
/// and the artist the folder above the song's own is named for, less a
/// snapshot's date (`crescendum-2026-10-08/unbreakable/` is "Crescendum").
fn song_name(daw: &daw_standalone::sync::Standalone, project: &ProjectContext) -> Option<String> {
    use daw::service::Projects as _;
    let info = daw.info(project.clone()).ok()?;
    let title = title_case(strip_working_suffix(&info.name));
    if title.is_empty() {
        return None;
    }
    let artist = std::path::Path::new(&info.path)
        .parent()
        .and_then(std::path::Path::parent)
        .and_then(std::path::Path::file_name)
        .and_then(std::ffi::OsStr::to_str)
        .map(|folder| title_case(&strip_date(folder)))
        .filter(|artist| !artist.is_empty());
    Some(match artist {
        Some(artist) => format!("{title} - {artist}"),
        None => title,
    })
}

/// Remove the markers that only name the song (`unbreakable.organized`,
/// a title an earlier tool left at the start): the SONG region names it
/// now. Returns their names.
fn drop_title_markers(
    daw: &daw_standalone::sync::Standalone,
    project: &ProjectContext,
) -> Vec<String> {
    use daw::service::{Markers, Projects as _};
    let Ok(info) = daw.info(project.clone()) else {
        return Vec::new();
    };
    let title = strip_working_suffix(&info.name).to_lowercase();
    let mut dropped = Vec::new();
    for marker in Markers::all(daw, project.clone()) {
        if strip_working_suffix(&marker.name).to_lowercase() == title
            && let Some(id) = marker.id
            && Markers::remove(daw, project.clone(), id).is_ok()
        {
            dropped.push(marker.name);
        }
    }
    dropped
}

/// `unbreakable.current` → `unbreakable`.
fn strip_working_suffix(name: &str) -> &str {
    [".current", ".organized", ".raw"]
        .iter()
        .find_map(|suffix| name.strip_suffix(suffix))
        .unwrap_or(name)
        .trim()
}

/// `crescendum-2026-10-08` → `crescendum`: a trailing `-YYYY-MM-DD`.
fn strip_date(folder: &str) -> String {
    let parts: Vec<&str> = folder.split('-').collect();
    let digits =
        |part: &str, len: usize| part.len() == len && part.bytes().all(|b| b.is_ascii_digit());
    match parts.as_slice() {
        [head @ .., year, month, day]
            if !head.is_empty() && digits(year, 4) && digits(month, 2) && digits(day, 2) =>
        {
            head.join("-").trim().to_string()
        }
        _ => folder.trim().to_string(),
    }
}

/// Each word's first letter capitalised, the rest as written
/// (`it knows my name` → `It Knows My Name`, `AG GTR` stays).
fn title_case(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod song_name_tests {
    #[test]
    fn the_end_is_before_the_first_silence() {
        let at = |s: u64, peak: f32| (s * 10, peak);
        // Heard to 3, quiet 4–8 (a gap of 5 >= 4), heard again at 9.
        let chunks = [
            at(0, 1.0),
            at(1, 1.0),
            at(2, 0.5),
            at(3, 0.2),
            at(4, 0.0),
            at(5, 0.0),
            at(6, 0.0),
            at(7, 0.0),
            at(8, 0.0),
            at(9, 0.9),
        ];
        assert_eq!(
            super::last_heard(&chunks, 0.1, 10, 40).map(|c| c.0),
            Some(30)
        );
        // A short pause does not end it.
        let paused = [at(0, 1.0), at(1, 0.0), at(2, 0.0), at(3, 0.8)];
        assert_eq!(
            super::last_heard(&paused, 0.1, 10, 40).map(|c| c.0),
            Some(30)
        );
    }

    #[test]
    fn a_song_is_named_title_dash_artist() {
        assert_eq!(
            super::strip_working_suffix("unbreakable.current"),
            "unbreakable"
        );
        assert_eq!(super::strip_date("crescendum-2026-10-08"), "crescendum");
        assert_eq!(super::strip_date("Crescendum"), "Crescendum");
        assert_eq!(
            super::strip_date("north-point-2026-1-08"),
            "north-point-2026-1-08"
        );
        assert_eq!(super::title_case("it knows my name"), "It Knows My Name");
        assert_eq!(super::title_case("unbreakable"), "Unbreakable");
    }
}

/// Where the song ends: where most of what is still playing at its last
/// marker stops — the median of those tracks' last item ends. Not the
/// latest end: a guitar take left running, a click rendered long, a
/// rehearsal in the references runs on for minutes past the song. The
/// references and the guide are not counted; with nothing else, all of it.
fn media_end(daw: &daw_standalone::sync::Standalone, project: &ProjectContext) -> MediaEnd {
    use daw::service::{Items as _, Markers, Tracks};
    let tracks = Tracks::all(daw, project.clone());
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> =
        tracks.iter().map(|t| (t.guid.as_str(), t)).collect();
    // Under REFERENCES or the Guide, however deep.
    let referenced = |guid: &str| {
        let mut at = by_guid.get(guid).copied();
        while let Some(track) = at {
            let name = track.name.trim();
            if name.eq_ignore_ascii_case(dynamic_template::apply::REFERENCES_FOLDER)
                || (track.parent_guid.is_none() && name.eq_ignore_ascii_case("Guide"))
            {
                return true;
            }
            at = track
                .parent_guid
                .as_deref()
                .and_then(|p| by_guid.get(p).copied());
        }
        false
    };
    let last_marker = Markers::all(daw, project.clone())
        .iter()
        .filter_map(|m| m.position.seconds())
        .fold(0.0, f64::max);
    let items = daw.get_all_items(project.clone());
    let ends = |song_only: bool| {
        let mut by_track: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
        for item in items
            .iter()
            .filter(|item| !song_only || !referenced(&item.track_guid))
        {
            let end = item.position.as_seconds() + item.length.as_seconds();
            let slot = by_track.entry(item.track_guid.as_str()).or_insert(0.0);
            *slot = slot.max(end);
        }
        let mut ends: Vec<f64> = by_track
            .into_values()
            .filter(|end| *end > last_marker)
            .collect();
        ends.sort_by(f64::total_cmp);
        Some((*ends.get(ends.len() / 2)?, *ends.last()?))
    };
    let (median, latest) = ends(true)
        .or_else(|| ends(false))
        .unwrap_or((last_marker, last_marker));
    MediaEnd {
        last_marker,
        median,
        latest,
        hidden: tracks
            .iter()
            .filter(|t| t.parent_guid.is_none() && referenced(&t.guid) && !t.muted)
            .map(|t| t.guid.clone())
            .collect(),
    }
}

/// Where a song's media runs to, past its last marker ([`media_end`]).
struct MediaEnd {
    last_marker: f64,
    /// The median end of the tracks still playing at the last marker.
    median: f64,
    /// The latest of them.
    latest: f64,
    /// The top-level folders not counted (REFERENCES, the Guide), unmuted.
    hidden: Vec<String>,
}

/// Where the song ends: the bar line after its sound dies away.
///
/// The mix from the last marker to where the media runs out is rendered
/// — the references and the guide muted for it — and the song ends at the
/// first bar line after the last moment it is within [`HEARD_DB`] of its
/// loudest there: the last hit's ring-out heard, the silence after it not.
/// unbreakable's guitars hit on bar 181 and ring through 184; its media
/// runs on to 317 s. Without a render (a page), the median end
/// ([`media_end`]), on a bar line too.
fn song_end(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
    project: &ProjectContext,
) -> f64 {
    use daw::service::TempoMap as _;
    let media = media_end(daw, project);
    #[cfg(feature = "native")]
    let heard = heard_end(daw, project_guid, project, &media);
    #[cfg(not(feature = "native"))]
    let heard: Option<f64> = {
        let _ = project_guid;
        None
    };
    let end = heard.unwrap_or(media.median);
    // On a bar line: this one when it is on one, else the next.
    let (measure, beat, fraction) = daw.time_to_musical(project.clone(), end);
    if beat == 1 && fraction < 1e-3 {
        return end;
    }
    daw.musical_to_time(project.clone(), measure.saturating_add(1), 1, 0.0)
}

/// How long a silence past the last marker ends the song: what plays
/// after one is not the song's — a take recorded after it (the ballad's
/// acoustic runs on four seconds past the drums' last ring, to 493 s).
const SILENCE_SECONDS: u64 = 3;

/// The last chunk heard (`peak >= floor`) before the first silence of
/// `gap` frames that follows something heard: `chunks` are `(frame,
/// peak)`, `chunk` frames each.
fn last_heard(chunks: &[(u64, f32)], floor: f32, chunk: u64, gap: u64) -> Option<&(u64, f32)> {
    let mut heard: Option<&(u64, f32)> = None;
    for each in chunks {
        if each.1 >= floor {
            heard = Some(each);
        } else if let Some(last) = heard
            && each.0.saturating_sub(last.0.saturating_add(chunk)) >= gap
        {
            break;
        }
    }
    heard
}

/// How far under the song's loudest its end still counts as heard: the
/// last hit's ring at -48 dB is the song's, the room's hiss under it not.
const HEARD_DB: f32 = 48.0;

/// The last moment the mix past the last marker is heard ([`HEARD_DB`]),
/// rendered with the references and the guide muted.
#[cfg(feature = "native")]
fn heard_end(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
    project: &ProjectContext,
    media: &MediaEnd,
) -> Option<f64> {
    use daw::service::{TrackRef, Tracks};
    use std::time::Duration;
    const RATE: u32 = 48_000;
    // 50 ms, in frames: one block and one peak.
    const CHUNK: usize = 2_400;
    const CHUNK_FRAMES: u64 = 2_400;
    let frame = |seconds: f64| {
        let nanos = Duration::try_from_secs_f64(seconds.max(0.0))
            .ok()?
            .as_nanos();
        u64::try_from(nanos.checked_mul(u128::from(RATE))? / 1_000_000_000).ok()
    };
    let (from, to) = (frame(media.last_marker)?, frame(media.latest)?);
    for guid in &media.hidden {
        let _ = Tracks::set_muted(daw, project.clone(), TrackRef::Guid(guid.clone()), true);
    }
    let renderer =
        daw::standalone::audio_engine::render::ProjectRenderer::new(daw, project_guid, RATE);
    let mut peaks: Vec<(u64, f32)> = Vec::new();
    let mut at = from;
    while at < to {
        let block = renderer.render_block(at, CHUNK);
        let peak = block.samples.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
        peaks.push((at, peak));
        at = at.saturating_add(CHUNK_FRAMES);
    }
    for guid in &media.hidden {
        let _ = Tracks::set_muted(daw, project.clone(), TrackRef::Guid(guid.clone()), false);
    }
    let loudest = peaks.iter().fold(0.0_f32, |p, (_, peak)| p.max(*peak));
    if loudest <= 0.0 {
        return None;
    }
    let floor = loudest * 10.0_f32.powf(-HEARD_DB / 20.0);
    last_heard(
        &peaks,
        floor,
        CHUNK_FRAMES,
        u64::from(RATE).saturating_mul(SILENCE_SECONDS),
    )
    .map(|(at, _)| {
        let frames = at.saturating_add(CHUNK_FRAMES);
        Duration::from_nanos(
            frames
                .saturating_mul(1_000_000_000)
                .checked_div(u64::from(RATE))
                .unwrap_or(0),
        )
        .as_secs_f64()
    })
}

/// The one `.kf` chart in the project's folder, if there is exactly one.
/// Two would be a guess, and a wrong chart is worse than none.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn chart_beside(project: &std::path::Path) -> Option<PathBuf> {
    crate::folder::Folder::only_with_extension(&crate::folder::Disk, project.parent()?, "kf")
}

/// Prepare the song at `rpp` and save it as `Song.session` beside it —
/// what the app does the first time it opens a multitrack, done ahead of
/// time (a whole setlist before a service, say). Returns where the session
/// was written. Any session already there is replaced: this is the
/// explicit way to prepare again.
///
/// One song per process — the daw facade the steps go through is
/// process-wide, and it is bound to the first engine opened.
///
/// # Errors
///
/// The song did not open, a step failed (nothing is saved then), or the
/// session could not be written.
#[cfg(feature = "native")]
pub fn prepare_and_save(rpp: &std::path::Path, chart: Option<PathBuf>) -> eyre::Result<PathBuf> {
    let prepare = Prepare {
        organize: true,
        chart,
        guide: true,
    };
    let opened = crate::open::open_silent(rpp)?;
    prepare.run(&opened)?;
    let dir = rpp.with_extension("session");
    crate::session_file::save_session(&opened.daw, &opened.project_guid, &dir)
        .map_err(|e| eyre::eyre!("saving {}: {e}", dir.display()))
}

/// Lay an edited chart over the open session: rebuild the song's structure
/// from it (tempo, markers, section regions, key and chords — see
/// [`session::keyflow::from_chart::rebuild_from_chart`]) and, when `guide`,
/// regenerate the click, count and cues to match.
///
/// The guide needs nothing cleared first: it writes one item per role over
/// the whole song, from the count-in, and every song starts there — so the
/// old song's item always overlaps the new one's span, and generating
/// replaces it.
///
/// The live half of editing a chart: Organize mode calls this after each
/// pause in typing. Text that does not parse changes nothing.
///
/// # Errors
///
/// The chart does not parse, or a step failed.
pub fn apply_chart(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
    chart: &str,
    guide: bool,
) -> eyre::Result<session::keyflow::from_chart::ChartRebuilt> {
    let project = ProjectContext::Project(project_guid.to_owned());
    let rebuilt = session::keyflow::from_chart::rebuild_from_chart(daw, &project, chart)?;
    if guide {
        session::guide::Guide::new(daw.clone())
            .with_instrument(GUIDE_INSTRUMENT)
            .generate(session::guide::GuideScope::All)
            .map_err(|e| eyre::eyre!("guide: {e}"))?;
    }
    Ok(rebuilt)
}

/// The FX the Click / Count / Guide tracks play through
/// (`guide_instrument::IDENT`).
pub const GUIDE_INSTRUMENT: &str = "fts.guide";

/// The steps themselves, on any host: the native window's opened engine,
/// or the web demo's (which has the chart's text rather than a file).
///
/// # Errors
///
/// The step that failed, and why.
pub fn steps(
    daw: &daw_standalone::sync::Standalone,
    project_guid: &str,
    organize: bool,
    chart: Option<&str>,
    guide: bool,
) -> eyre::Result<()> {
    let project = ProjectContext::Project(project_guid.to_owned());

    let mut target = dynamic_template::apply::DawTarget::on(daw.clone(), project.clone());
    if organize {
        // Folders first — Drums, Bass, Guitars … with any wrapper the
        // multitrack came in taken apart — so the bus pass routes
        // groups rather than loose tracks.
        let arranged = target.arrange_into_groups()?;
        tracing::info!(
            unwrapped = ?arranged.unwrapped,
            folders = ?arranged.created,
            placed = arranged.placed,
            "prepare: arranged into groups"
        );
        let organized = dynamic_template::apply::organize(&mut target)
            .map_err(|e| eyre::eyre!("organize: {e}"))?;
        let hidden = target.hide_captures()?;
        let buses = target.hide_buses()?;
        let triggers = target.mute_triggers()?;
        // The golden template's drum Process folder: the kit's parallel
        // compressors and rooms, sent from the kit, to DRUM BUS.
        let process = target.add_drum_process()?;
        tracing::info!(process = process.len(), "prepare: drum process");
        // The recorded tracks at the reference kit's levels, as item
        // volume: nothing rendered, the audio as it was recorded.
        let staged = crate::gain_stage::stage(daw, &project)?;
        // And the kit at the session's reference, −23 LUFS short-term at
        // its loudest: its folder's fader.
        #[cfg(feature = "native")]
        if let Some((loudest, fader)) =
            crate::gain_stage::kit_headroom(daw, project_guid, 0.0, media_end(daw, &project).latest)
        {
            tracing::info!(
                kit.short_term_lufs = loudest,
                kit.fader_db = fader,
                "prepare: kit level"
            );
        }
        tracing::info!(
            staged = staged.staged.len(),
            silent = ?staged.silent,
            toms = ?staged.toms,
            "prepare: gain staged"
        );
        for each in &staged.staged {
            tracing::debug!(
                track = %each.name,
                role = ?each.role,
                level_db = each.level,
                gain_db = each.gain_db,
                "prepare: staged"
            );
        }
        tracing::info!(
            hidden = ?hidden,
            buses = ?buses,
            triggers = ?triggers,
            "prepare: captures and buses hidden, triggers muted"
        );
        tracing::info!(
            tracks = organized.existing,
            buses = organized.buses.len(),
            unsorted = organized.unsorted.len(),
            unrouted = ?organized.routing.unrouted,
            "prepare: organized"
        );
    }
    if chart.is_none() {
        // No chart to lay the song out: its structure is the multitrack's
        // own markers — the sections made regions, the song's region and
        // bounds placed, the notes left marks — before the guide, whose
        // cues follow the sections.
        let mut converted = session::keyflow::actions::convert_markers_in(
            daw,
            &project,
            song_name(daw, &project).as_deref(),
            Some(song_end(daw, project_guid, &project)),
        )
        .map_err(|e| eyre::eyre!("markers: {e}"))?;
        let dropped = drop_title_markers(daw, &project);
        converted.marks.retain(|mark| !dropped.contains(mark));
        tracing::info!(
            dropped = ?dropped,
            sections = ?converted.sections,
            marks = ?converted.marks,
            song = ?converted.song,
            "prepare: markers made the song's sections"
        );
    }
    if let Some(text) = chart {
        // Rebuild, not build: a multitrack can arrive with structure of its
        // own — the importer's guessed `Cue N` sections, its SONGSTART —
        // and the chart replaces it rather than landing beside it.
        let built = session::keyflow::from_chart::rebuild_from_chart(daw, &project, text)
            .map_err(|e| eyre::eyre!("chart: {e}"))?
            .built;
        tracing::info!(
            title = %built.title,
            bpm = built.tempo_bpm,
            sections = built.sections,
            "prepare: song built from chart"
        );
        // The key and the chords read from the ruler's CHORDS lane, so
        // their tracks are out of the track panel, and LINES and HITS
        // with them until something writes melodies and hits there; the
        // Keyflow folder stays, for its Lyrics track — the lines as
        // items, to be moved and trimmed where the song has them.
        for track in ["KEY", "CHORD", "LINES", "HITS"] {
            target.hide_in_tcp(track)?;
        }
    }
    if guide {
        session::guide::Guide::new(daw.clone())
            .with_instrument(GUIDE_INSTRUMENT)
            .generate(session::guide::GuideScope::All)
            .map_err(|e| eyre::eyre!("guide: {e}"))?;
        tracing::info!("prepare: click and guide generated");
        // The multitrack's own click and guide stems, renamed and
        // muted by the guide: kept for reference, out of the panel.
        target.hide_in_tcp("Click Audio")?;
        target.hide_in_tcp("Guide Audio")?;
    }
    if organize {
        // Last, after the renders that read the song — the kit's level,
        // where the sound ends — which the reverbs and the pitch shifter
        // below would make cost a minute. The gates change the kit's
        // loudest moment little.
        //
        // The golden template's kit and vocal returns, made of the
        // session's tracks: each piece's Sum and what the template puts
        // beside it, the toms as pieces, the Vox FX the leads feed — then
        // each piece's gate keyed from its trigger where it now sits.
        let kit = target.apply_golden_kit()?;
        // What each kit track is, on the track: the scenes read it there.
        let marked = target.mark_kit_kinds()?;
        let vox_fx = target.apply_vox_fx()?;
        let keyed = target.key_drum_gates()?;
        tracing::info!(
            kit = ?kit,
            kinds_marked = marked,
            vox_fx = vox_fx.len(),
            keyed = ?keyed,
            "prepare: golden template"
        );
        // The mix's processing: the FTS processors built in (Tone on every
        // recorded track and EQ/Comp on the buses, bypassed; the Process
        // chains set).
        #[cfg(feature = "native")]
        {
            let chains = crate::mix_chains::add(daw, &project)?;
            tracing::info!(
                tone = chains.tone,
                buses = chains.buses,
                process = chains.process,
                gates = ?chains.gates,
                one_notes = ?chains.one_notes,
                returns = chains.returns,
                mics = ?chains.mics,
                unknown = ?chains.unknown,
                "prepare: mix chains"
            );
        }
        // The template's top-level order — what is played to first,
        // then the song, then the instruments, routing last — and the
        // routing out of the track panel.
        target.order_top_level(
            &["Guide", "Keyflow"],
            &[
                dynamic_template::apply::REFERENCES_FOLDER,
                "CLICK + GUIDE BUS",
                "MIX BUS",
                "UNSORTED",
            ],
        )?;
        target.hide_in_tcp("CLICK + GUIDE BUS")?;
        target.hide_in_tcp("MIX BUS")?;
    }
    Ok(())
}

#[cfg(test)]
mod chart_tests {
    use super::chart_beside;

    /// The chart beside a project is used only when it is the only one.
    #[test]
    fn a_session_folder_s_one_chart_is_its_chart() {
        let dir = std::env::temp_dir().join(format!("session-chart-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let project = dir.join("Song.RPP");
        std::fs::write(&project, "").unwrap();
        assert_eq!(chart_beside(&project), None, "no chart at all");

        std::fs::write(dir.join("Song.kf"), "").unwrap();
        assert_eq!(chart_beside(&project), Some(dir.join("Song.kf")));

        std::fs::write(dir.join("Other.KF"), "").unwrap();
        assert_eq!(chart_beside(&project), None, "two charts is a guess");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
