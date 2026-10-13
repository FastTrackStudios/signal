//! Export the 6 real worship demo songs into portable Song folders.
//!
//! For each song in [`session::setlist::service::demo::demo_songs_base`] (the
//! SAME source of truth the in-process session engine plays), this writes a
//! self-contained folder the fasttrackstudio.app browser player can stream:
//!
//! ```text
//! $HOME/.fts-scratch/data/orgs/days-to-praise/resources/songs/{slug}/
//!   chart.kf        the keyflow chart text (session::…::demo::demo_chart_for)
//!   manifest.json   { slug,title,artist,key,bpm,time_signature,duration_sec,
//!                     sections:[{name,start_sec,end_sec}],
//!                     stems:[{name,group,file,default_muted}] }
//!   stems/*.ogg     each source WAV transcoded → Opus (96 kbps)
//! ```
//!
//! Section timings come STRAIGHT from `demo_songs_base()` (chart-derived
//! seconds — never hand-typed); key/artist/section-labels come from
//! `chart_import::chart_to_layout` on the bundled chart. The per-song stem
//! list + display name + instrument group + default-muted flag replicate the
//! app's `session_engine.rs` seeding logic (curated table for Praise, folder
//! glob for the other five, click/guide/cue/count → default-muted).
//!
//! Run (from repo root, with ffmpeg on PATH):
//! ```bash
//! export CARGO_TARGET_DIR="$PWD/.wt-target"
//! export PATH="/nix/store/…-ffmpeg-8.1-bin/bin:$PATH"
//! cargo run -p session --example export_demo_songs
//! # optionally: EXPORT_ONLY="Praise,Washed" to transcode only those two
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use session::keyflow::actions::SectionKind;
use session::setlist::chart_import::{ChartLayout, chart_to_layout};
use session::setlist::service::demo::{DemoSong, demo_chart_for, demo_songs_base};

// ── Stem tables, replicated from apps/fasttrackstudio/src/session_engine.rs ──

/// Curated Praise stems: (display name, filename, group). Mirrors
/// `seed_praise_media`'s STEMS table exactly.
const PRAISE_STEMS: &[(&str, &str, &str)] = &[
    ("Click", "01 - Click.wav", "Guide"),
    ("Cue", "02 - Cue.wav", "Guide"),
    (
        "Original Track",
        "03 - Elevation Worship - Praise (Original Track).wav",
        "Reference",
    ),
    ("BGVS", "04 - BGVS.wav", "Vocals"),
    ("BGVS 2", "05 - BGVS 2.wav", "Vocals"),
    ("Choir", "06 - Choir.wav", "Vocals"),
    ("Organ", "07 - Organ.wav", "Keys"),
    ("Keys", "08 - Keys.wav", "Keys"),
    ("Piano", "09 - Piano.wav", "Keys"),
    ("Electric Bass 1", "10 - Electric Bass 1.wav", "Bass"),
    ("Electric Bass 2", "11 - Electric Bass 2.wav", "Bass"),
    ("Synth Bass", "12 - Synth Bass.wav", "Bass"),
    ("Acoustic Guitar", "13 - Acoustic Guitar.wav", "Guitars"),
    ("Electric Guitar 1", "14 - Electric Guitar 1.wav", "Guitars"),
    ("Electric Guitar 2", "15 - Electric Guitar 2.wav", "Guitars"),
    ("Electric Guitar 3", "16 - Electric Guitar 3.wav", "Guitars"),
    ("Electric Guitar 4", "17 - Electric Guitar 4.wav", "Guitars"),
    ("Electric Guitar 5", "18 - Electric Guitar 5.wav", "Guitars"),
    ("Electric Guitar 6", "19 - Electric Guitar 6.wav", "Guitars"),
    ("Electric Guitar 7", "20 - Electric Guitar 7.wav", "Guitars"),
    ("Loop", "21 - Loop.wav", "Tracks"),
    ("Hand Percussion", "22 - Hand Percussion.wav", "Percussion"),
    ("Percussion", "23 - Percussion.wav", "Percussion"),
];

/// On-disk folder (under `FTS_WORSHIP_STEMS`) for each globbed worship song.
/// Mirrors `session_engine::WORSHIP_FOLDERS`.
const WORSHIP_FOLDERS: &[(&str, &str)] = &[
    (
        "God, I'm Just Grateful",
        "God, I_m Just Grateful _ Elevation Worship _ D",
    ),
    ("Holy Forever", "Holy Forever _ Bethel Music _ Bb"),
    (
        "Thank God I'm Free",
        "Thank God I_m Free _ Elevation Rhythm _ E",
    ),
    ("Washed", "Washed _ Elevation Rhythm _ B"),
    ("Who Else", "Who Else _ Gateway Worship _ Ab"),
];

/// Instrument-family group for a stem name. Mirrors `session_engine::group_for`.
fn group_for(name: &str) -> Option<&'static str> {
    let n = name.to_ascii_lowercase();
    let any = |ks: &[&str]| ks.iter().any(|k| n.contains(k));
    if any(&["click", "cue", "guide"]) {
        Some("Guide")
    } else if any(&["loop"]) {
        Some("Tracks")
    } else if any(&["bass"]) {
        Some("Bass")
    } else if any(&["drum", "perc"]) {
        Some("Drums")
    } else if any(&["organ", "key", "piano", "rhodes", "synth", "pad"]) {
        Some("Keys")
    } else if any(&["guitar", "gtr", "acoustic", "electric", "ag", "eg"]) {
        Some("Guitars")
    } else if any(&["vocal", "vox", "bgv", "choir"]) {
        Some("Vocals")
    } else if any(&["sax", "horn", "brass", "string", "orch"]) {
        Some("Orchestra")
    } else if any(&["fx"]) {
        Some("FX")
    } else {
        None
    }
}

/// `true` when a stem is the multitrack's own click/guide/cue/count → default
/// muted. Mirrors `session_engine::is_click_or_guide`.
fn is_click_or_guide(group: Option<&str>, name: &str) -> bool {
    let hay = format!("{} {}", group.unwrap_or(""), name).to_ascii_lowercase();
    ["click", "guide", "cue", "count"]
        .iter()
        .any(|k| hay.contains(k))
}

// ── Stem discovery (the same resolution the app uses) ────────────────────────

struct SourceStem {
    name: String,
    group: Option<String>,
    path: PathBuf,
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_default())
}

/// Resolve a song's ordered source stems (WAV paths + display name + group),
/// exactly as `session_engine::seed_song_media` would.
fn source_stems(song: &str) -> Vec<SourceStem> {
    if song == "Praise" {
        let dir = std::env::var("FTS_PRAISE_STEMS")
            .map_or_else(|_| {
                home().join(
                    "Downloads/Elevation Worship - Praise-20260712T200150Z-2-001/Elevation Worship - Praise/- MultiTracks",
                )
            }, PathBuf::from);
        return PRAISE_STEMS
            .iter()
            .map(|(name, file, group)| SourceStem {
                name: (*name).to_string(),
                group: Some((*group).to_string()),
                path: dir.join(file),
            })
            .filter(|s| s.path.is_file())
            .collect();
    }
    let Some((_, folder)) = WORSHIP_FOLDERS.iter().find(|(n, _)| *n == song) else {
        return Vec::new();
    };
    let base = std::env::var("FTS_WORSHIP_STEMS").map_or_else(
        |_| home().join("Downloads/Worship MultiTracks"),
        PathBuf::from,
    );
    let dir = base.join(folder);
    let mut wavs: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("wav")))
            .collect(),
        Err(_) => return Vec::new(),
    };
    wavs.sort();
    wavs.iter()
        .map(|p| {
            let stem = p
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            // "Holy Forever - EG 2" → "EG 2"; "01 - Click" → "Click".
            let name = stem
                .rsplit(" - ")
                .next()
                .unwrap_or(&stem)
                .trim()
                .to_string();
            let group = group_for(&name).map(ToString::to_string);
            SourceStem {
                name,
                group,
                path: p.clone(),
            }
        })
        .collect()
}

// ── Section naming ───────────────────────────────────────────────────────────

const fn section_base_name(kind: SectionKind) -> &'static str {
    match kind {
        SectionKind::Intro => "Intro",
        SectionKind::Verse => "Verse",
        SectionKind::PreChorus => "Pre-Chorus",
        SectionKind::Chorus => "Chorus",
        SectionKind::Bridge => "Bridge",
        SectionKind::Outro => "Outro",
        SectionKind::Instrumental => "Instrumental",
        SectionKind::Solo => "Solo",
        SectionKind::Hits => "Hits",
        SectionKind::Interlude => "Interlude",
        SectionKind::Breakdown => "Breakdown",
        SectionKind::Vamp => "Vamp",
        SectionKind::Refrain => "Refrain",
        SectionKind::Tag => "Tag",
        SectionKind::Turnaround => "Turnaround",
        SectionKind::CountIn => "Count-In",
        SectionKind::End => "End",
    }
}

// ── Slug / JSON helpers ──────────────────────────────────────────────────────

fn kebab(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for ch in s.chars() {
        // Drop apostrophes entirely so "I'm" → "im" (not "i-m").
        if ch == '\'' || ch == '\u{2019}' {
            continue;
        }
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

// ── ffmpeg ───────────────────────────────────────────────────────────────────

fn ffmpeg_bin() -> String {
    std::env::var("FTS_FFMPEG").unwrap_or_else(|_| "ffmpeg".to_string())
}

fn have_ffmpeg() -> bool {
    Command::new(ffmpeg_bin())
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Transcode `src` WAV → `dst` Opus/Ogg at 96 kbps. Returns Ok on success.
fn transcode_opus(src: &Path, dst: &Path) -> std::io::Result<bool> {
    let status = Command::new(ffmpeg_bin())
        .args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(src)
        .args(["-c:a", "libopus", "-b:a", "96k"])
        .arg(dst)
        .status()?;
    Ok(status.success())
}

/// Duration in seconds of an audio file via ffprobe (best-effort).
fn probe_duration(path: &Path) -> Option<f64> {
    let probe = std::env::var("FTS_FFPROBE").unwrap_or_else(|_| "ffprobe".to_string());
    let out = Command::new(probe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .ok()
}

fn main() {
    let out_root = home().join(".fts-scratch/data/orgs/days-to-praise/resources/songs");
    if let Err(e) = std::fs::create_dir_all(&out_root) {
        eprintln!("ERROR: Failed to create songs root: {e}");
        std::process::exit(1);
    }

    let ffmpeg = have_ffmpeg();
    if !ffmpeg {
        eprintln!(
            "WARNING: ffmpeg not found (set FTS_FFMPEG or add to PATH) — falling back to copying WAVs."
        );
    }

    // Optional restriction: EXPORT_ONLY="Praise,Washed" transcodes only those,
    // others get manifest.json + chart.kf with an EMPTY stems array.
    let only: Option<Vec<String>> = std::env::var("EXPORT_ONLY").ok().map(|v| {
        v.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    });

    let songs = demo_songs_base();
    println!(
        "Exporting {} demo songs → {}",
        songs.len(),
        out_root.display()
    );

    for song in &songs {
        export_song(song, &out_root, ffmpeg, only.as_deref());
    }
    println!("\nDone.");
}

fn export_sections(song: &DemoSong, layout: Option<&ChartLayout>) -> (Vec<String>, f64) {
    let chart_secs: Vec<&session::setlist::chart_import::LaidSection> = layout
        .map(|l| {
            l.sections
                .iter()
                .filter(|s| s.kind != SectionKind::CountIn)
                .collect()
        })
        .unwrap_or_default();

    let mut kind_totals: std::collections::HashMap<&'static str, u32> = HashMap::default();
    for ds in &song.sections {
        kind_totals
            .entry(section_base_name(ds.kind))
            .and_modify(|c| *c = c.saturating_add(1))
            .or_insert(1);
    }
    let mut kind_seen: std::collections::HashMap<&'static str, u32> = HashMap::default();

    let mut sections_json = Vec::new();
    for (i, ds) in song.sections.iter().enumerate() {
        let base = section_base_name(ds.kind);
        let name = if kind_totals.get(base).copied().unwrap_or(0) > 1 {
            let n = kind_seen.entry(base).or_insert(0);
            *n = n.saturating_add(1);
            format!("{base} {n}")
        } else {
            base.to_string()
        };
        let final_name = if let Some(cs) = chart_secs.get(i)
            && let Some(label) = &cs.label
        {
            format!("{name} ({label})")
        } else {
            name
        };
        sections_json.push(format!(
            "    {{ \"name\": {}, \"start_sec\": {}, \"end_sec\": {} }}",
            json_str(&final_name),
            fmt_f(ds.start),
            fmt_f(ds.end),
        ));
    }
    let sections_end = song.sections.last().map_or(0.0, |s| s.end);
    (sections_json, sections_end)
}

fn export_stems(
    title: &str,
    stems_dir: &Path,
    ffmpeg: bool,
    only: Option<&[String]>,
) -> (Vec<String>, f64, usize) {
    let sources = source_stems(title);
    let do_stems = ffmpeg && only.is_none_or(|o| o.iter().any(|n| n == title));

    let mut stems_json = Vec::new();
    let mut longest_stem = 0.0_f64;
    let mut transcoded = 0usize;
    if do_stems && !sources.is_empty() {
        for (i, src) in sources.iter().enumerate() {
            let idx = i.saturating_add(1);
            let default_muted = is_click_or_guide(src.group.as_deref(), &src.name);
            let file_slug = kebab(&src.name);
            let (file_rel, ok) = if ffmpeg {
                let fname = format!("{idx:02}-{file_slug}.ogg");
                let dst = stems_dir.join(&fname);
                let ok = transcode_opus(&src.path, &dst).unwrap_or(false);
                (format!("stems/{fname}"), ok)
            } else {
                let fname = format!("{idx:02}-{file_slug}.wav");
                let dst = stems_dir.join(&fname);
                let ok = std::fs::copy(&src.path, &dst).is_ok();
                (format!("stems/{fname}"), ok)
            };
            if !ok {
                eprintln!("  {title}: FAILED to encode {}", src.path.display());
                continue;
            }
            transcoded = transcoded.saturating_add(1);
            if let Some(d) = probe_duration(&stems_dir.join(file_rel.trim_start_matches("stems/")))
            {
                longest_stem = longest_stem.max(d);
            }
            let group = src.group.clone().unwrap_or_else(|| "Other".to_string());
            stems_json.push(format!(
                "    {{ \"name\": {}, \"group\": {}, \"file\": {}, \"default_muted\": {} }}",
                json_str(&src.name),
                json_str(&group),
                json_str(&file_rel),
                default_muted,
            ));
        }
    }
    (stems_json, longest_stem, transcoded)
}

fn export_song(song: &DemoSong, out_root: &Path, ffmpeg: bool, only: Option<&[String]>) {
    let title = song.name;
    let slug = kebab(title);
    let chart = demo_chart_for(title).unwrap_or("");
    let layout: Option<ChartLayout> = chart_to_layout(chart).ok();

    let dir = out_root.join(&slug);
    let stems_dir = dir.join("stems");
    if let Err(e) = std::fs::create_dir_all(&stems_dir) {
        eprintln!("ERROR: Failed to create song dir: {e}");
        return;
    }

    if let Err(e) = std::fs::write(dir.join("chart.kf"), chart) {
        eprintln!("ERROR: Failed to write chart.kf: {e}");
        return;
    }

    let (sections_json, sections_end) = export_sections(song, layout.as_ref());

    let (stems_json, longest_stem, transcoded) = export_stems(title, &stems_dir, ffmpeg, only);

    let duration = sections_end.max(longest_stem);

    let key = layout
        .as_ref()
        .and_then(|l| l.key.clone())
        .unwrap_or_default();
    let artist = layout
        .as_ref()
        .and_then(|l| l.artist.clone())
        .unwrap_or_default();
    let time_sig = format!("{}/{}", song.time_sig_num, song.time_sig_den);
    let bpm = if song.tempo_bpm.is_finite() && song.tempo_bpm >= 0.0 {
        song.tempo_bpm.round()
    } else {
        0.0
    };

    let manifest = format!(
        "{{\n  \"slug\": {slug}, \"title\": {title}, \"artist\": {artist},\n  \
         \"key\": {key}, \"bpm\": {bpm:.0}, \"time_signature\": {ts}, \"duration_sec\": {dur},\n  \
         \"sections\": [\n{sections}\n  ],\n  \"stems\": [\n{stems}\n  ]\n}}\n",
        slug = json_str(&slug),
        title = json_str(title),
        artist = json_str(&artist),
        key = json_str(&key),
        bpm = bpm,
        ts = json_str(&time_sig),
        dur = fmt_f(duration),
        sections = sections_json.join(",\n"),
        stems = stems_json.join(",\n"),
    );
    if let Err(e) = std::fs::write(dir.join("manifest.json"), &manifest) {
        eprintln!("ERROR: Failed to write manifest.json: {e}");
        return;
    }

    let sources = source_stems(title);
    let do_stems = ffmpeg && only.is_none_or(|o| o.iter().any(|n| n == title));
    println!(
        "  {title:26} slug={slug:20} key={key:3} {bpm}bpm {time_sig}  sections={:2}  stems={}{}",
        song.sections.len(),
        transcoded,
        if !do_stems && !sources.is_empty() {
            format!(" (metadata-only; {} sources available)", sources.len())
        } else {
            String::new()
        },
    );
}

// Minimal JSON string escaper (ASCII-only content here).
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len().saturating_add(2));
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Format an f64 with up to 3 decimals, trimming trailing zeros (so integers
/// stay clean but 8.4 / 312.5 survive).
fn fmt_f(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}
