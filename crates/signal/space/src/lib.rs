//! **signal-space** — the asset similarity space (`docs/spec/sample-space.md`,
//! issue #77): analyze audio assets into feature vectors, project them to a
//! 2D map where proximity = similarity, classify them into categories, and
//! answer KNN similarity queries.
//!
//! A built space lives on disk as `<name>.space/` — `space.json` (items:
//! path, class, 2D coords, filterable scalars, freshness stamps) plus
//! `features.bin` (the packed f32 feature matrix). Consumers (map UI, kit
//! generation, similarity stepping) read the cache; analysis re-runs
//! incrementally only for changed files.

pub mod analyze;
pub mod build;
pub mod classify;
pub mod knn;
pub mod project;
pub mod service;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One asset in the space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceItem {
    /// Path relative to the space root (portable maps, Atlas-style).
    pub path: String,
    /// Category ("kick", "snare", "hat-closed", …) from [`classify`].
    pub class: String,
    /// Normalized map coordinates (0..1) from [`project`].
    pub x: f32,
    pub y: f32,
    /// Filterable scalars (XO's filter set).
    pub duration_s: f32,
    /// Spectral centroid in Hz — the "main frequency" axis.
    pub centroid_hz: f32,
    /// Overall level (dBFS RMS over the analysis window).
    pub rms_db: f32,
    /// 0..1 — how percussive (transient) vs sustained the asset is.
    pub percussiveness: f32,
    /// Freshness stamps for incremental rebuild.
    pub size_bytes: u64,
    pub mtime_s: u64,
    /// User favorite (Atlas star).
    #[serde(default)]
    pub favorite: bool,
}

/// The persisted space: items + where the feature matrix lives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Space {
    pub version: u32,
    pub name: String,
    /// Absolute root the item paths are relative to.
    pub root: String,
    /// Feature dimensionality of `features.bin`.
    pub dim: usize,
    pub items: Vec<SpaceItem>,
}

pub const SPACE_VERSION: u32 = 1;
const FEATURES_MAGIC: &[u8; 8] = b"SIGSPACE";

impl Space {
    pub fn space_dir(library_root: &Path, name: &str) -> PathBuf {
        library_root.join("Space").join(format!("{name}.space"))
    }

    pub fn load(dir: &Path) -> Result<(Self, Vec<f32>), String> {
        let space: Space = serde_json::from_slice(
            &std::fs::read(dir.join("space.json")).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let raw = std::fs::read(dir.join("features.bin")).map_err(|e| e.to_string())?;
        if raw.len() < 16 || &raw[..8] != FEATURES_MAGIC {
            return Err("bad features.bin header".into());
        }
        let dim = u32::from_le_bytes(raw[8..12].try_into().unwrap()) as usize;
        let count = u32::from_le_bytes(raw[12..16].try_into().unwrap()) as usize;
        if dim != space.dim || count != space.items.len() {
            return Err(format!(
                "features.bin shape {count}x{dim} != space.json {}x{}",
                space.items.len(),
                space.dim
            ));
        }
        let body = &raw[16..];
        if body.len() != dim * count * 4 {
            return Err("features.bin truncated".into());
        }
        let mut feats = Vec::with_capacity(dim * count);
        for c in body.chunks_exact(4) {
            feats.push(f32::from_le_bytes(c.try_into().unwrap()));
        }
        Ok((space, feats))
    }

    pub fn save(&self, dir: &Path, features: &[f32]) -> Result<(), String> {
        assert_eq!(features.len(), self.dim * self.items.len());
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        std::fs::write(
            dir.join("space.json"),
            serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let mut raw = Vec::with_capacity(16 + features.len() * 4);
        raw.extend_from_slice(FEATURES_MAGIC);
        raw.extend_from_slice(&(self.dim as u32).to_le_bytes());
        raw.extend_from_slice(&(self.items.len() as u32).to_le_bytes());
        for &f in features {
            raw.extend_from_slice(&f.to_le_bytes());
        }
        std::fs::write(dir.join("features.bin"), raw).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn burst(sr: usize, len_s: f32, f: impl Fn(usize) -> f32) -> Vec<f32> {
        (0..(sr as f32 * len_s) as usize).map(f).collect()
    }

    /// Synthetic kick (60 Hz decaying sine) vs hat (decaying noise): classes
    /// land right and each one's nearest neighbour is its own twin.
    #[test]
    fn classify_and_similar_on_synthetic_hits() {
        let sr = analyze::ANALYSIS_SR as usize;
        let kick = |detune: f32| {
            burst(sr, 0.5, move |i| {
                let t = i as f32 / sr as f32;
                (2.0 * core::f32::consts::PI * (60.0 + detune) * t).sin() * (-t * 9.0).exp()
            })
        };
        let hat = |seed: u32| {
            let s = std::cell::Cell::new(seed | 1);
            burst(sr, 0.15, move |i| {
                s.set(s.get().wrapping_mul(1664525).wrapping_add(1013904223));
                let t = i as f32 / sr as f32;
                ((s.get() >> 16) as f32 / 32768.0 - 1.0) * (-t * 40.0).exp() * 0.5
            })
        };
        let sounds = [kick(0.0), kick(3.0), hat(7), hat(99)];
        let mut feats = Vec::new();
        let mut classes = Vec::new();
        for s in &sounds {
            let a = analyze::analyze(s, s.len() as f32 / sr as f32).unwrap();
            classes.push(classify::classify(&a, "unnamed"));
            feats.extend_from_slice(&a.features);
        }
        assert_eq!(classes, ["kick", "kick", "hat-closed", "hat-closed"], "{classes:?}");
        let nn = |i: usize| knn::similar(&feats, analyze::DIM, i, 1, |_| true)[0].0;
        assert_eq!(nn(0), 1);
        assert_eq!(nn(1), 0);
        assert_eq!(nn(2), 3);
        assert_eq!(nn(3), 2);
    }

    /// Store round-trip preserves items and the feature matrix.
    #[test]
    fn store_round_trip() {
        let dir = std::env::temp_dir().join(format!("space-test-{}", std::process::id()));
        let space = Space {
            version: SPACE_VERSION,
            name: "t".into(),
            root: "/tmp".into(),
            dim: 3,
            items: vec![SpaceItem {
                path: "a.wav".into(),
                class: "kick".into(),
                x: 0.25,
                y: 0.75,
                duration_s: 0.5,
                centroid_hz: 100.0,
                rms_db: -12.0,
                percussiveness: 0.9,
                size_bytes: 42,
                mtime_s: 7,
                favorite: true,
            }],
        };
        let feats = vec![1.0f32, 2.0, 3.0];
        space.save(&dir, &feats).unwrap();
        let (loaded, lf) = Space::load(&dir).unwrap();
        assert_eq!(loaded.items[0].path, "a.wav");
        assert!(loaded.items[0].favorite);
        assert_eq!(lf, feats);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
