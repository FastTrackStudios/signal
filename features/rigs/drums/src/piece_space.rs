//! Piece subspaces (#77 M4) — a similarity space over the drum library's
//! swappable `.signalengine` pieces, so "find me another snare like this
//! one" is a ranked list instead of a scroll through filenames.
//!
//! Engines are specs, not audio, so each one is *rendered*: load it on an
//! offline sampler, strike a representative note, and run the rendered hit
//! through the same analyzer the sample space uses. The result is an
//! ordinary `.space` store (one node per engine) that KNN queries directly.

use std::path::{Path, PathBuf};

use signal_sampler::SamplerRig;
use signal_space::{analyze, knn, Space, SpaceItem, SPACE_VERSION};

/// The space built under `<library>/Space/pieces.space`.
pub const PIECE_SPACE: &str = "pieces";

/// Render one engine's representative hit to mono at the analyzer's rate.
fn render_engine(rig: &SamplerRig, engine_path: &Path, note: u8) -> Option<Vec<f32>> {
    let id = "probe";
    rig.unload_instrument(id);
    rig.load_engine(id.to_string(), engine_path).ok()?;
    let _ = rig.preload_instrument(id);
    let sr = rig.sample_rate() as usize;
    let mut buf = vec![0.0f32; 512 * 2];
    // Let the cache warm — a cold engine renders silence.
    for _ in 0..30 {
        buf.iter_mut().for_each(|s| *s = 0.0);
        let _ = rig.render_offline(&mut buf);
        std::thread::sleep(std::time::Duration::from_millis(6));
    }
    rig.note_on_instrument(id, note, 110);
    // ~1.5 s of mono, the analyzer's window.
    let want = sr * 3 / 2;
    let mut mono = Vec::with_capacity(want);
    while mono.len() < want {
        buf.iter_mut().for_each(|s| *s = 0.0);
        if rig.render_offline(&mut buf).is_err() {
            break;
        }
        for f in buf.chunks_exact(2) {
            mono.push((f[0] + f[1]) * 0.5);
        }
    }
    rig.note_off_instrument(id, note, 0);
    // Resample to the analysis rate (the analyzer's contract).
    if sr != analyze::ANALYSIS_SR as usize {
        let ratio = sr as f64 / analyze::ANALYSIS_SR as f64;
        let out_len = (mono.len() as f64 / ratio) as usize;
        let mut out = Vec::with_capacity(out_len);
        for i in 0..out_len {
            let pos = i as f64 * ratio;
            let i0 = pos as usize;
            let frac = (pos - i0 as f64) as f32;
            let a = mono.get(i0).copied().unwrap_or(0.0);
            let b = mono.get(i0 + 1).copied().unwrap_or(a);
            out.push(a + (b - a) * frac);
        }
        mono = out;
    }
    (mono.iter().any(|s| s.abs() > 1e-5)).then_some(mono)
}

/// The note a piece of this kind is normally struck on (GM drum map), so
/// key-ranged engines actually fire.
fn probe_note(kind: &str) -> u8 {
    match kind {
        "kick" => 36,
        "snare" => 38,
        "tom" => 45,
        "hi-hat" => 42,
        "ride" => 51,
        "crash" => 49,
        "china" => 52,
        "splash" => 55,
        _ => 38,
    }
}

/// Build (or rebuild) the piece space for a drum library.
///
/// Returns `(space dir, engines analyzed, engines skipped)`.
pub fn build(library_root: &Path) -> Result<(PathBuf, usize, usize), String> {
    let engines = crate::library::scan_engines(&crate::library::engines_dir(library_root));
    if engines.is_empty() {
        return Err(format!("no engines under {}", library_root.display()));
    }
    let rig = SamplerRig::new_offline(48_000);
    let mut items = Vec::new();
    let mut features = Vec::new();
    let mut skipped = 0usize;
    for piece in &engines {
        let path = PathBuf::from(&piece.path);
        let Some(mono) = render_engine(&rig, &path, probe_note(&piece.kind)) else {
            tracing::warn!(engine = %piece.path, "piece space: silent / unloadable — skipped");
            skipped += 1;
            continue;
        };
        let dur = mono.len() as f32 / analyze::ANALYSIS_SR as f32;
        let Some(a) = analyze::analyze(&mono, dur) else {
            skipped += 1;
            continue;
        };
        let rel = path
            .strip_prefix(library_root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        items.push(SpaceItem {
            path: rel,
            // The library's own kind is ground truth — better than any
            // acoustic guess, and it's what the swap UI filters on.
            class: piece.kind.clone(),
            x: 0.0,
            y: 0.0,
            duration_s: a.duration_s,
            centroid_hz: a.centroid_hz,
            rms_db: a.rms_db,
            percussiveness: a.percussiveness,
            size_bytes: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
            mtime_s: 0,
            favorite: false,
        });
        features.extend_from_slice(&a.features);
    }
    if items.is_empty() {
        return Err("no engine rendered audibly".into());
    }
    let coords = signal_space::project::project_2d(&features, items.len(), analyze::DIM);
    for (item, (x, y)) in items.iter_mut().zip(coords) {
        item.x = x;
        item.y = y;
    }
    let analyzed = items.len();
    let space = Space {
        version: SPACE_VERSION,
        name: PIECE_SPACE.to_string(),
        root: library_root.to_string_lossy().into_owned(),
        dim: analyze::DIM,
        items,
    };
    let dir = Space::space_dir(library_root, PIECE_SPACE);
    space.save(&dir, &features)?;
    Ok((dir, analyzed, skipped))
}

/// Rank the library's pieces by similarity to `engine_path`, restricted to
/// the same kind. Returns `(engine path, score)` best-first.
pub fn similar_to(
    library_root: &Path,
    engine_path: &Path,
    limit: usize,
) -> Result<Vec<(String, f32)>, String> {
    let dir = Space::space_dir(library_root, PIECE_SPACE);
    let (space, features) = Space::load(&dir)?;
    let rel = engine_path
        .strip_prefix(library_root)
        .unwrap_or(engine_path)
        .to_string_lossy()
        .into_owned();
    let idx = space
        .items
        .iter()
        .position(|i| i.path == rel)
        .ok_or_else(|| format!("{rel} not in the piece space"))?;
    let class = space.items[idx].class.clone();
    Ok(knn::similar(&features, space.dim, idx, limit, |i| {
        space.items[i].class == class
    })
    .into_iter()
    .map(|(i, score)| {
        (
            Path::new(&space.root)
                .join(&space.items[i].path)
                .to_string_lossy()
                .into_owned(),
            score,
        )
    })
    .collect())
}
