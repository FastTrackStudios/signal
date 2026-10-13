//! Native **duplex** engine — drives the project renderer from one
//! realtime callback (input + output, same cycle, no ring bridge between
//! separate streams). The backend is PipeWire `pw_filter` on Linux and a
//! CoreAudio HAL IOProc on macOS (`daw_audio_io::duplex::Backend`). This is the low-latency path real DAWs use; it replaces
//! the cpal output-stream + separate input-stream + lock-free ring of
//! [`AudioEngine`](super::AudioEngine) for live-input work.
//!
//! The callback pushes the block's hardware input into the renderer's live-input
//! ring and immediately renders the block
//! ([`ProjectRenderer::render_plan`] — varispeed and scheduled locates from
//! [`TransportShared::begin_block`]), which drains it
//! the same block — so the ring carries exactly one block and adds **zero**
//! latency (it exists only because the renderer's input path is ring-shaped).
//! The renderer — FX chains, plugins, routing, metering — is reused unchanged.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "linux")]
use std::time::Duration;

use daw_audio_io::AudioIoPrefs;
use daw_audio_io::duplex::{Backend, DuplexBackend, DuplexConfig, EngineStats, ProcessBlock};
#[cfg(target_os = "linux")]
use daw_audio_io::pw;

use crate::Standalone;
use crate::audio_engine::render::ProjectRenderer;
use crate::transport_engine::TransportShared;

/// The duplex node name (how it appears in the PipeWire graph / patchbays).
/// Default graph node name, when `AudioIoPrefs::node_name` is empty.
const NODE_NAME: &str = "FTS-Signal";

/// A live audio engine driven by the platform duplex backend. Dropping it
/// stops audio (the backend tears down its node / IOProc).
pub struct DuplexAudioEngine {
    shared: Arc<TransportShared>,
    _backend: Backend,
    _renderer: Arc<ProjectRenderer>,
    stats: Arc<EngineStats>,
    sample_rate: u32,
    /// Hardware `(input, output)` latency in frames, when the backend knows.
    latency: Option<(u32, u32)>,
    /// Tells the link watchdog thread to exit (set on drop).
    linker_stop: Arc<AtomicBool>,
}

impl Drop for DuplexAudioEngine {
    fn drop(&mut self) {
        self.linker_stop.store(true, Ordering::Relaxed);
    }
}

/// Live phones-bus levels — a process-wide handle the host app writes
/// How long the main pair's mute takes to fade out or back in: short
/// enough to read as instant, long enough not to click.
const MUTE_FADE_S: f32 = 0.008;

/// (headphone volume + self-mix) and the duplex callback reads lock-free.
pub struct PhonesBus {
    volume: std::sync::atomic::AtomicU32,
    self_mix: std::sync::atomic::AtomicU32,
    /// Blend the monitor-mix input pair into the phones here. Off when a
    /// separate headphone-mixer process plays the mix (so it keeps playing
    /// when this one stops or overruns): the phones then carry only this
    /// engine's own signal, and the device sums the two processes.
    blend_mix: std::sync::atomic::AtomicBool,
    /// Mute the main pair only (routing on): the phones keep the signal.
    main_mute: std::sync::atomic::AtomicBool,
    /// The output check's tone: `0` none, else `output << 2 | sides` —
    /// output 1 the main pair, 2 the phones; sides 1 left, 2 right, 3 both.
    tone: std::sync::atomic::AtomicU32,
}

impl PhonesBus {
    /// The process-wide bus.
    pub fn shared() -> &'static PhonesBus {
        static BUS: PhonesBus = PhonesBus {
            volume: std::sync::atomic::AtomicU32::new(0x3f800000), // 1.0
            self_mix: std::sync::atomic::AtomicU32::new(0x3f800000), // 1.0
            blend_mix: std::sync::atomic::AtomicBool::new(true),
            main_mute: std::sync::atomic::AtomicBool::new(false),
            tone: std::sync::atomic::AtomicU32::new(0),
        };
        &BUS
    }

    /// Play the output check's tone on `output` (1 main, 2 phones) — `left`,
    /// `right`, both, or (neither) off.
    pub fn set_test_tone(&self, output: u32, left: bool, right: bool) {
        let sides = u32::from(left) | u32::from(right) << 1;
        let v = if sides == 0 || output == 0 { 0 } else { output << 2 | sides };
        self.tone.store(v, std::sync::atomic::Ordering::Relaxed);
    }

    /// The tone's gain on (main left, main right, phones left, phones right).
    fn tone_gains(&self) -> [f32; 4] {
        let v = self.tone.load(std::sync::atomic::Ordering::Relaxed);
        let (output, l, r) = (v >> 2, v & 1 != 0, v & 2 != 0);
        let on = |o: u32, side: bool| if output == o && side { 1.0 } else { 0.0 };
        [on(1, l), on(1, r), on(2, l), on(2, r)]
    }

    pub fn set(&self, volume: f32, self_mix: f32) {
        use std::sync::atomic::Ordering::Relaxed;
        self.volume.store(volume.to_bits(), Relaxed);
        self.self_mix.store(self_mix.to_bits(), Relaxed);
    }

    /// Whether this engine blends the monitor-mix input into the phones
    /// (see the field).
    pub fn set_blend_mix(&self, on: bool) {
        self.blend_mix
            .store(on, std::sync::atomic::Ordering::Relaxed);
    }

    /// Mute the main output pair and keep the phones (routing on; with
    /// routing off there is one pair, and the caller mutes the master).
    /// Mute the main pair (with the phones on a pair of their own, the
    /// phones keep the guitar; on one shared pair, everything), fading over
    /// [`MUTE_FADE_S`].
    pub fn set_main_mute(&self, on: bool) {
        self.main_mute
            .store(on, std::sync::atomic::Ordering::Relaxed);
    }

    fn main_muted(&self) -> bool {
        self.main_mute.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn blends_mix(&self) -> bool {
        self.blend_mix.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn levels(&self) -> (f32, f32) {
        use std::sync::atomic::Ordering::Relaxed;
        (
            f32::from_bits(self.volume.load(Relaxed)),
            f32::from_bits(self.self_mix.load(Relaxed)),
        )
    }
}

impl DuplexAudioEngine {
    /// Build a duplex engine for `project_guid` on `daw`, sharing `shared`'s
    /// transport clock. Opens one input port per hardware channel up to the
    /// highest a track records from, plus stereo output, and wires the duplex
    /// node to the prefs' input/output devices.
    pub fn with_project_prefs(
        daw: Standalone,
        project_guid: String,
        shared: Arc<TransportShared>,
        prefs: &AudioIoPrefs,
    ) -> Result<Self, String> {
        // Where the device decides (iOS: the audio session), the rate it
        // will actually run at — the prefs' asked for first — so the
        // renderer is built for it: an interface that only runs at 44.1 kHz
        // otherwise met a renderer built for 48 and the engine refused it.
        let sample_rate = daw_audio_io::negotiate_sample_rate(prefs.sample_rate_opt())
            .or_else(|| prefs.sample_rate_opt())
            .unwrap_or_else(|| native_rate(prefs));
        let buffer = prefs.buffer_size_opt().unwrap_or(128);
        shared.set_sample_rate(sample_rate);

        let renderer = Arc::new(ProjectRenderer::new(&daw, &project_guid, sample_rate));
        // A live callback: never wait on the project lock, never build a
        // snapshot on the audio thread.
        renderer.set_realtime();

        // Live / programmatic MIDI ring (same wiring as the cpal engine).
        {
            let (prod, cons) =
                rtrb::RingBuffer::<crate::audio_engine::render::LiveMidiEvent>::new(4096);
            renderer.set_live_midi(cons);
            daw.set_live_midi_producer(prod);
        }

        // Open enough input channels to reach the highest a track taps — none
        // for an output-only engine (a synth): on a laptop the "default
        // input" is its microphone, which it has no use for.
        let in_channels = if prefs.want_input {
            max_armed_channel(&daw, &project_guid)
                .map(|c| c + 1)
                .unwrap_or(1)
                .max(1)
        } else {
            0
        };

        // Live-input ring: producer fed by the duplex callback, consumer drained
        // by the renderer in the SAME callback. Modest capacity (a few blocks) is
        // plenty since it never accumulates.
        let capacity = (in_channels * buffer as usize * 4).max(8192);
        let (mut prod, cons) = rtrb::RingBuffer::<f32>::new(capacity);
        renderer.set_live_input(cons, in_channels);

        let r = renderer.clone();
        let sh = shared.clone();
        // Output routing: master lands on `main_out`; an optional phones bus
        // (master × self-mix + an external monitor-mix input pair, × volume)
        // lands on `phones_out`. Legacy prefs (routing off) stay 2-out 0/1.
        let routing = prefs.phones_routing;
        let (main_l, main_r) = if routing {
            (prefs.main_out_l, prefs.main_out_r)
        } else {
            (0, 1)
        };
        let (ph_l, ph_r) = (prefs.phones_out_l, prefs.phones_out_r);
        let (mix_l, mix_r) = (prefs.phones_mix_in_l, prefs.phones_mix_in_r);
        let phones = PhonesBus::shared();
        let ph = phones; // &'static — the callback closure copies the reference
        let out_ports = if routing {
            main_l.max(main_r).max(ph_l).max(ph_r) + 1
        } else {
            2
        };
        let in_ports = if routing && prefs.want_input {
            in_channels.max(mix_l.max(mix_r) + 1)
        } else {
            in_channels
        };
        // One process can run several engines (guitar rig + keys rig), and the
        // linker matches ports by name — so the name must be per-engine, not a
        // constant. Empty prefs keep the historical name.
        let node_name = if prefs.node_name.is_empty() {
            NODE_NAME.to_string()
        } else {
            prefs.node_name.clone()
        };
        let cfg = DuplexConfig {
            name: node_name.clone(),
            inputs: in_ports,
            outputs: out_ports,
            latency: Some((buffer, sample_rate)),
            input_device: prefs.input_name().map(str::to_string),
            // A guitar rig monitors through the interface it plays into.
            output_device: prefs
                .output_name()
                .or(prefs.input_name())
                .map(str::to_string),
            allow_builtin_mic: prefs.allow_builtin_mic,
            buffer: None,
            // An output-only engine shares the device's per-process block
            // with a duplex one: it may lower it, never raise it.
            buffer_lower_only: !prefs.want_input,
        };
        let mut main_gain = 1.0f32;
        // The output check's tone: 440 Hz at −18 dBFS, its phase carried
        // across blocks.
        let mut tone_phase = 0.0f32;
        let tone_step = 440.0 * std::f32::consts::TAU / sample_rate as f32;
        // When each buffer really starts, in the sync clock (a DLL over
        // callback entry times) — stamps every buffer's sync snapshot.
        let mut stamps = daw_transport_sync::BufferClock::default();
        let backend = Backend::start(
            cfg,
            Box::new(move |b: &mut ProcessBlock| {
                let entry_micros = crate::transport_sync::now_micros();
                let frames = b.frames;
                // Open the buffer: stamp it, land a scheduled locate that
                // falls in it, commit the playhead, publish the sync
                // snapshot.
                let stamp = stamps.tick(entry_micros, frames as u32, f64::from(sample_rate));
                let plan = sh.begin_block(frames as u32, stamp, stamps.period_micros());
                // Push this block's hardware input, interleaved [f0c0,f0c1,…], so
                // the renderer's stage-0 de-interleave matches the cpal path.
                for f in 0..frames {
                    for c in 0..in_channels {
                        let s = b.inputs.get(c).map(|ch| ch[f]).unwrap_or(0.0);
                        let _ = prod.push(s);
                    }
                }
                // Render this block (drains exactly what we just pushed).
                // Stopped runs still render (live input + FX), at the
                // frozen playhead.
                let block = r.render_plan(&plan, true);
                let outs = b.outputs.len();
                let (vol, self_mix) = ph.levels();
                let blend = ph.blends_mix();
                // The main pair's mute fades in and out over `MUTE_FADE_S`
                // whatever the buffer's size (a fade across one block was
                // 1.3 ms at 64 frames — a click), carried across blocks.
                let main_to = if ph.main_muted() { 0.0 } else { 1.0 };
                let fade_step = 1.0 / (MUTE_FADE_S * sample_rate as f32).max(1.0);
                let [tm_l, tm_r, tp_l, tp_r] = ph.tone_gains();
                let toning = tm_l + tm_r + tp_l + tp_r > 0.0;
                // With one pair (routing off, or the phones on the main
                // pair) the phones' check plays there.
                let one_pair = !routing || (ph_l == main_l && ph_r == main_r);
                let (tm_l, tm_r) = if one_pair { (tm_l.max(tp_l), tm_r.max(tp_r)) } else { (tm_l, tm_r) };
                for f in 0..frames {
                    let l = block.samples.get(f * 2).copied().unwrap_or(0.0);
                    let rr = block.samples.get(f * 2 + 1).copied().unwrap_or(0.0);
                    let tone = if toning {
                        tone_phase = (tone_phase + tone_step) % std::f32::consts::TAU;
                        tone_phase.sin() * 0.125
                    } else {
                        0.0
                    };
                    main_gain += (main_to - main_gain).clamp(-fade_step, fade_step);
                    let g = main_gain;
                    if main_l < outs {
                        b.outputs[main_l][f] = l * g + tone * tm_l;
                    }
                    if main_r < outs {
                        b.outputs[main_r][f] = rr * g + tone * tm_r;
                    }
                    if routing && (ph_l != main_l || ph_r != main_r) {
                        let ext = |c: usize| {
                            if blend {
                                b.inputs.get(c).map_or(0.0, |ch| ch[f])
                            } else {
                                0.0
                            }
                        };
                        let (ext_l, ext_r) = (ext(mix_l), ext(mix_r));
                        if ph_l < outs {
                            b.outputs[ph_l][f] = (l * self_mix + ext_l) * vol + tone * tp_l;
                        }
                        if ph_r < outs {
                            b.outputs[ph_r][f] = (rr * self_mix + ext_r) * vol + tone * tp_r;
                        }
                    }
                }
                // (The playhead was committed by `begin_block`.)
            }),
        )?;
        let stats = backend.stats();
        let latency = backend.latency_frames();
        if backend.sample_rate() != sample_rate {
            return Err(format!(
                "duplex backend runs at {} Hz, renderer was built for {sample_rate} Hz",
                backend.sample_rate()
            ));
        }

        // Wire the duplex node to the hardware (a DSP filter isn't auto-linked).
        // Async + retried: the node's ports take a beat to appear in the graph.
        // The thread then stays alive as a re-link watchdog for device loss.
        let linker_stop = Arc::new(AtomicBool::new(false));
        // CoreAudio opens the devices itself; only a graph needs linking.
        #[cfg(target_os = "linux")]
        spawn_linker(
            prefs.clone(),
            in_channels,
            linker_stop.clone(),
            stats.clone(),
        );

        Ok(Self {
            shared,
            _backend: backend,
            _renderer: renderer,
            stats,
            sample_rate,
            latency,
            linker_stop,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    /// Live realtime metrics (render time, block size, xruns) — the source of the
    /// rig's DSP-load + xrun meters.
    pub fn stats(&self) -> Arc<EngineStats> {
        self.stats.clone()
    }
    pub fn shared(&self) -> &Arc<TransportShared> {
        &self.shared
    }
    /// Hardware `(input, output)` latency in frames at
    /// [`sample_rate`](Self::sample_rate) — their sum is the round trip from
    /// the input jack to the output. `None` when the backend cannot tell.
    pub fn latency_frames(&self) -> Option<(u32, u32)> {
        self.latency
    }
}

/// The rate to run at when none is requested. PipeWire resamples to any
/// rate, so the historical 48 kHz stands; CoreAudio runs the device's clock
/// directly, so keep whatever the device is already at rather than
/// switching an interface other apps may be using.
fn native_rate(prefs: &AudioIoPrefs) -> u32 {
    #[cfg(target_os = "macos")]
    {
        let host = daw_audio_io::audio_host();
        let name = prefs.output_name().or(prefs.input_name());
        if let Ok(caps) = daw_audio_io::device_caps(&host, name, false) {
            return caps.default_sample_rate;
        }
    }
    let _ = prefs;
    48_000
}

/// Highest hardware input channel any track records from, or `None`.
fn max_armed_channel(daw: &Standalone, project_guid: &str) -> Option<usize> {
    daw.read_project(project_guid, |p| {
        p.tracks
            .iter()
            .filter_map(|t| match p.track_ext.get(&t.guid).map(|e| e.record_input) {
                Some(daw_proto::track::RecordInput::Audio { channel }) => Some(channel as usize),
                _ => None,
            })
            .max()
    })
    .flatten()
}

#[cfg(target_os = "linux")]
/// One watchdog pass over every expected hardware↔duplex link.
#[derive(Default)]
struct LinkPass {
    /// Expected links whose device node is currently in the graph.
    live: usize,
    /// Links newly created this pass (0 on a healthy steady-state pass).
    created: usize,
    /// Total expected links (0 only when no device is configured).
    total: usize,
}

/// Idempotently (re-)establish every expected link for this engine's device
/// prefs. `Exists` = healthy, `Created` = it was missing and came back (device
/// re-enumerated / first appearance), `Failed` = the port is gone.
#[cfg(target_os = "linux")]
fn link_pass(
    input: Option<&str>,
    output: Option<&str>,
    in_channels: usize,
    node_name: &str,
    allow_builtin_mic: bool,
) -> LinkPass {
    use daw_audio_io::pw::LinkStatus;
    fn run(pass: &mut LinkPass, src: &str, dst: &str) {
        pass.total += 1;
        match pw::ensure_link(src, dst) {
            LinkStatus::Created => {
                pass.created += 1;
                pass.live += 1;
            }
            LinkStatus::Exists => pass.live += 1,
            LinkStatus::Failed => {}
        }
    }
    let mut pass = LinkPass::default();
    // An absent name means "system default" (see `AudioIoPrefs`), not "do not
    // link" — resolve it rather than silently wiring nothing. That distinction
    // is the difference between a rig that plays and one that runs perfectly
    // into a disconnected graph, meters dead, in total silence.
    let in_dev = match input {
        Some(n) => pw::device_node_name(n, true),
        None => pw::default_device_node_name(true),
    }
    // Never link a built-in mic into a live rig: through the speakers it
    // feeds back. The node plays output-only instead (see input_guard).
    .filter(|dev| daw_audio_io::input_guard::check_input(dev, allow_builtin_mic).is_ok());
    let out_dev = match output {
        Some(n) => pw::device_node_name(n, false),
        None => pw::default_device_node_name(false),
    };
    // The device's own port names, in channel order, rather than an assumed
    // `capture_{n}` / `playback_{n}`. A multichannel interface presented as
    // one node numbers its ports (a Yamaha TF has `capture_1..34`); a device
    // PipeWire presents through a UCM profile names them by channel (a
    // MiniFuse 4 route has `capture_FL` / `capture_FR`). Assuming the first
    // shape meant every link to the second failed silently and the engine ran
    // into nothing at `paused`.
    match in_dev {
        Some(dev) => {
            let ports = pw::node_ports(&dev, true);
            if ports.is_empty() {
                tracing::warn!(
                    device.node = %dev,
                    "audio: capture device has no audio ports to link"
                );
            }
            // Only as many as the filter has inputs for, and only as many as
            // the device has: a mono instrument input feeds one.
            for (c, port) in ports.iter().take(in_channels).enumerate() {
                run(
                    &mut pass,
                    &format!("{dev}:{port}"),
                    &format!("{node_name}:input_{c}"),
                );
            }
            pass.total += in_channels.saturating_sub(ports.len());
        }
        // Device node absent — all its links count as down.
        None => pass.total += in_channels,
    }
    match out_dev {
        Some(dev) => {
            let ports = pw::node_ports(&dev, false);
            if ports.is_empty() {
                tracing::warn!(
                    device.node = %dev,
                    "audio: playback device has no audio ports to link"
                );
            }
            // Stereo out: the first two of whatever the device calls them.
            for (c, port) in ports.iter().take(2).enumerate() {
                run(
                    &mut pass,
                    &format!("{node_name}:output_{c}"),
                    &format!("{dev}:{port}"),
                );
            }
            pass.total += 2usize.saturating_sub(ports.len().min(2));
        }
        None => pass.total += 2,
    }
    pass
}

/// Link the hardware device's capture/playback ports to the duplex node, and
/// keep them linked: after the initial (fast-retried) linking pass the thread
/// stays alive as a watchdog, re-running the idempotent link pass every
/// [`WATCH_INTERVAL`] so a device that drops and re-enumerates (USB interface
/// power blip, cable re-seat) is re-linked automatically instead of leaving
/// the engine "running" into a disconnected graph. Exits when `stop` is set
/// (the owning [`DuplexAudioEngine`] sets it on drop).
#[cfg(target_os = "linux")]
fn spawn_linker(
    prefs: AudioIoPrefs,
    in_channels: usize,
    stop: Arc<AtomicBool>,
    stats: Arc<EngineStats>,
) {
    // Must match the name the backend registered, or the linker hunts for
    // ports that do not exist and the node plays to nothing.
    let node_name = if prefs.node_name.is_empty() {
        NODE_NAME.to_string()
    } else {
        prefs.node_name.clone()
    };
    const WATCH_INTERVAL: Duration = Duration::from_millis(1500);
    /// Startup cadence, until the duplex node's own ports appear in the graph.
    const STARTUP_INTERVAL: Duration = Duration::from_millis(100);
    /// Re-linking hardware ports can't heal a filter whose own PipeWire
    /// connection died (daemon restart) — the node is gone from the new
    /// graph. Once the filter has streamed, this many consecutive
    /// error/unconnected passes (~10 s) means only a process restart can
    /// reconnect: crash-only, the service manager brings it back in ~1 s.
    const DEAD_CONNECTION_PASSES: u32 = 7;
    /// `pw_filter_state`: -1 = error, 0 = unconnected, 3 = streaming.
    const STATE_STREAMING: i32 = 3;

    let input = prefs.input_name().map(str::to_string);
    // Default output to the input interface (a guitar rig monitors through the
    // same device) when no explicit output device is set.
    let output = prefs
        .output_name()
        .or(prefs.input_name())
        .map(str::to_string);
    // NOTE: no early return when both names are empty. Empty means "system
    // default" (see `AudioIoPrefs`), which is the NORMAL case for an
    // instrument that just wants to be heard — the keys rig passes exactly
    // that. Returning here left its node in the graph with ports and no
    // links: audio running perfectly into nothing, meters dead, and nothing
    // logged to say so. `link_pass` resolves the defaults per pass, so the
    // watchdog also follows the default device when the user changes it.
    std::thread::spawn(move || {
        let mut was_linked = false;
        let mut ever_linked = false;
        let mut startup_retries = 20u32;
        let mut ever_streamed = false;
        let mut dead_passes = 0u32;
        loop {
            // Sleep in short slices so drop tears the thread down promptly.
            let interval = if was_linked || startup_retries == 0 {
                WATCH_INTERVAL
            } else {
                startup_retries -= 1;
                STARTUP_INTERVAL
            };
            let mut slept = Duration::ZERO;
            while slept < interval {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                let slice = Duration::from_millis(100).min(interval - slept);
                std::thread::sleep(slice);
                slept += slice;
            }

            let state = stats.stream_state.load(Ordering::Relaxed);
            if state == STATE_STREAMING {
                ever_streamed = true;
                dead_passes = 0;
            } else if ever_streamed && state <= 0 {
                dead_passes += 1;
                if dead_passes >= DEAD_CONNECTION_PASSES {
                    tracing::error!(
                        "duplex linker: PipeWire connection dead for {dead_passes} passes \
                         (filter state {state}) — exiting for a supervised restart",
                    );
                    std::process::exit(70);
                }
            } else {
                dead_passes = 0;
            }

            let pass = link_pass(
                input.as_deref(),
                output.as_deref(),
                in_channels,
                &node_name,
                prefs.allow_builtin_mic,
            );
            let now_linked = pass.live == pass.total;
            if now_linked && !was_linked {
                if ever_linked {
                    tracing::warn!(
                        "duplex linker: AUDIO DEVICE RE-LINKED ({}/{} links up)",
                        pass.live,
                        pass.total,
                    );
                } else {
                    tracing::info!(
                        "duplex linker: audio device linked ({}/{} links up)",
                        pass.live,
                        pass.total,
                    );
                }
                ever_linked = true;
            } else if !now_linked && was_linked {
                tracing::error!(
                    "duplex linker: AUDIO DEVICE LOST ({}/{} links up; input={:?} output={:?}) — \
                     watching for it to return",
                    pass.live,
                    pass.total,
                    input,
                    output,
                );
            } else if pass.created > 0 && was_linked {
                // Partial re-link within one poll interval (fast re-enumerate).
                tracing::warn!(
                    "duplex linker: re-linked {} audio port(s) after device change",
                    pass.created,
                );
            }
            was_linked = now_linked;
        }
    });
}
