//! Synthetic media for the fixture: short, deterministic, uncommitted.
//!
//! Every audio leaf's items play one file, `media/<slug>.wav`.
//!
//! The waveform is a decaying triangle at a pitch drawn from the track's
//! path — enough for a loader to open, a peak reader to draw, and a
//! byte-for-byte compare to catch a change, and built in integer
//! arithmetic so there is no rounding to disagree about between one
//! machine and the next. The files are generated, never committed: the
//! project text is the fixture, and `just daw-template` regenerates the
//! media beside it. MIDI is the triggers', embedded in the project the
//! way REAPER stores a MIDI take.

use std::path::Path;

use super::guid::hash;
use super::rpp::Flat;

/// Sample rate of the generated audio.
pub const SAMPLE_RATE: u32 = 44_100;
/// Length of every generated file, in seconds.
pub const SECONDS: u32 = 1;
/// The peak the waveform starts at, backed off from full scale.
const PEAK: i32 = 26_000;

/// The samples-per-cycle of a track's tone: 50 to 400, which at 44.1 kHz
/// is 110 Hz to 880 Hz — three octaves, so two tracks rarely sound the
/// same and none of them is inaudibly low.
fn period(path: &str) -> u32 {
    let spread = u32::try_from(hash(&format!("media:{path}")) % 351).unwrap_or(0);
    spread.saturating_add(50)
}

/// The PCM samples of a track's media: a decaying triangle, pitched from
/// the path so two tracks never play the same tone.
#[must_use]
pub fn samples(path: &str) -> Vec<i16> {
    let period = period(path);
    let half = period.max(2) / 2;
    let total = SAMPLE_RATE.saturating_mul(SECONDS);
    (0..total)
        .map(|n| {
            // The triangle: up across the first half of the cycle, down
            // across the second, as a fraction of the peak.
            let phase = n.checked_rem(period.max(1)).unwrap_or(0);
            let rise = if phase < half {
                i32::try_from(phase).unwrap_or(0)
            } else {
                i32::try_from(period.saturating_sub(phase)).unwrap_or(0)
            };
            let span = i32::try_from(half.max(1)).unwrap_or(1);
            let shape = PEAK
                .saturating_mul(rise.saturating_mul(2).saturating_sub(span))
                .checked_div(span)
                .unwrap_or(0);
            // And the decay: linear to silence across the file, so the
            // peak reader has a shape to draw rather than a block.
            let left = total.saturating_sub(n);
            let decayed = shape
                .saturating_mul(i32::try_from(left).unwrap_or(0))
                .checked_div(i32::try_from(total.max(1)).unwrap_or(1))
                .unwrap_or(0);
            i16::try_from(decayed.clamp(i32::from(i16::MIN), i32::from(i16::MAX))).unwrap_or(0)
        })
        .collect()
}

/// The right channel of a stereo pair's media.
///
/// A different tone from the left, not a copy of it: a stereo pair's two
/// sides are two microphones or two performances, and a fixture whose
/// sides are identical could not tell a folder item that draws two sides
/// from one that draws the same side twice
/// (`flow.guitars.folder-items`). Quieter as well, so which side is
/// which is visible in a picture.
#[must_use]
pub fn right_samples(path: &str) -> Vec<i16> {
    samples(&format!("{path}/R"))
        .into_iter()
        .map(|s| {
            i16::try_from(i32::from(s).saturating_mul(5).checked_div(8).unwrap_or(0)).unwrap_or(0)
        })
        .collect()
}

/// A stereo pair's media, interleaved `[l, r, l, r, ...]`.
#[must_use]
pub fn stereo_samples(path: &str) -> Vec<i16> {
    let left = samples(path);
    let right = right_samples(path);
    left.into_iter()
        .zip(right)
        .flat_map(<[i16; 2]>::from)
        .collect()
}

/// A 16-bit PCM WAV file's bytes, `channels` interleaved.
#[must_use]
pub fn wav(samples: &[i16], channels: u16) -> Vec<u8> {
    wav_at(samples, channels, SAMPLE_RATE)
}

/// [`wav`] at `rate` samples a second.
#[must_use]
pub fn wav_at(samples: &[i16], channels: u16, rate: u32) -> Vec<u8> {
    let channels = channels.max(1);
    let block_align = channels.saturating_mul(2);
    let byte_rate = rate.saturating_mul(u32::from(block_align));
    let data_len = u32::try_from(samples.len().saturating_mul(2)).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(usize::try_from(data_len).unwrap_or(0).saturating_add(44));
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&data_len.saturating_add(36).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&16_u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

/// Write every audio track's media file under `dir` (the project's
/// directory). Returns the files written, relative to `dir`.
///
/// # Errors
///
/// When a file cannot be written.
pub fn write_all(
    dir: &Path,
    tracks: &[Flat],
    shape: &super::shape::Shape,
) -> std::io::Result<Vec<String>> {
    let mut written: Vec<String> = Vec::new();
    for track in tracks {
        let Some(file) = track.media_file() else {
            continue;
        };
        // A performance shared by several tracks is written once.
        if written.contains(&file) {
            continue;
        }
        let path = dir.join(&file);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // A stereo pair is ONE two-channel track, so its media is one
        // two-channel file — a mono source under a stereo track is a
        // fixture that cannot tell L from R.
        let bytes = if let Some((_, perf)) = super::perform::performance(&track.path) {
            wav_at(
                &super::perform::render(perf, shape),
                1,
                super::perform::RATE,
            )
        } else if track.stereo {
            wav(&stereo_samples(&track.path), 2)
        } else {
            wav(&samples(&track.path), 1)
        };
        std::fs::write(&path, bytes)?;
        written.push(file);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::{samples, stereo_samples, wav, PEAK, SAMPLE_RATE};

    #[test]
    fn media_is_deterministic_and_a_valid_wav() {
        let a = samples("Drums/Kick/Sum/In");
        assert_eq!(a, samples("Drums/Kick/Sum/In"));
        assert_ne!(a, samples("Drums/Kick/Sum/Out"));
        assert_eq!(a.len(), 44_100);
        let bytes = wav(&a, 1);
        assert_eq!(bytes.get(..4), Some(b"RIFF".as_slice()));
        assert_eq!(bytes.get(8..12), Some(b"WAVE".as_slice()));
        assert_eq!(bytes.len(), 44 + 44_100 * 2);
        assert_eq!(bytes.get(22..24), Some(1_u16.to_le_bytes().as_slice()));
    }

    /// A stereo pair's file really is two channels, and its two sides
    /// are different — which is what makes a two-sided folder item
    /// provable from the fixture rather than assumed.
    #[test]
    fn a_stereo_pair_gets_two_different_channels() {
        let both = stereo_samples("Guitars/Electric/Rhythm");
        assert_eq!(both.len(), 44_100 * 2);
        let left: Vec<i16> = both.iter().step_by(2).copied().collect();
        let right: Vec<i16> = both.iter().skip(1).step_by(2).copied().collect();
        assert_eq!(left, samples("Guitars/Electric/Rhythm"));
        assert_ne!(left, right, "the two sides are two performances");
        let loudest = |c: &[i16]| c.iter().map(|s| i32::from(*s).abs()).max().unwrap_or(0);
        assert!(
            loudest(&right) < loudest(&left),
            "the right side is quieter"
        );
        let bytes = wav(&both, 2);
        assert_eq!(bytes.get(22..24), Some(2_u16.to_le_bytes().as_slice()));
        assert_eq!(bytes.get(32..34), Some(4_u16.to_le_bytes().as_slice()));
        assert_eq!(bytes.len(), 44 + 44_100 * 4);
    }

    #[test]
    fn the_tone_swings_both_ways_and_decays() {
        let a = samples("Drums/Snare/Sum/Top");
        let peak = a.iter().copied().max().unwrap_or(0);
        let trough = a.iter().copied().min().unwrap_or(0);
        assert!(i32::from(peak) > PEAK / 2, "peak {peak}");
        assert!(i32::from(trough) < -PEAK / 2, "trough {trough}");
        // The last tenth is quieter than the first: a shape a peak
        // reader can draw rather than a block.
        let tenth = usize::try_from(SAMPLE_RATE / 10).unwrap_or(0);
        let loudest = |window: &[i16]| {
            window
                .iter()
                .map(|s| i32::from(*s).abs())
                .max()
                .unwrap_or(0)
        };
        let head = a.get(..tenth).map_or(0, loudest);
        let tail = a.get(a.len().saturating_sub(tenth)..).map_or(0, loudest);
        assert!(tail * 4 < head, "head {head}, tail {tail}");
    }
}
