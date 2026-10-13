//! Reading WAV headers, because the kit pages do not state them.
//!
//! None of the three DrumGizmo wiki pages says a sample rate or a bit
//! depth, so #158 could only record the question. The answer, read off
//! the actual headers:
//!
//! | Kit | Rate | Sample format | Channels per file |
//! |---|---|---|---|
//! | CrocellKit 1.1 | 48 000 Hz | 32-bit IEEE float | 15 |
//! | DRSKit 2.1 | 44 100 Hz | 32-bit IEEE float | 13 |
//!
//! Three consequences, each of which would have bitten later:
//!
//! 1. **The two kits are at different rates.** A harness that assumes
//!    one rate per corpus is wrong on its second kit — the same finding
//!    the material inventory (#159) made about the demo project.
//! 2. **The samples are float, not integer.** `format` here reports
//!    that, because a reader that assumes PCM silently produces noise.
//! 3. **One file per hit holds the whole array**, interleaved. So a
//!    "sample" is a 15-channel object and the bleed is inside it; there
//!    is no per-mic file to load in isolation.
//!
//! The parser itself now lives in `fts_sample::probe` (it was ported
//! there verbatim — chunk walk, `WAVE_FORMAT_EXTENSIBLE` resolution and
//! all); this module keeps the corpus-shaped view of it: a fmt-only
//! [`WavHeader`] that [`summarize`] can group on, plus per-channel
//! reading. Probing reads headers and nothing else — never audio — so
//! probing a whole kit is a few thousand short seeks rather than
//! gigabytes of I/O.

use std::io;
use std::path::{Path, PathBuf};

/// How samples are encoded — `fts_sample`'s tag enum (`Pcm`, `Float`,
/// `Extensible`, `Other`), re-exported under the name the corpus API
/// always had.
pub use fts_sample::WavFormat as Format;

/// What a WAV file's `fmt ` chunk says.
///
/// Deliberately excludes the data length: [`summarize`] groups files by
/// this header, and every file in a kit has its own length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WavHeader {
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    pub format: Format,
}

/// Read one file's header (via `fts_sample::probe` — no audio decode).
pub fn probe(path: &Path) -> io::Result<WavHeader> {
    let info = fts_sample::probe(path).map_err(|e| match e {
        fts_sample::SamplerError::Io(e) => e,
        other => io::Error::new(io::ErrorKind::InvalidData, other.to_string()),
    })?;
    Ok(WavHeader {
        channels: info.channels,
        sample_rate: info.sample_rate,
        bits_per_sample: info.bits_per_sample,
        format: info.format,
    })
}

/// Every `.wav` under a directory, with its header.
///
/// Sorted by path so two runs over the same kit report in the same
/// order — a probe whose output reshuffles cannot be diffed between
/// kit versions, which is half of what it is for.
pub fn probe_tree(root: &Path) -> io::Result<Vec<(PathBuf, WavHeader)>> {
    let mut paths = Vec::new();
    collect(root, &mut paths)?;
    paths.sort();
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        match probe(&path) {
            Ok(header) => out.push((path, header)),
            // A kit with one unreadable file should still report the
            // other seven hundred.
            Err(e) => eprintln!("  skipped {}: {e}", path.display()),
        }
    }
    Ok(out)
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    if dir.is_file() {
        out.push(dir.to_path_buf());
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
        {
            out.push(path);
        }
    }
    Ok(())
}

/// Read one channel of a file as `f64`, with its sample rate.
///
/// `channel` picks a mic out of an interleaved file; `None` sums to
/// mono. Summing is offered but is rarely what you want here — mixing
/// fifteen mics together is a way to *manufacture* the bleed the corpus
/// exists to measure honestly, so the detector should normally be run
/// per channel.
pub fn read_channel(path: &Path, channel: Option<usize>) -> io::Result<(Vec<f64>, f64)> {
    let reader = hound::WavReader::open(path).map_err(to_io)?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let wanted = channel.unwrap_or(0);
    if channel.is_some_and(|c| c >= channels) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{}: channel {wanted} of {channels}", path.display()),
        ));
    }

    let scale = match spec.sample_format {
        hound::SampleFormat::Float => 1.0,
        // `bits_per_sample` is the meaningful width; hound hands back
        // the full container, so the divisor is the width's full scale.
        hound::SampleFormat::Int => (1i64 << (spec.bits_per_sample - 1)) as f64,
    };
    let raw: Vec<f64> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .map(|s| s.map(|v| v as f64))
            .collect::<Result<_, _>>()
            .map_err(to_io)?,
        hound::SampleFormat::Int => reader
            .into_samples::<i32>()
            .map(|s| s.map(|v| v as f64 / scale))
            .collect::<Result<_, _>>()
            .map_err(to_io)?,
    };

    let frames = raw.len() / channels;
    let mut out = Vec::with_capacity(frames);
    for f in 0..frames {
        let frame = &raw[f * channels..(f + 1) * channels];
        out.push(match channel {
            Some(c) => frame[c],
            None => frame.iter().sum::<f64>() / channels as f64,
        });
    }
    Ok((out, spec.sample_rate as f64))
}

/// Write a mono `f64` buffer as 32-bit float, the format the kits
/// themselves are in.
pub fn write_mono(path: &Path, samples: &[f64], sample_rate: f64) -> io::Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sample_rate as u32,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(path, spec).map_err(to_io)?;
    for &s in samples {
        writer.write_sample(s as f32).map_err(to_io)?;
    }
    writer.finalize().map_err(to_io)
}

fn to_io(e: hound::Error) -> io::Error {
    match e {
        hound::Error::IoError(e) => e,
        other => io::Error::new(io::ErrorKind::InvalidData, other.to_string()),
    }
}

/// The distinct headers in a probe, with how many files carry each.
///
/// The useful form of a kit probe: seven hundred identical lines say
/// nothing, and "all 758 files are 15ch/48000/float32" is the whole
/// answer. A kit that comes back with two rows has a mixed-rate
/// problem worth knowing about before rendering.
pub fn summarize(probed: &[(PathBuf, WavHeader)]) -> Vec<(WavHeader, usize)> {
    let mut out: Vec<(WavHeader, usize)> = Vec::new();
    for (_, header) in probed {
        match out.iter_mut().find(|(h, _)| h == header) {
            Some((_, n)) => *n += 1,
            None => out.push((*header, 1)),
        }
    }
    out.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    out
}
