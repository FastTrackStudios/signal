//! A take *as it plays*: the source behind it, read through the take's
//! placement (start offset, play rate, item bounds) and its stretch
//! markers. This is the one place standalone turns "take playback time"
//! into "source frame", so the audio accessor and the peaks builder
//! cannot disagree about where a sample is — which matters because the
//! expression editor analyses through the first and draws through the
//! second, and writes edits back expecting both to match REAPER.
//!
//! REAPER's take accessor and `GetPeaks` both honour placement; ours
//! must too, or an edit computed here lands on the wrong sample there.

use std::sync::Arc;

use daw_proto::{ItemRef, Items, ProjectContext, StretchMarker, TakeRef, Takes};

use crate::audio_engine::AudioSource;
use crate::sync::Standalone;

/// One take, readable in take playback time.
pub(crate) struct TakeReader {
    pub source: Arc<AudioSource>,
    /// Seconds into the source where take time 0 starts (ignored when
    /// markers are present — they carry their own source positions).
    pub start_offset: f64,
    pub play_rate: f64,
    /// Length of the item in project seconds — the accessor's extent.
    pub length: f64,
    /// The take's stretch markers, sorted by position.
    pub markers: Vec<StretchMarker>,
    /// The source's on-disk media file, when the resolver (or the raw
    /// path) points at one — the key for the `.reapeaks` sidecar.
    #[cfg(all(feature = "reapeaks", any(feature = "audio", feature = "decode")))]
    pub media_path: Option<std::path::PathBuf>,
    /// The file name the take asks for (`Bass.wav`) — what a stand-in's
    /// peaks are filed under when the file playing is its proxy.
    #[cfg(all(feature = "reapeaks", any(feature = "audio", feature = "decode")))]
    pub original_name: Option<String>,
    /// Parsed source-level peak mipmap (see [`crate::peak_store`]);
    /// populated by [`Self::ensure_reapeaks`], never eagerly — the
    /// audio accessor opens readers too and must stay scan-free.
    #[cfg(all(feature = "reapeaks", any(feature = "audio", feature = "decode")))]
    pub reapeaks: Option<std::sync::Arc<dawfile_reaper::reapeaks::ReaPeaks>>,
}

impl TakeReader {
    /// Resolve a take and its source. `None` when the take has no
    /// materialized audio (missing media, MIDI, or the project was
    /// opened without a resolver).
    pub(crate) fn open(
        daw: &Standalone,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
    ) -> Option<Self> {
        let guid = project_guid(daw, &project)?;
        let takes = daw.get_takes(project.clone(), item.clone());
        let index = match take {
            TakeRef::Active => takes.iter().position(|t| t.is_active).unwrap_or(0),
            TakeRef::Index(i) => i as usize,
            TakeRef::Guid(ref g) => takes.iter().position(|t| t.guid == *g)?,
        };
        let t = takes.get(index)?;
        let source = daw.audio_source(&guid, &t.guid)?;
        let length = daw
            .get_item(project, item)
            .map(|i| i.length.as_seconds())
            .unwrap_or_else(|| source.duration_seconds());
        let markers = daw
            .read_project(&guid, |p| p.stretch_markers.get(&t.guid).cloned())
            .flatten()
            .unwrap_or_default();
        #[cfg(all(feature = "reapeaks", any(feature = "audio", feature = "decode")))]
        let media_path = t.source_file_path.as_deref().and_then(|p| {
            // The bay resolver is authoritative (project-relative paths);
            // a plain absolute path that exists works without one.
            daw.media_bay().resolve_file_path(p).or_else(|| {
                let raw = std::path::Path::new(p);
                raw.is_file().then(|| raw.to_path_buf())
            })
        });
        Some(Self {
            source,
            start_offset: t.start_offset.as_seconds(),
            play_rate: if t.play_rate > 0.0 { t.play_rate } else { 1.0 },
            length,
            markers,
            #[cfg(all(feature = "reapeaks", any(feature = "audio", feature = "decode")))]
            media_path,
            #[cfg(all(feature = "reapeaks", any(feature = "audio", feature = "decode")))]
            original_name: t
                .source_file_path
                .as_deref()
                .and_then(|p| std::path::Path::new(p).file_name())
                .map(|n| n.to_string_lossy().into_owned()),
            #[cfg(all(feature = "reapeaks", any(feature = "audio", feature = "decode")))]
            reapeaks: None,
        })
    }

    /// [`TakeReader::open`], decoding the take's file into the project
    /// when nothing materialized it (a seeded project, or one opened
    /// without a resolver). The decoded source is attached to the take
    /// so the renderer and the next reader see the same audio.
    pub(crate) fn open_or_decode(
        daw: &Standalone,
        project: ProjectContext,
        item: ItemRef,
        take: TakeRef,
    ) -> Option<Self> {
        if let Some(r) = Self::open(daw, project.clone(), item.clone(), take.clone()) {
            return Some(r);
        }
        let guid = project_guid(daw, &project)?;
        let takes = daw.get_takes(project.clone(), item.clone());
        let index = match take {
            TakeRef::Active => takes.iter().position(|t| t.is_active).unwrap_or(0),
            TakeRef::Index(i) => i as usize,
            TakeRef::Guid(ref g) => takes.iter().position(|t| t.guid == *g)?,
        };
        let t = takes.get(index)?;
        let path = t.source_file_path.clone()?;
        let bytes = std::fs::read(&path).ok()?;
        let extension = std::path::Path::new(&path)
            .extension()
            .and_then(|e| e.to_str());
        // The extension is a hint only: symphonia probes the content, and
        // a file named `.wav` that is really a FLAC still decodes.
        let decoded = match extension {
            Some(ext) => crate::audio_engine::decode_audio_with_extension(&bytes, ext),
            None => crate::audio_engine::decode_audio(&bytes),
        }?;
        let take_guid = t.guid.clone();
        daw.with_project_mut(&guid, |p| {
            p.audio_sources
                .insert(take_guid.clone(), Arc::new(AudioSource::Memory(decoded)));
        })
        .ok()?;
        Self::open(daw, project, item, take)
    }

    /// Load-or-build the source-level `.reapeaks` mipmap for this
    /// take's media (see [`crate::peak_store`]). Only for on-disk
    /// PCM-file sources — in-memory decodes scan RAM cheaply and get
    /// no sidecar. Called by the peaks builder, deliberately not by
    /// [`Self::open`]: the first build scans the source once, and the
    /// audio accessor's opens must stay scan-free.
    #[cfg(all(feature = "reapeaks", any(feature = "audio", feature = "decode")))]
    pub(crate) fn ensure_reapeaks(&mut self) {
        if self.reapeaks.is_some() {
            return;
        }
        // A streamed source carrying its original's waveform, handed in by
        // whoever streams it (a browser, which has no disk to look on).
        if let AudioSource::Streamed(streamed) = &*self.source
            && let Some(peaks) = streamed.peaks()
        {
            self.reapeaks = Some(peaks);
            return;
        }
        // A proxy streaming in place of its original: the original's
        // cache, fetched beside it (`Media/Peaks/Bass.wav.sessionpeaks`
        // for `Media/Proxies/Bass.ogg` standing in for `Media/Bass.wav`).
        if let (AudioSource::Streamed(_), Some(proxy), Some(original)) =
            (&*self.source, &self.media_path, &self.original_name)
            && let Some(media) = proxy.parent().and_then(std::path::Path::parent)
        {
            self.reapeaks = crate::peak_store::for_stand_in(&media.join(original), &self.source);
            return;
        }
        // An on-disk file: its cache, built once if it has none. (A page
        // has no disk, and no file sources.)
        #[cfg(not(target_arch = "wasm32"))]
        if matches!(*self.source, AudioSource::PcmFile(_))
            && let Some(path) = &self.media_path
        {
            self.reapeaks = crate::peak_store::get_or_build(path, &self.source);
        }
    }

    #[cfg(not(all(feature = "reapeaks", any(feature = "audio", feature = "decode"))))]
    pub(crate) fn ensure_reapeaks(&mut self) {}

    pub(crate) fn channels(&self) -> u16 {
        self.source.channels()
    }

    pub(crate) fn sample_rate(&self) -> u32 {
        self.source.sample_rate()
    }

    /// Source time (seconds into the media) heard at take time `t`
    /// (seconds from the item start, project scale).
    // r[impl drums.open.accessor-placement]
    pub(crate) fn source_time(&self, t: f64) -> f64 {
        let played = t * self.play_rate;
        match StretchMarker::source_position_at(&self.markers, played) {
            Some(s) => s,
            None => self.start_offset + played,
        }
    }

    /// Source frame (fractional) heard at take time `t`.
    pub(crate) fn source_frame(&self, t: f64) -> f64 {
        self.source_time(t) * self.sample_rate() as f64
    }

    /// One channel at take time `t`, linearly interpolated; silence
    /// outside the item and outside the source.
    pub(crate) fn sample(&self, t: f64, channel: usize) -> f32 {
        if t < 0.0 || t >= self.length {
            return 0.0;
        }
        let f = self.source_frame(t);
        if f < 0.0 {
            return 0.0;
        }
        let i0 = f.floor() as usize;
        let frames = self.source.frame_count();
        if i0 >= frames {
            return 0.0;
        }
        let i1 = (i0 + 1).min(frames.saturating_sub(1));
        self.source
            .channel_interp(i0, i1, (f - i0 as f64) as f32, channel)
    }

    /// Min/max per channel over the source frames heard between take
    /// times `t0..t1`. Interleaved `[ch0_min, ch0_max, ch1_min, ...]`.
    // r[impl drums.open.peaks]
    pub(crate) fn peak_block(&self, t0: f64, t1: f64, out: &mut [f64]) {
        let ch = self.channels().max(1) as usize;
        for c in 0..ch {
            out[c * 2] = 0.0;
            out[c * 2 + 1] = 0.0;
        }
        let t0 = t0.max(0.0);
        let t1 = t1.min(self.length);
        if t1 <= t0 {
            return;
        }
        let frames = self.source.frame_count();
        let (a, b) = (self.source_frame(t0), self.source_frame(t1));
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        let lo = lo.max(0.0).floor() as usize;
        let hi = (hi.ceil() as usize).min(frames);
        if lo >= hi {
            return;
        }
        // Coarse zooms fold from the persistent source mipmap instead of
        // scanning PCM; fine zooms (span below the finest mipmap ratio)
        // still read the source — the mipmap can't resolve them.
        #[cfg(all(feature = "reapeaks", any(feature = "audio", feature = "decode")))]
        if let Some(pk) = &self.reapeaks
            && let Some(fine) = pk.levels.first()
            && (hi - lo) as u64 >= fine.samples_per_peak.max(1) as u64
        {
            for c in 0..ch {
                let (mn, mx) = crate::peak_store::min_max_block(pk, lo, hi, c);
                out[c * 2] = (mn as f64).min(0.0);
                out[c * 2 + 1] = (mx as f64).max(0.0);
            }
            return;
        }
        for c in 0..ch {
            let (mn, mx) = self.source.min_max_block(lo, hi, c);
            out[c * 2] = mn.min(0.0) as f64;
            out[c * 2 + 1] = mx.max(0.0) as f64;
        }
    }
}

fn project_guid(daw: &Standalone, ctx: &ProjectContext) -> Option<String> {
    match ctx {
        ProjectContext::Project(g) => Some(g.clone()),
        ProjectContext::Current => daw
            .state
            .lock()
            .ok()
            .and_then(|s| s.current_project_guid.clone()),
    }
}
