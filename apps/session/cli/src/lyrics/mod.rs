//! `session lyrics fetch` — line-synced lyrics for a song, written as
//! `<Song>.lrc` beside its `.RPP`.
//!
//! What to look for comes from the song's chart (`<Song>.kf` beside the
//! project): its first line is `Title - Artist` or `Title - Artist1,
//! Artist2`, and its sections give the song's own length, SONGSTART to
//! SONGEND. Every provider holds several versions of a popular song — the
//! album cut, the live cut, a radio edit — and the one whose length is
//! closest to ours is the one our arrangement was built on.
//!
//! ```text
//! session lyrics fetch Praise/Praise.RPP             # search, pick by length, write Praise.lrc
//! session lyrics fetch */*.RPP                       # a whole setlist
//! session lyrics fetch Praise.RPP --pick 2           # take the second listed candidate instead
//! session lyrics fetch Praise.RPP --lrclib-id 123    # this exact LRCLIB record
//! session lyrics fetch X.RPP --title T --artist A    # when the chart is missing or wrong
//! ```

mod lrclib;
mod provider;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use tracing::field::Empty;

use provider::{Candidate, LyricsProvider, Query};

/// Two versions whose lengths are this close are the same length, for the
/// purpose of preferring the studio cut over a live one.
const TIE_SECONDS: f64 = 3.0;

/// `session lyrics fetch`'s options.
pub struct FetchArgs {
    pub paths: Vec<PathBuf>,
    pub title: Option<String>,
    pub artists: Vec<String>,
    pub lrclib_id: Option<String>,
    pub pick: Option<usize>,
    pub force: bool,
}

/// Fetch and write lyrics for every song named.
///
/// # Errors
///
/// When any song ended without a `.lrc` (one already there does not count),
/// after trying every song.
pub fn fetch(args: &FetchArgs) -> eyre::Result<()> {
    if args.paths.is_empty() {
        eyre::bail!("name at least one song's .RPP");
    }
    let per_song = args.title.is_some()
        || !args.artists.is_empty()
        || args.lrclib_id.is_some()
        || args.pick.is_some();
    if per_song && args.paths.len() > 1 {
        eyre::bail!(
            "--title, --artist, --lrclib-id and --pick describe one song — name only one .RPP"
        );
    }

    let provider = lrclib::Lrclib::new();
    let mut failed = Vec::new();
    for path in &args.paths {
        match fetch_one(&provider, path, args) {
            Ok(()) => {}
            Err(e) => {
                println!("  ✗ {e}");
                failed.push(path.display().to_string());
            }
        }
        println!();
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(eyre::eyre!(
            "no lyrics written for {} song(s): {}",
            failed.len(),
            failed.join(", ")
        ))
    }
}

/// One song, start to finish. The span is the wide event: one per song,
/// every decision recorded on it.
fn fetch_one(provider: &dyn LyricsProvider, path: &Path, args: &FetchArgs) -> eyre::Result<()> {
    let span = tracing::info_span!(
        "lyrics.fetch",
        lyrics.provider = provider.name(),
        lyrics.title = Empty,
        lyrics.artists = Empty,
        lyrics.target_seconds = Empty,
        lyrics.candidates = Empty,
        lyrics.by_title_only = Empty,
        lyrics.picked_id = Empty,
        lyrics.picked_by = Empty,
        lyrics.picked_seconds = Empty,
        lyrics.lines = Empty,
        lyrics.outcome = Empty,
    );
    let _entered = span.enter();

    let result = run_one(provider, path, args, &span);
    match &result {
        Ok(outcome) => {
            span.record("lyrics.outcome", *outcome);
        }
        Err(e) => {
            span.record("lyrics.outcome", "error");
            tracing::warn!(error = %e, "lyrics fetch failed");
        }
    }
    result.map(|_| ())
}

fn run_one(
    provider: &dyn LyricsProvider,
    path: &Path,
    args: &FetchArgs,
    span: &tracing::Span,
) -> eyre::Result<&'static str> {
    let rpp = crate::proxies::project_file(path)?;
    println!("{}", rpp.display());
    let out = rpp.with_extension("lrc");
    if out.exists() && !args.force {
        println!(
            "  {} is already there — --force to replace it",
            file_name(&out)
        );
        return Ok("exists");
    }

    let song = SongInfo::for_project(&rpp, args)?;
    span.record("lyrics.title", song.title.as_str());
    span.record("lyrics.artists", song.artists.join(", ").as_str());
    if let Some(length) = song.length {
        span.record("lyrics.target_seconds", length);
    }
    println!(
        "  looking for \"{}\"{} — {}",
        song.title,
        if song.artists.is_empty() {
            String::new()
        } else {
            format!(" by {}", song.artists.join(" / "))
        },
        song.length.map_or_else(
            || "length unknown (no chart)".to_string(),
            |l| format!("{} long, SONGSTART to SONGEND", clock(l))
        ),
    );

    let (listed, chosen, picked_by) = if let Some(id) = &args.lrclib_id {
        let candidate = provider.get(id)?;
        (vec![candidate], 0, "id")
    } else {
        let gathered = gather(provider, &song.title, &song.artists)?;
        span.record("lyrics.by_title_only", gathered.by_title_only);
        if gathered.by_title_only {
            println!("  nothing synced under the artist credit — searched by title alone");
        }
        let relevant = relevant(
            gathered.candidates,
            &song.title,
            &song.artists,
            gathered.by_title_only,
        );
        let (ranked, best) = rank(relevant, song.length);
        match args.pick {
            Some(n) => {
                let index = n
                    .checked_sub(1)
                    .filter(|i| *i < ranked.len())
                    .ok_or_else(|| {
                        eyre::eyre!(
                            "--pick {n}: there are {} candidates, numbered from 1",
                            ranked.len()
                        )
                    })?;
                (ranked, index, "pick")
            }
            None => (ranked, best, "length"),
        }
    };
    span.record("lyrics.candidates", listed.len());
    if listed.is_empty() {
        eyre::bail!(
            "{} has no synced lyrics for \"{}\"",
            provider.name(),
            song.title
        );
    }
    print!("{}", candidate_table(&listed, chosen, song.length));

    let picked = listed
        .get(chosen)
        .ok_or_else(|| eyre::eyre!("no candidate {chosen}"))?;
    span.record("lyrics.picked_id", picked.id.as_str());
    span.record("lyrics.picked_by", picked_by);
    if let Some(d) = picked.duration {
        span.record("lyrics.picked_seconds", d);
    }
    if !picked.is_synced() {
        eyre::bail!(
            "{} {} has no line-synced lyrics",
            picked.provider,
            picked.id
        );
    }

    let text = render_lrc(picked);
    let lines = keyflow::lrc::parse(&text).lines.len();
    span.record("lyrics.lines", lines);
    if lines == 0 {
        eyre::bail!(
            "{} {}'s lyrics parse to no timed lines",
            picked.provider,
            picked.id
        );
    }
    std::fs::write(&out, text).map_err(|e| eyre::eyre!("writing {}: {e}", out.display()))?;
    println!(
        "  wrote {} — {} {}, {lines} lines",
        file_name(&out),
        picked.provider,
        picked.id
    );
    Ok("written")
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

// ── What we are looking for ─────────────────────────────────────────────

/// The song as its chart describes it.
#[derive(Debug, Clone, PartialEq)]
struct SongInfo {
    title: String,
    /// Every artist credited; each is searched in turn.
    artists: Vec<String>,
    /// SONGSTART to SONGEND, in seconds.
    length: Option<f64>,
}

impl SongInfo {
    /// From the chart beside `rpp`, with the command line's overrides on top.
    /// With no chart and no `--title`, the project's own name is the title.
    fn for_project(rpp: &Path, args: &FetchArgs) -> eyre::Result<Self> {
        let chart = find_chart(rpp)
            .map(|p| {
                std::fs::read_to_string(&p).map_err(|e| eyre::eyre!("reading {}: {e}", p.display()))
            })
            .transpose()?;
        let mut song = chart.as_deref().map_or_else(
            || Self {
                title: rpp
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                artists: Vec::new(),
                length: None,
            },
            Self::from_chart,
        );
        if let Some(title) = &args.title {
            song.title.clone_from(title);
        }
        if !args.artists.is_empty() {
            song.artists.clone_from(&args.artists);
        }
        if song.title.trim().is_empty() {
            eyre::bail!("no title to search for — pass --title");
        }
        Ok(song)
    }

    fn from_chart(text: &str) -> Self {
        let first = text
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or_default();
        let (title, artists) = chart_header(first);
        Self {
            title,
            artists,
            length: song_length(text),
        }
    }
}

/// `Title - Artist1, Artist2` → the title and each artist.
fn chart_header(line: &str) -> (String, Vec<String>) {
    let Some((title, credit)) = line.split_once(" - ") else {
        return (line.trim().to_string(), Vec::new());
    };
    let artists = credit
        .split(',')
        .map(str::trim)
        .filter(|a| !a.is_empty())
        .map(str::to_string)
        .collect();
    (title.trim().to_string(), artists)
}

/// SONGSTART to SONGEND of a chart, in seconds.
fn song_length(chart: &str) -> Option<f64> {
    let layout = session::setlist::chart_import::chart_to_layout(chart).ok()?;
    let length = layout.song_end_seconds - layout.song_start_seconds;
    (length > 0.0).then_some(length)
}

/// The chart beside a project: `<Song>.kf`, else one whose name differs
/// only in case and `_`-for-space (`Always_on_Time.kf`), else the folder's
/// only chart.
fn find_chart(rpp: &Path) -> Option<PathBuf> {
    let exact = rpp.with_extension("kf");
    if exact.is_file() {
        return Some(exact);
    }
    let stem = loose_name(&rpp.file_stem()?.to_string_lossy());
    let charts: Vec<PathBuf> = std::fs::read_dir(rpp.parent()?)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("kf")))
        .collect();
    if let Some(same) = charts.iter().find(|p| {
        p.file_stem()
            .is_some_and(|s| loose_name(&s.to_string_lossy()) == stem)
    }) {
        return Some(same.clone());
    }
    match charts.as_slice() {
        [only] => Some(only.clone()),
        _ => None,
    }
}

fn loose_name(name: &str) -> String {
    name.to_lowercase().replace('_', " ")
}

// ── Finding and choosing a version ──────────────────────────────────────

struct Gathered {
    /// Synced candidates only, each once.
    candidates: Vec<Candidate>,
    /// The artist searches found nothing synced, so these came from a
    /// search by title alone.
    by_title_only: bool,
}

/// Search under each credited artist; if none of that turns up synced
/// lyrics, search by title alone. ("Always On Time" is credited "Pat
/// Barrett, Elevation Worship" and has nothing under Pat Barrett.)
fn gather(
    provider: &dyn LyricsProvider,
    title: &str,
    artists: &[String],
) -> eyre::Result<Gathered> {
    let mut candidates = Vec::new();
    for artist in artists {
        let found = provider.search(&Query {
            title,
            artist: Some(artist),
        })?;
        add_synced(&mut candidates, found);
    }
    let by_title_only = candidates.is_empty();
    if by_title_only {
        add_synced(
            &mut candidates,
            provider.search(&Query {
                title,
                artist: None,
            })?,
        );
    }
    Ok(Gathered {
        candidates,
        by_title_only,
    })
}

fn add_synced(into: &mut Vec<Candidate>, found: Vec<Candidate>) {
    for c in found {
        if c.is_synced()
            && !into
                .iter()
                .any(|seen| seen.provider == c.provider && seen.id == c.id)
        {
            into.push(c);
        }
    }
}

/// Drop what is plainly another song: keep titles that are ours once
/// "- Live" / "(feat. …)" is set aside, else titles that contain ours,
/// else everything. A title-only search also has to match an artist we
/// credit, when any result does.
fn relevant(
    candidates: Vec<Candidate>,
    title: &str,
    artists: &[String],
    by_title_only: bool,
) -> Vec<Candidate> {
    let want = core_title(title);
    let exact: Vec<Candidate> = candidates
        .iter()
        .filter(|c| core_title(&c.track) == want)
        .cloned()
        .collect();
    let mut kept = if exact.is_empty() {
        let near: Vec<Candidate> = candidates
            .iter()
            .filter(|c| {
                let got = core_title(&c.track);
                got.contains(&want) || want.contains(&got)
            })
            .cloned()
            .collect();
        if near.is_empty() { candidates } else { near }
    } else {
        exact
    };
    if by_title_only && !artists.is_empty() {
        let wanted: Vec<String> = artists.iter().map(|a| normalise(a)).collect();
        let credited: Vec<Candidate> = kept
            .iter()
            .filter(|c| {
                let got = normalise(&c.artist);
                wanted.iter().any(|w| got.contains(w.as_str()))
            })
            .cloned()
            .collect();
        if !credited.is_empty() {
            kept = credited;
        }
    }
    kept
}

/// A title with its version dressing removed: "Who Else - Live" and
/// "Who Else (Live)" are both "who else".
fn core_title(title: &str) -> String {
    let cut = title.split(['(', '[']).next().unwrap_or_default();
    let cut = cut.split(" - ").next().unwrap_or_default();
    normalise(cut)
}

/// Lowercase letters and digits, single-spaced: "God, I'm Just Grateful"
/// → "god im just grateful".
fn normalise(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// How far a candidate's length is from ours; unknown sorts last.
fn distance(candidate: &Candidate, target: Option<f64>) -> f64 {
    match (candidate.duration, target) {
        (Some(d), Some(t)) => (d - t).abs(),
        (Some(_), None) => 0.0,
        (None, _) => f64::INFINITY,
    }
}

/// Order candidates by how close their length is to ours, and say which
/// one to take: the closest, unless it is live and a studio version is
/// within [`TIE_SECONDS`] of it.
fn rank(mut candidates: Vec<Candidate>, target: Option<f64>) -> (Vec<Candidate>, usize) {
    candidates.sort_by(|a, b| {
        distance(a, target)
            .total_cmp(&distance(b, target))
            .then_with(|| a.is_live().cmp(&b.is_live()))
    });
    let chosen = candidates.first().map_or(0, |best| {
        if !best.is_live() {
            return 0;
        }
        let limit = distance(best, target) + TIE_SECONDS;
        candidates
            .iter()
            .position(|c| !c.is_live() && distance(c, target) <= limit)
            .unwrap_or(0)
    });
    (candidates, chosen)
}

fn candidate_table(listed: &[Candidate], chosen: usize, target: Option<f64>) -> String {
    let mut out = String::new();
    for (i, c) in listed.iter().enumerate() {
        let delta = match (c.duration, target) {
            (Some(d), Some(t)) => format!("{:+.0}s", d - t),
            _ => "?".to_string(),
        };
        let _ = writeln!(
            out,
            "  {} {:>2}. {} {:<9} {:>6} {:>5}  {} — {}{}",
            if i == chosen { "→" } else { " " },
            i.saturating_add(1),
            c.provider,
            c.id,
            c.duration.map_or_else(|| "?".to_string(), clock),
            delta,
            c.track,
            c.artist,
            c.album
                .as_deref()
                .map_or_else(String::new, |a| format!(" [{a}]")),
        );
    }
    out
}

/// `m:ss`, for people.
fn clock(seconds: f64) -> String {
    let total = seconds.max(0.0).round();
    let minutes = (total / 60.0).floor();
    format!("{minutes:.0}:{:02.0}", total - minutes * 60.0)
}

/// `mm:ss.xx`, as LRC writes a length.
fn lrc_length(seconds: f64) -> String {
    let hundredths = (seconds.max(0.0) * 100.0).round() / 100.0;
    let minutes = (hundredths / 60.0).floor();
    format!("{minutes:02.0}:{:05.2}", hundredths - minutes * 60.0)
}

// ── Writing ─────────────────────────────────────────────────────────────

/// The `.lrc` for a candidate: `[ti:]`, `[ar:]`, `[al:]`, `[length:]`, a
/// comment naming where it came from, then the provider's lines verbatim.
fn render_lrc(c: &Candidate) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "[ti:{}]", tag_value(&c.track));
    let _ = writeln!(out, "[ar:{}]", tag_value(&c.artist));
    if let Some(album) = &c.album {
        let _ = writeln!(out, "[al:{}]", tag_value(album));
    }
    if let Some(d) = c.duration {
        let _ = writeln!(out, "[length:{}]", lrc_length(d));
    }
    let _ = writeln!(out, "[#:source {} id {}]", c.provider, c.id);
    out.push_str(c.synced.as_deref().unwrap_or_default().trim_end());
    out.push('\n');
    out
}

/// A header value cannot hold the `]` that would close its tag, or a line break.
fn tag_value(value: &str) -> String {
    value
        .replace(']', ")")
        .replace(['\n', '\r'], " ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    /// Canned LRCLIB `/api/search` bodies, keyed by the query they answer.
    struct Canned {
        answers: Vec<(Option<&'static str>, &'static str)>,
        asked: RefCell<Vec<Option<String>>>,
    }

    impl LyricsProvider for Canned {
        fn name(&self) -> &'static str {
            "lrclib"
        }
        fn search(&self, query: &Query<'_>) -> eyre::Result<Vec<Candidate>> {
            self.asked
                .borrow_mut()
                .push(query.artist.map(str::to_string));
            let body = self
                .answers
                .iter()
                .find(|(artist, _)| *artist == query.artist)
                .map_or("[]", |(_, body)| body);
            lrclib::parse_search(body)
        }
        fn get(&self, id: &str) -> eyre::Result<Candidate> {
            Err(eyre::eyre!("no {id}"))
        }
    }

    const PRAISE: &str = r#"[
      {"id": 1, "trackName": "Praise", "artistName": "Elevation Worship", "albumName": "Can You Imagine?",
       "duration": 308.0, "instrumental": false, "plainLyrics": "x", "syncedLyrics": "[00:10.00] Let everything\n[00:12.00] that has breath"},
      {"id": 2, "trackName": "Praise (feat. Brandon Lake)", "artistName": "Elevation Worship", "albumName": "Praise - Single",
       "duration": 239.0, "instrumental": false, "plainLyrics": "x", "syncedLyrics": "[00:05.00] Let everything"},
      {"id": 3, "trackName": "Praise", "artistName": "Elevation Worship", "albumName": null,
       "duration": 240.0, "instrumental": false, "plainLyrics": "x", "syncedLyrics": null},
      {"id": 4, "trackName": "Praise You Anywhere", "artistName": "Elevation Worship", "albumName": "x",
       "duration": 241.0, "instrumental": false, "plainLyrics": "x", "syncedLyrics": "[00:01.00] no"}
    ]"#;

    fn candidates(body: &str) -> Vec<Candidate> {
        lrclib::parse_search(body).expect("canned json parses")
    }

    #[test]
    fn lrclib_records_become_candidates() {
        let found = candidates(PRAISE);
        assert_eq!(found.len(), 4);
        assert_eq!(found[0].id, "1");
        assert_eq!(found[0].album.as_deref(), Some("Can You Imagine?"));
        assert!(found[0].duration.is_some_and(|d| (d - 308.0).abs() < 1e-9));
        assert!(found[0].is_synced());
        assert!(
            !found[2].is_synced(),
            "a record with no syncedLyrics is not synced"
        );
        let one = lrclib::parse_get(r#"{"id": 9, "trackName": "T", "artistName": "A", "duration": 1.5, "instrumental": true, "syncedLyrics": "[00:01.00] x"}"#)
            .expect("get body parses");
        assert!(!one.is_synced(), "an instrumental has no words");
    }

    #[test]
    fn the_closest_length_wins() {
        let found = candidates(PRAISE);
        let mut synced = Vec::new();
        add_synced(&mut synced, found);
        let kept = relevant(synced, "Praise", &["Elevation Worship".into()], false);
        // "Praise You Anywhere" is another song, however close its length;
        // the featured cut is the same song in other dress.
        assert_eq!(
            kept.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["1", "2"]
        );

        let (ranked, chosen) = rank(kept.clone(), Some(238.0));
        assert_eq!(ranked[chosen].id, "2");
        let (ranked, chosen) = rank(kept, Some(300.0));
        assert_eq!(ranked[chosen].id, "1");
    }

    #[test]
    fn length_orders_the_list_and_picks_the_nearest() {
        let found = candidates(PRAISE)
            .into_iter()
            .filter(Candidate::is_synced)
            .collect();
        let (ranked, chosen) = rank(found, Some(300.0));
        let ids: Vec<&str> = ranked.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["1", "4", "2"]);
        assert_eq!(chosen, 0);
    }

    #[test]
    fn a_live_cut_loses_only_a_tie() {
        let body = r#"[
          {"id": 10, "trackName": "Who Else - Live", "artistName": "Gateway Worship", "duration": 330.0, "syncedLyrics": "[00:01.00] a"},
          {"id": 11, "trackName": "Who Else", "artistName": "Gateway Worship", "duration": 331.0, "syncedLyrics": "[00:01.00] a"},
          {"id": 12, "trackName": "Who Else (Live)", "artistName": "Gateway Worship", "duration": 400.0, "syncedLyrics": "[00:01.00] a"}
        ]"#;
        let (ranked, chosen) = rank(candidates(body), Some(330.0));
        assert_eq!(
            ranked[chosen].id, "11",
            "a studio cut a second longer is a tie, and wins it"
        );

        let (ranked, chosen) = rank(candidates(body), Some(400.0));
        assert_eq!(
            ranked[chosen].id, "12",
            "a live cut that is the right length is the right cut"
        );
    }

    #[test]
    fn every_artist_is_searched_then_the_title_alone() {
        let body = r#"[
          {"id": 20, "trackName": "Always On Time", "artistName": "Elevation Worship & Pat Barrett", "duration": 300.0, "syncedLyrics": "[00:01.00] a"},
          {"id": 21, "trackName": "Always On Time", "artistName": "Ja Rule", "duration": 245.0, "syncedLyrics": "[00:01.00] b"}
        ]"#;
        let provider = Canned {
            answers: vec![(None, body)],
            asked: RefCell::new(Vec::new()),
        };
        let artists = vec!["Pat Barrett".to_string(), "Elevation Worship".to_string()];
        let gathered = gather(&provider, "Always On Time", &artists).expect("gathers");
        assert!(gathered.by_title_only);
        assert_eq!(
            *provider.asked.borrow(),
            [
                Some("Pat Barrett".to_string()),
                Some("Elevation Worship".to_string()),
                None
            ]
        );
        // The title-only search keeps the version an artist we credit is on,
        // even though the other is closer to our length.
        let kept = relevant(gathered.candidates, "Always On Time", &artists, true);
        let (ranked, chosen) = rank(kept, Some(245.0));
        assert_eq!(ranked[chosen].id, "20");
    }

    #[test]
    fn an_artist_hit_skips_the_title_search() {
        let provider = Canned {
            answers: vec![(Some("Elevation Worship"), PRAISE)],
            asked: RefCell::new(Vec::new()),
        };
        let gathered = gather(&provider, "Praise", &["Elevation Worship".into()]).expect("gathers");
        assert!(!gathered.by_title_only);
        assert_eq!(provider.asked.borrow().len(), 1);
        assert_eq!(
            gathered.candidates.len(),
            3,
            "the unsynced record is dropped"
        );
    }

    #[test]
    fn the_chart_header_names_the_title_and_each_artist() {
        assert_eq!(
            chart_header("Always On Time - Pat Barrett, Elevation Worship"),
            (
                "Always On Time".to_string(),
                vec!["Pat Barrett".to_string(), "Elevation Worship".to_string()]
            )
        );
        assert_eq!(
            chart_header("Praise - Elevation Worship"),
            ("Praise".to_string(), vec!["Elevation Worship".to_string()])
        );
        assert_eq!(
            chart_header("Untitled"),
            ("Untitled".to_string(), Vec::new())
        );
    }

    #[test]
    fn the_chart_gives_the_length_from_songstart_to_songend() {
        // 60 bpm 4/4: a bar is 4 s. Two bars of count-in are not the song;
        // the four bars of verse and four of chorus are.
        let song = SongInfo::from_chart("Song - A, B\n60bpm 4/4 #C\n\nCount 2\nVS 4\nCH 4\n");
        assert_eq!(song.title, "Song");
        assert_eq!(song.artists, ["A", "B"]);
        let length = song.length.expect("a chart with sections has a length");
        assert!((length - 32.0).abs() < 1e-9, "{length}");
    }

    #[test]
    fn the_lrc_carries_its_headers_and_parses_back() {
        let found = candidates(PRAISE);
        let text = render_lrc(&found[0]);
        assert_eq!(
            text,
            "[ti:Praise]\n[ar:Elevation Worship]\n[al:Can You Imagine?]\n[length:05:08.00]\n\
             [#:source lrclib id 1]\n[00:10.00] Let everything\n[00:12.00] that has breath\n"
        );
        let lrc = keyflow::lrc::parse(&text);
        assert_eq!(lrc.title.as_deref(), Some("Praise"));
        assert_eq!(lrc.album.as_deref(), Some("Can You Imagine?"));
        assert_eq!(lrc.lines.len(), 2);
        assert!((lrc.lines[1].start - 12.0).abs() < 1e-6);
    }

    #[test]
    fn lengths_print_as_clock_time() {
        assert_eq!(clock(308.0), "5:08");
        assert_eq!(clock(59.6), "1:00");
        assert_eq!(lrc_length(239.456), "03:59.46");
    }

    #[test]
    fn titles_are_compared_without_their_dressing() {
        assert_eq!(core_title("Who Else - Live"), "who else");
        assert_eq!(
            core_title("God, I'm Just Grateful (feat. X)"),
            "god im just grateful"
        );
    }
}
