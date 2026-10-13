//! `session peaks` — the waveform cache for a session's media, written
//! beside it: `Media/Bass.wav` → `Media/Peaks/Bass.wav.sessionpeaks`.
//!
//! The same shape as [`crate::proxies`], for the same reason: a cache
//! that lives in the session folder is versioned with the session,
//! synced with it, and *served* with it. A share link hands out any
//! non-media file whole, so the browser fetches
//! `<link>/doc/Media/Peaks/Bass.wav.sessionpeaks` and draws the real
//! waveform without downloading a note of audio — a 7-minute stereo stem
//! at 44.1 kHz is 111 MB of WAV, 5 MB of Ogg proxy, and 1 MB of peaks,
//! of which the two coarse levels a whole-song view draws from are the
//! last 70 KB.
//!
//! The file is REAPER's own format (see
//! `dawfile_reaper::sessionpeaks`), which buys the best case for free: a
//! session REAPER has already scanned has `Media/peaks/*.reapeaks`
//! sitting there, and those bytes are simply adopted rather than
//! recomputed.
//!
//! Where there is no WAV — a session that travels as proxies only — the
//! peaks are built from the Ogg. Lossy compression moves a peak by a
//! fraction of a dB, which is nothing a pixel column can show.

use std::path::{Path, PathBuf};
use std::time::Instant;

use dawfile_reaper::reapeaks::ReaPeaks;
use dawfile_reaper::sessionpeaks;

use crate::proxies::{project_file, proxy_of, sources};

/// What happened to one media file.
enum Outcome {
    /// Our cache was already there and still matches the media.
    Current,
    /// REAPER's own cache was there and still matches: copied across,
    /// not recomputed.
    Adopted(PathBuf),
    /// Scanned. Carries what was scanned (the WAV, or the proxy).
    Built(PathBuf),
    /// Nothing to scan, and no cache to keep.
    Skipped(String),
}

/// The cache for one media file, by the cheapest route that is still
/// correct: keep, adopt, or scan.
fn one(media: &Path, force: bool) -> eyre::Result<(Outcome, PathBuf)> {
    let cache = sessionpeaks::cache_path(media)
        .ok_or_else(|| eyre::eyre!("{} has no peaks folder", media.display()))?;
    let exists = media.is_file();

    if !force {
        // Ours, still matching — or, when the media has travelled away
        // and only the cache is left, ours at all: there is nothing to
        // validate against and nothing better to do than keep it.
        if let Ok(peaks) = ReaPeaks::read(&cache)
            && (!exists || sessionpeaks::is_current(&peaks, media))
        {
            return Ok((Outcome::Current, cache));
        }
        // REAPER's, still matching: the same bytes under another name.
        if exists {
            for candidate in sessionpeaks::candidates(media).into_iter().skip(1) {
                if let Ok(peaks) = ReaPeaks::read(&candidate)
                    && sessionpeaks::is_current(&peaks, media)
                {
                    let at = sessionpeaks::write(media, &peaks)?;
                    return Ok((Outcome::Adopted(candidate), at));
                }
            }
        }
    }

    // Scan. The WAV when it is here, the proxy when it is not — a
    // proxy-built cache is stamped with the WAV's size and mtime when
    // there is a WAV, so the next run can tell whether it is still good.
    let proxy = proxy_of(media).filter(|p| p.is_file());
    let (scanned, mut peaks) = if exists {
        (media.to_path_buf(), sessionpeaks::build(media)?)
    } else if let Some(proxy) = proxy {
        let bytes: std::sync::Arc<[u8]> = std::fs::read(&proxy)?.into();
        (proxy, sessionpeaks::build_from_ogg(bytes)?)
    } else {
        return Ok((Outcome::Skipped("no media and no proxy".to_owned()), cache));
    };
    peaks.source_stamp = sessionpeaks::media_stamp(media).unwrap_or(peaks.source_stamp);
    let at = sessionpeaks::write(media, &peaks)?;
    Ok((Outcome::Built(scanned), at))
}

/// Write the peaks cache for every source a session's project names.
///
/// # Errors
///
/// The project could not be read, or a scan failed.
pub fn write(session: &Path, force: bool) -> eyre::Result<()> {
    let project = project_file(session)?;
    let media = sources(&project)?;
    let started = Instant::now();

    let workers = std::thread::available_parallelism()
        .map_or(4, usize::from)
        .min(8);
    let queue = std::sync::Mutex::new(media.into_iter());
    let done = std::sync::Mutex::new(Vec::new());
    let failures = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let next = queue.lock().expect("queue").next();
                    let Some(media) = next else { break };
                    match one(&media, force) {
                        Ok(result) => done.lock().expect("done").push((media, result)),
                        Err(e) => failures
                            .lock()
                            .expect("failures")
                            .push(format!("{}: {e}", media.display())),
                    }
                }
            });
        }
    });

    let mut done = done.into_inner().expect("done");
    done.sort_by(|a, b| a.0.cmp(&b.0));
    let mut bytes = 0u64;
    let (mut built, mut adopted, mut kept, mut skipped) = (0, 0, 0, 0);
    for (media, (outcome, cache)) in &done {
        let size = std::fs::metadata(cache).map_or(0, |m| m.len());
        bytes += size;
        match outcome {
            Outcome::Current => {
                kept += 1;
                println!("{}  up to date", cache.display());
            }
            Outcome::Adopted(from) => {
                adopted += 1;
                println!("{}  adopted {}", cache.display(), from.display());
            }
            Outcome::Built(from) => {
                built += 1;
                let name = from
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into());
                println!("{}  {} KB, scanned {name}", cache.display(), size / 1024);
            }
            Outcome::Skipped(why) => {
                skipped += 1;
                println!("{}  skipped — {why}", media.display());
            }
        }
    }
    println!(
        "{} sources: {built} scanned, {adopted} adopted, {kept} kept, {skipped} skipped — \
         {} KB of peaks in {:.1}s",
        done.len(),
        bytes / 1024,
        started.elapsed().as_secs_f64(),
    );

    let failures = failures.into_inner().expect("failures");
    if failures.is_empty() {
        Ok(())
    } else {
        Err(eyre::eyre!(
            "{} sources failed:\n{}",
            failures.len(),
            failures.join("\n")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16-bit mono WAV of `frames` at 44.1 kHz — a rate that matters,
    /// because REAPER's finest level is `sr/300` and 44.1 kHz is 147
    /// rather than the 160 a 48 kHz session gets.
    fn write_wav(path: &Path, frames: usize) {
        const RATE: u32 = 44_100;
        let mut data = Vec::with_capacity(44 + frames * 2);
        data.extend_from_slice(b"RIFF");
        data.extend_from_slice(&((36 + frames * 2) as u32).to_le_bytes());
        data.extend_from_slice(b"WAVEfmt ");
        data.extend_from_slice(&16u32.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&RATE.to_le_bytes());
        data.extend_from_slice(&(RATE * 2).to_le_bytes());
        data.extend_from_slice(&2u16.to_le_bytes());
        data.extend_from_slice(&16u16.to_le_bytes());
        data.extend_from_slice(b"data");
        data.extend_from_slice(&((frames * 2) as u32).to_le_bytes());
        for i in 0..frames {
            let t = i as f64 / f64::from(RATE);
            let s = 0.8 * (t * 220.0 * std::f64::consts::TAU).sin();
            data.extend_from_slice(&((s * f64::from(i16::MAX)) as i16).to_le_bytes());
        }
        std::fs::write(path, data).expect("wav");
    }

    fn session(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("session-peaks-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Media")).expect("dir");
        std::fs::write(
            dir.join("Song.RPP"),
            "<REAPER_PROJECT\n  <SOURCE WAVE\n    FILE \"Media/Bass.wav\"\n  >\n>\n",
        )
        .expect("rpp");
        dir
    }

    #[test]
    fn a_session_gets_one_cache_per_source_and_keeps_it() {
        let dir = session("keep");
        let media = dir.join("Media").join("Bass.wav");
        write_wav(&media, 44_100);

        write(&dir, false).expect("first pass");
        let cache = dir.join("Media/Peaks/Bass.wav.sessionpeaks");
        assert!(cache.is_file(), "{} was not written", cache.display());

        let peaks = ReaPeaks::read(&cache).expect("parses");
        assert_eq!(peaks.samplerate, 44_100);
        assert_eq!(peaks.channels, 1);
        // REAPER's 44.1 kHz ladder, not a hard-coded 48 kHz one.
        assert_eq!(peaks.levels[0].samples_per_peak, 147);
        assert_eq!(peaks.levels[1].samples_per_peak, 2205);
        assert_eq!(peaks.levels[2].samples_per_peak, 44_100);
        assert!(sessionpeaks::is_current(&peaks, &media));
        let columns = peaks.columns(0, 0.0, 1.0, 64);
        assert!(columns.iter().all(|(max, min)| *max > 0.7 && *min < -0.7));

        // A second pass keeps it: same bytes, and `one` says so.
        let before = std::fs::read(&cache).expect("bytes");
        let (outcome, _) = one(&media, false).expect("second pass");
        assert!(matches!(outcome, Outcome::Current));
        assert_eq!(std::fs::read(&cache).expect("bytes"), before);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The interchange, the way round that saves the most work: a
    /// session REAPER has already scanned needs no scan at all.
    #[test]
    fn a_reaper_cache_is_adopted_rather_than_recomputed() {
        let dir = session("adopt");
        let media = dir.join("Media").join("Bass.wav");
        write_wav(&media, 44_100);

        // Stand in for REAPER: a cache in its folder, with its
        // stamp, whose peaks are a flat 0.25 — nothing like the 0.8
        // sine on disk, so adopting it is visible in the values.
        let mut theirs = ReaPeaks::compute(1, 44_100, 44_100, |_, _| 0.25);
        theirs.source_stamp = sessionpeaks::media_stamp(&media).expect("stamp");
        let folder = dir.join("Media").join("peaks");
        std::fs::create_dir_all(&folder).expect("dir");
        theirs
            .write(folder.join("Bass.wav.reapeaks"))
            .expect("their cache");

        let (outcome, cache) = one(&media, false).expect("adopt");
        assert!(matches!(outcome, Outcome::Adopted(_)), "did not adopt");
        let peaks = ReaPeaks::read(&cache).expect("parses");
        let (max, _) = peaks.levels[0].pair(1, 0, 10);
        assert!(
            (max - 0.25).abs() < 2e-3,
            "recomputed instead of adopted: {max}"
        );

        // `--force` scans anyway, and then the real audio shows up.
        let (outcome, cache) = one(&media, true).expect("force");
        assert!(matches!(outcome, Outcome::Built(_)));
        let peaks = ReaPeaks::read(&cache).expect("parses");
        // Over the whole second, not one 147-frame window: at 220 Hz a
        // single window need not contain a crest.
        let (max, min) = peaks.columns(0, 0.0, 1.0, 1)[0];
        assert!(
            max > 0.75 && min < -0.75,
            "forced scan carries the real audio: {max}/{min}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Touching the media invalidates the cache — the whole point of
    /// carrying REAPER's size-and-mtime stamp.
    #[test]
    fn a_changed_source_is_rescanned() {
        let dir = session("stale");
        let media = dir.join("Media").join("Bass.wav");
        write_wav(&media, 44_100);
        let (_, cache) = one(&media, false).expect("first");
        let before = ReaPeaks::read(&cache).expect("parses");

        // A different length is a different size, so the stamp moves
        // even on a filesystem with one-second mtimes.
        write_wav(&media, 22_050);
        let (outcome, cache) = one(&media, false).expect("second");
        assert!(matches!(outcome, Outcome::Built(_)), "stale cache was kept");
        let after = ReaPeaks::read(&cache).expect("parses");
        assert_eq!(after.levels[0].count, 22_050usize.div_ceil(147));
        assert_ne!(before.levels[0].count, after.levels[0].count);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A session that travelled as proxies only: no WAV, so the Ogg is
    /// what gets scanned, and the cache is still named after the WAV
    /// because that is the name the project — and the browser — uses.
    #[test]
    fn a_proxy_only_session_is_scanned_from_the_proxy() {
        let dir = session("proxy");
        let media = dir.join("Media").join("Bass.wav");
        write_wav(&media, 44_100);
        let proxy = proxy_of(&media).expect("proxy path");
        std::fs::create_dir_all(proxy.parent().expect("parent")).expect("dir");
        fts_sample::cache::write_ogg_proxy(&media, &proxy, 0.4).expect("proxy");
        std::fs::remove_file(&media).expect("drop the wav");

        let (outcome, cache) = one(&media, false).expect("proxy scan");
        match &outcome {
            Outcome::Built(from) => assert_eq!(from, &proxy),
            _ => panic!("expected a scan of the proxy"),
        }
        assert!(cache.ends_with("Media/Peaks/Bass.wav.sessionpeaks"));
        let peaks = ReaPeaks::read(&cache).expect("parses");
        assert_eq!(peaks.samplerate, 44_100);
        let columns = peaks.columns(0, 0.0, 0.9, 32);
        assert!(columns.iter().all(|(max, min)| *max > 0.6 && *min < -0.6));

        // With the WAV gone there is nothing to validate against, so the
        // next run keeps what it has rather than rescanning forever.
        let (outcome, _) = one(&media, false).expect("second");
        assert!(matches!(outcome, Outcome::Current));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
