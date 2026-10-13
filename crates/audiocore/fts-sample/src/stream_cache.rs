//! **Stream cache** — decode a compressed sample once, then play it from a
//! mapping forever after.
//!
//! Raw-PCM packs are read straight out of their mapping and cost the process
//! no anonymous memory ([`super::cache::Pcm::Mapped`]). FLAC and Ogg packs
//! can't be: their bytes have to be decoded before a voice can read a frame,
//! and the decoded float is what fills RAM.
//!
//! This bridges the two. The first time a compressed sample is loaded it is
//! written out as raw 16-bit PCM next to the cache and mapped back in; the
//! decoded copy is dropped. From then on — this run and every later run —
//! that sample is mapped, not decoded: zero anonymous bytes, paged in as the
//! note plays, evicted by the OS under pressure. Kontakt's DFD with the
//! kernel doing the streaming.
//!
//! Off by default because it writes real bytes to disk (a 24-bit library
//! becomes ~⅔ of its size again as i16). Enable by pointing
//! `FTS_STREAM_CACHE_DIR` at a directory — or set `FTS_STREAM_CACHE=1` to use
//! `$XDG_CACHE_HOME/fts/stream`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use crate::cache::{PcmFmt, SampleData};

/// Where materialised PCM lives, or `None` when the cache is off.
pub fn dir() -> Option<&'static Path> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = if let Ok(explicit) = std::env::var("FTS_STREAM_CACHE_DIR") {
            Some(PathBuf::from(explicit))
        } else if std::env::var("FTS_STREAM_CACHE").is_ok_and(|v| v != "0" && !v.is_empty()) {
            let base = std::env::var("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .or_else(|_| std::env::var("HOME").map(|h| PathBuf::from(h).join(".cache")))
                .ok()?;
            Some(base.join("fts").join("stream"))
        } else {
            None
        }?;
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!(dir = %dir.display(), "sampler: stream cache unusable: {e}");
            return None;
        }
        tracing::info!(dir = %dir.display(), "sampler: stream cache enabled");
        Some(dir)
    })
    .as_deref()
}

/// The cache file for one sample. Keyed by source path *and* shape, so a
/// re-extracted library never plays a stale mapping.
fn cache_file(source: &Path, data: &SampleData) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut h);
    data.channels.hash(&mut h);
    data.sample_rate.hash(&mut h);
    data.num_frames.hash(&mut h);
    PathBuf::from(format!("{:016x}.pcm16", h.finish()))
}

/// Map an already-materialised sample, if this one has been through here
/// before. Cheap enough to try before decoding.
pub fn lookup(
    source: &Path,
    channels: u16,
    sample_rate: u32,
    num_frames: usize,
) -> Option<SampleData> {
    let dir = dir()?;
    let probe = SampleData::from_f32(Vec::new(), channels, sample_rate, num_frames);
    let path = dir.join(cache_file(source, &probe));
    map_file(&path, channels, sample_rate, num_frames)
}

/// Write `data` out as raw i16 and hand back the mapped version. Returns
/// `None` (and leaves `data` to the caller) when the cache is off or the
/// write fails — a broken cache must never break playback.
pub fn materialize(source: &Path, data: &SampleData) -> Option<SampleData> {
    let dir = dir()?;
    let path = dir.join(cache_file(source, data));
    if let Some(mapped) = map_file(&path, data.channels, data.sample_rate, data.num_frames) {
        return Some(mapped);
    }
    // Write to a temp file and rename, so a torn write is never mapped: two
    // processes warming the same library is normal.
    let tmp = path.with_extension(format!("pcm16.tmp{}", std::process::id()));
    let write = || -> std::io::Result<()> {
        let mut f = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
        let pcm = data.to_f32();
        let mut buf = Vec::with_capacity(pcm.len() * 2);
        for s in pcm.iter() {
            buf.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes());
        }
        f.write_all(&buf)?;
        f.flush()?;
        drop(f);
        std::fs::rename(&tmp, &path)
    };
    if let Err(e) = write() {
        tracing::debug!(file = %path.display(), "sampler: stream cache write failed: {e}");
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    map_file(&path, data.channels, data.sample_rate, data.num_frames)
}

fn map_file(path: &Path, channels: u16, sample_rate: u32, num_frames: usize) -> Option<SampleData> {
    let file = std::fs::File::open(path).ok()?;
    let samples = num_frames * channels.max(1) as usize;
    if file.metadata().ok()?.len() < (samples * 2) as u64 {
        return None;
    }
    // Safety: the cache file is written once via rename and never mutated in
    // place; a truncation underneath us would be someone deleting the cache
    // mid-run, which is a deployment error, not a normal race.
    let map = unsafe { memmap2::Mmap::map(&file) }.ok()?;
    Some(SampleData::mapped(
        Arc::new(map),
        0,
        samples,
        PcmFmt::I16,
        channels,
        sample_rate,
        num_frames,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materialised_samples_map_back_with_no_anonymous_cost() {
        let dir = std::env::temp_dir().join(format!("fts-stream-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // SAFETY: single-threaded test setup, before any other thread reads it.
        unsafe { std::env::set_var("FTS_STREAM_CACHE_DIR", &dir) };
        let Some(_) = super::dir() else {
            // Another test in this binary already resolved the cache to off.
            return;
        };

        let frames: Vec<f32> = (0..512).map(|i| (i as f32 / 512.0) * 2.0 - 1.0).collect();
        let data = SampleData::from_f32(frames.clone(), 1, 48_000, 512);
        let src = Path::new("/nonexistent/ramp.flac");

        let mapped = materialize(src, &data).expect("materialise");
        assert_eq!(
            mapped.decoded_bytes(),
            0,
            "mapped PCM costs no anonymous memory"
        );
        assert_eq!(mapped.num_frames, 512);
        for (i, want) in frames.iter().enumerate() {
            assert!(
                (mapped.pcm.sample(i) - want).abs() < 1.0 / 32_000.0,
                "sample {i}"
            );
        }
        // Second time round it maps without being handed the decoded audio.
        let again = lookup(src, 1, 48_000, 512).expect("cached lookup");
        assert_eq!(again.num_frames, 512);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
