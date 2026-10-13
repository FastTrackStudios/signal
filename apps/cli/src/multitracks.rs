//! `session import` — a folder of multitracks becomes a session on the
//! grid.
//!
//! Commercial multitracks arrive as a pile of stems, all starting at the
//! same instant, with a **Click** stem and a **Guide** (or Cue) stem beside
//! them, and the tempo written into the folder's name. Nothing in them says
//! where a bar line is: the stems usually open with a count-in, so dropping
//! them at zero puts every bar line in the wrong place and the session's
//! own click fights the record's.
//!
//! So the click stem is read, not guessed at: [`onsets`] finds its ticks,
//! their spacing gives the tempo (checked against the name's, which is
//! sometimes double), and the first tick is where beat one is. Every stem
//! is then trimmed by that much — `SOFFS`, REAPER's slip offset, so nothing
//! is copied or rewritten — and bar one of the session lands exactly on the
//! record's first beat.
//!
//! The guide stem's spoken cues mark the sections. Their onsets become
//! regions, snapped to the bar, named `Section 1…` — the shape of the
//! arrangement, for somebody to name properly in the chart afterwards.

use std::path::{Path, PathBuf};

use fts_sample::mapped::PcmFile;

/// What a song's folder says about it, and what its click says.
#[derive(Debug, Clone, PartialEq)]
pub struct Song {
    pub title: String,
    pub bpm: f64,
    /// Beats per bar, and the beat's note value.
    pub time_sig: (u32, u32),
    pub key: Option<String>,
    /// Where the click's first tick is, in seconds — what every stem is
    /// trimmed by.
    pub first_beat: f64,
    /// Where the music comes in, relative to the trim.
    pub songstart: f64,
    /// Section starts (after the trim), from the guide's cues.
    pub sections: Vec<f64>,
}

/// One stem: the file, and the name it takes in the session.
#[derive(Debug, Clone, PartialEq)]
pub struct Stem {
    pub path: PathBuf,
    pub name: String,
    pub seconds: f64,
}

/// The name a stem takes in the session: the file's own, with the song's
/// name and any track number taken off the front — "God, I'm Just Grateful
/// - EG 4.wav" and "14 - Electric Guitar 1.wav" are "EG 4" and "Electric
/// Guitar 1".
#[must_use]
pub fn stem_name(file: &str) -> String {
    let stem = file.rsplit_once('.').map_or(file, |(name, _)| name);
    let after_dash = stem.rsplit_once(" - ").map_or(stem, |(_, name)| name);
    after_dash.trim().to_owned()
}

/// The song's own name, as its stems spell it: the part before " - " that
/// every stem shares ("Holy Forever - Click.wav", "Holy Forever - AG.wav"
/// → "Holy Forever"). `None` when they do not agree — numbered stems
/// ("01 - Click.wav"), or a folder of loose files.
#[must_use]
pub fn title_from_stems(files: &[String]) -> Option<String> {
    let mut prefix: Option<String> = None;
    for file in files {
        let stem = file
            .rsplit_once('.')
            .map_or(file.as_str(), |(name, _)| name);
        let (before, _) = stem.split_once(" - ")?;
        // A track number is not a name.
        if before.trim().chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        match &prefix {
            None => prefix = Some(before.trim().to_owned()),
            Some(seen) if seen == before.trim() => {}
            Some(_) => return None,
        }
    }
    prefix.filter(|p| !p.is_empty())
}

/// Whether a stem is the click, the guide, or neither — by name, the way
/// every vendor labels them.
fn role(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    if lower.contains("click") {
        Some("click")
    } else if lower.contains("guide") || lower.contains("cue") {
        Some("guide")
    } else {
        None
    }
}

/// What a multitrack folder's name says: the title, the tempo, the key and
/// the time signature, in whatever order and separator the vendor used
/// (`AlwaysOnTime-ElevationWorship_F_136_2-4`, `Holy Forever _ Bethel Music
/// _ Bb`, `4-4  _  127 BPM  _  A Major`).
#[must_use]
pub fn read_name(name: &str) -> (String, Option<f64>, Option<(u32, u32)>, Option<String>) {
    let (mut bpm, mut sig, mut key) = (None, None, None);
    let mut words: Vec<String> = Vec::new();
    for raw in name
        .split(['_', '-', ' '])
        .filter(|part| !part.trim().is_empty())
    {
        let part = raw.trim();
        let upper = part.to_uppercase();
        if let Ok(number) = part.parse::<f64>() {
            // A tempo, or a time signature's half if the next part is too.
            if (40.0..=220.0).contains(&number) && bpm.is_none() {
                bpm = Some(number);
            } else if (1.0..=16.0).contains(&number) {
                sig = match sig {
                    None => Some((number as u32, 4)),
                    Some((beats, _)) => Some((beats, number as u32)),
                };
            }
            continue;
        }
        if upper == "BPM" || upper == "MAJOR" || upper == "MINOR" {
            continue;
        }
        // A key is a note name, on its own, possibly with a flat or sharp.
        let is_key = matches!(upper.len(), 1..=2)
            && upper.starts_with(['A', 'B', 'C', 'D', 'E', 'F', 'G'])
            && upper[1..].chars().all(|c| matches!(c, 'B' | '#'));
        if is_key && key.is_none() {
            key = Some(part.to_owned());
            continue;
        }
        words.push(part.to_owned());
    }
    // The title is what is left, up to the artist — vendors put the song
    // first, so the run of words before a known publisher word is enough.
    let title = words.join(" ");
    (title, bpm, sig, key)
}

/// The tempo and metre a chart would call this, from what the vendor
/// wrote and what the click ticks.
///
/// Vendors click in halves ("2/4 at 136" for a song in 4/4 at 68) and in
/// eighths (a ballad clicking at 252), because a click is for playing to,
/// not for notating with. Neither changes where the bar lines fall — every
/// other tick is still a beat — so folding them costs nothing and gives a
/// chart the tempo a band would say out loud.
#[must_use]
pub fn musical_tempo(bpm: f64, sig: (u32, u32)) -> (f64, (u32, u32)) {
    // A 2/4 vendor bar is half of the 4/4 one everybody counts.
    let (mut bpm, sig) = if sig == (2, 4) {
        (bpm / 2.0, (4, 4))
    } else {
        (bpm, sig)
    };
    // And a click in eighths (or sixteenths) is that again.
    while bpm > 160.0 {
        bpm /= 2.0;
    }
    (bpm, sig)
}

/// A title as it should read: `AlwaysOnTime` is three words, and the
/// artist is not part of the song's name.
#[must_use]
pub fn tidy_title(raw: &str) -> String {
    /// Bands and publishers, so a folder named after both keeps the song.
    const ARTISTS: [&str; 6] = [
        "Elevation Worship",
        "Elevation Rhythm",
        "Bethel Music",
        "Gateway Worship",
        "Elevation",
        "Bethel",
    ];
    // A filesystem-safe apostrophe comes back: `I_m` → `I'm`.
    let raw: String = raw
        .char_indices()
        .map(|(i, c)| {
            let between_letters = c == '_'
                && raw[..i]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_alphabetic)
                && raw[i + 1..].chars().next().is_some_and(char::is_alphabetic);
            if between_letters { '\'' } else { c }
        })
        .collect();
    let raw = raw.as_str();
    // Split camel case first: `AlwaysOnTime` → `Always On Time`.
    let mut spaced = String::new();
    for (i, c) in raw.char_indices() {
        let previous = raw[..i].chars().next_back();
        if c.is_uppercase() && previous.is_some_and(|p| p.is_lowercase() || p.is_ascii_digit()) {
            spaced.push(' ');
        }
        spaced.push(c);
    }
    let mut title = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    for artist in ARTISTS {
        // Only when something is left: a folder is sometimes the artist
        // first ("Elevation Worship - Praise").
        let without = title.replace(artist, " ");
        let cleaned = without.split_whitespace().collect::<Vec<_>>().join(" ");
        if !cleaned.is_empty() {
            title = cleaned;
        }
    }
    title
}

/// Every `.wav` a song folder holds, preferring a `MultiTracks` folder over
/// a `SingleTrack` mix beside it.
///
/// # Errors
///
/// The folder cannot be read.
pub fn stems(folder: &Path) -> eyre::Result<Vec<Stem>> {
    let mut found: Vec<PathBuf> = Vec::new();
    collect(folder, &mut found)?;
    let multi: Vec<PathBuf> = found
        .iter()
        .filter(|p| {
            p.components().any(|c| {
                c.as_os_str()
                    .to_string_lossy()
                    .to_lowercase()
                    .contains("multitrack")
            })
        })
        .cloned()
        .collect();
    let chosen = if multi.is_empty() { found } else { multi };
    let mut stems: Vec<Stem> = Vec::new();
    for path in chosen {
        // A single stereo mix is not a stem.
        if path.components().any(|c| {
            c.as_os_str()
                .to_string_lossy()
                .to_lowercase()
                .contains("singletrack")
        }) {
            continue;
        }
        let name = stem_name(&path.file_name().unwrap_or_default().to_string_lossy());
        let seconds = PcmFile::open(&path).map_or(0.0, |pcm: PcmFile| {
            pcm.frames() as f64 / f64::from(pcm.sample_rate().max(1))
        });
        stems.push(Stem {
            path,
            name,
            seconds,
        });
    }
    stems.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(stems)
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> eyre::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("wav"))
        {
            out.push(path);
        }
    }
    Ok(())
}

/// Where a signal starts over and over: the times (in seconds) a short
/// window's energy rises past `rise` times the recent average, no closer
/// together than `apart`.
///
/// Deliberately plain: a click track is the easiest onset detection there
/// is (silence, tick, silence), and a guide's spoken cues are the same
/// shape at a slower rate. Reading the file through a memory map keeps a
/// ten-minute stem off the heap.
#[must_use]
pub fn onsets(pcm: &PcmFile, apart: f64, rise: f32) -> Vec<f64> {
    const HOP: usize = 256;
    let rate = f64::from(pcm.sample_rate().max(1));
    let frames = pcm.frames();
    let channels = usize::from(pcm.channels().max(1));
    let mut out = Vec::new();
    let mut average = 0.0f32;
    let mut last = f64::NEG_INFINITY;
    let mut at = 0usize;
    while at + HOP <= frames {
        let mut peak = 0.0f32;
        for frame in (at..at + HOP).step_by(4) {
            for channel in 0..channels.min(2) {
                peak = peak.max(pcm.sample(frame, channel).abs());
            }
        }
        let seconds = at as f64 / rate;
        if peak > 0.02 && peak > average * rise && seconds - last >= apart {
            out.push(seconds);
            last = seconds;
        }
        // A slow follower, so a steady tick does not raise its own bar.
        average = average.mul_add(0.98, peak * 0.02);
        at += HOP;
    }
    out
}

/// Where a stem first makes a sound worth calling a start.
#[must_use]
pub fn first_sound(pcm: &PcmFile, floor: f32) -> Option<f64> {
    const HOP: usize = 512;
    let rate = f64::from(pcm.sample_rate().max(1));
    let channels = usize::from(pcm.channels().max(1));
    let mut at = 0usize;
    while at + HOP <= pcm.frames() {
        let mut peak = 0.0f32;
        for frame in (at..at + HOP).step_by(8) {
            for channel in 0..channels.min(2) {
                peak = peak.max(pcm.sample(frame, channel).abs());
            }
        }
        if peak > floor {
            return Some(at as f64 / rate);
        }
        at += HOP;
    }
    None
}

/// The tempo a run of clicks keeps, as beats a minute: the middle of the
/// gaps between them, which ignores a missed tick or an extra one.
#[must_use]
pub fn tempo_of(onsets: &[f64]) -> Option<f64> {
    if onsets.len() < 8 {
        return None;
    }
    let mut gaps: Vec<f64> = onsets.windows(2).map(|pair| pair[1] - pair[0]).collect();
    gaps.sort_by(f64::total_cmp);
    let middle = gaps[gaps.len() / 2];
    (middle > 0.05).then(|| 60.0 / middle)
}

/// The tempo to write down, given what the folder's name says and what the
/// click actually does. The click wins; the name settles a click that came
/// out at half or double (a 2/4 record clicked in eighths).
#[must_use]
pub fn settle_tempo(named: Option<f64>, clicked: Option<f64>) -> Option<f64> {
    match (named, clicked) {
        (Some(named), Some(clicked)) => {
            let close = |a: f64, b: f64| (a - b).abs() / b < 0.03;
            if close(named, clicked)
                || !(close(named, clicked * 2.0) || close(named, clicked / 2.0))
            {
                Some(clicked)
            } else {
                // The name is a double or a half of the click: take the
                // name, which is what the chart and the band call it.
                Some(named)
            }
        }
        (named, clicked) => named.or(clicked),
    }
}

/// Snap a time to the nearest bar line at `bpm`.
#[must_use]
pub fn snap_to_bar(seconds: f64, bpm: f64, beats_per_bar: u32) -> f64 {
    if bpm <= 0.0 || beats_per_bar == 0 {
        return seconds;
    }
    let bar = 60.0 / bpm * f64::from(beats_per_bar);
    (seconds / bar).round() * bar
}

/// Read a song folder: what its name says, what its click says, and where
/// its guide's cues fall.
///
/// # Errors
///
/// The folder holds no `.wav` at all, or cannot be read.
pub fn analyse(folder: &Path) -> eyre::Result<(Song, Vec<Stem>)> {
    let stems = stems(folder)?;
    if stems.is_empty() {
        return Err(eyre::eyre!("{}: no stems", folder.display()));
    }
    let name = folder
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (from_name, mut named_bpm, mut sig, mut key) = read_name(&name);
    // Vendors also write it on a folder INSIDE ("4-4 _ 127 BPM _ A Major").
    if named_bpm.is_none() {
        for entry in std::fs::read_dir(folder)?.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let (_, bpm, inner_sig, inner_key) = read_name(&entry.file_name().to_string_lossy());
            if bpm.is_some() {
                named_bpm = bpm;
                sig = sig.or(inner_sig);
                key = key.or(inner_key);
                break;
            }
        }
    }
    let title = title_from_stems(
        &stems
            .iter()
            .filter_map(|s| s.path.file_name().map(|f| f.to_string_lossy().into_owned()))
            .collect::<Vec<_>>(),
    )
    .unwrap_or(from_name);
    let title = tidy_title(&title);
    let time_sig = sig.unwrap_or((4, 4));

    let find = |what: &str| stems.iter().find(|s| role(&s.name) == Some(what));
    let click = find("click").map(|s| PcmFile::open(&s.path)).transpose()?;
    let ticks = click
        .as_ref()
        .map(|pcm| onsets(pcm, 0.08, 2.5))
        .unwrap_or_default();
    let clicked = settle_tempo(named_bpm, tempo_of(&ticks))
        .ok_or_else(|| eyre::eyre!("{title}: no tempo, in the name or in a click"))?;
    let (bpm, time_sig) = musical_tempo(clicked, time_sig);
    let first_beat = ticks.first().copied().unwrap_or(0.0);

    // Where the record comes in, after the count-in: the first sound on a
    // stem that is neither the click nor the guide, snapped to its bar.
    let music = stems
        .iter()
        .filter(|s| role(&s.name).is_none())
        .filter_map(|s| PcmFile::open(&s.path).ok())
        .filter_map(|pcm| first_sound(&pcm, 0.01))
        .fold(f64::INFINITY, f64::min);
    let songstart = if music.is_finite() {
        snap_to_bar((music - first_beat).max(0.0), bpm, time_sig.0)
    } else {
        0.0
    };

    // The guide's cues, which is where the sections change.
    // A cue is a few spoken words, and a section is bars long: take the
    // START of each run of speech, and only where a section could have
    // changed (two bars at the least).
    let bar = 60.0 / bpm * f64::from(time_sig.0);
    let mut sections: Vec<f64> = find("guide")
        .and_then(|s| PcmFile::open(&s.path).ok())
        .map(|pcm| {
            let mut cues: Vec<f64> = Vec::new();
            for at in onsets(&pcm, 0.25, 2.5) {
                let at = snap_to_bar((at - first_beat).max(0.0), bpm, time_sig.0);
                if cues.last().is_none_or(|last: &f64| at - last >= bar * 2.0) {
                    cues.push(at);
                }
            }
            cues
        })
        .unwrap_or_default();
    sections.dedup_by(|a, b| (*a - *b).abs() < f64::EPSILON);

    Ok((
        Song {
            title: if title.trim().is_empty() {
                tidy_title(&name)
            } else {
                title
            },
            bpm,
            time_sig,
            key,
            first_beat,
            songstart,
            sections,
        },
        stems,
    ))
}

/// The session's project, as REAPER reads it: every stem at bar one,
/// trimmed to the click's first tick.
#[must_use]
pub fn project_text(song: &Song, stems: &[Stem]) -> String {
    use dawfile_reaper::builder::ReaperProjectBuilder;

    let mut builder = ReaperProjectBuilder::new()
        .tempo_with_time_sig(
            song.bpm,
            i32::try_from(song.time_sig.0).unwrap_or(4),
            i32::try_from(song.time_sig.1).unwrap_or(4),
        )
        .sample_rate(44_100);
    for stem in stems {
        let length = (stem.seconds - song.first_beat).max(0.0);
        let file = format!("Media/{}.wav", stem.name);
        let (name, offset) = (stem.name.clone(), song.first_beat);
        builder = builder.track(&stem.name, |track| {
            track.item(0.0, length, |item| {
                item.name(name).source_wave(file).slip_offset(offset)
            })
        });
    }
    // Where the record comes in, which is what the chart's count-in runs up
    // to, and the cues the guide called.
    builder = builder.marker(1, song.songstart, "SONGSTART");
    let bar = 60.0 / song.bpm * f64::from(song.time_sig.0);
    let end = stems
        .iter()
        .map(|s| s.seconds - song.first_beat)
        .fold(0.0, f64::max);
    // On the SECTIONS lane, where sections live: they are the importer's
    // guess at the song's sections, and a chart built over the song
    // replaces what is on that lane — so a real chart supersedes the
    // guesses instead of landing beside them. The file numbers lanes from
    // one; `CoreLane` is REAPER's API numbering, from zero.
    let sections_lane =
        i32::try_from(session::ruler_lanes::CoreLane::Sections.lane_index() + 1).unwrap_or(2);
    for (index, start) in song.sections.iter().enumerate() {
        let next = song.sections.get(index + 1).copied().unwrap_or(end);
        builder = builder.add_marker(
            dawfile_reaper::builder::MarkerBuilder::region(
                i32::try_from(index + 2).unwrap_or(2),
                *start,
                next.max(start + bar),
                format!("Cue {}", index + 1),
            )
            .lane(sections_lane)
            .build(),
        );
    }
    use dawfile_reaper::RppSerialize as _;
    builder.build().to_rpp_string()
}

/// What the folder's name said the tempo was, for a note when the click
/// disagrees.
fn named_bpm_of(folder: &Path) -> Option<f64> {
    let name = folder.file_name()?.to_string_lossy().into_owned();
    read_name(&name).1
}

/// A chart to start from: what the folder's name knows, and a section per
/// cue the guide called — named `Cue 1…` for somebody to name properly,
/// with the chords still to write.
#[must_use]
pub fn chart_text(song: &Song) -> String {
    let mut out = format!(
        "{}\n{}bpm {}/{}",
        song.title,
        round(song.bpm),
        song.time_sig.0,
        song.time_sig.1
    );
    if let Some(key) = &song.key {
        out.push_str(&format!(" #{key}"));
    }
    out.push('\n');
    let bar = 60.0 / song.bpm * f64::from(song.time_sig.0);
    if song.songstart > bar / 2.0 {
        out.push_str(&format!(
            "In {}\n",
            ((song.songstart / bar).round() as u32).max(1)
        ));
    }
    for (index, start) in song.sections.iter().enumerate() {
        let next = song
            .sections
            .get(index + 1)
            .copied()
            .unwrap_or(start + bar * 4.0);
        let bars = (((next - start) / bar).round() as u32).max(1);
        out.push_str(&format!("inst \"Cue {}\" {bars}\n", index + 1));
    }
    out
}

fn round(value: f64) -> String {
    if (value - value.round()).abs() < 0.05 {
        format!("{}", value.round() as i64)
    } else {
        format!("{value:.1}")
    }
}

/// Import a multitrack folder as a session under `out`: the stems linked
/// into `Media/` (hard links — nothing is copied), the project on the
/// grid, and a chart to start from.
///
/// # Errors
///
/// The folder cannot be read, or the session cannot be written.
pub fn import(folder: &Path, out: &Path, force: bool) -> eyre::Result<PathBuf> {
    let (song, stems) = analyse(folder)?;
    let session = out.join(&song.title);
    if session.exists() && !force {
        return Err(eyre::eyre!(
            "{} is already there — pass --force to write it again",
            session.display()
        ));
    }
    std::fs::create_dir_all(session.join("Media"))?;
    for stem in &stems {
        let link = session.join("Media").join(format!("{}.wav", stem.name));
        if link.exists() {
            std::fs::remove_file(&link)?;
        }
        // A hard link: the stems stay where they were downloaded, and the
        // session costs nothing to make.
        if std::fs::hard_link(&stem.path, &link).is_err() {
            std::os::unix::fs::symlink(&stem.path, &link)?;
        }
    }
    let project = project_text(&song, &stems);
    // It has to open: a project that does not parse is not an import, and
    // finding that out now costs one parse.
    let parsed = dawfile_reaper::parse_project_text(&project)
        .map_err(|e| eyre::eyre!("{}: the project written does not parse: {e}", song.title))?;
    if parsed.tracks.len() != stems.len() {
        return Err(eyre::eyre!(
            "{}: wrote {} stems, read back {} tracks",
            song.title,
            stems.len(),
            parsed.tracks.len()
        ));
    }
    std::fs::write(session.join(format!("{}.RPP", song.title)), project)?;
    let chart = chart_text(&song);
    // Only a chart that parses is written: a broken one would stop the
    // session opening, and the tempo is already in the project.
    match keyflow::parse(&chart) {
        Ok(_) => std::fs::write(session.join(format!("{}.kf", song.title)), chart)?,
        Err(e) => {
            tracing::warn!(title = song.title, error = %e, "the starting chart did not parse")
        }
    }
    println!(
        "{}  {} bpm {}/{}{}  {} stems  first beat {:.3}s  songstart {:.2}s  {} cues",
        song.title,
        round(song.bpm),
        song.time_sig.0,
        song.time_sig.1,
        song.key
            .as_ref()
            .map(|k| format!(" {k}"))
            .unwrap_or_default(),
        stems.len(),
        song.first_beat,
        song.songstart,
        song.sections.len(),
    );
    if let Some(named) = named_bpm_of(folder)
        && (named - song.bpm).abs() > 0.5
    {
        // The vendor counts the click; the chart counts the song.
        println!(
            "    (its folder counts {named} — the same grid, {} in 4/4)",
            round(song.bpm)
        );
    }
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folders_name_gives_up_its_tempo_key_and_title() {
        let (title, bpm, sig, key) = read_name("AlwaysOnTime-ElevationWorship_F_136_2-4");
        assert_eq!(bpm, Some(136.0));
        assert_eq!(key.as_deref(), Some("F"));
        assert_eq!(sig, Some((2, 4)));
        assert!(title.contains("AlwaysOnTime"), "{title}");

        let (_, bpm, sig, key) = read_name("4-4  _  127 BPM  _  A Major");
        assert_eq!(
            (bpm, sig, key.as_deref()),
            (Some(127.0), Some((4, 4)), Some("A"))
        );

        let (title, bpm, _, key) = read_name("Holy Forever _ Bethel Music _ Bb");
        assert_eq!((bpm, key.as_deref()), (None, Some("Bb")));
        assert!(title.starts_with("Holy Forever"), "{title}");
    }

    #[test]
    fn a_vendors_click_folds_into_the_tempo_a_band_would_say() {
        // 2/4 at 136 is 4/4 at 68 — Always On Time, as its chart has it.
        assert_eq!(musical_tempo(136.0, (2, 4)), (68.0, (4, 4)));
        // A ballad clicked in eighths.
        assert_eq!(musical_tempo(252.1, (4, 4)).0, 126.05);
        // And one that is already what it says.
        assert_eq!(musical_tempo(143.6, (4, 4)), (143.6, (4, 4)));
    }

    #[test]
    fn a_title_reads_as_a_title() {
        assert_eq!(
            tidy_title("AlwaysOnTime ElevationWorship"),
            "Always On Time"
        );
        assert_eq!(tidy_title("Holy Forever Bethel Music"), "Holy Forever");
        assert_eq!(tidy_title("Elevation Worship Praise"), "Praise");
        assert_eq!(tidy_title("Who Else Gateway Worship"), "Who Else");
        assert_eq!(
            tidy_title("God, I_m Just Grateful"),
            "God, I'm Just Grateful"
        );
    }

    #[test]
    fn the_stems_agree_on_the_songs_name() {
        let files = vec![
            "Holy Forever - Click.wav".to_owned(),
            "Holy Forever - AG.wav".to_owned(),
        ];
        assert_eq!(title_from_stems(&files).as_deref(), Some("Holy Forever"));
        assert_eq!(
            title_from_stems(&["01 - Click.wav".to_owned(), "02 - Cue.wav".to_owned()]),
            None,
            "numbered stems name no song"
        );
        assert_eq!(title_from_stems(&["Bass.wav".to_owned()]), None);
    }

    #[test]
    fn a_stems_name_loses_the_song_and_the_number() {
        assert_eq!(stem_name("God, I_m Just Grateful - EG 4.wav"), "EG 4");
        assert_eq!(stem_name("14 - Electric Guitar 1.wav"), "Electric Guitar 1");
        assert_eq!(stem_name("Bass.wav"), "Bass");
        assert_eq!(role("Holy Forever - Click"), Some("click"));
        assert_eq!(role("02 - Cue"), Some("guide"));
        assert_eq!(role("Bass"), None);
    }

    #[test]
    fn the_click_settles_the_tempo_and_the_name_settles_a_double() {
        // A click at 120, named 120: the click.
        assert_eq!(settle_tempo(Some(120.0), Some(119.9)), Some(119.9));
        // Clicked in eighths at 136, named 68: the name.
        assert_eq!(settle_tempo(Some(68.0), Some(136.0)), Some(68.0));
        // Nothing to compare with: whatever there is.
        assert_eq!(settle_tempo(None, Some(94.0)), Some(94.0));
        assert_eq!(settle_tempo(Some(94.0), None), Some(94.0));
    }

    #[test]
    fn a_run_of_ticks_gives_its_tempo() {
        let ticks: Vec<f64> = (0..32).map(|i| 1.5 + f64::from(i) * 0.5).collect();
        let bpm = tempo_of(&ticks).expect("a tempo");
        assert!((bpm - 120.0).abs() < 0.001, "{bpm}");
        assert_eq!(tempo_of(&ticks[..4]), None, "too few to be sure");
    }

    #[test]
    fn a_time_snaps_to_its_bar() {
        // 120 bpm, 4/4: a bar is 2 s.
        assert!((snap_to_bar(4.1, 120.0, 4) - 4.0).abs() < 1e-9);
        assert!((snap_to_bar(5.2, 120.0, 4) - 6.0).abs() < 1e-9);
    }
}
