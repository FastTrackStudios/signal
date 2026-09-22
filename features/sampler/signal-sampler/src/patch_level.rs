//! Levelling patches against each other: how loud a whole patch actually is.
//!
//! # Why the rig needed a third loudness mechanism
//!
//! There were already two, and neither answers this question.
//!
//! The **per-block drive calibration** ([`crate::nam_calibrate`]) holds each
//! capture at unity as its drive knob moves, so turning a pedal up adds dirt
//! rather than volume. It is about one block over its own range.
//!
//! The **patch level-match** in [`crate::rig_profile`] trims by the *amp
//! capture's* measured loudness. That is one block of a chain of fifteen, and
//! it double-counts what the drive calibration has already done — which is why
//! the guitar rig switches it off.
//!
//! Neither of them can know what a patch sounds like, because a patch's
//! loudness is a property of the whole chain: how many gain stages are
//! stacked, where the EQ sits, how hard the compressor is working, how much
//! reverb is in the mix. A clean patch and a high-gain patch built from
//! individually-levelled blocks still arrive at the listener several dB apart.
//! So the only honest measurement is to render the patch and listen to it.
//!
//! # How
//!
//! Every patch is fed the same DI reference — the player's own capture when
//! they have installed one, else the deterministic synthetic guitar — through
//! its complete chain, and the result is measured as integrated LUFS, the same
//! unit the rest of the calibration speaks. A patch's trim is then simply the
//! distance from that to the target.
//!
//! This is offline and slow (a NAM block is far from realtime, and there are
//! several per patch), so it is cached: keyed on what the chain *is*, so an
//! edit to a patch re-measures that patch and nothing else.

use std::path::PathBuf;

use facet::Facet;

use crate::loudness::{SILENCE_LUFS, integrated_lufs};
use crate::nam_calibrate::{DiReference, hash_file};
use crate::rig::{RigBlock, build_block};

/// Block size for the offline render. Large, because nothing here is realtime
/// and a bigger block is fewer per-block overheads over ~2 seconds of audio.
const RENDER_BLOCK: usize = 512;

/// The loudness target patches are levelled to, LUFS.
///
/// −18 LUFS is the same target the patch level-match used, and it leaves
/// headroom for the peaks a guitar makes above its integrated level.
pub const TARGET_LUFS: f64 = -18.0;

/// A patch's measured loudness, cached.
#[derive(Clone, Debug, Facet)]
pub struct PatchLevelEntry {
    /// What was measured — identifies the chain, not the patch's name, so
    /// renaming a patch does not throw the measurement away and editing one
    /// does.
    pub chain_hash: String,
    /// Which DI it was measured against.
    pub di_id: String,
    pub sample_rate: u32,
    /// Integrated loudness of the rendered patch, LUFS.
    pub lufs: f64,
}

#[derive(Clone, Debug, Default, Facet)]
pub struct PatchLevelCache {
    pub entries: Vec<PatchLevelEntry>,
}

impl PatchLevelCache {
    #[must_use]
    pub fn path() -> PathBuf {
        crate::nam_calibrate::calibration_dir().join("patch-levels.styx")
    }

    #[must_use]
    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|t| facet_styx::from_str::<Self>(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = facet_styx::to_string(self) {
            let _ = std::fs::write(path, text);
        }
    }

    #[must_use]
    pub fn lookup(&self, chain_hash: &str, di_id: &str, sample_rate: u32) -> Option<&PatchLevelEntry> {
        self.entries.iter().find(|e| {
            e.chain_hash == chain_hash && e.di_id == di_id && e.sample_rate == sample_rate
        })
    }

    fn insert(&mut self, entry: PatchLevelEntry) {
        self.entries.retain(|e| {
            !(e.chain_hash == entry.chain_hash
                && e.di_id == entry.di_id
                && e.sample_rate == entry.sample_rate)
        });
        self.entries.push(entry);
    }
}

fn cache() -> &'static std::sync::Mutex<PatchLevelCache> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<PatchLevelCache>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(PatchLevelCache::load()))
}

/// What a chain *is*, for cache purposes.
///
/// Every block's asset and its parameters, in order — so a different capture,
/// a moved knob or a reordered chain all read as a different chain, while the
/// same chain under another name reads as the same one. Capture files
/// contribute their content hash (via the memo, so this is cheap after the
/// first pass) rather than their path, keeping a measurement valid when a
/// library is reorganised on disk.
#[must_use]
pub fn chain_hash(blocks: &[RigBlock]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    for b in blocks {
        h.update(b.name.as_bytes());
        h.update([0]);
        h.update(format!("{:?}", b.block_type).as_bytes());
        h.update([0]);
        h.update([u8::from(b.bypassed)]);
        let asset = b.asset_path();
        if asset.is_empty() {
            h.update([0]);
        } else {
            // The capture's content, not where it sits.
            match hash_file(std::path::Path::new(&asset)) {
                Ok(digest) => h.update(digest.as_bytes()),
                Err(_) => h.update(asset.as_bytes()),
            }
        }
        h.update([0]);
        // Parameters, in a stable order — a knob move is a different chain.
        let mut params: Vec<(&str, &str)> = b
            .params
            .iter()
            .map(|p| (p.name.as_str(), p.value.as_str()))
            .collect();
        params.sort_unstable();
        for (name, value) in params {
            h.update(name.as_bytes());
            h.update([b'=']);
            h.update(value.as_bytes());
            h.update([b';']);
        }
        h.update([0xff]);
    }
    let digest = h.finalize();
    let mut out = String::with_capacity(32);
    for byte in &digest[..16] {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Render `blocks` in series against the DI and return integrated LUFS.
///
/// `None` when a block cannot be built — a missing capture, say. Silence
/// reports [`SILENCE_LUFS`] rather than `None`: a patch that is inaudible is a
/// measurement, not a failure, and levelling it up to the target would be the
/// wrong answer either way.
#[must_use]
pub fn measure_chain_lufs(blocks: &[RigBlock], sample_rate: u32) -> Option<f64> {
    let di = DiReference::load_or_synthetic(f64::from(sample_rate));
    render_lufs(blocks, sample_rate, &di)
}

fn render_lufs(blocks: &[RigBlock], sample_rate: u32, di: &DiReference) -> Option<f64> {
    let sr = f64::from(sample_rate);

    // Only the blocks that are actually in the signal path.
    //
    // A patch's chain holds every slot the rig can offer — the gate, six
    // modulation blocks, two delays, two reverbs — and a patch engages a
    // handful of them. Rendering the bypassed ones too does not merely add
    // error: the gate, given a DI it was not set up for, closes and the whole
    // chain measures as silence. A bypassed block passes its input through, so
    // skipping it is exactly equivalent and cheaper.
    let path: Vec<&RigBlock> = blocks.iter().filter(|b| !b.bypassed).collect();

    let mut boxes = Vec::with_capacity(path.len());
    for b in &path {
        let mut built = build_block(b, sample_rate).ok()?;
        built.boxed.prepare(sr, RENDER_BLOCK as u32).ok()?;
        boxes.push(built.boxed);
    }

    // The DI is mono; the chain is stereo from wherever it widens, so carry
    // both and measure their mean — that is what a listener on two speakers
    // gets, and it keeps a stereo reverb from reading quieter than a mono one.
    let n = di.samples.len();
    let mut l: Vec<f32> = di.samples.iter().map(|&s| s as f32).collect();
    let mut r = l.clone();
    let mut out_l = vec![0.0f32; RENDER_BLOCK];
    let mut out_r = vec![0.0f32; RENDER_BLOCK];

    for (index, boxed) in boxes.iter_mut().enumerate() {
        let mut pos = 0;
        while pos < n {
            let len = RENDER_BLOCK.min(n - pos);
            if boxed
                .process_block(
                    &l[pos..pos + len],
                    &r[pos..pos + len],
                    &mut out_l[..len],
                    &mut out_r[..len],
                    &signal_plugin_host::PluginEvents::EMPTY,
                )
                .is_err()
            {
                return None;
            }
            l[pos..pos + len].copy_from_slice(&out_l[..len]);
            r[pos..pos + len].copy_from_slice(&out_r[..len]);
            pos += len;
        }
        // Per-block, because "the whole chain measured silent" does not say
        // which block silenced it, and that is the only thing worth knowing.
        if tracing::enabled!(tracing::Level::DEBUG) {
            let peak = l.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            tracing::debug!(
                slot = index,
                block = %path.get(index).map_or("?", |b| b.name.as_str()),
                peak,
                "patch level: after block"
            );
        }
    }

    let mixed: Vec<f64> = l
        .iter()
        .zip(&r)
        .map(|(&a, &b)| f64::midpoint(f64::from(a), f64::from(b)))
        .collect();
    let lufs = integrated_lufs(&mixed, di.sample_rate);
    Some(if lufs.is_finite() { lufs } else { SILENCE_LUFS })
}

/// The output trim, in dB, that puts `blocks` at [`TARGET_LUFS`].
///
/// Measured once per distinct chain and cached; a second call for the same
/// chain is a lookup. `None` when the chain cannot be rendered.
#[must_use]
pub fn trim_for_target(blocks: &[RigBlock], sample_rate: u32, target_lufs: f64) -> Option<f32> {
    let lufs = level_of(blocks, sample_rate)?;
    // A patch measured as silent gets no trim: +78 dB of makeup on silence
    // produces nothing but noise when something does arrive.
    if lufs <= SILENCE_LUFS {
        return Some(0.0);
    }
    Some((target_lufs - lufs) as f32)
}

/// A patch's loudness, from the cache or by rendering it.
#[must_use]
pub fn level_of(blocks: &[RigBlock], sample_rate: u32) -> Option<f64> {
    let di = DiReference::load_or_synthetic(f64::from(sample_rate));
    // Calibration changes what a chain sounds like without changing the
    // chain, so it is part of what was measured.
    let hash = match crate::nam::interface_calibration_dbu() {
        Some(cal) => format!("{}-cal{cal}", chain_hash(blocks)),
        None => chain_hash(blocks),
    };
    {
        let cache = cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(hit) = cache.lookup(&hash, &di.id, sample_rate) {
            return Some(hit.lufs);
        }
    }
    tracing::info!(blocks = blocks.len(), "patch level: rendering");
    let lufs = render_lufs(blocks, sample_rate, &di)?;
    // Silence is not a measurement worth keeping. A chain that renders to
    // nothing means something is wrong with the render, not that the patch is
    // silent, and caching it would make the bug permanent and invisible.
    if !lufs.is_finite() || lufs <= SILENCE_LUFS {
        tracing::warn!(lufs, blocks = blocks.len(), "patch level: rendered silent — not caching");
        return Some(lufs);
    }
    {
        let mut cache = cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        cache.insert(PatchLevelEntry {
            chain_hash: hash,
            di_id: di.id.clone(),
            sample_rate,
            lufs,
        });
        cache.save();
    }
    Some(lufs)
}

/// Forget every measurement — for when the DI changes under them.
pub fn clear_cache() {
    let mut cache = cache().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    *cache = PatchLevelCache::default();
    let _ = std::fs::remove_file(PatchLevelCache::path());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(name: &str, gain: f32) -> RigBlock {
        RigBlock::effect(signal_proto::block::BlockType::Boost, name).with_param("gain", gain.to_string())
    }

    /// A chain hashes by what it is, not what it is called: renaming a patch
    /// keeps its measurement, moving a knob throws it away.
    #[test]
    fn a_chain_is_identified_by_its_content() {
        let a = vec![block("Boost", 0.5)];
        let same = vec![block("Boost", 0.5)];
        assert_eq!(chain_hash(&a), chain_hash(&same));

        let moved = vec![block("Boost", 0.6)];
        assert_ne!(
            chain_hash(&a),
            chain_hash(&moved),
            "a knob move is a different chain"
        );

        let longer = vec![block("Boost", 0.5), block("Boost 2", 0.5)];
        assert_ne!(chain_hash(&a), chain_hash(&longer));
    }

    /// A chain that passes audio measures as audio. Guards the render loop
    /// itself: every patch reading −inf means the measurement is broken, not
    /// that thirteen patches are silent.
    #[test]
    fn a_chain_that_passes_audio_is_not_silent() {
        let di = DiReference::synthetic(48_000.0);
        assert!(!di.samples.is_empty(), "the DI must have samples");
        let di_lufs = integrated_lufs(&di.samples, di.sample_rate);
        assert!(
            di_lufs.is_finite() && di_lufs > SILENCE_LUFS,
            "the DI itself measures {di_lufs}"
        );

        // A single unity boost: whatever goes in comes out.
        let chain = vec![block("Boost", 0.5)];
        let lufs = render_lufs(&chain, 48_000, &di).expect("chain renders");
        assert!(
            lufs.is_finite() && lufs > SILENCE_LUFS,
            "a unity chain measured {lufs}"
        );
    }

    /// Order matters — the same blocks in a different order is a different
    /// sound, so it must not share a measurement.
    #[test]
    fn order_is_part_of_the_chain() {
        let ab = vec![block("A", 0.2), block("B", 0.8)];
        let ba = vec![block("B", 0.8), block("A", 0.2)];
        assert_ne!(chain_hash(&ab), chain_hash(&ba));
    }

    /// The trim is the distance to the target, so a patch already at the
    /// target needs none.
    #[test]
    fn the_trim_is_the_distance_to_the_target() {
        let cache_key = "test";
        let mut cache = PatchLevelCache::default();
        cache.insert(PatchLevelEntry {
            chain_hash: cache_key.to_string(),
            di_id: "di".to_string(),
            sample_rate: 48_000,
            lufs: -24.0,
        });
        let hit = cache.lookup(cache_key, "di", 48_000).expect("inserted");
        assert!((TARGET_LUFS - hit.lufs - 6.0).abs() < 1e-9, "−24 needs +6");
        // A second insert for the same chain replaces rather than accumulates.
        cache.insert(PatchLevelEntry {
            chain_hash: cache_key.to_string(),
            di_id: "di".to_string(),
            sample_rate: 48_000,
            lufs: -20.0,
        });
        assert_eq!(cache.entries.len(), 1);
        assert!((cache.lookup(cache_key, "di", 48_000).expect("replaced").lufs + 20.0).abs() < 1e-9);
    }
}
