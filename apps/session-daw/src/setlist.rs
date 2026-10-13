//! The setlist: the songs a session holds, and which of them is up.
//!
//! One engine, several projects (`daw_standalone::sync::Standalone` keeps
//! them side by side, as REAPER keeps tabs), and one of them current. This
//! is what the window reads: the songs in order, where the playhead is in
//! the one playing, and the colour each is known by — the setlist tabs in
//! the top bar are that colour, filling as the song plays
//! ([`crate::shell::SongTabs`]).
//!
//! In **Remote** and **Cue** (`crate::audio_mode`) the songs are the
//! projects the driven system has open — REAPER's project tabs, in tab
//! order, the one it has current current ([`Setlist::attach`]). A song's
//! `project` is then the remote's guid, picking a tab selects that project
//! on the remote (`crate::open::switch_song`), and a tab picked in REAPER
//! itself is followed here (`crate::collab_bar::use_follow_song`). The set
//! is what REAPER has open, not a `.setlist` file: to play a set remotely,
//! open its songs as tabs in REAPER.
//!
//! A song's SPAN is the SONG region the chart stamps (`prepare`), which is
//! the part that is the song rather than the count-in before it; without
//! one it is everything the project holds. The span is what a progress
//! reads against, and what tells Live mode a song has ended.

use crate::studio::StudioSession;

/// The colours offered when a song's colour is set by hand — distinct at a
/// glance in a row of tabs rather than a smooth ramp: the question a
/// setlist tab answers is "which song is that", and two greens apart by a
/// shade answer it slowly.
pub const COLORS: [&str; 8] = [
    "#3aa0ff", "#f0883e", "#4ac26b", "#d2a8ff", "#f778ba", "#e3b341", "#56d4dd", "#ff7b72",
];

/// A song's colour from its title, when nobody has set one: the same title
/// is always the same colour, from one run to the next and on every
/// machine, so a song is recognised by its colour before its name is read.
///
/// The hue comes from a hash of the title (FNV-1a — stable, unlike std's
/// hasher, which is seeded per process); saturation and lightness are
/// fixed where a tab's white title stays legible on the colour.
#[must_use]
pub fn title_color(title: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in title.trim().to_lowercase().bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    #[expect(clippy::cast_precision_loss, reason = "a hue out of 360")]
    let hue = (hash % 360) as f64;
    hsl_hex(hue, 0.62, 0.52)
}

/// `hsl(h, s, l)` as `#rrggbb` — Blitz reads hex everywhere.
fn hsl_hex(hue: f64, saturation: f64, lightness: f64) -> String {
    let chroma = (1.0 - (2.0f64.mul_add(lightness, -1.0)).abs()) * saturation;
    let h = hue / 60.0;
    let x = chroma * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = lightness - chroma / 2.0;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a channel in 0..=255"
    )]
    let byte = |v: f64| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
}

/// One song in the setlist.
#[derive(Clone)]
pub struct Song {
    pub name: String,
    /// The project's guid in the engine — what `set_current_project` takes
    /// — or, in Remote and Cue, on the system this window drives.
    pub project: String,
    pub session: StudioSession,
    /// The song's own span in project seconds: the SONG region, or
    /// everything there is.
    pub span: (f64, f64),
    /// The colour it is known by, as CSS: the one set on its SONG region,
    /// else [`title_color`].
    pub color: String,
    /// Where its playhead was when another song was picked — so its tab
    /// still shows how far through it the set got.
    pub left_at: f64,
    /// Its saved `.session`, which a change made here (its colour) is
    /// written back to. `None` for a song with none.
    pub saved: Option<std::path::PathBuf>,
}

impl PartialEq for Song {
    fn eq(&self, other: &Self) -> bool {
        self.project == other.project && self.name == other.name && self.span == other.span
    }
}

impl Song {
    /// A song from what was opened.
    #[must_use]
    pub fn of(name: String, project: String, session: StudioSession) -> Self {
        let sections = &session.project.sections;
        let song = sections.iter().find(|s| s.lane == 0);
        let span = song.map(|s| (s.start, s.end)).unwrap_or_else(|| {
            let start = sections
                .iter()
                .map(|s| s.start)
                .fold(f64::INFINITY, f64::min);
            let end = sections.iter().map(|s| s.end).fold(0.0, f64::max);
            if end > start {
                (start, end)
            } else {
                (0.0, 0.0)
            }
        });
        let color = song
            .and_then(|s| s.color.clone())
            .unwrap_or_else(|| title_color(&name));
        Self {
            name,
            project,
            session,
            span,
            color,
            left_at: 0.0,
            saved: None,
        }
    }

    /// What the song is shown as: its SONG region's name ("Unbreakable -
    /// Crescendum", the one prepare gives it), else its [`Self::name`] —
    /// the file's, which stays what the song is KNOWN by (collaboration
    /// and the modes it was worked in are kept under it).
    #[must_use]
    pub fn title(&self) -> String {
        self.session
            .project
            .sections
            .iter()
            .find(|s| s.lane == 0)
            .map(|s| s.name.trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| self.name.clone())
    }

    /// How far through the song `at` seconds is, 0 before it and 1 at its
    /// end.
    #[must_use]
    pub fn progress(&self, at: f64) -> f64 {
        let (from, to) = self.span;
        if to <= from {
            return 0.0;
        }
        ((at - from) / (to - from)).clamp(0.0, 1.0)
    }

    /// Whether the playhead has run past the song's end — what Live mode
    /// reads to start the next one.
    #[must_use]
    pub fn ended(&self, at: f64) -> bool {
        self.span.1 > self.span.0 && at >= self.span.1
    }
}

/// The songs, in order, and which one is up.
#[derive(Clone, Default, PartialEq)]
pub struct Setlist {
    pub songs: Vec<Song>,
    /// Which song is current — an index into `songs`, past the end when
    /// the list is empty.
    pub at: usize,
    /// Songs of the set still on their way, by title, in set order — a page
    /// shows its first song while the rest open behind it; each moves into
    /// `songs` as it arrives. Their tabs show them coming.
    pub pending: Vec<String>,
}

impl Setlist {
    #[must_use]
    pub fn of(songs: Vec<Song>) -> Self {
        Self {
            songs,
            at: 0,
            pending: Vec::new(),
        }
    }

    #[must_use]
    pub fn current(&self) -> Option<&Song> {
        self.songs.get(self.at)
    }

    /// The song after the current one, if the set goes on.
    #[must_use]
    pub fn next(&self) -> Option<usize> {
        (self.at + 1 < self.songs.len()).then_some(self.at + 1)
    }

    /// Make `to` the current song, remembering where the one it replaces
    /// had got to (`at`, its playhead). Returns the song now current, or
    /// `None` when `to` is out of range or already current.
    pub fn pick(&mut self, to: usize, at: f64) -> Option<&Song> {
        if to == self.at || to >= self.songs.len() {
            return None;
        }
        if let Some(leaving) = self.songs.get_mut(self.at) {
            leaving.left_at = at;
        }
        self.at = to;
        self.songs.get(to)
    }

    /// How far through `song` the set is: the playhead in the current one,
    /// where it was left in the others.
    #[must_use]
    pub fn progress_of(&self, song: usize, playhead: f64) -> f64 {
        self.songs.get(song).map_or(0.0, |s| {
            s.progress(if song == self.at { playhead } else { s.left_at })
        })
    }

    /// Move `song` to `to`, carrying the current selection with it so the
    /// song playing stays the song playing.
    pub fn reorder(&mut self, song: usize, to: usize) {
        if song >= self.songs.len() || to >= self.songs.len() || song == to {
            return;
        }
        let current = self.songs.get(self.at).cloned();
        let moved = self.songs.remove(song);
        self.songs.insert(to, moved);
        if let Some(current) = current
            && let Some(found) = self.songs.iter().position(|s| *s == current)
        {
            self.at = found;
        }
    }

    /// Drop a song. The selection follows the song that was playing, or
    /// stays in range.
    pub fn remove(&mut self, song: usize) {
        if song >= self.songs.len() {
            return;
        }
        let current = self.songs.get(self.at).cloned();
        self.songs.remove(song);
        self.at = current
            .filter(|c| self.songs.iter().any(|s| s == c))
            .and_then(|c| self.songs.iter().position(|s| *s == c))
            .unwrap_or_else(|| self.at.min(self.songs.len().saturating_sub(1)));
    }
}

#[cfg(feature = "native")]
impl Song {
    /// The song opened from `path` into the engine as `project`: named by
    /// `title`, or its file's; saved into its `.session` folder, when it has
    /// one.
    fn opened(
        path: &std::path::Path,
        title: Option<String>,
        project: String,
        session: StudioSession,
    ) -> Self {
        let name = title.unwrap_or_else(|| {
            path.file_stem()
                .map_or_else(|| path.to_string_lossy(), |s| s.to_string_lossy())
                .into_owned()
        });
        let mut song = Self::of(name, project, session);
        song.saved = Some(if crate::open::is_session(path) {
            path.to_path_buf()
        } else {
            path.with_extension("session")
        })
        .filter(|dir| dir.is_dir());
        song
    }
}

/// A song of the set arriving after the set opened — the songs behind the
/// first open one by one while it is on screen.
pub enum Arrival {
    Song(Song),
    /// A song that could not be opened: its tab goes.
    Failed(String),
}

impl Setlist {
    /// `arrival` into the set: its song in its place, out of the pending.
    pub fn arrive(&mut self, arrival: Arrival) {
        match arrival {
            Arrival::Song(song) => {
                if let Some(at) = self.pending.iter().position(|t| *t == song.name) {
                    self.pending.remove(at);
                }
                self.songs.push(song);
            }
            Arrival::Failed(title) => self.pending.retain(|t| *t != title),
        }
    }
}

/// A song for a set to open: its project — and, when it is streamed in
/// rather than on this disk, the stream its media plays from (the project
/// is then in the stream's mirror) and the title the set gives it.
#[cfg(feature = "native")]
pub struct Opening {
    pub path: std::path::PathBuf,
    pub title: Option<String>,
    pub streamed: Option<crate::song_stream::StreamedSong>,
}

#[cfg(feature = "native")]
impl From<std::path::PathBuf> for Opening {
    fn from(path: std::path::PathBuf) -> Self {
        Self {
            path,
            title: None,
            streamed: None,
        }
    }
}

#[cfg(feature = "native")]
impl Setlist {
    /// Open every song into the one engine — each prepared once and saved
    /// as `.session` if it was not already (see `StudioSession::open`) —
    /// and make the first current, with the audio.
    ///
    /// A song that fails to open is left out, with the reason logged: one
    /// broken song folder should not take the rest of the set with it.
    ///
    /// # Errors
    ///
    /// None of the songs opened.
    pub fn open(paths: &[std::path::PathBuf]) -> eyre::Result<Self> {
        Self::open_with(paths.iter().cloned().map(Opening::from).collect(), true)
    }

    /// [`Self::open`], for songs some of which are streamed in rather than
    /// on this disk (`crate::stream_set`): each opens the same way, its
    /// media deferred and then attached to its streams.
    ///
    /// # Errors
    ///
    /// As [`Self::open`].
    pub fn open_streamed(songs: Vec<Opening>) -> eyre::Result<Self> {
        Self::open_with(songs, true)
    }

    /// Open one more song into the engine the set is in, behind the one on
    /// screen — it does not become current, and nothing on screen moves: a
    /// streamed set's songs after the first, arriving (see [`Arrival`]).
    ///
    /// # Errors
    ///
    /// The song did not open, or could not be read back.
    pub fn open_behind(opening: Opening) -> eyre::Result<Song> {
        let Opening {
            path,
            title,
            streamed,
        } = opening;
        let engine = crate::open::local_engine()
            .ok_or_else(|| eyre::eyre!("this window has no engine of its own to open into"))?;
        let media = if streamed.is_some() {
            crate::open::Media::Deferred
        } else {
            crate::open::Media::All
        };
        let prepare = crate::prepare::Prepare::for_song(&path);
        let (opened, plan) = crate::open::open_song_into_with(engine, &path, &prepare, media)?;
        let guid = opened.project_guid.clone();
        if let Some(streamed) = streamed {
            crate::stream_set::stream(streamed, &guid);
        } else if let Some(folder) = path.parent() {
            // A song downloaded by its reference plays it from disk.
            crate::reference_play::hear_local(&guid, folder);
        }
        let rpp_text = crate::open::project_text(&plan.open)?.text;
        let chart_text = prepare
            .chart
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok());
        let mut session = crate::open::engine_runtime()?.block_on(StudioSession::read_project(
            &guid,
            &opened.name,
            &rpp_text,
            chart_text.as_deref(),
            true,
        ))?;
        session.chart_file = prepare.chart.clone();
        crate::collab::song_opened(&guid);
        Ok(Song::opened(&path, title, guid, session))
    }

    /// [`Self::open`], with no audio device — for a test, or anything that
    /// looks at a set without playing it.
    ///
    /// # Errors
    ///
    /// As [`Self::open`].
    pub fn open_silent(paths: &[std::path::PathBuf]) -> eyre::Result<Self> {
        Self::open_with(paths.iter().cloned().map(Opening::from).collect(), false)
    }

    fn open_with(openings: Vec<Opening>, audio: bool) -> eyre::Result<Self> {
        let count = openings.len();
        let mut songs = Vec::new();
        for Opening {
            path,
            title,
            streamed,
        } in openings
        {
            let path = &path;
            let prepare = crate::prepare::Prepare::for_song(path);
            let media = if streamed.is_some() {
                crate::open::Media::Deferred
            } else {
                crate::open::Media::All
            };
            match StudioSession::open_first_as(
                path,
                &prepare,
                songs.is_empty().then_some(audio),
                media,
            ) {
                Ok((session, guid)) => {
                    if let Some(streamed) = streamed {
                        crate::stream_set::stream(streamed, &guid);
                    } else if let Some(folder) = path.parent() {
                        // A song downloaded by its reference plays it from disk.
                        crate::reference_play::hear_local(&guid, folder);
                    }
                    songs.push(Song::opened(path, title, guid, session));
                }
                Err(e) => tracing::error!(
                    setlist.song = %path.display(),
                    error = %e,
                    "a song did not open; the set goes on without it"
                ),
            }
        }
        let first = songs
            .first()
            .ok_or_else(|| eyre::eyre!("none of the {count} songs opened"))?;
        crate::open::switch_song(&first.project);
        Ok(Self::of(songs))
    }

    /// The set a Remote or Cue window drives: attach to `target`, and make
    /// a song of every project it has open, in its order, its current one
    /// current.
    ///
    /// Each project is read the way an opened song is
    /// (`StudioSession::read_current`) — which reads the CURRENT project, so
    /// the remote's projects are selected one at a time while they are read
    /// and the one that was current is selected again at the end. A project
    /// saved on this machine gets its track kinds and chart from beside it;
    /// one that is not (another machine's — any reached over the network —
    /// or never saved) opens without.
    ///
    /// # Errors
    ///
    /// The attach failed, the remote's projects could not be listed, or
    /// none could be read.
    pub fn attach(target: &crate::open::RemoteTarget) -> eyre::Result<Self> {
        let attached = crate::open::attach(target)?;
        // A project's path means this machine's disk only when the system
        // driven is on it: a local REAPER over its socket. A Session engine
        // or a REAPER's bridge is reached over the network, so its paths
        // are another machine's — read here they are at best missing and at
        // worst a folder the OS guards (a phone simulator on a Mac stalls on
        // a privacy prompt for its owner's Downloads).
        let local = matches!(target, crate::open::RemoteTarget::Reaper { .. });
        let listed = crate::open::facade_blocking("list projects", |daw| async move {
            let mut out = Vec::new();
            for project in daw.projects().await? {
                let info = project.info().await?;
                out.push(RemoteProject {
                    guid: info.guid,
                    name: info.name,
                    path: Some(std::path::PathBuf::from(info.path))
                        .filter(|p| local && !p.as_os_str().is_empty()),
                });
            }
            Ok(out)
        })?;
        let (projects, _) = remote_songs(listed, &attached.project_guid);
        let mut selected = attached.project_guid.clone();
        let mut songs = Vec::new();
        for project in projects {
            if project.guid != selected {
                let guid = project.guid.clone();
                if let Err(e) = crate::open::facade_blocking("select project", |daw| async move {
                    daw.select_project(guid).await.map(|_| ())
                }) {
                    tracing::error!(error = %e, "a remote project could not be selected to read; the set goes on without it");
                    continue;
                }
                selected.clone_from(&project.guid);
            }
            let chart = project
                .path
                .as_deref()
                .and_then(crate::prepare::chart_beside);
            match StudioSession::read_current(project.path.as_deref(), chart) {
                Ok(session) => songs.push(Song::of(project.song_name(), project.guid, session)),
                Err(e) => {
                    tracing::error!(error = %e, "a remote project could not be read; the set goes on without it")
                }
            }
        }
        if selected != attached.project_guid {
            let guid = attached.project_guid.clone();
            if let Err(e) = crate::open::facade_blocking("select project", |daw| async move {
                daw.select_project(guid).await.map(|_| ())
            }) {
                tracing::warn!(error = %e, "the remote's current project could not be selected again");
            }
        }
        if songs.is_empty() {
            eyre::bail!("none of the remote's projects could be read");
        }
        let at = songs
            .iter()
            .position(|s| s.project == attached.project_guid)
            .unwrap_or(0);
        crate::open::switch_song(&songs[at].project);
        Ok(Self {
            songs,
            at,
            pending: Vec::new(),
        })
    }

    /// Give `index` a colour by hand — or, with `None`, give it back the
    /// colour its title makes. Set on its SONG region, where REAPER shows
    /// it too, and saved into its `.session`, so it is the song's colour
    /// from now on and not this window's.
    pub fn recolor(&mut self, index: usize, color: Option<String>) {
        let Some(song) = self.songs.get_mut(index) else {
            return;
        };
        let rgb = color.as_deref().and_then(hex_rgb);
        song.color = color
            .filter(|_| rgb.is_some())
            .unwrap_or_else(|| title_color(&song.name));
        crate::open::set_song_color(&song.project, rgb.unwrap_or(0), song.saved.as_deref());
    }
}

/// A project the driven system has open, as a Remote setlist sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteProject {
    pub guid: String,
    /// The remote's name for it (REAPER's: the file name, or empty).
    pub name: String,
    /// Where it is saved, when it is.
    pub path: Option<std::path::PathBuf>,
}

impl RemoteProject {
    /// The song's name: the file's stem (`Washed`, the name every machine
    /// knows the song by — `crate::collab` keys songs by it), else the
    /// remote's own name without an extension, else `Untitled`.
    #[must_use]
    pub fn song_name(&self) -> String {
        let stem = |p: &std::path::Path| p.file_stem().map(|s| s.to_string_lossy().into_owned());
        self.path
            .as_deref()
            .and_then(stem)
            .or_else(|| stem(std::path::Path::new(self.name.trim())))
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "Untitled".to_owned())
    }
}

/// The remote's projects as a set: all of them, in the remote's order
/// (REAPER's tabs), and the index of `current` — `0` when it is not among
/// them. Every tab is a song, saved or not: an empty tab on stage is
/// still what REAPER will play if it is picked.
#[must_use]
pub fn remote_songs(projects: Vec<RemoteProject>, current: &str) -> (Vec<RemoteProject>, usize) {
    let at = projects.iter().position(|p| p.guid == current).unwrap_or(0);
    (projects, at)
}

/// `#rrggbb` as `0xRRGGBB`.
#[cfg(feature = "native")]
fn hex_rgb(css: &str) -> Option<u32> {
    let hex = css.strip_prefix('#')?;
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

/// The songs a setlist names, in order.
///
/// A **folder** is a set of song folders (`Set/Song/Song.RPP`, …), taken
/// in name order; a **file** lists one song per line — a `.session`, a
/// `.RPP` or a song's folder — relative to the file, with blank lines and
/// `#` comments skipped. A song folder's `.session` is preferred to its
/// `.RPP`, which is what opening an `.RPP` does anyway.
///
/// # Errors
///
/// The folder or file could not be read, or it names no songs.
#[cfg(feature = "native")]
pub fn read_setlist(path: &std::path::Path) -> eyre::Result<Vec<std::path::PathBuf>> {
    let songs: Vec<std::path::PathBuf> = if path.is_dir() {
        let mut dirs: Vec<std::path::PathBuf> = std::fs::read_dir(path)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|p| p.is_dir() && !crate::open::is_session(p))
            .collect();
        dirs.sort();
        dirs.into_iter().filter_map(|dir| song_in(&dir)).collect()
    } else {
        let base = path.parent().unwrap_or_else(|| std::path::Path::new("."));
        std::fs::read_to_string(path)?
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .filter_map(|line| {
                let entry = base.join(line);
                if entry.is_dir() && !crate::open::is_session(&entry) {
                    song_in(&entry)
                } else {
                    Some(entry)
                }
            })
            .collect()
    };
    if songs.is_empty() {
        eyre::bail!("{} names no songs", path.display());
    }
    Ok(songs)
}

/// The song in a song folder: its `.session`, else its one `.RPP`. `None`
/// for a folder that is not a song's (a setlist's folder of songs).
#[cfg(feature = "native")]
pub fn song_in(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let entries: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    let with = |ext: &str| {
        entries
            .iter()
            .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)))
            .cloned()
    };
    with("session").or_else(|| with("rpp"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(name: &str) -> Song {
        Song {
            name: name.to_owned(),
            project: format!("{name}-guid"),
            session: StudioSession {
                project: daw_ui::studio::ProjectRef(std::sync::Arc::default()),
                rows: daw_ui::studio::RowsRef(std::sync::Arc::default()),
                previews: crate::midi::Previews::default(),
                chart: None,
                chart_file: None,
                planner: crate::studio::Planner {
                    raw: std::sync::Arc::default(),
                    scene: None,
                    kinds: std::sync::Arc::default(),
                },
            },
            span: (4.0, 24.0),
            color: "#fff".to_owned(),
            left_at: 0.0,
            saved: None,
        }
    }

    /// A song is shown by its SONG region's name, and known by its file's.
    #[test]
    fn a_song_is_shown_by_its_song_region() {
        let mut named = song("unbreakable.current");
        assert_eq!(
            named.title(),
            "unbreakable.current",
            "no SONG region: its name"
        );
        let mut project = daw_ui::studio::project::Project::default();
        project.sections.push(daw_ui::studio::project::Section {
            id: 0,
            start: 0.0,
            end: 8.0,
            name: "Unbreakable - Crescendum".into(),
            color: None,
            lane: 0,
        });
        project.sections.push(daw_ui::studio::project::Section {
            id: 1,
            start: 0.0,
            end: 8.0,
            name: "IN".into(),
            color: None,
            lane: 1,
        });
        named.session.project = daw_ui::studio::ProjectRef(std::sync::Arc::new(project));
        assert_eq!(named.title(), "Unbreakable - Crescendum");
        assert_eq!(named.name, "unbreakable.current", "still known by its file");
    }

    #[test]
    fn a_progress_runs_over_the_songs_own_span() {
        let song = song("one");
        assert_eq!(song.progress(0.0), 0.0, "the count-in is before the song");
        assert_eq!(song.progress(14.0), 0.5);
        assert_eq!(song.progress(99.0), 1.0);
        assert!(!song.ended(23.9) && song.ended(24.0));
    }

    #[test]
    fn reordering_keeps_the_song_that_is_playing_current() {
        let mut setlist = Setlist::of(vec![song("one"), song("two"), song("three")]);
        setlist.at = 2;
        setlist.reorder(2, 0);
        assert_eq!(
            setlist.current().map(|s| s.name.clone()),
            Some("three".into())
        );
        assert_eq!(setlist.at, 0);
    }

    #[test]
    fn removing_another_song_leaves_this_one_playing() {
        let mut setlist = Setlist::of(vec![song("one"), song("two"), song("three")]);
        setlist.at = 2;
        setlist.remove(0);
        assert_eq!(
            setlist.current().map(|s| s.name.clone()),
            Some("three".into())
        );
        setlist.remove(setlist.at);
        assert_eq!(
            setlist.current().map(|s| s.name.clone()),
            Some("two".into())
        );
    }

    #[test]
    fn a_title_always_gets_the_same_colour_and_titles_differ() {
        assert_eq!(title_color("Praise"), title_color("Praise"));
        assert_eq!(
            title_color(" praise "),
            title_color("Praise"),
            "case and edges do not count"
        );
        assert_ne!(title_color("Praise"), title_color("Washed"));
        assert!(title_color("Who Else").starts_with('#') && title_color("Who Else").len() == 7);
    }

    #[test]
    fn picking_a_song_remembers_where_the_last_one_got_to() {
        let mut setlist = Setlist::of(vec![song("one"), song("two")]);
        assert!(setlist.pick(1, 14.0).is_some());
        assert_eq!(setlist.at, 1);
        assert_eq!(
            setlist.progress_of(0, 99.0),
            0.5,
            "left halfway, whatever is playing now"
        );
        assert_eq!(
            setlist.progress_of(1, 4.0),
            0.0,
            "the current one reads the playhead"
        );
        assert!(setlist.pick(1, 0.0).is_none(), "already current");
    }

    #[cfg(feature = "native")]
    #[test]
    fn a_remote_set_is_the_remotes_tabs_in_order_its_current_current() {
        let project = |guid: &str, name: &str, path: Option<&str>| RemoteProject {
            guid: guid.into(),
            name: name.into(),
            path: path.map(Into::into),
        };
        let (songs, at) = remote_songs(
            vec![
                project("a", "Washed.RPP", Some("/set/Washed/Washed.RPP")),
                project("b", "", None),
                project("c", "Who Else.rpp", None),
            ],
            "c",
        );
        assert_eq!(
            songs.iter().map(|p| p.guid.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        assert_eq!(at, 2);
        assert_eq!(songs[0].song_name(), "Washed", "the file's stem");
        assert_eq!(songs[1].song_name(), "Untitled", "a tab never saved");
        assert_eq!(
            songs[2].song_name(),
            "Who Else",
            "the remote's name, less its extension"
        );
        assert_eq!(
            remote_songs(songs, "gone").1,
            0,
            "a current not in the list starts at the top"
        );
    }

    #[cfg(feature = "native")]
    #[test]
    fn a_setlist_folder_is_its_song_folders_in_order_sessions_first() {
        let dir = tempfile::tempdir().expect("tempdir");
        for (song, files) in [
            ("B Song", &["B Song.RPP"][..]),
            ("A Song", &["A Song.RPP", "A Song.session/"][..]),
        ] {
            let folder = dir.path().join(song);
            std::fs::create_dir_all(&folder).unwrap();
            for file in files {
                if let Some(d) = file.strip_suffix('/') {
                    std::fs::create_dir_all(folder.join(d)).unwrap();
                } else {
                    std::fs::write(folder.join(file), "").unwrap();
                }
            }
        }
        let songs = read_setlist(dir.path()).expect("songs");
        assert_eq!(
            songs,
            vec![
                dir.path().join("A Song/A Song.session"),
                dir.path().join("B Song/B Song.RPP"),
            ]
        );
        let list = dir.path().join("set.setlist");
        std::fs::write(&list, "# tonight\nB Song\n\nA Song/A Song.RPP\n").unwrap();
        assert_eq!(
            read_setlist(&list).expect("songs"),
            vec![
                dir.path().join("B Song/B Song.RPP"),
                dir.path().join("A Song/A Song.RPP"),
            ]
        );
    }
}
