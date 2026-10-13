//! Live duplex monitor: cpal input → caller-supplied process → cpal output.
//!
//! This owns only the *plumbing* — device streams, a lock-free input ring, and
//! the in/out peak meters. The actual processing (e.g. a guitar amp/FX chain) is
//! a caller-supplied [`MonitorProcess`] closure, so this crate stays free of any
//! DSP / model dependencies. It implements the **single-track** case of the
//! routing model: one input channel tapped, one output channel pair written.
//!
//! ## Limitation
//!
//! The input + output streams are opened as `f32`. Devices whose native format
//! is `i16`/`u16` will fail to build an `f32` stream here; pro interfaces under
//! JACK/PipeWire are `f32`, which covers the live-rig use case. Multi-format
//! input dispatch is a future addition.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use cpal::traits::{DeviceTrait, StreamTrait};

use crate::device::device_name;
use crate::host::audio_host;
use crate::open::{open_input, open_output};
use crate::prefs::{AudioIoPrefs, TrackIo};

/// Max block the scratch buffers are sized for. cpal callback blocks are
/// normally 64–1024 frames; 8192 is safe against larger backend buffers.
const MAX_BLOCK: usize = 8192;

/// Peak-meter decay applied once per output block so the UI sees a smooth fall.
const METER_DECAY: f32 = 0.85;

/// Per-block processing: `mono_in` (one channel, `frames` samples) →
/// `stereo_out` (interleaved L/R, `frames * 2` samples, pre-zeroed). `Send` so
/// it can move onto the cpal output thread. The closure does whatever it likes
/// (mono→stereo broadcast, gain, an FX chain) to fill `stereo_out`.
pub type MonitorProcess = Box<dyn FnMut(&[f32], &mut [f32]) + Send>;

struct MonitorShared {
    input_peak: AtomicU32,
    output_peak: AtomicU32,
    underruns: AtomicU64,
    overruns: AtomicU64,
}

impl MonitorShared {
    fn new() -> Self {
        Self {
            input_peak: AtomicU32::new(0),
            output_peak: AtomicU32::new(0),
            underruns: AtomicU64::new(0),
            overruns: AtomicU64::new(0),
        }
    }

    fn store_peak(slot: &AtomicU32, block_peak: f32) {
        let prev = f32::from_bits(slot.load(Ordering::Relaxed));
        let held = (prev * METER_DECAY).max(block_peak);
        slot.store(held.to_bits(), Ordering::Relaxed);
    }
}

/// Output-callback-owned processing state.
struct MonitorDsp {
    process: MonitorProcess,
    input_cons: rtrb::Consumer<f32>,
    /// Mono input scratch (de-ringed). Pre-sized to `MAX_BLOCK`.
    in_mono: Vec<f32>,
    /// Interleaved-stereo scratch the process fills. `2 * MAX_BLOCK`.
    stereo: Vec<f32>,
    shared: Arc<MonitorShared>,
    out_left: usize,
    out_right: usize,
    out_channels: usize,
}

impl MonitorDsp {
    fn render(&mut self, out: &mut [f32]) {
        let out_ch = self.out_channels;
        if out_ch == 0 {
            return;
        }
        let frames = (out.len() / out_ch).min(self.in_mono.len());
        if frames == 0 {
            return;
        }

        // 1. Pull mono input from the ring; zero-fill any shortfall.
        let avail = self.input_cons.slots();
        let n = frames.min(avail);
        if n < frames {
            self.shared.underruns.fetch_add(1, Ordering::Relaxed);
        }
        if n > 0
            && let Ok(chunk) = self.input_cons.read_chunk(n)
        {
            let (a, b) = chunk.as_slices();
            self.in_mono[..a.len()].copy_from_slice(a);
            self.in_mono[a.len()..a.len() + b.len()].copy_from_slice(b);
            chunk.commit_all();
        }
        for s in &mut self.in_mono[n..frames] {
            *s = 0.0;
        }

        // 2. Input meter (post-ring, pre-process).
        let mut ipk = 0.0f32;
        for &s in &self.in_mono[..frames] {
            ipk = ipk.max(s.abs());
        }
        MonitorShared::store_peak(&self.shared.input_peak, ipk);

        // 3. Run the caller's process: mono in → stereo scratch (pre-zeroed).
        {
            let stereo = &mut self.stereo[..frames * 2];
            for v in stereo.iter_mut() {
                *v = 0.0;
            }
            (self.process)(&self.in_mono[..frames], stereo);
        }

        // 4. Write the stereo result into the chosen output channel pair; zero
        //    every other channel of the interleaved output buffer.
        out.fill(0.0);
        let mut opk = 0.0f32;
        for i in 0..frames {
            let l = self.stereo[2 * i];
            let r = self.stereo[2 * i + 1];
            opk = opk.max(l.abs().max(r.abs()));
            let base = i * out_ch;
            if self.out_left < out_ch {
                out[base + self.out_left] = l;
            }
            if self.out_right < out_ch {
                out[base + self.out_right] = r;
            }
        }
        MonitorShared::store_peak(&self.shared.output_peak, opk);
    }
}

/// A live duplex monitor: cpal input → [`MonitorProcess`] → cpal output.
/// Dropping it tears down both streams.
pub struct DuplexMonitor {
    _input_stream: cpal::Stream,
    _output_stream: cpal::Stream,
    pub sample_rate: u32,
    prefs: AudioIoPrefs,
    track: TrackIo,
    shared: Arc<MonitorShared>,
}

impl DuplexMonitor {
    /// Open the engine input + output devices from `prefs`, tap
    /// `track.input_channel`, run `process` each block, and write to
    /// `track.output_channels` of the output device. Returns a running monitor.
    pub fn open(
        prefs: &AudioIoPrefs,
        track: TrackIo,
        process: MonitorProcess,
    ) -> Result<Self, String> {
        let host = audio_host();
        let out = open_output(&host, prefs)?;
        let sample_rate = out.sample_rate;
        let inp = open_input(&host, prefs, sample_rate, track.input_channel)?;

        let ring_capacity = (sample_rate as usize / 10).max(MAX_BLOCK * 4);
        let (mut prod, cons) = rtrb::RingBuffer::<f32>::new(ring_capacity);

        let shared = Arc::new(MonitorShared::new());
        let in_channels = inp.channels;
        let in_channel = track.input_channel;

        let shared_in = Arc::clone(&shared);
        let input_stream = inp
            .device
            .build_input_stream(
                inp.config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    for frame in data.chunks(in_channels) {
                        // Pick the configured channel (falls back to channel 0).
                        let s = frame
                            .get(in_channel)
                            .or_else(|| frame.first())
                            .copied()
                            .unwrap_or(0.0);
                        if prod.push(s).is_err() {
                            shared_in.overruns.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                },
                |err| tracing::warn!("daw-audio-io: monitor input stream error: {err}"),
                None,
            )
            .map_err(|e| format!("build input stream: {e}"))?;

        let mut dsp = MonitorDsp {
            process,
            input_cons: cons,
            in_mono: vec![0.0; MAX_BLOCK],
            stereo: vec![0.0; MAX_BLOCK * 2],
            shared: Arc::clone(&shared),
            out_left: track.output_left,
            out_right: track.output_right,
            out_channels: out.channels as usize,
        };
        let output_stream = out
            .device
            .build_output_stream(
                out.config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| dsp.render(data),
                |err| tracing::warn!("daw-audio-io: monitor output stream error: {err}"),
                None,
            )
            .map_err(|e| format!("build output stream: {e}"))?;

        input_stream
            .play()
            .map_err(|e| format!("play input: {e}"))?;
        output_stream
            .play()
            .map_err(|e| format!("play output: {e}"))?;

        // Effective prefs — the actually-matched device names, so the caller can
        // persist exactly what was opened ("remember last used").
        let effective = AudioIoPrefs {
            input_device: device_name(&inp.device),
            output_device: device_name(&out.device),
            sample_rate,
            buffer_size: prefs.buffer_size,
            want_input: true,
            ..prefs.clone()
        };

        tracing::info!(
            input = %effective.input_device,
            output = %effective.output_device,
            sample_rate,
            in_channels,
            out_channels = out.channels,
            input_channel = in_channel,
            "daw-audio-io: duplex monitor started"
        );

        Ok(Self {
            _input_stream: input_stream,
            _output_stream: output_stream,
            sample_rate,
            prefs: effective,
            track,
            shared,
        })
    }

    /// The prefs the monitor actually opened with (resolved device names).
    pub fn prefs(&self) -> &AudioIoPrefs {
        &self.prefs
    }

    /// The track routing this monitor was opened with.
    pub fn track(&self) -> TrackIo {
        self.track
    }

    /// Post-ring input peak (linear) for an input meter.
    pub fn input_peak(&self) -> f32 {
        f32::from_bits(self.shared.input_peak.load(Ordering::Relaxed))
    }

    /// Output peak (linear) for an output meter.
    pub fn output_peak(&self) -> f32 {
        f32::from_bits(self.shared.output_peak.load(Ordering::Relaxed))
    }

    /// Output-callback ring underruns (input not keeping up).
    pub fn underruns(&self) -> u64 {
        self.shared.underruns.load(Ordering::Relaxed)
    }

    /// Input-callback ring overruns (output not draining fast enough).
    pub fn overruns(&self) -> u64 {
        self.shared.overruns.load(Ordering::Relaxed)
    }
}
