//! A song's reference: its whole mix but the guide, bounced to one stereo
//! stream.
//!
//! What a phone plays by default in a shared set (the web page's
//! reference mode): one proxy of a few megabytes instead of every stem.
//! The click, count and cues are left out because they play on their own
//! (the guide instrument), where each person can turn them up or down;
//! the Keyflow folder carries no audio worth hearing twice.
//!
//! The bounce is written beside the song's media the way its stems are,
//! so it syncs and streams with it:
//!
//! - `Media/Reference.wav` — 48 kHz, 32-bit float;
//! - `Media/Proxies/Reference.ogg` and its page index — what streams;
//! - `Media/Proxies/Reference Preview.ogg` and its index — the same mix at
//!   a quarter of the size (mono, 24 kHz), what a page plays first, until
//!   the reference proper has arrived where it is playing;
//! - `Media/Peaks/Reference.wav.sessionpeaks` — its waveform.
//!
//! The song itself is not changed: the reference is not a track of it,
//! so nothing about it is shared, saved or mixed by accident. A song that
//! already plays a `Media/Reference.wav` of its own (a multitrack pack's
//! reference mix) keeps it: that one is proxied, not rendered over.
//!
//! What the reference leaves out ([`left_out`]) is also what a page in
//! reference mode lets you change without the stems: those tracks play
//! live either way.

use std::collections::HashSet;
#[cfg(feature = "native")]
use std::path::{Path, PathBuf};

#[cfg(feature = "native")]
use daw::service::{Items, TrackRef};
use daw::service::{ProjectContext, Tracks};
use daw::standalone::Standalone;
use daw::standalone::audio_engine::streamed::{CHUNK, Streamed};

/// The reference proxy's file stem, lower-case, as a streamed song keys
/// its proxies (`Media/Proxies/Reference.ogg`).
pub const STEM: &str = "reference";
/// The preview proxy's file stem, lower-case.
pub const PREVIEW_STEM: &str = "reference preview";
/// The reference's sample rate: the web page's output rate, so its frames
/// line up with the playhead without resampling.
pub const RATE: u32 = 48_000;
/// The reference's name in the song's `Media` folder.
pub const NAME: &str = "Reference.wav";
/// The folders whose tracks the reference leaves out.
const LEFT_OUT: [&str; 2] = ["Guide", "Keyflow"];
/// Frames rendered at a time.
#[cfg(feature = "native")]
const BLOCK: usize = 8_192;
/// libvorbis quality of the proxy (0.4 ≈ 128 kbps stereo — the stems').
#[cfg(feature = "native")]
const QUALITY: f32 = 0.4;
/// The preview's rate, channels and quality: ~35 kbps.
#[cfg(feature = "native")]
const PREVIEW: (u32, f32) = (24_000, 0.1);

/// Bounce `song` (its `.session`, `.RPP`, or the folder holding it) to its
/// reference. Returns the reference's WAV.
///
/// # Errors
///
/// The song could not be opened, or a file could not be written.
#[cfg(feature = "native")]
pub fn bounce(song: &Path) -> eyre::Result<PathBuf> {
    let song = song_file(song)?;
    let prepare = crate::prepare::Prepare::for_song(&song);
    let daw = Standalone::new();
    crate::open::equip(&daw);
    let (opened, plan) = crate::open_core::open_song_into(&daw, &song, &prepare)?;
    let project = opened.project_guid.clone();
    let media = plan
        .open
        .parent()
        .map_or_else(|| PathBuf::from("Media"), |dir| dir.join("Media"));
    std::fs::create_dir_all(&media)?;
    let wav = media.join(NAME);
    let own = daw::standalone::audio_engine::materialize::pending_media(&daw, &project)
        .iter()
        .any(|m| {
            Path::new(&m.path)
                .file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case(NAME))
        });
    let seconds = if own {
        None
    } else {
        Some(render(&daw, &project, &wav)?)
    };

    // The proxy, written aside and moved into place (as `session proxies`
    // does), so a sync agent never picks up half of one.
    let proxy = media.join("Proxies").join("Reference.ogg");
    if let Some(dir) = proxy.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = proxy.with_extension("ogg.partial");
    fts_sample::cache::write_ogg_proxy(&wav, &partial, QUALITY)
        .map_err(|e| eyre::eyre!("{}: {e}", wav.display()))?;
    std::fs::rename(&partial, &proxy)?;
    let index = fts_sample::ogg_index::OggIndex::path_for;
    std::fs::rename(index(&partial), index(&proxy))?;

    preview(&wav, &media.join("Proxies").join("Reference Preview.ogg"))?;

    let peaks = dawfile_reaper::sessionpeaks::build(&wav)
        .map_err(|e| eyre::eyre!("{}: {e}", wav.display()))?;
    dawfile_reaper::sessionpeaks::write(&wav, &peaks)?;
    tracing::info!(
        reference.song = %song.display(),
        reference.own = own,
        reference.seconds = seconds,
        reference.proxy_kb = std::fs::metadata(&proxy).map_or(0, |m| m.len() / 1024),
        "reference bounced"
    );
    Ok(wav)
}

/// The preview proxy of `wav` at `ogg`, and its page index: resampled,
/// folded to mono, encoded small.
#[cfg(feature = "native")]
fn preview(wav: &Path, ogg: &Path) -> eyre::Result<()> {
    let (rate, quality) = PREVIEW;
    let audio = fts_sample::load_audio(wav, f64::from(rate))
        .map_err(|e| eyre::eyre!("{}: {e}", wav.display()))?;
    #[allow(clippy::cast_possible_truncation)]
    let mono: Vec<f32> = audio
        .data
        .iter()
        .map(|[l, r]| ((l + r) * 0.5) as f32)
        .collect();
    let bytes = fts_sample::cache::encode_ogg_vorbis(&mono, 1, rate, quality)
        .map_err(|e| eyre::eyre!("{}: {e}", ogg.display()))?;
    let partial = ogg.with_extension("ogg.partial");
    std::fs::write(&partial, bytes)?;
    std::fs::rename(&partial, ogg)?;
    fts_sample::cache::write_ogg_index(ogg, u64::from(rate))
        .map_err(|e| eyre::eyre!("{}: {e}", ogg.display()))?;
    Ok(())
}

/// Render `project`'s mix, less what [`left_out`] names, into `wav`.
/// Returns its length, seconds.
#[cfg(feature = "native")]
fn render(daw: &Standalone, project: &str, wav: &Path) -> eyre::Result<f64> {
    for guid in left_out(daw, project) {
        let _ = Tracks::set_muted(daw, ctx(project), TrackRef::Guid(guid), true);
    }
    let frames = (song_end(daw, project) * f64::from(RATE)).ceil() as u64;
    let renderer = daw::standalone::audio_engine::render::ProjectRenderer::new(daw, project, RATE);
    let capacity = usize::try_from(frames).unwrap_or(0);
    let (mut left, mut right) = (Vec::with_capacity(capacity), Vec::with_capacity(capacity));
    let mut at = 0u64;
    while at < frames {
        let n = BLOCK.min(usize::try_from(frames - at).unwrap_or(BLOCK));
        let block = renderer.render_block(at, n);
        for frame in block.samples.chunks_exact(2).take(n) {
            left.push(frame[0]);
            right.push(frame[1]);
        }
        at += n as u64;
    }
    fts_sample::write_wav_f32(wav, RATE, &[left, right])
        .map_err(|e| eyre::eyre!("{}: {e}", wav.display()))?;
    Ok(frames as f64 / f64::from(RATE))
}

fn ctx(project: &str) -> ProjectContext {
    ProjectContext::Project(project.to_owned())
}

/// The song file under `path`: itself, or the `.session` (else `.RPP`) in
/// the folder it names.
#[cfg(feature = "native")]
fn song_file(path: &Path) -> eyre::Result<PathBuf> {
    if !path.is_dir() || crate::open_core::is_session(path) {
        return Ok(path.to_path_buf());
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    [format!("{name}.session"), format!("{name}.RPP")]
        .into_iter()
        .map(|file| path.join(file))
        .find(|file| file.exists())
        .ok_or_else(|| eyre::eyre!("{}: no {name}.session or {name}.RPP in it", path.display()))
}

/// The tracks of `project` the reference leaves out: the folders it names
/// and everything in them, to any depth.
#[must_use]
pub fn left_out(daw: &Standalone, project: &str) -> HashSet<String> {
    let tracks = Tracks::all(daw, ctx(project));
    let mut out: HashSet<String> = tracks
        .iter()
        .filter(|t| LEFT_OUT.contains(&t.name.as_str()))
        .map(|t| t.guid.clone())
        .collect();
    loop {
        let before = out.len();
        for track in &tracks {
            if track.parent_guid.as_ref().is_some_and(|p| out.contains(p)) {
                out.insert(track.guid.clone());
            }
        }
        if out.len() == before {
            return out;
        }
    }
}

/// Add a song's reference into `buf` — interleaved, `channels` a frame, at
/// `rate` — from `at` seconds on, at the transport's `playrate`: the
/// reference proper where it has arrived at the playhead, else its
/// `preview`. Each is told that is where it is read, so both decode there.
/// Silent where neither has arrived. Allocation-free: it runs on the audio
/// thread (a device's callback, a page's audio loop).
pub fn mix(
    full: &Streamed,
    preview: Option<&Streamed>,
    at: f64,
    rate: f64,
    playrate: f64,
    buf: &mut [f32],
    channels: usize,
) {
    let channels = channels.max(1);
    let frames = buf.len() / channels;
    let here = |s: &Streamed| (at.max(0.0) * f64::from(s.sample_rate())) as u64;
    full.want(here(full));
    if let Some(preview) = preview {
        preview.want(here(preview));
    }
    let arrived = |s: &Streamed| {
        let chunk = |frame: u64| usize::try_from(frame).unwrap_or(usize::MAX) / CHUNK;
        let span = frames as f64 * f64::from(s.sample_rate()) / rate.max(1.0) * playrate;
        let last = here(s) + span as u64;
        s.resident(chunk(here(s))) && s.resident(chunk(last.min(s.frames().saturating_sub(1))))
    };
    let source = match preview {
        Some(preview) if !arrived(full) => preview,
        _ => full,
    };
    mix_in(source, at, rate, playrate, buf, channels);
}

/// Add `source` from `at` seconds into `buf` — read at its own rate between
/// samples, a mono one to both sides, and both sides folded into a mono
/// output.
fn mix_in(source: &Streamed, at: f64, rate: f64, playrate: f64, buf: &mut [f32], channels: usize) {
    let source_rate = f64::from(source.sample_rate().max(1));
    let step = source_rate / rate.max(1.0) * playrate;
    let right = usize::from(source.channels() > 1);
    let from = at.max(0.0) * source_rate;
    for (i, out) in buf.chunks_exact_mut(channels).enumerate() {
        let position = from + i as f64 * step;
        let frame = position as usize;
        let frac = (position - frame as f64) as f32;
        let read = |ch: usize| {
            let a = source.sample(frame, ch);
            a + (source.sample(frame + 1, ch) - a) * frac
        };
        let (l, r) = (read(0), read(right));
        match out {
            [mono] => *mono += (l + r) * 0.5,
            [left, right, ..] => {
                *left += l;
                *right += r;
            }
            [] => {}
        }
    }
}

/// Where the song's last item ends, in seconds.
#[cfg(feature = "native")]
fn song_end(daw: &Standalone, project: &str) -> f64 {
    Items::get_all_items(daw, ProjectContext::Project(project.to_owned()))
        .iter()
        .map(|item| item.position.as_seconds() + item.length.as_seconds())
        .fold(0.0, f64::max)
}
