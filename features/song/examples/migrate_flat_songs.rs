//! Migrate legacy **flat** song folders into the colocated [`song`] schema.
//!
//! Legacy layout (what the demo exporter wrote, still live on prod):
//! ```text
//! <songs-root>/<slug>/
//!   manifest.json          title/key/bpm/sections/stems
//!   chart.kf               the keyflow chart
//!   stems/NN-<name>.ogg    the audio stems
//! ```
//!
//! Target layout (this crate's [`song::to_folder`]):
//! ```text
//! <songs-root>/<slug>/
//!   song.md                          # index (id, title, defaultArrangement, arrangements[])
//!   arrangements/default/
//!     arrangement.md                 # key + chartRef + attachmentRefs
//!   (chart.kf + stems/ are left in place, REFERENCED by relative path)
//! ```
//!
//! **Purely additive**: it writes only `song.md` + `arrangements/default/
//! arrangement.md`, referencing the existing `chart.kf` and `stems/*.ogg`
//! in place — it never moves or deletes the legacy files, so existing
//! `manifest.json`-based playback keeps working. Idempotent: a folder that
//! already has `song.md` is skipped.
//!
//! Usage:
//! ```text
//! cargo run -p song --example migrate_flat_songs -- <songs-root> [--apply] [slug ...]
//! ```
//! Without `--apply` it's a dry run (prints what it would write). With one
//! or more trailing `slug` args, only those song folders are considered;
//! otherwise every flat folder under `<songs-root>` is.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use song::{Arrangement, AttachmentRef, ChartRef, Key, PartsManifest, Song};
use uuid::Uuid;

/// The fields we need out of a legacy `manifest.json`.
#[derive(Deserialize)]
struct LegacyManifest {
    title: Option<String>,
    key: Option<String>,
    #[serde(default)]
    stems: Vec<LegacyStem>,
}

#[derive(Deserialize)]
struct LegacyStem {
    /// Relative path within the song folder, e.g. `stems/03-original-track.ogg`.
    file: String,
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut apply = false;
    let mut verify = false;
    let mut root: Option<PathBuf> = None;
    let mut only: Vec<String> = Vec::new();
    for a in &args {
        match a.as_str() {
            "--apply" => apply = true,
            "--verify" => verify = true,
            "-h" | "--help" => {
                eprintln!("usage: migrate_flat_songs <songs-root> [--apply] [slug ...]");
                return std::process::ExitCode::SUCCESS;
            }
            _ if root.is_none() => root = Some(PathBuf::from(a)),
            slug => only.push(slug.to_string()),
        }
    }
    let Some(root) = root else {
        eprintln!("error: missing <songs-root>");
        return std::process::ExitCode::from(2);
    };

    if verify {
        run_verify(&root, &only)
    } else {
        run_migrate(&root, &only, apply)
    }
}

/// `--verify`: read every migrated folder back through `song::from_folder`
/// and print what the reader sees (round-trip validation, no writes).
fn run_verify(root: &Path, only: &[String]) -> std::process::ExitCode {
    let mut ok: u32 = 0;
    let mut bad: u32 = 0;
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("song.md").is_file())
        .collect();
    dirs.sort();
    for dir in dirs {
        let slug = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if !only.is_empty() && !only.contains(&slug) {
            continue;
        }
        match song::from_folder(&dir) {
            Ok(s) => {
                let a = s.default().or_else(|| s.arrangements.first());
                let (chart, stems) = a.map_or((None, 0), |a| {
                    (
                        a.chart_ref.as_ref().and_then(|c| c.path.clone()),
                        a.attachment_refs.len(),
                    )
                });
                println!(
                    "ok     {slug}  \"{}\"  arrangements={}  default-key={}  chart={:?}  stems={stems}",
                    s.title,
                    s.arrangements.len(),
                    a.map(|a| a.key.to_string()).unwrap_or_default(),
                    chart
                );
                ok = ok.saturating_add(1);
            }
            Err(e) => {
                eprintln!("BAD    {slug}: {e}");
                bad = bad.saturating_add(1);
            }
        }
    }
    println!("\nverified {ok} ok, {bad} bad");
    if bad > 0 {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}

fn run_migrate(root: &Path, only: &[String], apply: bool) -> std::process::ExitCode {
    let mut migrated: u32 = 0;
    let mut skipped: u32 = 0;
    let mut failed: u32 = 0;

    let dirs = std::fs::read_dir(root);
    let mut dirs: Vec<PathBuf> = match dirs {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect(),
        Err(e) => {
            eprintln!("error: reading {}: {e}", root.display());
            return std::process::ExitCode::FAILURE;
        }
    };
    dirs.sort();

    for dir in dirs {
        let slug = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if !only.is_empty() && !only.contains(&slug) {
            continue;
        }
        // Only flat songs: have a manifest.json, not yet migrated (no song.md).
        if !dir.join("manifest.json").is_file() {
            continue;
        }
        if dir.join("song.md").exists() {
            println!("skip   {slug} (already has song.md)");
            skipped = skipped.saturating_add(1);
            continue;
        }
        match build_song(&dir, &slug) {
            Ok(song) => {
                let Some(arr) = song.arrangements.first() else {
                    eprintln!("FAIL   {slug}: built song has no arrangements");
                    failed = failed.saturating_add(1);
                    continue;
                };
                let stem_count = arr.attachment_refs.len();
                let has_chart = arr.chart_ref.is_some();
                if apply {
                    match song::to_folder(&song, &dir) {
                        Ok(()) => {
                            println!(
                                "wrote  {slug}  (\"{}\", key {}, chart={}, {stem_count} stems)",
                                song.title, arr.key, has_chart
                            );
                            migrated = migrated.saturating_add(1);
                        }
                        Err(e) => {
                            eprintln!("FAIL   {slug}: {e}");
                            failed = failed.saturating_add(1);
                        }
                    }
                } else {
                    println!(
                        "would  {slug}  (\"{}\", key {}, chart={}, {stem_count} stems)",
                        song.title, arr.key, has_chart
                    );
                    migrated = migrated.saturating_add(1);
                }
            }
            Err(e) => {
                eprintln!("FAIL   {slug}: {e}");
                failed = failed.saturating_add(1);
            }
        }
    }

    println!(
        "\n{} {migrated} song(s), skipped {skipped}, failed {failed}",
        if apply { "migrated" } else { "would migrate" }
    );
    if failed > 0 {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}

/// Build a [`Song`] from a flat folder — one "Default" arrangement whose
/// chart + stems reference the legacy files in place.
fn build_song(dir: &Path, slug: &str) -> Result<Song, String> {
    let manifest_txt = std::fs::read_to_string(dir.join("manifest.json"))
        .map_err(|e| format!("read manifest.json: {e}"))?;
    let m: LegacyManifest =
        serde_json::from_str(&manifest_txt).map_err(|e| format!("parse manifest.json: {e}"))?;

    let title = m
        .title
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| titleize(slug));
    let key: Key = m
        .key
        .as_deref()
        .and_then(|k| k.parse().ok())
        .unwrap_or_default();

    // Stems become audio attachment references (path only — name/group/mute
    // are re-derived from the filename convention by the reader).
    let attachment_refs: Vec<AttachmentRef> = m
        .stems
        .iter()
        .map(|s| {
            let id = Path::new(&s.file)
                .file_stem()
                .and_then(|x| x.to_str())
                .unwrap_or(&s.file)
                .to_string();
            AttachmentRef {
                id,
                path: Some(s.file.clone()),
                sha256: None,
                kind: Some("audio".to_string()),
            }
        })
        .collect();

    // The chart is referenced in place at the legacy top-level path.
    let chart_ref = dir
        .join("chart.kf")
        .is_file()
        .then(|| ChartRef::from_path("chart.kf"));

    let arr = Arrangement {
        id: Uuid::new_v4(),
        name: "Default".to_string(),
        key,
        // A legacy flat song carries no tempo, meter or part list — leave them
        // absent rather than inventing a 120/4-4 default that would read as
        // authored data later.
        tempo_bpm: None,
        time_signature: None,
        chart_ref,
        parts: PartsManifest::default(),
        attachment_refs,
    };
    Ok(Song {
        id: Uuid::new_v4(),
        title,
        tags: Vec::new(),
        default_arrangement: arr.id,
        arrangements: vec![arr],
    })
}

/// `praise-elevation-worship` → `Praise Elevation Worship` (a last-resort
/// title when the manifest has none).
fn titleize(slug: &str) -> String {
    slug.split('-')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or_else(String::new, |f| {
                f.to_uppercase().collect::<String>() + c.as_str()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}
