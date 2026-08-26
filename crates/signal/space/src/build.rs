//! Space builder — scan a sample root, analyze (rayon-parallel, incremental
//! against a previous build), classify, project, persist.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rayon::prelude::*;

use crate::analyze::{self, DIM};
use crate::{SPACE_VERSION, Space, SpaceItem};

/// Progress callback: (analyzed_so_far, total_to_analyze).
pub type Progress = dyn Fn(usize, usize) + Sync;

/// Node granularity for the built space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    /// One node per audio file — electronic one-shot folders.
    Sample,
    /// One node per kit piece: a multisampled instrument (all its RR /
    /// velocity / articulation / mic files) collapses to a single node,
    /// so a 16k-file acoustic library maps as ~dozens of pieces.
    Piece,
}

/// Directory / token vocabulary that marks a path component as a VARIANT
/// bucket (articulation, mic, velocity, round-robin) rather than a piece.
fn is_variant_component(c: &str) -> bool {
    const VARIANTS: &[&str] = &[
        "hit",
        "hits",
        "choke",
        "chokes",
        "bow",
        "bell",
        "edge",
        "tip",
        "open",
        "closed",
        "tight",
        "rimshot",
        "rim",
        "crossstick",
        "cross-stick",
        "cross stick",
        "flam",
        "roll",
        "ruff",
        "wires",
        "wireson",
        "wiresoff",
        "oh",
        "overhead",
        "overheads",
        "room",
        "rooms",
        "close",
        "in",
        "out",
        "sub",
        "top",
        "bottom",
        "mixed",
        "mono",
        "stereo",
        "samples",
        "wav",
        "soft",
        "medium",
        "hard",
        "pedal",
        "chick",
        "ching",
        "1-shot",
        "oneshot",
        "tb",
        "trigger",
        "snareoff",
        "snareson",
    ];
    // "OH (Overhead)"-style components match on the text before the paren.
    let lc = c.to_lowercase();
    let lc = lc.split(" (").next().unwrap_or(&lc).trim();
    VARIANTS.contains(&lc) || regex_like_rr_vl(lc)
}

/// rr3 / vl2 / v10 / velocity buckets.
fn regex_like_rr_vl(s: &str) -> bool {
    for prefix in ["rr", "vl", "v"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

/// Piece key for a file: its directory path with trailing variant buckets
/// stripped. Flat files (no non-variant dir) fall back to the filename stem
/// with RR/VL tokens removed, so flat multisample dumps still group.
fn piece_key(rel: &Path) -> String {
    let mut dirs: Vec<&str> = rel
        .parent()
        .map(|p| p.iter().filter_map(|c| c.to_str()).collect())
        .unwrap_or_default();
    while let Some(last) = dirs.last() {
        if is_variant_component(last) {
            dirs.pop();
        } else {
            break;
        }
    }
    if dirs.is_empty() {
        let stem = rel.file_stem().and_then(|s| s.to_str()).unwrap_or("sample");
        let cleaned: String = stem
            .split(['_', '-', ' '])
            .filter(|t| !is_variant_component(t))
            .collect::<Vec<_>>()
            .join(" ");
        if cleaned.is_empty() {
            stem.to_string()
        } else {
            cleaned
        }
    } else {
        dirs.join("/")
    }
}

/// How many member files of a piece get analyzed (evenly spaced through the
/// sorted member list — covers the velocity range without paying for RRs).
const PIECE_ANALYSIS_CAP: usize = 12;

pub struct BuildReport {
    pub space: Space,
    pub features: Vec<f32>,
    pub analyzed: usize,
    pub reused: usize,
    pub failed: Vec<(PathBuf, String)>,
}

/// Build (or incrementally rebuild) a space over every `.wav` under `root`.
/// `previous` supplies reusable analyses keyed by (rel path, size, mtime) —
/// in `Piece` mode the key is (piece key, total bytes, max mtime).
pub fn build(
    name: &str,
    root: &Path,
    granularity: Granularity,
    previous: Option<(&Space, &[f32])>,
    progress: &Progress,
) -> BuildReport {
    // ── discover ──
    let mut files: Vec<(PathBuf, u64, u64)> = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case("wav"))
        })
        .filter_map(|e| {
            let md = e.metadata().ok()?;
            let mtime = md
                .modified()
                .ok()?
                .duration_since(UNIX_EPOCH)
                .ok()?
                .as_secs();
            Some((e.into_path(), md.len(), mtime))
        })
        .collect();
    files.sort();

    if granularity == Granularity::Piece {
        return build_pieces(name, root, files, previous, progress);
    }

    // ── reuse map from the previous build ──
    let mut reusable: std::collections::HashMap<(String, u64, u64), usize> =
        std::collections::HashMap::new();
    if let Some((prev, _)) = previous {
        for (i, it) in prev.items.iter().enumerate() {
            reusable.insert((it.path.clone(), it.size_bytes, it.mtime_s), i);
        }
    }

    let total = files.len();
    let done = std::sync::atomic::AtomicUsize::new(0);
    struct Row {
        item: SpaceItem,
        features: Vec<f32>,
    }
    enum Out {
        Ok(Box<Row>),
        Reused(Box<Row>),
        Err(PathBuf, String),
    }
    let rows: Vec<Out> = files
        .par_iter()
        .map(|(path, size, mtime)| {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .into_owned();
            let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if n.is_multiple_of(500) {
                progress(n, total);
            }
            if let (Some((prev, prev_feats)), Some(&pi)) =
                (previous, reusable.get(&(rel.clone(), *size, *mtime)))
            {
                let mut item = prev.items[pi].clone();
                item.x = 0.0;
                item.y = 0.0;
                return Out::Reused(Box::new(Row {
                    item,
                    features: prev_feats[pi * DIM..(pi + 1) * DIM].to_vec(),
                }));
            }
            let (mono, dur) = match analyze::decode_wav_mono(path) {
                Ok(v) => v,
                Err(e) => return Out::Err(path.clone(), e),
            };
            let Some(a) = analyze::analyze(&mono, dur) else {
                return Out::Err(path.clone(), "too short".into());
            };
            let name_lc = path
                .file_name()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let class = crate::classify::classify(&a, &name_lc);
            Out::Ok(Box::new(Row {
                item: SpaceItem {
                    path: rel,
                    class: class.to_string(),
                    x: 0.0,
                    y: 0.0,
                    duration_s: a.duration_s,
                    centroid_hz: a.centroid_hz,
                    rms_db: a.rms_db,
                    percussiveness: a.percussiveness,
                    size_bytes: *size,
                    mtime_s: *mtime,
                    favorite: false,
                },
                features: a.features.to_vec(),
            }))
        })
        .collect();

    let mut items = Vec::new();
    let mut features = Vec::new();
    let mut failed = Vec::new();
    let (mut analyzed, mut reused) = (0usize, 0usize);
    for out in rows {
        match out {
            Out::Ok(r) => {
                analyzed += 1;
                items.push(r.item);
                features.extend_from_slice(&r.features);
            }
            Out::Reused(r) => {
                reused += 1;
                items.push(r.item);
                features.extend_from_slice(&r.features);
            }
            Out::Err(p, e) => failed.push((p, e)),
        }
    }

    // ── project ──
    let coords = crate::project::project_2d(&features, items.len(), DIM);
    for (item, (x, y)) in items.iter_mut().zip(coords) {
        item.x = x;
        item.y = y;
    }

    BuildReport {
        space: Space {
            version: SPACE_VERSION,
            name: name.to_string(),
            root: root.to_string_lossy().into_owned(),
            dim: DIM,
            items,
        },
        features,
        analyzed,
        reused,
        failed,
    }
}

/// Piece-granularity build: one node per kit piece. Members are grouped by
/// [`piece_key`]; up to [`PIECE_ANALYSIS_CAP`] members (evenly spaced through
/// the sorted list, spanning the velocity range) are analyzed and their
/// vectors averaged.
fn build_pieces(
    name: &str,
    root: &Path,
    files: Vec<(PathBuf, u64, u64)>,
    previous: Option<(&Space, &[f32])>,
    progress: &Progress,
) -> BuildReport {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<String, Vec<&(PathBuf, u64, u64)>> = BTreeMap::new();
    for f in &files {
        let rel = f.0.strip_prefix(root).unwrap_or(&f.0);
        groups.entry(piece_key(rel)).or_default().push(f);
    }
    let mut reusable: std::collections::HashMap<(String, u64, u64), usize> =
        std::collections::HashMap::new();
    if let Some((prev, _)) = previous {
        for (i, it) in prev.items.iter().enumerate() {
            reusable.insert((it.path.clone(), it.size_bytes, it.mtime_s), i);
        }
    }
    let total = groups.len();
    let done = std::sync::atomic::AtomicUsize::new(0);
    enum Out {
        Ok(Box<SpaceItem>, Vec<f32>, bool),
        Err(PathBuf, String),
    }
    let groups: Vec<(String, Vec<&(PathBuf, u64, u64)>)> = groups.into_iter().collect();
    let rows: Vec<Out> = groups
        .par_iter()
        .map(|(key, members)| {
            let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if n.is_multiple_of(20) {
                progress(n, total);
            }
            let total_bytes: u64 = members.iter().map(|m| m.1).sum();
            let max_mtime: u64 = members.iter().map(|m| m.2).max().unwrap_or(0);
            if let (Some((prev, prev_feats)), Some(&pi)) = (
                previous,
                reusable.get(&(key.clone(), total_bytes, max_mtime)),
            ) {
                let mut item = prev.items[pi].clone();
                item.x = 0.0;
                item.y = 0.0;
                let dim = prev.dim;
                return Out::Ok(
                    Box::new(item),
                    prev_feats[pi * dim..(pi + 1) * dim].to_vec(),
                    true,
                );
            }
            // Evenly spaced member subset across the sorted group.
            let stride = (members.len() / PIECE_ANALYSIS_CAP).max(1);
            let mut acc = [0.0f64; DIM];
            let mut scal = (0.0f32, 0.0f32, 0.0f32, 0.0f32); // dur, centroid, rms, perc
            let mut n_ok = 0usize;
            let mut last_err: Option<String> = None;
            for m in members.iter().step_by(stride).take(PIECE_ANALYSIS_CAP) {
                match analyze::decode_wav_mono(&m.0).and_then(|(mono, dur)| {
                    analyze::analyze(&mono, dur).ok_or_else(|| "too short".into())
                }) {
                    Ok(a) => {
                        for (dst, src) in acc.iter_mut().zip(a.features.iter()) {
                            *dst += *src as f64;
                        }
                        scal.0 += a.duration_s;
                        scal.1 += a.centroid_hz;
                        scal.2 += a.rms_db;
                        scal.3 += a.percussiveness;
                        n_ok += 1;
                    }
                    Err(e) => last_err = Some(e),
                }
            }
            if n_ok == 0 {
                return Out::Err(
                    members[0].0.clone(),
                    last_err.unwrap_or_else(|| "no analyzable members".into()),
                );
            }
            let mut features = vec![0.0f32; DIM];
            for (dst, src) in features.iter_mut().zip(acc.iter()) {
                *dst = (*src / n_ok as f64) as f32;
            }
            let inv = 1.0 / n_ok as f32;
            // Rebuild a lightweight Analysis view for classification from the
            // aggregate scalars + the piece path (the strongest hint).
            let rep = members[members.len() / 2];
            let rep_a = analyze::decode_wav_mono(&rep.0)
                .ok()
                .and_then(|(mono, dur)| analyze::analyze(&mono, dur));
            let hint = key.to_lowercase();
            let class = rep_a
                .as_ref()
                .map(|a| crate::classify::classify(a, &hint))
                .unwrap_or("perc");
            Out::Ok(
                Box::new(SpaceItem {
                    path: key.clone(),
                    class: class.to_string(),
                    x: 0.0,
                    y: 0.0,
                    duration_s: scal.0 * inv,
                    centroid_hz: scal.1 * inv,
                    rms_db: scal.2 * inv,
                    percussiveness: scal.3 * inv,
                    size_bytes: total_bytes,
                    mtime_s: max_mtime,
                    favorite: false,
                }),
                features,
                false,
            )
        })
        .collect();

    let mut items = Vec::new();
    let mut features = Vec::new();
    let mut failed = Vec::new();
    let (mut analyzed, mut reused) = (0usize, 0usize);
    for out in rows {
        match out {
            Out::Ok(item, f, was_reused) => {
                if was_reused {
                    reused += 1;
                } else {
                    analyzed += 1;
                }
                items.push(*item);
                features.extend_from_slice(&f);
            }
            Out::Err(p, e) => failed.push((p, e)),
        }
    }
    let coords = crate::project::project_2d(&features, items.len(), DIM);
    for (item, (x, y)) in items.iter_mut().zip(coords) {
        item.x = x;
        item.y = y;
    }
    BuildReport {
        space: Space {
            version: SPACE_VERSION,
            name: name.to_string(),
            root: root.to_string_lossy().into_owned(),
            dim: DIM,
            items,
        },
        features,
        analyzed,
        reused,
        failed,
    }
}
