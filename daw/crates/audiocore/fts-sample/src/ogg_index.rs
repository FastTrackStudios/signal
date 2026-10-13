//! **Ogg page index** — which bytes of a proxy hold which stretch of time.
//!
//! Streaming a proxy from somewhere else (Task, a peer) means fetching the
//! bytes *in front of the playhead* first — and an Ogg Vorbis stream has no
//! fixed bytes-per-second (it is VBR). What it has is pages: each starts
//! with `OggS` and says, in its granule position, how many frames have been
//! completed by its end. Walking the page headers (no audio decoded) gives
//! a map from time to byte offset; kept every second or so it is a few KB
//! for a whole song, and it is written beside the proxy
//! (`Proxies/Bass.ogg.idx`) so a client fetches it first and then asks for
//! exactly the bytes it needs.
//!
//! To decode frames `a..b` a reader needs the stream's **header** (its
//! first three packets: identification, comments, setup — [`OggIndex::header`])
//! and the pages from the last one that ends at or before `a` through the
//! first that ends at or after `b` ([`OggIndex::bytes_for`]).

use std::ops::Range;

/// One indexed page: where it starts, and the frames completed by its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PagePoint {
    /// Byte offset of the page.
    pub offset: u64,
    /// Byte offset just past it.
    pub end: u64,
    /// Its granule position: frames completed by the end of the page.
    pub frames: u64,
}

/// A proxy's time → bytes map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OggIndex {
    /// The file's length, bytes.
    pub len: u64,
    /// Frames in the stream (the last page's granule).
    pub frames: u64,
    /// Where the audio starts: everything before is the header pages.
    pub audio_start: u64,
    /// Channels and sample rate, from the Vorbis identification header —
    /// so a player can set a streamed take up from the index alone.
    pub channels: u16,
    pub sample_rate: u32,
    /// Indexed pages, ascending (about one a second, and the last page).
    pub points: Vec<PagePoint>,
}

/// One page's header, if a page starts at `at`: its granule position and
/// its whole length.
fn page_at(bytes: &[u8], at: usize) -> Option<(i64, usize)> {
    let head = bytes.get(at..at.checked_add(27)?)?;
    if head.get(..4)? != b"OggS" {
        return None;
    }
    let granule = i64::from_le_bytes(head.get(6..14)?.try_into().ok()?);
    let segments = usize::from(*head.get(26)?);
    let table = bytes.get(at.checked_add(27)?..at.checked_add(27)?.checked_add(segments)?)?;
    let body: usize = table.iter().map(|&s| usize::from(s)).sum();
    Some((granule, 27usize.checked_add(segments)?.checked_add(body)?))
}

/// Channels and sample rate, if the page at `at` opens with the Vorbis
/// identification header (`\x01vorbis`, version, channels, rate).
fn vorbis_ident(bytes: &[u8], at: usize) -> Option<(u16, u32)> {
    let segments = usize::from(*bytes.get(at.checked_add(26)?)?);
    let body = at.checked_add(27)?.checked_add(segments)?;
    let packet = bytes.get(body..body.checked_add(16)?)?;
    if packet.get(..7)? != b"\x01vorbis" {
        return None;
    }
    let channels = u16::from(*packet.get(11)?);
    let rate = u32::from_le_bytes(packet.get(12..16)?.try_into().ok()?);
    Some((channels, rate))
}

fn u64_of(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

impl OggIndex {
    /// Index a whole Ogg stream, keeping a page about every `step` frames
    /// (the sample rate: one a second).
    ///
    /// `None` when the bytes are not an Ogg stream.
    #[must_use]
    pub fn build(bytes: &[u8], step: u64) -> Option<Self> {
        let mut at = 0usize;
        let mut audio_start = None;
        let mut points: Vec<PagePoint> = Vec::new();
        let mut last = None;
        let mut next_mark = 0u64;
        let mut ident: Option<(u16, u32)> = None;
        while let Some((granule, len)) = page_at(bytes, at) {
            if ident.is_none() {
                ident = vorbis_ident(bytes, at);
            }
            // Header pages carry granule 0; the first page with audio
            // completes frames.
            if granule > 0 {
                let frames = u64::try_from(granule).unwrap_or(0);
                audio_start.get_or_insert(u64_of(at));
                let point = PagePoint {
                    offset: u64_of(at),
                    end: u64_of(at.saturating_add(len)),
                    frames,
                };
                if frames >= next_mark {
                    points.push(point);
                    next_mark = frames.saturating_add(step.max(1));
                }
                last = Some(point);
            }
            at = at.checked_add(len)?;
        }
        if at == 0 {
            return None;
        }
        let last = last?;
        if points.last() != Some(&last) {
            points.push(last);
        }
        let (channels, sample_rate) = ident.unwrap_or((0, 0));
        Some(Self {
            len: u64_of(at.min(bytes.len())),
            frames: last.frames,
            audio_start: audio_start?,
            channels,
            sample_rate,
            points,
        })
    }

    /// The header pages every reader needs first.
    #[must_use]
    pub const fn header(&self) -> Range<u64> {
        0..self.audio_start
    }

    /// The bytes holding frames `from..to`: from the last indexed page that
    /// ends at or before `from` (decoding starts on the page after it)
    /// through the end of the first indexed page that ends at or after `to`
    /// — up to a step of margin either side, which is what an index kept
    /// every second costs.
    #[must_use]
    pub fn bytes_for(&self, from: u64, to: u64) -> Range<u64> {
        let start = self
            .points
            .iter()
            .rev()
            .find(|p| p.frames <= from)
            .map_or(self.audio_start, |p| p.offset);
        let end = self
            .points
            .iter()
            .find(|p| p.frames >= to)
            .map_or(self.len, |p| p.end);
        start..end.max(start)
    }

    /// Serialise (the `.idx` sidecar): a line of totals, then one line per
    /// point — small, and readable by eye.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = format!(
            "ogg-index 2 {} {} {} {} {}\n",
            self.len, self.frames, self.audio_start, self.channels, self.sample_rate
        );
        for p in &self.points {
            out.push_str(&format!("{} {} {}\n", p.frames, p.offset, p.end));
        }
        out
    }

    /// Read a sidecar written by [`Self::to_text`].
    #[must_use]
    pub fn from_text(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        let mut head = lines.next()?.split_whitespace();
        if head.next()? != "ogg-index" || head.next()? != "2" {
            return None;
        }
        let len = head.next()?.parse().ok()?;
        let frames = head.next()?.parse().ok()?;
        let audio_start = head.next()?.parse().ok()?;
        let channels = head.next()?.parse().ok()?;
        let sample_rate = head.next()?.parse().ok()?;
        let points = lines
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let mut it = l.split_whitespace();
                Some(PagePoint {
                    frames: it.next()?.parse().ok()?,
                    offset: it.next()?.parse().ok()?,
                    end: it.next()?.parse().ok()?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            len,
            frames,
            audio_start,
            channels,
            sample_rate,
            points,
        })
    }

    /// Where a proxy's index lives: beside it, `.idx` appended.
    #[must_use]
    pub fn path_for(ogg: &std::path::Path) -> std::path::PathBuf {
        let mut name = ogg.as_os_str().to_owned();
        name.push(".idx");
        std::path::PathBuf::from(name)
    }
}

/// Without an index: the bytes for frames `from..to` of a stream of
/// `frames` frames in `len` bytes, by proportion, widened by `margin` (a
/// fraction of the whole) either side — VBR is only roughly linear.
#[must_use]
pub fn estimate_bytes(len: u64, frames: u64, from: u64, to: u64, margin: f64) -> Range<u64> {
    if frames == 0 {
        return 0..len;
    }
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let at = |f: u64, pad: f64| -> u64 {
        let x = (f as f64 / frames as f64 + pad).clamp(0.0, 1.0);
        (x * len as f64) as u64
    };
    at(from, -margin)..at(to, margin)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake Ogg stream: pages of `body` bytes, the header pages first
    /// (granule 0), then audio pages completing `per_page` frames each.
    fn stream(header_pages: usize, audio_pages: usize, per_page: u64, body: usize) -> Vec<u8> {
        let mut out = Vec::new();
        let page = |out: &mut Vec<u8>, granule: i64| {
            out.extend_from_slice(b"OggS");
            out.extend_from_slice(&[0, 0]);
            out.extend_from_slice(&granule.to_le_bytes());
            out.extend_from_slice(&[0; 12]);
            out.push(1);
            out.push(u8::try_from(body).unwrap_or(255));
            out.extend(std::iter::repeat_n(0xAB, body));
        };
        for _ in 0..header_pages {
            page(&mut out, 0);
        }
        for p in 1..=audio_pages {
            page(
                &mut out,
                i64::try_from(u64::try_from(p).unwrap_or(0) * per_page).unwrap_or(0),
            );
        }
        out
    }

    #[test]
    fn pages_are_indexed_about_every_step() {
        // 100 pages of 4800 frames at 48 kHz: 10 s; a point every second.
        let bytes = stream(3, 100, 4_800, 200);
        let index = OggIndex::build(&bytes, 48_000).unwrap();
        assert_eq!(index.frames, 480_000);
        assert_eq!(index.len, bytes.len() as u64);
        assert_eq!(index.audio_start, 3 * 228);
        assert!(
            (10..=12).contains(&index.points.len()),
            "{} points",
            index.points.len()
        );
        assert_eq!(index.points.last().unwrap().frames, 480_000);
    }

    #[test]
    fn the_bytes_for_a_stretch_cover_it_with_a_step_either_side() {
        let bytes = stream(3, 100, 4_800, 200);
        let index = OggIndex::build(&bytes, 48_000).unwrap();
        let page = 228u64;
        // Frames 5 s..6 s: from the page ending at or before 5 s, through
        // the one ending at or after 6 s.
        let range = index.bytes_for(240_000, 288_000);
        let first_needed = index.audio_start + 50 * page; // the page completing 5 s + 4800
        let last_needed = index.audio_start + 60 * page; // the page completing 6 s
        assert!(
            range.start <= first_needed && range.end >= last_needed,
            "{range:?}"
        );
        // And not the whole file.
        assert!(
            range.end - range.start < bytes.len() as u64 / 4,
            "{range:?}"
        );
    }

    #[test]
    fn the_sidecar_round_trips() {
        let bytes = stream(3, 40, 4_800, 100);
        let index = OggIndex::build(&bytes, 48_000).unwrap();
        assert_eq!(OggIndex::from_text(&index.to_text()), Some(index));
        assert_eq!(OggIndex::from_text("not an index"), None);
    }

    #[test]
    fn not_ogg_is_no_index() {
        assert_eq!(OggIndex::build(b"RIFF....WAVEfmt ", 48_000), None);
    }

    #[test]
    fn an_estimate_is_proportional_with_a_margin() {
        let r = estimate_bytes(1_000_000, 480_000, 240_000, 288_000, 0.05);
        assert!(r.start <= 450_000 && r.end >= 650_000, "{r:?}");
    }
}
