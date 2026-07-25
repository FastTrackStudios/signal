//! **FLAC frame index** — the thing that makes streaming a compressed sample
//! possible.
//!
//! A FLAC stream is a metadata header followed by independently decodable
//! frames, each carrying its own frame/sample number. So a decoder can start
//! at *any* frame boundary — the only missing piece is knowing where the
//! boundaries are, which is exactly what a `SEEKTABLE` would tell us and our
//! packed entries don't have.
//!
//! This builds that map by walking frame headers (sync code + CRC-8 check, no
//! audio decoded), and then hands out byte ranges: "the frames covering
//! samples 480 000 to 528 000 live at bytes X..Y". Prefix those bytes with
//! the stream's own metadata header and any FLAC decoder will read them as a
//! complete little stream.
//!
//! This is how Kontakt streams NCW and Omnisphere streams its own compressed
//! library: the format stays compressed on disk, and only the chunk under the
//! playhead is ever decoded.

/// One indexed frame: where it starts, and the first sample it carries.
#[derive(Debug, Clone, Copy)]
pub struct FramePoint {
    /// Byte offset of the frame, relative to the start of the FLAC stream.
    pub offset: u32,
    /// Index of the frame's first sample (per channel).
    pub first_frame: u32,
}

/// A parsed FLAC stream: its metadata header, and where its audio frames are.
///
/// The frame map is built **lazily**. Scanning is linear in bytes, and a
/// library is gigabytes: indexing every entry up front at load turns "open
/// the piano" into "read the whole piano", which is exactly the work
/// streaming exists to avoid. Instead the scan runs only as far as the audio
/// asked for so far, and extends when a later chunk is wanted.
#[derive(Debug)]
pub struct FlacIndex {
    /// `fLaC` + metadata blocks — the prefix every synthesised chunk needs.
    pub header: Box<[u8]>,
    /// Frame boundaries found so far, ascending by sample position.
    points: std::sync::Mutex<Vec<FramePoint>>,
    /// Byte offset (into the whole stream) where scanning stopped.
    scanned_to: std::sync::Mutex<usize>,
    /// Block size when the stream is fixed-strategy, else 0.
    fixed_block: u64,
    /// Total sample frames (per channel), from STREAMINFO.
    pub total_frames: u64,
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
}

impl FlacIndex {
    /// Walk `bytes` (one complete FLAC stream) and index its frames.
    ///
    /// Returns `None` when the bytes are not FLAC or the header is malformed —
    /// callers fall back to decoding the whole thing.
    pub fn build(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 8 || &bytes[..4] != b"fLaC" {
            return None;
        }
        // Metadata block chain: [last(1) | type(7)][len:24][body].
        let mut at = 4usize;
        let mut streaminfo: Option<&[u8]> = None;
        loop {
            let head = bytes.get(at..at + 4)?;
            let last = head[0] & 0x80 != 0;
            let kind = head[0] & 0x7f;
            let len = u32::from_be_bytes([0, head[1], head[2], head[3]]) as usize;
            let body = bytes.get(at + 4..at + 4 + len)?;
            if kind == 0 {
                streaminfo = Some(body);
            }
            at += 4 + len;
            if last {
                break;
            }
        }
        let info = streaminfo?;
        if info.len() < 34 {
            return None;
        }
        let min_block = u16::from_be_bytes([info[0], info[1]]);
        let max_block = u16::from_be_bytes([info[2], info[3]]);
        let sample_rate = (u32::from(info[10]) << 12)
            | (u32::from(info[11]) << 4)
            | (u32::from(info[12]) >> 4);
        let channels = ((info[12] >> 1) & 0x07) as u16 + 1;
        let bits_per_sample =
            ((((info[12] & 0x01) as u16) << 4) | ((info[13] >> 4) as u16)) + 1;
        let total_frames = ((info[13] & 0x0f) as u64) << 32
            | (info[14] as u64) << 24
            | (info[15] as u64) << 16
            | (info[16] as u64) << 8
            | info[17] as u64;

        let index = Self {
            header: bytes[..at].to_vec().into_boxed_slice(),
            points: std::sync::Mutex::new(Vec::new()),
            scanned_to: std::sync::Mutex::new(at),
            // Our packs encode with a FIXED blocking strategy, so a frame's
            // "number" counts frames and the sample position is number ×
            // block size. Variable-strategy streams carry the sample number.
            fixed_block: if min_block == max_block { min_block as u64 } else { 0 },
            total_frames,
            channels,
            sample_rate,
            bits_per_sample,
        };
        // One frame proves it really is a frame stream.
        index.scan_to(bytes, 1);
        if index.points.lock().map(|p| p.is_empty()).unwrap_or(true) {
            return None;
        }
        Some(index)
    }

    /// Extend the frame map until it covers `frame`, or the stream ends.
    fn scan_to(&self, bytes: &[u8], frame: u32) {
        let Ok(mut points) = self.points.lock() else { return };
        if points.last().is_some_and(|p| p.first_frame >= frame) {
            return;
        }
        let Ok(mut pos) = self.scanned_to.lock() else { return };
        while *pos + 2 < bytes.len() {
            let Some(fh) = FrameHeader::parse(&bytes[*pos..]) else {
                *pos += 1;
                continue;
            };
            let first_frame = if fh.variable_blocking {
                fh.number
            } else if self.fixed_block > 0 {
                fh.number * self.fixed_block
            } else {
                fh.number * fh.block_size as u64
            };
            // A CRC-8 that happens to match inside compressed audio still
            // produces a phantom frame, and one bogus point misaligns every
            // chunk decoded from it. Real frames run strictly forward, and
            // under a fixed blocking strategy they land on block boundaries —
            // anything else is noise that passed the checksum.
            let plausible = points
                .last()
                .map(|prev: &FramePoint| first_frame > prev.first_frame as u64)
                .unwrap_or(true)
                && (self.fixed_block == 0 || first_frame % self.fixed_block == 0);
            if !plausible {
                *pos += 1;
                continue;
            }
            points.push(FramePoint {
                offset: *pos as u32,
                first_frame: first_frame.min(u32::MAX as u64) as u32,
            });
            // Frames are variable length with no length field, so the next
            // sync has to be found by scanning; skipping the header we just
            // validated is enough not to re-find this one.
            *pos += fh.len.max(4);
            if first_frame >= frame as u64 {
                return;
            }
        }
    }

    /// Total sample frames (per channel), from STREAMINFO.
    pub fn total_frames(&self) -> u64 {
        self.total_frames
    }

    /// Bytes covering `frames` starting at `from_frame`, as a standalone FLAC
    /// stream, plus the sample position the stream actually starts at (frame
    /// boundaries rarely line up with the request).
    pub fn chunk(&self, bytes: &[u8], from_frame: u32, frames: u32) -> Option<(Vec<u8>, u32)> {
        let want_end = from_frame.saturating_add(frames);
        // Make sure the map reaches past what is being asked for, so the end
        // boundary is a real frame and not "the rest of the file".
        self.scan_to(bytes, want_end.saturating_add(1));
        let points = self.points.lock().ok()?;
        let start_i = match points.binary_search_by_key(&from_frame, |p| p.first_frame) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        };
        let end_i = points
            .iter()
            .position(|p| p.first_frame >= want_end)
            .unwrap_or(points.len());
        let start = *points.get(start_i)?;
        let end_byte = points
            .get(end_i)
            .map(|p| p.offset as usize)
            .unwrap_or(bytes.len());
        let body = bytes.get(start.offset as usize..end_byte)?;
        let mut out = Vec::with_capacity(self.header.len() + body.len());
        out.extend_from_slice(&self.header);
        out.extend_from_slice(body);
        Some((out, start.first_frame))
    }
}

/// The parts of a FLAC frame header we need: how long it is, which frame or
/// sample it starts at, and its block size.
struct FrameHeader {
    len: usize,
    number: u64,
    block_size: u32,
    variable_blocking: bool,
}

impl FrameHeader {
    /// Parse and CRC-check a frame header at the start of `b`.
    ///
    /// The CRC-8 is what makes scanning safe: sync patterns turn up inside
    /// compressed audio all the time, and without the check an index would be
    /// full of phantom frames.
    fn parse(b: &[u8]) -> Option<Self> {
        if b.len() < 5 || b[0] != 0xFF || (b[1] & 0xFE) != 0xF8 {
            return None;
        }
        let variable_blocking = b[1] & 0x01 != 0;
        let bs_code = b[2] >> 4;
        let sr_code = b[2] & 0x0f;
        let ch_code = b[3] >> 4;
        let bps_code = (b[3] >> 1) & 0x07;
        if ch_code > 0b1010 || bps_code == 0b011 || bps_code == 0b111 || b[3] & 0x01 != 0 {
            return None;
        }
        if sr_code == 0b1111 || bs_code == 0 {
            return None;
        }
        let mut at = 4usize;
        let number = read_utf8_number(b, &mut at)?;
        let block_size = match bs_code {
            0b0001 => 192,
            n @ 0b0010..=0b0101 => 576 << (n - 2),
            0b0110 => {
                let v = *b.get(at)? as u32 + 1;
                at += 1;
                v
            }
            0b0111 => {
                let v = u16::from_be_bytes([*b.get(at)?, *b.get(at + 1)?]) as u32 + 1;
                at += 2;
                v
            }
            n @ 0b1000..=0b1111 => 256 << (n - 8),
            _ => return None,
        };
        match sr_code {
            0b1100 => at += 1,
            0b1101 | 0b1110 => at += 2,
            _ => {}
        }
        let crc_byte = *b.get(at)?;
        if crc8(&b[..at]) != crc_byte {
            return None;
        }
        Some(Self { len: at + 1, number, block_size, variable_blocking })
    }
}

/// FLAC's UTF-8-style variable-length integer (frame or sample number).
fn read_utf8_number(b: &[u8], at: &mut usize) -> Option<u64> {
    let first = *b.get(*at)?;
    let extra = match first {
        0x00..=0x7f => 0,
        0xc0..=0xdf => 1,
        0xe0..=0xef => 2,
        0xf0..=0xf7 => 3,
        0xf8..=0xfb => 4,
        0xfc..=0xfd => 5,
        0xfe => 6,
        _ => return None,
    };
    let mut value = match extra {
        0 => first as u64,
        n => (first as u64) & (0x7f >> (n + 1)),
    };
    for i in 0..extra {
        let byte = *b.get(*at + 1 + i)?;
        if byte & 0xc0 != 0x80 {
            return None;
        }
        value = (value << 6) | (byte & 0x3f) as u64;
    }
    *at += 1 + extra;
    Some(value)
}

/// CRC-8 with polynomial x^8 + x^2 + x + 1, as FLAC frame headers use.
fn crc8(bytes: &[u8]) -> u8 {
    let mut crc = 0u8;
    for b in bytes {
        crc ^= b;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 { (crc << 1) ^ 0x07 } else { crc << 1 };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc8_is_the_flac_polynomial() {
        assert_eq!(crc8(&[]), 0x00);
        assert_eq!(crc8(&[0x00]), 0x00);
        // Eight shifts of a single set bit through x^8+x^2+x+1: the low bit
        // walks up to 0x80 and reduces once (0x07); the high bit reduces on
        // the first shift and again twice more, landing on 0x89.
        assert_eq!(crc8(&[0x01]), 0x07);
        assert_eq!(crc8(&[0x80]), 0x89);
    }

    #[test]
    fn utf8_numbers_round_trip_the_shapes_flac_uses() {
        let mut at = 0;
        assert_eq!(read_utf8_number(&[0x00], &mut at), Some(0));
        at = 0;
        assert_eq!(read_utf8_number(&[0x7f], &mut at), Some(127));
        at = 0;
        // 0xc2 0x80 = 128
        assert_eq!(read_utf8_number(&[0xc2, 0x80], &mut at), Some(128));
        assert_eq!(at, 2);
        at = 0;
        // A continuation byte that isn't 10xxxxxx is not a number.
        assert_eq!(read_utf8_number(&[0xc2, 0x40], &mut at), None);
    }
}
