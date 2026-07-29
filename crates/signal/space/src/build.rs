//! Space builder — scan a sample root, analyze (rayon-parallel, incremental
//! against a previous build), classify, project, persist.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rayon::prelude::*;

use crate::analyze::{self, DIM};
use crate::{Space, SpaceItem, SPACE_VERSION};

/// Progress callback: (analyzed_so_far, total_to_analyze).
pub type Progress = dyn Fn(usize, usize) + Sync;

pub struct BuildReport {
    pub space: Space,
    pub features: Vec<f32>,
    pub analyzed: usize,
    pub reused: usize,
    pub failed: Vec<(PathBuf, String)>,
}

/// Build (or incrementally rebuild) a space over every `.wav` under `root`.
/// `previous` supplies reusable analyses keyed by (rel path, size, mtime).
pub fn build(
    name: &str,
    root: &Path,
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
            let mtime = md.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs();
            Some((e.into_path(), md.len(), mtime))
        })
        .collect();
    files.sort();

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
