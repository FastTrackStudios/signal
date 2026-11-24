//! Just enough WAV to read an impulse response from bytes, without `std`:
//! PCM 16/24/32-bit and 32-bit float, any channel count (the first is
//! kept). The host loads the file; this reads what it handed over.

use alloc::vec::Vec;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    let s = b.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([*s.first()?, *s.get(1)?]))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([
        *s.first()?,
        *s.get(1)?,
        *s.get(2)?,
        *s.get(3)?,
    ]))
}

/// The first channel of a WAV file, as `f32` in −1…1.
///
/// # Errors
///
/// A short description when it isn't a WAV this reads.
pub fn first_channel(bytes: &[u8]) -> Result<Vec<f32>, &'static str> {
    if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err("not a RIFF/WAVE file");
    }
    let mut at = 12_usize;
    let mut format = None;
    while let (Some(id), Some(len)) = (
        bytes.get(at..at.saturating_add(4)),
        u32_at(bytes, at.saturating_add(4)),
    ) {
        let len = usize::try_from(len).map_err(|_| "chunk too long")?;
        let body = at.saturating_add(8);
        let end = body.checked_add(len).ok_or("chunk too long")?;
        match id {
            b"fmt " => {
                let tag = u16_at(bytes, body).ok_or("short fmt chunk")?;
                let channels = u16_at(bytes, body.saturating_add(2)).ok_or("short fmt chunk")?;
                let bits = u16_at(bytes, body.saturating_add(14)).ok_or("short fmt chunk")?;
                // WAVE_FORMAT_EXTENSIBLE: the real tag is the sub-format's first two bytes.
                let tag = if tag == 0xfffe {
                    u16_at(bytes, body.saturating_add(24)).ok_or("short fmt chunk")?
                } else {
                    tag
                };
                format = Some((tag, channels.max(1), bits));
            }
            b"data" => {
                let (tag, channels, bits) = format.ok_or("data before fmt")?;
                let data = bytes
                    .get(body..end.min(bytes.len()))
                    .ok_or("short data chunk")?;
                return decode(data, tag, usize::from(channels), bits);
            }
            _ => {}
        }
        // Chunks are padded to even lengths.
        at = end.saturating_add(len % 2);
    }
    Err("no data chunk")
}

fn decode(data: &[u8], tag: u16, channels: usize, bits: u16) -> Result<Vec<f32>, &'static str> {
    let width = usize::from(bits / 8);
    let frame = width
        .checked_mul(channels)
        .filter(|f| *f > 0)
        .ok_or("bad format")?;
    let sample = |s: &[u8]| -> Option<f32> {
        match (tag, bits) {
            (1, 16) => Some(f32::from(i16::from_le_bytes([*s.first()?, *s.get(1)?])) / 32_768.0),
            (1, 24) => {
                let v = i32::from_le_bytes([0, *s.first()?, *s.get(1)?, *s.get(2)?]) >> 8;
                Some(signal_dsp_core::num::narrow(f64::from(v) / 8_388_608.0))
            }
            (1, 32) => {
                let v = i32::from_le_bytes([*s.first()?, *s.get(1)?, *s.get(2)?, *s.get(3)?]);
                Some(signal_dsp_core::num::narrow(f64::from(v) / 2_147_483_648.0))
            }
            (3, 32) => Some(f32::from_le_bytes([
                *s.first()?,
                *s.get(1)?,
                *s.get(2)?,
                *s.get(3)?,
            ])),
            _ => None,
        }
    };
    if !matches!((tag, bits), (1, 16 | 24 | 32) | (3, 32)) {
        return Err("unsupported sample format");
    }
    Ok(data.chunks_exact(frame).filter_map(sample).collect())
}

/// A mono 32-bit float WAV of `samples`: tests and synthesized IRs.
#[must_use]
pub fn write_mono_f32(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_len = u32::try_from(samples.len().saturating_mul(4)).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(samples.len().saturating_mul(4).saturating_add(44));
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&data_len.saturating_add(36).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&3_u16.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&sample_rate.saturating_mul(4).to_le_bytes());
    out.extend_from_slice(&4_u16.to_le_bytes());
    out.extend_from_slice(&32_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_it_writes() {
        let ir = [1.0_f32, 0.5, -0.25, 0.0];
        let back = first_channel(&write_mono_f32(&ir, 48_000));
        assert_eq!(back.as_deref(), Ok(&ir[..]));
        assert!(first_channel(b"nope").is_err());
    }
}
