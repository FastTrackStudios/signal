//! An Ogg Vorbis stream decoded a window at a time: open it, seek to a
//! frame, decode forward from exactly there.
//!
//! What a player streaming a session's proxies needs (`Media/Proxies/
//! Bass.ogg`, see `cache::write_ogg_proxy`): seven minutes of 21 stems is
//! 3 GB as f32, so nothing decodes a proxy whole. The compressed bytes are
//! held (a few MB a stem); the player keeps a few seconds decoded around
//! the playhead and asks this for more as it moves. Pure Rust (symphonia):
//! from bytes the caller fetched ([`OggStream::open`], the browser), or
//! read from a file as it goes ([`OggStream::open_file`], natively).

use std::sync::Arc;

use symphonia_codec_vorbis::VorbisDecoder;
use symphonia_core::audio::SampleBuffer;
use symphonia_core::codecs::{Decoder as _, DecoderOptions};
use symphonia_core::errors::Error as SymphoniaError;
use symphonia_core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia_core::io::MediaSourceStream;
use symphonia_format_ogg::OggReader;

use crate::SamplerError;

/// A view the Ogg reader must not seek: it would read the file's last page
/// to learn the length (the index says it) and bisect for a seek (a view
/// is opened at the page instead) — both reads of bytes that may not be
/// there.
struct UnseekableView(crate::sparse::SparseView);

impl std::io::Read for UnseekableView {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl std::io::Seek for UnseekableView {
    fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64> {
        std::io::Seek::seek(&mut self.0, to)
    }
}

impl symphonia_core::io::MediaSource for UnseekableView {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}

/// One Ogg Vorbis stream, positioned somewhere in it.
pub struct OggStream {
    reader: OggReader,
    decoder: VorbisDecoder,
    track: u32,
    channels: u16,
    sample_rate: u32,
    frames: u64,
    /// Frames before this are dropped from the next packet decoded — what
    /// makes a seek land on the frame asked for, not the page before it.
    skip_to: u64,
    samples: Option<SampleBuffer<f32>>,
}

fn io(e: SymphoniaError) -> SamplerError {
    match e {
        // Kept as it is: `WouldBlock` from a streamed proxy means "not
        // arrived yet" ([`SamplerError::is_not_yet`]), not a broken stream.
        SymphoniaError::IoError(e) => SamplerError::Io(e),
        e => SamplerError::Io(std::io::Error::other(e.to_string())),
    }
}

impl OggStream {
    /// Open a stream from its whole compressed bytes, positioned at frame 0.
    ///
    /// # Errors
    ///
    /// Not an Ogg Vorbis stream.
    pub fn open(bytes: Arc<[u8]>) -> Result<Self, SamplerError> {
        Self::from_source(MediaSourceStream::new(
            Box::new(std::io::Cursor::new(bytes)),
            Default::default(),
        ))
    }

    /// Open a stream straight from a file, positioned at frame 0.
    ///
    /// Nothing is read into memory up front: the reader seeks in the file
    /// and pulls the pages it decodes, and the OS page cache does the
    /// rest — the proxy counterpart of a memory-mapped WAV. A setlist's
    /// proxies held as bytes were most of a gigabyte for no reason; they
    /// were on disk already.
    ///
    /// # Errors
    ///
    /// The file cannot be opened, or is not an Ogg Vorbis stream.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_file(path: &std::path::Path) -> Result<Self, SamplerError> {
        let file = std::fs::File::open(path).map_err(SamplerError::Io)?;
        Self::from_source(MediaSourceStream::new(Box::new(file), Default::default()))
    }

    /// Open a stream fetched from elsewhere, ready to decode from `frame`:
    /// the header pages, then the file from the indexed page before it —
    /// so opening needs only those bytes (the index says which), and a
    /// seek is a fresh open, never a bisection through bytes that may not
    /// have arrived.
    ///
    /// # Errors
    ///
    /// The header or the page has not arrived yet
    /// ([`SamplerError::is_not_yet`]), or the stream is not Ogg Vorbis.
    pub fn open_view(
        bytes: Arc<crate::sparse::SparseBytes>,
        index: &crate::ogg_index::OggIndex,
        frame: u64,
    ) -> Result<Self, SamplerError> {
        // A fresh decoder spends its first packet priming the overlap, so
        // start early and drop what lies before `frame` as it decodes.
        const PREROLL: u64 = 4096;
        let target = frame.saturating_sub(PREROLL);
        let from = index
            .points
            .iter()
            .rev()
            .find(|p| p.frames <= target)
            .map_or(index.audio_start, |p| p.offset);
        let view = UnseekableView(crate::sparse::SparseView::new(bytes, index.header(), from));
        let mut stream =
            Self::from_source(MediaSourceStream::new(Box::new(view), Default::default()))?;
        stream.frames = index.frames;
        stream.skip_to = frame.min(index.frames);
        Ok(stream)
    }

    fn from_source(source: MediaSourceStream) -> Result<Self, SamplerError> {
        let reader = OggReader::try_new(source, &FormatOptions::default()).map_err(io)?;
        let track = reader
            .default_track()
            .ok_or_else(|| SamplerError::Decode("ogg: no default track".into()))?;
        let params = track.codec_params.clone();
        let track = track.id;
        let decoder = VorbisDecoder::try_new(&params, &DecoderOptions::default()).map_err(io)?;
        Ok(Self {
            reader,
            decoder,
            track,
            channels: params.channels.map_or(1, |c| c.count() as u16),
            sample_rate: params.sample_rate.unwrap_or(0),
            frames: params.n_frames.unwrap_or(0),
            skip_to: 0,
            samples: None,
        })
    }

    #[must_use]
    pub fn channels(&self) -> u16 {
        self.channels
    }

    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The stream's length in frames, as its last page declares it — which
    /// may include the encoder's padding of the final block (silence).
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Position the stream so the next [`Self::decode`] starts at `frame`.
    ///
    /// # Errors
    ///
    /// The stream could not be read at that point.
    pub fn seek(&mut self, frame: u64) -> Result<(), SamplerError> {
        // A reset decoder spends its first packet priming the overlap and
        // outputs nothing for it, so land a long block early and drop the
        // audio before `frame` (`skip_to`) as it decodes.
        const PREROLL: u64 = 4096;
        let frame = frame.min(self.frames);
        self.reader
            .seek(
                SeekMode::Accurate,
                SeekTo::TimeStamp {
                    ts: frame.saturating_sub(PREROLL),
                    track_id: self.track,
                },
            )
            .map_err(io)?;
        self.decoder.reset();
        self.skip_to = frame;
        Ok(())
    }

    /// Decode the next packet onto the end of `out` (interleaved). Returns
    /// the frame the appended audio starts at and how many frames it is,
    /// or `None` at the end of the stream.
    ///
    /// # Errors
    ///
    /// The stream is corrupt past what a decoder can skip.
    pub fn decode(&mut self, out: &mut Vec<f32>) -> Result<Option<(u64, usize)>, SamplerError> {
        let channels = usize::from(self.channels.max(1));
        loop {
            let packet = match self.reader.next_packet() {
                Ok(p) => p,
                Err(SymphoniaError::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None);
                }
                Err(SymphoniaError::ResetRequired) => return Ok(None),
                Err(e) => return Err(io(e)),
            };
            if packet.track_id() != self.track {
                continue;
            }
            let start = packet.ts();
            let decoded = match self.decoder.decode(&packet) {
                Ok(d) => d,
                // One bad packet is a click, not the end of the stem.
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(e) => return Err(io(e)),
            };
            let spec = *decoded.spec();
            let samples = self
                .samples
                .get_or_insert_with(|| SampleBuffer::new(decoded.capacity() as u64, spec));
            if samples.capacity() < decoded.capacity() * channels {
                *samples = SampleBuffer::new(decoded.capacity() as u64, spec);
            }
            samples.copy_interleaved_ref(decoded);
            let got = samples.samples();
            let frames = got.len() / channels;
            // Drop what lies before the frame a seek asked for.
            let skip = usize::try_from(self.skip_to.saturating_sub(start))
                .unwrap_or(usize::MAX)
                .min(frames);
            if skip == frames {
                continue;
            }
            self.skip_to = 0;
            // The last packet may run past the stream's declared end.
            let keep = usize::try_from(self.frames.saturating_sub(start + skip as u64))
                .unwrap_or(usize::MAX)
                .min(frames - skip);
            if keep == 0 {
                return Ok(None);
            }
            out.extend_from_slice(&got[skip * channels..(skip + keep) * channels]);
            return Ok(Some((start + skip as u64, keep)));
        }
    }
}

#[cfg(all(test, feature = "engine-native"))]
mod tests {
    use super::*;

    /// A stereo proxy: 220 Hz left, 330 Hz right — steady tones, so a
    /// window decoded after a seek can be compared with a straight decode.
    fn proxy(frames: usize) -> Arc<[u8]> {
        let rate = 44_100u32;
        let mut pcm = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let t = i as f32 / rate as f32;
            pcm.push((t * 220.0 * std::f32::consts::TAU).sin() * 0.5);
            pcm.push((t * 330.0 * std::f32::consts::TAU).sin() * 0.5);
        }
        crate::cache::encode_ogg_vorbis(&pcm, 2, rate, 0.4)
            .expect("encode")
            .into()
    }

    fn decode_all(stream: &mut OggStream) -> (u64, Vec<f32>) {
        let mut out = Vec::new();
        let mut first = None;
        while let Some((at, _)) = stream.decode(&mut out).expect("decode") {
            first.get_or_insert(at);
        }
        (first.unwrap_or(0), out)
    }

    /// A proxy fetched from elsewhere: with only its header and the bytes
    /// the index names for a stretch, that stretch decodes as a straight
    /// decode does — and a read past what arrived is "not yet", not an
    /// error.
    #[test]
    fn a_stretch_decodes_from_only_its_own_bytes() {
        let bytes = proxy(44_100 * 20);
        let mut whole = OggStream::open(Arc::clone(&bytes)).expect("open");
        let (_, all) = decode_all(&mut whole);
        let index = crate::ogg_index::OggIndex::build(&bytes, 44_100).expect("index");
        assert_eq!(
            (index.channels, index.sample_rate),
            (2, 44_100),
            "set up from the index alone"
        );

        let target: u64 = 44_100 * 12 + 777;
        let want = 8192u64;
        let sparse = crate::sparse::SparseBytes::in_memory(bytes.len() as u64);
        let header = index.header();
        sparse
            .insert(
                header.start,
                &bytes[header.start as usize..header.end as usize],
            )
            .unwrap();
        let range = index.bytes_for(target.saturating_sub(4096), target + want);
        sparse
            .insert(
                range.start,
                &bytes[range.start as usize..range.end as usize],
            )
            .unwrap();
        assert!(
            range.start > index.audio_start && range.end < bytes.len() as u64,
            "a stretch, not the file: {range:?} of {}",
            bytes.len()
        );

        let mut stream =
            OggStream::open_view(Arc::clone(&sparse), &index, target).expect("open at the stretch");
        let mut out = Vec::new();
        let (at, _) = stream.decode(&mut out).expect("decode").expect("audio");
        assert_eq!(at, target, "the first frame out is the one asked for");
        while out.len() < want as usize * 2 {
            stream.decode(&mut out).expect("decode").expect("more");
        }
        let base = target as usize * 2;
        let worst = out[..want as usize * 2]
            .iter()
            .zip(&all[base..base + want as usize * 2])
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(worst < 0.02, "matches the straight decode: {worst}");

        // Past what arrived: not yet.
        let err = loop {
            match stream.decode(&mut out) {
                Ok(Some(_)) => continue,
                Ok(None) => panic!("the stream cannot end before its bytes arrive"),
                Err(e) => break e,
            }
        };
        assert!(err.is_not_yet(), "{err}");
        assert!(
            sparse.wanted().is_some(),
            "the fetcher is told what to bring"
        );
    }

    #[test]
    fn a_seek_lands_on_the_frame_asked_for() {
        let bytes = proxy(44_100 * 4);
        let mut whole = OggStream::open(Arc::clone(&bytes)).expect("open");
        assert_eq!((whole.channels(), whole.sample_rate()), (2, 44_100));
        let (zero, all) = decode_all(&mut whole);
        assert_eq!(zero, 0);
        // Frame 0 out is frame 0 in: no shift against the source.
        let tone = |i: usize| (i as f32 / 44_100.0 * 220.0 * std::f32::consts::TAU).sin() * 0.5;
        let error_at = |lag: i64| -> f32 {
            (8192..16_384)
                .map(|i: usize| (all[i * 2] - tone((i as i64 + lag) as usize)).abs())
                .sum()
        };
        let best = (-2048i64..=2048)
            .min_by(|a, b| error_at(*a).total_cmp(&error_at(*b)))
            .unwrap();
        assert_eq!(best, 0, "decoded audio is shifted by {best} frames");
        // The declared length may carry the encoder's last-block padding —
        // a few ms of silence at the end, never a shift at the start.
        assert!(
            (44_100 * 4..44_100 * 4 + 2048).contains(&whole.frames()),
            "the declared length: {}",
            whole.frames()
        );
        assert_eq!(
            all.len() as u64,
            whole.frames() * 2,
            "every declared frame, and no more"
        );

        let mut sought = OggStream::open(bytes).expect("open");
        let target = 44_100 * 2 + 12_345;
        sought.seek(target).expect("seek");
        let mut out = Vec::new();
        let (at, _) = sought.decode(&mut out).expect("decode").expect("audio");
        assert_eq!(at, target, "the first frame out is the one asked for");
        while out.len() < 8192 * 2 {
            sought.decode(&mut out).expect("decode").expect("more");
        }
        // After the decoder's own warm-up, a seek reads what a straight
        // decode does at that frame.
        let base = target as usize * 2;
        let worst = out[4096..8192 * 2]
            .iter()
            .zip(&all[base + 4096..base + 8192 * 2])
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(
            worst < 0.02,
            "a sought window matches the straight decode: {worst}"
        );
    }
}
