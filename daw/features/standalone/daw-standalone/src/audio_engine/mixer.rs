//! Multi-track audio mixer with cpal output.
//!
//! Playhead + play state live in the shared [`TransportShared`] atomic
//! (see `crate::transport_engine`). The cpal callback reads the
//! playhead, mixes frames, then advances the engine via
//! `shared.advance(frames)` — so the engine drives **both** the soft
//! clock and the audio mixer when audio is active.
//!
//! Per-track gain / mute / solo + the track list are mixer-private
//! state, still held in `Arc<Mutex<MixerState>>`. The mutex is only
//! locked on control-thread mutations + once per audio callback (for
//! the track list snapshot); never blocking under contention because
//! both sides are short-lived.

use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use tracing::{error, info};

use crate::metering::{HOLD_DECAY, Meters};
use crate::sync::Standalone;
use crate::transport_engine::{PlayStateRepr, TransportShared};

use super::DecodedAudio;
use super::aux_render::{AuxClock, AuxRenderer, AuxSlot};
use super::render::ProjectRenderer;
use super::routing;

/// Largest block (in frames) the aux staging buffer covers. Callbacks
/// larger than this fall back to the direct path (no aux hook) so the
/// audio thread never allocates.
const AUX_MAX_FRAMES: usize = 16_384;

// Engine-level device selection. On native this is the shared
// `daw_audio_io::AudioIoPrefs`; on wasm (where `daw_audio_io` is a
// native-only dep) a minimal local shim with the same all-default
// "system default output" meaning keeps the public constructor
// signatures target-agnostic.
#[cfg(not(target_arch = "wasm32"))]
pub use daw_audio_io::AudioIoPrefs;

/// wasm fallback `AudioIoPrefs` — `daw_audio_io` is native-only, so on
/// the web only the all-default value is meaningful (system default
/// output device, native rate, default buffer, no input).
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioIoPrefs {
    pub input_device: String,
    pub output_device: String,
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub want_input: bool,
}

/// Resolved output device + stream config, opened from [`AudioIoPrefs`].
/// Two target impls: native goes through `daw_audio_io::open_output`
/// (JACK-aware device selection); wasm uses the cpal default host.
struct ResolvedOutput {
    device: cpal::Device,
    config: StreamConfig,
    sample_format: SampleFormat,
    sample_rate: u32,
    channels: u16,
}

impl ResolvedOutput {
    #[cfg(not(target_arch = "wasm32"))]
    fn open(prefs: &AudioIoPrefs) -> Result<Self, String> {
        let host = daw_audio_io::audio_host();
        let out = daw_audio_io::open_output(&host, prefs)?;
        Ok(Self {
            sample_rate: out.sample_rate,
            channels: out.channels,
            sample_format: out.sample_format,
            config: out.config,
            device: out.device,
        })
    }

    #[cfg(target_arch = "wasm32")]
    fn open(_prefs: &AudioIoPrefs) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("No audio output device found")?;
        let supported = device
            .default_output_config()
            .map_err(|e| format!("Failed to get default output config: {e}"))?;
        let sample_rate = supported.sample_rate();
        let channels = supported.channels();
        Ok(Self {
            config: StreamConfig {
                channels,
                sample_rate,
                buffer_size: cpal::BufferSize::Default,
            },
            sample_format: supported.sample_format(),
            sample_rate,
            channels,
            device,
        })
    }
}

/// Handle to a loaded track in the audio engine.
///
/// Use this to control per-track gain, mute, and solo after loading.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TrackHandle(pub(crate) usize);

/// Per-track audio state shared with the mixer callback.
struct TrackAudio {
    /// Decoded PCM data for this track
    buffer: Arc<DecodedAudio>,
    /// Linear gain multiplier (0.0 = silent, 1.0 = unity)
    gain: f32,
    /// Whether this track is muted
    muted: bool,
    /// Whether this track is soloed
    soloed: bool,
    /// Index of this track's cell in the [`Meters`] bank — i.e. the *project*
    /// track index this audio is metering, which need not equal the mixer's own
    /// track ordering.
    meter_index: usize,
}

/// Mixer-private state. Playhead + play state are *not* here — they
/// live in [`TransportShared`].
struct MixerState {
    /// Output sample rate (mirrors `shared.sample_rate()`; cached so
    /// the callback doesn't take an atomic load per frame).
    sample_rate: u32,
    /// Output channel count.
    channels: u16,
    /// All loaded tracks.
    tracks: Vec<TrackAudio>,
    /// Master gain.
    master_gain: f32,
    /// Per-track peak-meter bank, written once per block from `fill_buffer`.
    meters: Arc<Meters>,
}

impl MixerState {
    /// Check if any track is soloed
    fn any_soloed(&self) -> bool {
        self.tracks.iter().any(|t| t.soloed)
    }

    /// Mix all tracks into the output buffer starting at `position`
    /// (in output-rate sample frames). Does not advance the engine —
    /// the caller (audio callback) does that.
    ///
    /// As a side effect, each contributing track's L/R block peak is written
    /// into the [`Meters`] bank (after master gain) so the UI can show real
    /// per-track levels. When stopped (or silent) every cell is written `0`, so
    /// the meters decay to silence rather than freezing.
    fn fill_buffer(&self, output: &mut [f32], position: i64, playing: bool) {
        let channels = self.channels as usize;

        // Per-track block peak accumulators (L, R), indexed by meter cell.
        let mut peaks = vec![(0.0f32, 0.0f32); self.meters.len()];

        if channels == 0 || !playing {
            output.fill(0.0);
            self.flush_meters(&peaks);
            return;
        }

        let num_frames = output.len() / channels;
        let any_soloed = self.any_soloed();

        output.fill(0.0);

        for track in &self.tracks {
            if track.muted || (any_soloed && !track.soloed) || track.gain == 0.0 {
                continue;
            }

            let buf = &track.buffer;
            let track_channels = buf.channels as usize;
            let track_rate = buf.sample_rate;
            let (mut peak_l, mut peak_r) = (0.0f32, 0.0f32);

            for frame in 0..num_frames {
                let out_frame = position + frame as i64;
                if out_frame < 0 {
                    continue;
                }
                let out_frame_u = out_frame as u64;
                let track_frame = if track_rate == self.sample_rate {
                    out_frame_u as usize
                } else {
                    (out_frame_u as f64 * track_rate as f64 / self.sample_rate as f64) as usize
                };

                if track_frame >= buf.frame_count() {
                    continue;
                }

                let src_offset = track_frame * track_channels;
                let dst_offset = frame * channels;

                for ch in 0..channels {
                    let src_ch = if ch < track_channels { ch } else { 0 };
                    let sample = buf.samples.get(src_offset + src_ch).copied().unwrap_or(0.0);
                    let contribution = sample * track.gain * self.master_gain;
                    output[dst_offset + ch] += contribution;
                    match ch {
                        0 => peak_l = peak_l.max(contribution.abs()),
                        1 => peak_r = peak_r.max(contribution.abs()),
                        _ => {}
                    }
                }
            }

            // Mono sources meter as dual-mono so the strip's two columns match.
            if track_channels < 2 {
                peak_r = peak_l;
            }
            if let Some(slot) = peaks.get_mut(track.meter_index) {
                slot.0 = slot.0.max(peak_l);
                slot.1 = slot.1.max(peak_r);
            }
        }

        self.flush_meters(&peaks);
    }

    /// Push the per-track block peaks into the shared [`Meters`] bank.
    fn flush_meters(&self, peaks: &[(f32, f32)]) {
        for (i, &(l, r)) in peaks.iter().enumerate() {
            if let Some(cell) = self.meters.cell(i) {
                cell.write(l, r, HOLD_DECAY);
            }
        }
    }
}

/// Multi-track audio engine.
///
/// Load audio tracks, control playback (play/stop/seek), adjust per-track
/// gain/mute/solo. Audio is mixed and output via cpal on all platforms.
pub struct AudioEngine {
    state: Arc<Mutex<MixerState>>,
    shared: Arc<TransportShared>,
    /// Set by the cpal stream's error callback when the output device
    /// dies (e.g. PipeWire drops the connection mid-playback). The stream
    /// stops firing, so the callback can no longer drive `advance()`; the
    /// error handler re-enables the project soft clock immediately (playhead
    /// keeps moving, silently) and raises this flag so a supervisor can
    /// reopen the device. Observe via [`stream_errored`](Self::stream_errored).
    stream_error: Arc<AtomicBool>,
    /// Post-render aux hook slot (project mode only; see
    /// [`aux_render`](super::aux_render)). The audio callback `try_lock`s
    /// it once per block; [`set_aux_renderer`](Self::set_aux_renderer)
    /// installs the hook from a control thread.
    aux: AuxSlot,
    // cpal stream is kept alive; dropping it stops audio output
    _stream: Stream,
    /// Live hardware-input stream (project mode, when any track records
    /// from a hardware channel or `prefs.want_input`). Kept alive so the
    /// input callback keeps feeding the renderer's ring; dropping the
    /// engine tears it down. `None` for pure-playback engines.
    #[cfg(not(target_arch = "wasm32"))]
    _input_stream: Option<Stream>,
    /// Media read-ahead worker (project mode only) — stops with the engine.
    #[cfg(not(target_arch = "wasm32"))]
    _prefetch: Option<super::prefetch::PrefetchWorker>,
    /// Realtime metrics from the output callback — the block size the
    /// device actually runs at, render time, over-budget blocks. Same
    /// type the duplex engine reports. Project mode only.
    #[cfg(not(target_arch = "wasm32"))]
    stats: Option<Arc<daw_audio_io::duplex::EngineStats>>,
}

impl AudioEngine {
    /// Create a new audio engine with its own private [`TransportShared`].
    /// Use [`with_shared`](Self::with_shared) when wiring into a
    /// `Standalone` so the engine + service share a playhead.
    pub fn new() -> Result<Self, String> {
        Self::with_shared(Arc::new(TransportShared::new(48_000, 120.0)))
    }

    /// Attach a private-mixer engine to a project's transport clock *and*
    /// peak-meter bank: shares `daw`'s playhead (disabling the soft clock so the
    /// audio callback drives `advance`), and installs a freshly-sized
    /// [`Meters`] bank on `daw` so per-track levels reach the `Peaks` service.
    /// Load one track per project track with [`add_track_metered`](Self::add_track_metered).
    pub fn metered_for(
        daw: &Standalone,
        project_guid: &str,
        track_count: usize,
    ) -> Result<Self, String> {
        let bundle = daw.transport_engine_for(project_guid);
        bundle.disable_soft_clock();
        let meters = Meters::new(track_count);
        daw.set_meters(meters.clone());
        Self::with_shared_metered(bundle.shared.clone(), meters)
    }

    /// Create an engine wired to a specific project on `daw`. The
    /// callback renders via [`ProjectRenderer`] every block, so the
    /// project's tracks / items / routing / audio sources are heard
    /// the moment they're loaded — no `add_track` boilerplate needed.
    ///
    /// Sample rate, transport shared state, and soft-clock disable are
    /// all handled here. Drop the returned `AudioEngine` to stop the
    /// stream.
    pub fn attached_to(daw: &Standalone, project_guid: &str) -> Result<Self, String> {
        Self::attached_to_prefs(daw, project_guid, &AudioIoPrefs::default())
    }

    /// As [`attached_to`](Self::attached_to), but the device, sample rate
    /// and buffer size come from `prefs`. A requested rate is applied to the
    /// device itself (CoreAudio's nominal rate); a requested buffer must be
    /// inside the device's supported range or opening fails.
    pub fn attached_to_prefs(
        daw: &Standalone,
        project_guid: &str,
        prefs: &AudioIoPrefs,
    ) -> Result<Self, String> {
        let bundle = daw.transport_engine_for(project_guid);
        bundle.disable_soft_clock();
        // One meter cell per project track — `ProjectRenderer` writes
        // post-fader block peaks during playback.
        let track_count = daw
            .read_project(project_guid, |p| p.tracks.len())
            .unwrap_or(0);
        daw.set_meters(crate::metering::Meters::new(track_count));
        Self::with_project_prefs(
            daw.clone(),
            project_guid.to_string(),
            bundle.shared.clone(),
            prefs,
        )
    }

    /// Create an engine that shares its sample clock with the given
    /// `TransportShared`. The shared sample-rate is rewritten to the
    /// device's actual rate at startup. Metering is disabled (empty bank).
    ///
    /// Opens the default output device (native rate, default buffer) —
    /// equivalent to [`with_shared_prefs`](Self::with_shared_prefs) with
    /// `AudioIoPrefs::default()`.
    pub fn with_shared(shared: Arc<TransportShared>) -> Result<Self, String> {
        Self::with_shared_metered(shared, Meters::empty())
    }

    /// As [`with_shared`](Self::with_shared), but the mixer writes per-track
    /// block peaks into `meters` (one cell per project track index).
    pub fn with_shared_metered(
        shared: Arc<TransportShared>,
        meters: Arc<Meters>,
    ) -> Result<Self, String> {
        Self::with_shared_prefs(shared, meters, &AudioIoPrefs::default())
    }

    /// As [`with_shared_metered`](Self::with_shared_metered), but the output
    /// device / sample rate / buffer come from `prefs` (resolved via
    /// `daw_audio_io::open_output`). `AudioIoPrefs::default()` reproduces the
    /// classic default-output-device, native-rate, default-buffer behavior.
    pub fn with_shared_prefs(
        shared: Arc<TransportShared>,
        meters: Arc<Meters>,
        prefs: &AudioIoPrefs,
    ) -> Result<Self, String> {
        let out = ResolvedOutput::open(prefs)?;
        let sample_rate = out.sample_rate;
        let channels = out.channels;

        info!(
            "Audio engine: {} channels, {} Hz, format {:?}, {} meter cells",
            channels,
            sample_rate,
            out.sample_format,
            meters.len(),
        );

        shared.set_sample_rate(sample_rate);

        let state = Arc::new(Mutex::new(MixerState {
            sample_rate,
            channels,
            tracks: Vec::new(),
            master_gain: 1.0,
            meters,
        }));

        let config = out.config;
        let stream_error = Arc::new(AtomicBool::new(false));
        let stream = match out.sample_format {
            SampleFormat::F32 => Self::build_stream::<f32>(
                &out.device,
                &config,
                state.clone(),
                shared.clone(),
                stream_error.clone(),
            )?,
            SampleFormat::I16 => Self::build_stream::<i16>(
                &out.device,
                &config,
                state.clone(),
                shared.clone(),
                stream_error.clone(),
            )?,
            SampleFormat::U16 => Self::build_stream::<u16>(
                &out.device,
                &config,
                state.clone(),
                shared.clone(),
                stream_error.clone(),
            )?,
            format => return Err(format!("Unsupported sample format: {format:?}")),
        };

        stream
            .play()
            .map_err(|e| format!("Failed to start audio stream: {e}"))?;

        Ok(Self {
            state,
            shared,
            stream_error,
            // Slot exists for API symmetry but is never read by the
            // private-mixer callback — the aux hook only fires in
            // project mode (`with_project*` / `attached_to`).
            aux: AuxSlot::default(),
            _stream: stream,
            #[cfg(not(target_arch = "wasm32"))]
            _input_stream: None,
            // Private-track-list mode mixes from memory — no mmap to warm.
            #[cfg(not(target_arch = "wasm32"))]
            _prefetch: None,
            #[cfg(not(target_arch = "wasm32"))]
            #[cfg(not(target_arch = "wasm32"))]
            stats: None,
        })
    }

    /// Project-mode constructor. Same as [`with_shared`](Self::with_shared)
    /// but the callback renders via [`ProjectRenderer`] for
    /// `(daw, project_guid)` instead of mixing the engine's private
    /// track list. Use [`attached_to`](Self::attached_to) for the
    /// common case where you want to discover/share the
    /// `TransportShared` from a `Standalone`.
    pub fn with_project(
        daw: Standalone,
        project_guid: String,
        shared: Arc<TransportShared>,
    ) -> Result<Self, String> {
        Self::with_project_prefs(daw, project_guid, shared, &AudioIoPrefs::default())
    }

    /// As [`with_project`](Self::with_project), but the output device /
    /// sample rate / buffer come from `prefs`, and — when `prefs.want_input`
    /// is set OR any project track records from a hardware channel
    /// (`RecordInput::Audio`) — a cpal **input** stream is opened too. The
    /// input callback pushes interleaved frames into a lock-free `rtrb` ring;
    /// the `ProjectRenderer` drains it in stage 0, routing each track's tapped
    /// channel into its bus (and FX chain).
    ///
    /// `AudioIoPrefs::default()` (no input device, `want_input == false`,
    /// native rate, default buffer) reproduces [`with_project`]'s classic
    /// output-only behavior unless a track itself requests audio input.
    pub fn with_project_prefs(
        daw: Standalone,
        project_guid: String,
        shared: Arc<TransportShared>,
        prefs: &AudioIoPrefs,
    ) -> Result<Self, String> {
        let out = ResolvedOutput::open(prefs)?;
        let sample_rate = out.sample_rate;
        let channels = out.channels;
        info!(
            "Audio engine (project mode): {} channels, {} Hz, format {:?}, guid={}",
            channels, sample_rate, out.sample_format, project_guid,
        );
        shared.set_sample_rate(sample_rate);

        // The project-render path meters via `ProjectRenderer`, not the private
        // track list, so the mixer's own bank stays empty here.
        let state = Arc::new(Mutex::new(MixerState {
            sample_rate,
            channels,
            tracks: Vec::new(),
            master_gain: 1.0,
            meters: Meters::empty(),
        }));

        // One renderer for the stream's lifetime — its snapshot cache only
        // rebuilds when the project revision moves, so steady playback
        // re-walks nothing.
        let renderer = Arc::new(ProjectRenderer::new(&daw, &project_guid, sample_rate));
        // A live callback: never wait on the project lock, never build a
        // snapshot on the audio thread.
        #[cfg(not(target_arch = "wasm32"))]
        renderer.set_realtime();

        // Live / programmatic MIDI ring: the UI thread pushes events
        // through `Standalone::push_note_on`/`_off`/`_cc` (producer), the
        // renderer drains them once per block (consumer). SPSC + lock-free
        // like the live-input ring. Sized for a generous burst of events.
        {
            let (prod, cons) = rtrb::RingBuffer::<super::render::LiveMidiEvent>::new(4096);
            renderer.set_live_midi(cons);
            daw.set_live_midi_producer(prod);
        }

        // Live input: open an input stream when prefs ask for it OR any track
        // records from a hardware channel. The highest tapped channel sets how
        // many input channels we must open to reach it. Native-only —
        // `daw_audio_io` (and live capture) don't exist on the web.
        //
        // Not on iOS for a track's input alone: there the input is a second
        // RemoteIO unit beside the output's, and the session's microphone
        // with it — for a phone that only plays, opened for nothing. There
        // it opens when asked for.
        #[cfg(not(target_arch = "wasm32"))]
        let input_stream = {
            let max_armed = Self::max_audio_input_channel(&daw, &project_guid);
            let tracks_want_input = max_armed.is_some() && cfg!(not(target_os = "ios"));
            if prefs.want_input || tracks_want_input {
                let max_channel = max_armed.unwrap_or(0);
                let host = daw_audio_io::audio_host();
                match Self::open_live_input(&host, prefs, sample_rate, max_channel, &renderer) {
                    Ok(s) => Some(s),
                    Err(e) => {
                        error!("Audio engine: live input unavailable ({e}); output-only");
                        None
                    }
                }
            } else {
                None
            }
        };

        let config = out.config;
        let aux: AuxSlot = AuxSlot::default();
        // Recovery wiring: if the output device dies mid-playback, the stream's
        // error callback re-enables THIS project's soft clock (so the playhead
        // keeps advancing) and raises `stream_error` for the supervisor to
        // reopen the device. The soft-clock flag is the same `Arc` the audio
        // callback disabled when it took over driving `advance()`.
        let soft_clock_enabled = daw
            .transport_engine_for(&project_guid)
            .soft_clock_enabled
            .clone();
        let stream_error = Arc::new(AtomicBool::new(false));
        #[cfg(not(target_arch = "wasm32"))]
        let stats = Arc::new(daw_audio_io::duplex::EngineStats::default());
        let stream = match out.sample_format {
            SampleFormat::F32 => Self::build_project_stream::<f32>(
                &out.device,
                &config,
                renderer.clone(),
                shared.clone(),
                aux.clone(),
                soft_clock_enabled.clone(),
                stream_error.clone(),
                #[cfg(not(target_arch = "wasm32"))]
                stats.clone(),
            )?,
            SampleFormat::I16 => Self::build_project_stream::<i16>(
                &out.device,
                &config,
                renderer.clone(),
                shared.clone(),
                aux.clone(),
                soft_clock_enabled.clone(),
                stream_error.clone(),
                #[cfg(not(target_arch = "wasm32"))]
                stats.clone(),
            )?,
            SampleFormat::U16 => Self::build_project_stream::<u16>(
                &out.device,
                &config,
                renderer.clone(),
                shared.clone(),
                aux.clone(),
                soft_clock_enabled.clone(),
                stream_error.clone(),
                #[cfg(not(target_arch = "wasm32"))]
                stats.clone(),
            )?,
            format => return Err(format!("Unsupported sample format: {format:?}")),
        };
        stream
            .play()
            .map_err(|e| format!("Failed to start audio stream: {e}"))?;
        // Media read-ahead: warm mmap pages ahead of the playhead so
        // the callback never page-faults on cold USB/network media.
        #[cfg(not(target_arch = "wasm32"))]
        let prefetch = Some(super::prefetch::PrefetchWorker::spawn(
            daw,
            project_guid,
            shared.clone(),
        ));
        Ok(Self {
            state,
            shared,
            stream_error,
            aux,
            _stream: stream,
            #[cfg(not(target_arch = "wasm32"))]
            _input_stream: input_stream,
            #[cfg(not(target_arch = "wasm32"))]
            _prefetch: prefetch,
            #[cfg(not(target_arch = "wasm32"))]
            #[cfg(not(target_arch = "wasm32"))]
            stats: Some(stats),
        })
    }

    /// Install (or replace) the post-render aux hook — see
    /// [`aux_render`](super::aux_render) for the contract. Project-mode
    /// engines call it once per audio callback after the project block is
    /// written into the **interleaved** f32 staging buffer (and before
    /// device-format conversion); the hook ADDS into that buffer. It is
    /// also called while stopped (`playing == false`, zeroed buffer) so
    /// tails can flush. The audio callback only `try_lock`s the slot, so
    /// installing never blocks the audio thread. No-op audio-wise on
    /// private-mixer engines ([`new`](Self::new) / [`with_shared`](Self::with_shared)).
    pub fn set_aux_renderer(&self, hook: AuxRenderer) {
        if let Ok(mut slot) = self.aux.lock() {
            *slot = Some(hook);
        }
    }

    /// Remove the aux hook (dropped off the audio thread, here).
    pub fn clear_aux_renderer(&self) {
        if let Ok(mut slot) = self.aux.lock() {
            *slot = None;
        }
    }

    /// Highest hardware input channel any track records from
    /// (`RecordInput::Audio { channel }`), or `None` when no track does.
    #[cfg(not(target_arch = "wasm32"))]
    fn max_audio_input_channel(daw: &Standalone, project_guid: &str) -> Option<usize> {
        daw.read_project(project_guid, |p| {
            p.tracks
                .iter()
                .filter_map(|t| match p.track_ext.get(&t.guid).map(|e| e.record_input) {
                    Some(daw_proto::track::RecordInput::Audio { channel }) => {
                        Some(channel as usize)
                    }
                    _ => None,
                })
                .max()
        })
        .flatten()
    }

    /// Open the cpal input stream + lock-free ring and hand the consumer to
    /// `renderer` (stage 0 drains it). The input callback pushes the
    /// **interleaved** f32 frames it receives (all `inp.channels`) into the
    /// ring; overruns are counted. f32 input only.
    #[cfg(not(target_arch = "wasm32"))]
    fn open_live_input(
        host: &cpal::Host,
        prefs: &daw_audio_io::AudioIoPrefs,
        sample_rate: u32,
        max_channel: usize,
        renderer: &ProjectRenderer,
    ) -> Result<Stream, String> {
        use cpal::traits::{DeviceTrait, StreamTrait};
        use std::sync::atomic::{AtomicU64, Ordering};

        let inp = daw_audio_io::open_input(host, prefs, sample_rate, max_channel)?;
        let channels = inp.channels;
        // Ring sized for ~100ms of interleaved input (min 8192 frames worth),
        // matching the duplex monitor's headroom.
        let capacity = ((sample_rate as usize / 10).max(8192 * channels)).max(channels);
        let (mut prod, cons) = rtrb::RingBuffer::<f32>::new(capacity);
        renderer.set_live_input(cons, channels);

        let overruns = Arc::new(AtomicU64::new(0));
        let overruns_cb = overruns.clone();
        let mut realtime = false;
        let stream = inp
            .device
            .build_input_stream(
                inp.config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if !realtime {
                        realtime = true;
                        let _ = daw_audio_io::realtime::promote_current_thread();
                    }
                    // Push every interleaved sample (all channels); the
                    // renderer de-interleaves per track in stage 0.
                    for &s in data {
                        if prod.push(s).is_err() {
                            overruns_cb.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                },
                move |err| {
                    if is_xrun(&err) {
                        tracing::warn!("Audio input stream xrun: {err}");
                    } else {
                        error!("Audio input stream error: {err}");
                    }
                },
                None,
            )
            .map_err(|e| format!("Failed to build input stream: {e}"))?;
        stream
            .play()
            .map_err(|e| format!("Failed to start input stream: {e}"))?;
        info!(
            "Audio engine: live input open — {} channels, {} Hz, max armed channel {}",
            channels, sample_rate, max_channel,
        );
        Ok(stream)
    }

    fn build_project_stream<T: cpal::SizedSample + cpal::FromSample<f32>>(
        device: &cpal::Device,
        config: &StreamConfig,
        renderer: Arc<ProjectRenderer>,
        shared: Arc<TransportShared>,
        aux: AuxSlot,
        soft_clock_enabled: Arc<AtomicBool>,
        stream_error: Arc<AtomicBool>,
        #[cfg(not(target_arch = "wasm32"))] stats: Arc<daw_audio_io::duplex::EngineStats>,
    ) -> Result<Stream, String> {
        let channels = config.channels as usize;
        // Publish the device's real channel count so the metronome UI can
        // offer every output pair, and default the routing pairs sensibly.
        routing::MixerRouting::shared().set_channel_count(channels);
        let sample_rate = config.sample_rate;
        let daw = renderer.daw().clone();
        // Interleaved f32 staging buffer, pre-sized so the callback
        // never allocates. The block is written here first, the aux
        // hook adds into it, then it's converted to the device format.
        let mut stage: Vec<f32> = vec![0.0; AUX_MAX_FRAMES * channels.max(1)];
        // Runs the aux hook (if installed) over the staged block.
        // try_lock only: a control-thread install in progress just
        // skips the hook for this block.
        let run_aux = move |aux: &AuxSlot,
                            renderer: &ProjectRenderer,
                            buf: &mut [f32],
                            playing: bool,
                            pos_seconds: f64,
                            playrate: f64| {
            let Ok(mut slot) = aux.try_lock() else { return };
            let Some(hook) = slot.as_mut() else { return };
            let (pos_beats, tempo_bpm, time_sig_num, time_sig_den) =
                renderer.clock_info(pos_seconds);
            let clock = AuxClock {
                playing,
                pos_seconds,
                pos_beats,
                tempo_bpm,
                time_sig_num,
                time_sig_den,
                sample_rate: sample_rate as f64,
                channels,
                playrate,
            };
            hook(buf, &clock);
        };
        #[cfg(not(target_arch = "wasm32"))]
        let stats_on_error = stats.clone();
        // When each buffer really starts, in the sync clock: a DLL over
        // callback entry times (scheduler jitter filtered out). Owned by
        // the callback; stamps every buffer's sync snapshot.
        let mut stamps = daw_transport_sync::BufferClock::default();
        // The callback's thread goes realtime on its first block (see
        // `daw_audio_io::realtime`).
        let mut realtime = false;
        let stream = device
            .build_output_stream(
                *config,
                move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                    if !realtime {
                        realtime = true;
                        let _ = daw_audio_io::realtime::promote_current_thread();
                    }
                    let entry_micros = crate::transport_sync::now_micros();
                    let num_samples = data.len();
                    if channels == 0 || num_samples == 0 {
                        return;
                    }
                    let num_frames = num_samples / channels;
                    // Open the buffer: stamp it, land a scheduled locate
                    // that falls in it, commit the playhead, publish the
                    // sync snapshot. The plan says what to render.
                    let stamp =
                        stamps.tick(entry_micros, num_frames as u32, f64::from(sample_rate));
                    let plan = shared.begin_block(num_frames as u32, stamp, stamps.period_micros());
                    #[cfg(not(target_arch = "wasm32"))]
                    let t0 = std::time::Instant::now();
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        stats.calls.fetch_add(1, AtomicOrdering::Relaxed);
                        stats
                            .block_frames
                            .store(num_frames as u32, AtomicOrdering::Relaxed);
                    }
                    let playing = plan.any_playing();
                    // One clock per block for the aux hook: the run that
                    // plays (a locate landing mid-buffer: the new one).
                    let clock_run = plan.last_playing().unwrap_or(plan.first());
                    let pos_seconds =
                        clock_run.start_at_frame_zero().max(0.0) / f64::from(sample_rate);
                    let playrate = clock_run.rate;
                    // Oversized blocks bypass the staging buffer (and the
                    // aux hook) rather than allocate on the audio thread.
                    let use_stage = num_samples <= stage.len();

                    if !playing {
                        // Meters fall to silence instead of freezing
                        // at the last rendered block's peaks.
                        let meters = daw.meters();
                        for i in 0..meters.len() {
                            if let Some(cell) = meters.cell(i) {
                                cell.write(0.0, 0.0, crate::metering::HOLD_DECAY);
                            }
                        }
                        if use_stage {
                            let stage = &mut stage[..num_samples];
                            stage.fill(0.0);
                            // Stopped: zeroed block, hook flushes tails
                            // (count-in / section cues) onto the guide pair.
                            run_aux(&aux, &renderer, stage, false, pos_seconds, playrate);
                            // Sum the guide into the headphone-check bus even
                            // while stopped, and honor the main mute.
                            routing::finish_routing(
                                stage,
                                channels,
                                num_frames,
                                routing::MixerRouting::shared().snapshot(),
                            );
                            for (out, &v) in data.iter_mut().zip(stage.iter()) {
                                *out = T::from_sample(v);
                            }
                        } else {
                            for s in data.iter_mut() {
                                *s = T::from_sample(0.0);
                            }
                        }
                        return;
                    }

                    // Render. The renderer briefly acquires the project
                    // Mutex (revision check; full re-walk only after an
                    // edit). Future work: lock-free RCU snapshot so the
                    // callback never blocks. Varispeed + a landing locate
                    // are in the plan.
                    let block = renderer.render_plan(&plan, false);

                    // Interleave the stereo block (handle channel-count
                    // mismatch by duplicating or summing), staging as f32
                    // so the aux hook can add before format conversion.
                    let snap = routing::MixerRouting::shared().snapshot();
                    if use_stage {
                        let stage = &mut stage[..num_samples];
                        // Main mix onto the configured main pair (zeroes the
                        // block first); the aux hook then adds the guide onto
                        // the guide pair.
                        routing::stage_main(stage, channels, num_frames, &block.samples, snap);
                        run_aux(&aux, &renderer, stage, true, pos_seconds, playrate);
                        // Headphone-check sum (main + guide) + main mute.
                        routing::finish_routing(stage, channels, num_frames, snap);
                        for (out, &v) in data.iter_mut().zip(stage.iter()) {
                            *out = T::from_sample(v);
                        }
                    } else {
                        // Oversized block: bypass staging (and the aux hook,
                        // so no guide/phones bus) — route the main mix onto
                        // the main pair, honoring the mute.
                        let (ml, mr) = snap.main;
                        for s in data.iter_mut() {
                            *s = T::from_sample(0.0);
                        }
                        if !snap.main_muted {
                            for frame in 0..num_frames {
                                let l = block.samples.get(frame * 2).copied().unwrap_or(0.0);
                                let r = block.samples.get(frame * 2 + 1).copied().unwrap_or(0.0);
                                let base = frame * channels;
                                if ml < channels {
                                    data[base + ml] = T::from_sample(l);
                                }
                                if mr < channels {
                                    data[base + mr] = T::from_sample(r);
                                }
                            }
                        }
                    }

                    // (The playhead was committed by `begin_block`.)
                    #[cfg(not(target_arch = "wasm32"))]
                    stats.record_render_at(t0.elapsed().as_nanos() as u64, sample_rate);
                },
                move |err| {
                    // An xrun is a glitch, not a death: CoreAudio reports
                    // every underrun here and the stream keeps running.
                    // Treating it as a dead device re-enabled the soft
                    // clock beside a callback that was still advancing the
                    // playhead, and asked for a pointless re-attach.
                    if is_xrun(&err) {
                        #[cfg(not(target_arch = "wasm32"))]
                        stats_on_error.xruns.fetch_add(1, AtomicOrdering::Relaxed);
                        tracing::warn!("Audio stream xrun: {err}");
                        return;
                    }
                    // Device died (e.g. PipeWire dropped the connection). The
                    // callback that drives `advance()` will stop firing, so
                    // re-enable the soft clock NOW — the playhead must never
                    // freeze from a dead device (play would silently do nothing).
                    // Raise `stream_error` so the audio supervisor reopens the
                    // device and audio comes back on its own.
                    soft_clock_enabled.store(true, AtomicOrdering::Relaxed);
                    stream_error.store(true, AtomicOrdering::Relaxed);
                    error!(
                        "Audio stream error: {err} — soft clock re-enabled (playhead continues); \
                         flagged for device re-attach"
                    );
                },
                None,
            )
            .map_err(|e| format!("Failed to build output stream: {e}"))?;
        Ok(stream)
    }

    /// Realtime metrics from the output callback (project mode): the block
    /// size the device actually runs at, render time, over-budget blocks.
    /// `None` for the legacy track-list engines.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn stats(&self) -> Option<Arc<daw_audio_io::duplex::EngineStats>> {
        self.stats.clone()
    }

    /// Access the shared transport state — useful for wiring into the
    /// transport service / soft clock.
    pub fn shared(&self) -> &Arc<TransportShared> {
        &self.shared
    }

    /// `true` once the output device's error callback has fired (the stream is
    /// dead). The engine has already re-enabled the project soft clock so the
    /// playhead keeps advancing; a supervisor should drop this engine and
    /// re-attach to reopen the device. Latched — stays `true` until the engine
    /// is dropped and replaced.
    pub fn stream_errored(&self) -> bool {
        self.stream_error.load(AtomicOrdering::Relaxed)
    }

    fn build_stream<T: cpal::SizedSample + cpal::FromSample<f32>>(
        device: &cpal::Device,
        config: &StreamConfig,
        state: Arc<Mutex<MixerState>>,
        shared: Arc<TransportShared>,
        stream_error: Arc<AtomicBool>,
    ) -> Result<Stream, String> {
        let channels = config.channels as usize;
        let max_buffer_size = 8192 * channels;
        let mix_buffer = Arc::new(Mutex::new(vec![0.0f32; max_buffer_size]));

        let mut realtime = false;
        let stream = device
            .build_output_stream(
                *config,
                move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                    if !realtime {
                        realtime = true;
                        let _ = daw_audio_io::realtime::promote_current_thread();
                    }
                    let num_samples = data.len();
                    let mut mix = mix_buffer.lock().unwrap();
                    if mix.len() < num_samples {
                        mix.resize(num_samples, 0.0);
                    }

                    // Snapshot engine state — playhead at *start* of
                    // block, advance after mixing so the next block
                    // sees the post-advance position.
                    let playing = shared.play_state().is_advancing();
                    let start = shared.playhead_samples().0;
                    let num_frames = (num_samples / channels) as u32;

                    {
                        let st = state.lock().unwrap();
                        st.fill_buffer(&mut mix[..num_samples], start, playing);
                    }

                    if playing {
                        shared.advance(num_frames);
                    }

                    for (out, &mixed) in data.iter_mut().zip(mix.iter()) {
                        *out = T::from_sample(mixed);
                    }
                },
                move |err| {
                    if is_xrun(&err) {
                        tracing::warn!("Audio stream xrun: {err}");
                        return;
                    }
                    // Private-mixer path has no project soft clock to fall back
                    // on, but still flag the death so a supervisor can reopen.
                    stream_error.store(true, AtomicOrdering::Relaxed);
                    error!("Audio stream error: {err} — flagged for device re-attach");
                },
                None,
            )
            .map_err(|e| format!("Failed to build output stream: {e}"))?;

        Ok(stream)
    }

    // ─── Track Management ────────────────────────────────────────────────

    /// Load a decoded audio buffer as a new track. Returns a handle for control.
    /// The track meters into the cell matching its own load order; use
    /// [`add_track_metered`](Self::add_track_metered) to meter into a specific
    /// project track index instead.
    pub fn add_track(&self, audio: DecodedAudio) -> TrackHandle {
        let index = self.state.lock().unwrap().tracks.len();
        self.add_track_metered(audio, index)
    }

    /// Load a decoded buffer as a new track that meters into `meter_index` (the
    /// project track index this audio represents). Returns a handle for control.
    pub fn add_track_metered(&self, audio: DecodedAudio, meter_index: usize) -> TrackHandle {
        let mut state = self.state.lock().unwrap();
        let index = state.tracks.len();
        state.tracks.push(TrackAudio {
            buffer: Arc::new(audio),
            gain: 1.0,
            muted: false,
            soloed: false,
            meter_index,
        });
        info!("Added track {index} (meter cell {meter_index})");
        TrackHandle(index)
    }

    /// Remove all tracks.
    pub fn clear_tracks(&self) {
        let mut state = self.state.lock().unwrap();
        state.tracks.clear();
        self.shared
            .set_playhead(crate::transport_engine::InstantSamples(0));
        info!("Cleared all tracks");
    }

    /// Get the number of loaded tracks.
    pub fn track_count(&self) -> usize {
        self.state.lock().unwrap().tracks.len()
    }

    // ─── Per-Track Control ───────────────────────────────────────────────

    /// Set the gain (volume) for a track. 0.0 = silent, 1.0 = unity.
    pub fn set_track_gain(&self, handle: TrackHandle, gain: f32) {
        let mut state = self.state.lock().unwrap();
        if let Some(track) = state.tracks.get_mut(handle.0) {
            track.gain = gain.max(0.0);
        }
    }

    /// Get the gain for a track.
    pub fn track_gain(&self, handle: TrackHandle) -> f32 {
        self.state
            .lock()
            .unwrap()
            .tracks
            .get(handle.0)
            .map(|t| t.gain)
            .unwrap_or(0.0)
    }

    /// Set mute state for a track.
    pub fn set_track_muted(&self, handle: TrackHandle, muted: bool) {
        let mut state = self.state.lock().unwrap();
        if let Some(track) = state.tracks.get_mut(handle.0) {
            track.muted = muted;
        }
    }

    /// Set solo state for a track.
    pub fn set_track_soloed(&self, handle: TrackHandle, soloed: bool) {
        let mut state = self.state.lock().unwrap();
        if let Some(track) = state.tracks.get_mut(handle.0) {
            track.soloed = soloed;
        }
    }

    // ─── Transport Control (delegated to TransportShared) ────────────────

    /// Start or resume playback from the current position.
    pub fn play(&self) {
        self.shared.set_play_state(PlayStateRepr::Playing);
        info!(
            "Playback started at frame {}",
            self.shared.playhead_samples().0
        );
    }

    /// Pause playback, preserving position.
    pub fn pause(&self) {
        self.shared.set_play_state(PlayStateRepr::Paused);
        info!(
            "Playback paused at frame {}",
            self.shared.playhead_samples().0
        );
    }

    /// Stop playback and reset position to start.
    pub fn stop(&self) {
        self.shared.set_play_state(PlayStateRepr::Stopped);
        self.shared
            .set_playhead(crate::transport_engine::InstantSamples(0));
        info!("Playback stopped");
    }

    /// Whether playback is active.
    pub fn is_playing(&self) -> bool {
        self.shared.play_state().is_advancing()
    }

    /// Seek to a position in seconds.
    pub fn seek(&self, seconds: f64) {
        let clock = crate::transport_engine::SampleClock::new(self.shared.sample_rate());
        let samples = clock.seconds_to_samples(crate::transport_engine::InstantSeconds(seconds));
        self.shared.set_playhead(samples);
    }

    /// Get the current playback position in seconds.
    pub fn position_seconds(&self) -> f64 {
        let clock = crate::transport_engine::SampleClock::new(self.shared.sample_rate());
        clock.samples_to_seconds(self.shared.playhead_samples()).0
    }

    /// Get the output sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.shared.sample_rate()
    }

    /// Set the master gain (applied after mixing all tracks).
    pub fn set_master_gain(&self, gain: f32) {
        self.state.lock().unwrap().master_gain = gain.max(0.0);
    }

    /// Get the longest track duration in seconds.
    pub fn duration_seconds(&self) -> f64 {
        let state = self.state.lock().unwrap();
        state
            .tracks
            .iter()
            .map(|t| t.buffer.duration_seconds())
            .fold(0.0f64, f64::max)
    }
}

/// Whether a stream error is only a buffer under/overrun — a glitch the
/// stream survives — rather than a device that has gone away.
fn is_xrun(err: &cpal::Error) -> bool {
    err.kind() == cpal::ErrorKind::Xrun
}
