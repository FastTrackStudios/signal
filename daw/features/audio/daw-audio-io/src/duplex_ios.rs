//! iOS duplex backend: ONE RemoteIO unit, input pulled inside the output
//! render callback.
//!
//! This is how every iOS audio app that plays what it hears is built —
//! Apple's aurioTouch, JUCE (`juce_Audio_ios.cpp`), AVAudioEngine (whose
//! input and output nodes share one I/O unit): EnableIO on the input bus
//! (element 1), a render callback on the output bus (element 0), and in that
//! callback `AudioUnitRender(unit, …, bus 1, …)` pulls the input for the
//! same cycle. Input and output are sample-locked, there is no ring, and
//! nothing else holds the hardware.
//!
//! What it replaces on iOS: cpal's backend, which opens TWO RemoteIO units
//! (one capture-only, one playback) and sizes its capture buffer through a
//! deprecated C session API. On a phone with an interface routed, its input
//! read nothing — a guitar at the floor, the meters at −90 dB.
//!
//! The session (category, mode, preferred input, activation) is the app's:
//! this opens on whatever route it has set up, at the session's rate, with
//! every channel the route has.

use std::ffi::c_void;
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use objc2::runtime::{AnyObject, Bool};
use objc2::{class, msg_send};

use crate::duplex::{DuplexBackend, DuplexConfig, EngineStats, ProcessBlock, ProcessFn};

#[allow(non_upper_case_globals, non_snake_case, dead_code)]
mod ffi {
    use std::ffi::c_void;

    pub type OSStatus = i32;
    pub type AudioUnit = *mut c_void;
    pub type AudioComponent = *mut c_void;

    #[repr(C)]
    pub struct AudioComponentDescription {
        pub component_type: u32,
        pub component_sub_type: u32,
        pub component_manufacturer: u32,
        pub component_flags: u32,
        pub component_flags_mask: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct AudioStreamBasicDescription {
        pub sample_rate: f64,
        pub format_id: u32,
        pub format_flags: u32,
        pub bytes_per_packet: u32,
        pub frames_per_packet: u32,
        pub bytes_per_frame: u32,
        pub channels_per_frame: u32,
        pub bits_per_channel: u32,
        pub reserved: u32,
    }

    #[repr(C)]
    pub struct AudioBuffer {
        pub number_channels: u32,
        pub data_byte_size: u32,
        pub data: *mut c_void,
    }

    /// `AudioBufferList` with its buffers inline (`mBuffers[1]` in C, more
    /// following in the same allocation).
    #[repr(C)]
    pub struct AudioBufferList {
        pub number_buffers: u32,
        pub buffers: [AudioBuffer; 1],
    }

    impl AudioBufferList {
        /// Buffer `i` of a list that has at least `i + 1`.
        pub unsafe fn buffer(list: *mut AudioBufferList, i: usize) -> *mut AudioBuffer {
            unsafe { (&raw mut (*list).buffers).cast::<AudioBuffer>().add(i) }
        }
    }

    pub type RenderCallback = unsafe extern "C" fn(
        ref_con: *mut c_void,
        action_flags: *mut u32,
        time_stamp: *const c_void,
        bus: u32,
        frames: u32,
        data: *mut AudioBufferList,
    ) -> OSStatus;

    #[repr(C)]
    pub struct AURenderCallbackStruct {
        pub input_proc: RenderCallback,
        pub input_proc_ref_con: *mut c_void,
    }

    pub const kAudioUnitType_Output: u32 = u32::from_be_bytes(*b"auou");
    pub const kAudioUnitSubType_RemoteIO: u32 = u32::from_be_bytes(*b"rioc");
    pub const kAudioUnitManufacturer_Apple: u32 = u32::from_be_bytes(*b"appl");
    pub const kAudioFormatLinearPCM: u32 = u32::from_be_bytes(*b"lpcm");

    pub const kAudioFormatFlagIsFloat: u32 = 1;
    pub const kAudioFormatFlagIsPacked: u32 = 8;
    pub const kAudioFormatFlagIsNonInterleaved: u32 = 32;

    pub const kAudioUnitProperty_StreamFormat: u32 = 8;
    pub const kAudioUnitProperty_MaximumFramesPerSlice: u32 = 14;
    pub const kAudioUnitProperty_SetRenderCallback: u32 = 23;
    pub const kAudioOutputUnitProperty_EnableIO: u32 = 2003;

    pub const kAudioUnitScope_Global: u32 = 0;
    pub const kAudioUnitScope_Input: u32 = 1;
    pub const kAudioUnitScope_Output: u32 = 2;

    /// The output bus (to the speaker / interface outputs).
    pub const OUTPUT_BUS: u32 = 0;
    /// The input bus (from the mic / interface inputs).
    pub const INPUT_BUS: u32 = 1;

    #[link(name = "AudioToolbox", kind = "framework")]
    unsafe extern "C" {
        pub fn AudioComponentFindNext(
            component: AudioComponent,
            desc: *const AudioComponentDescription,
        ) -> AudioComponent;
        pub fn AudioComponentInstanceNew(component: AudioComponent, out: *mut AudioUnit) -> OSStatus;
        pub fn AudioComponentInstanceDispose(unit: AudioUnit) -> OSStatus;
        pub fn AudioUnitSetProperty(
            unit: AudioUnit,
            id: u32,
            scope: u32,
            element: u32,
            data: *const c_void,
            size: u32,
        ) -> OSStatus;
        pub fn AudioUnitInitialize(unit: AudioUnit) -> OSStatus;
        pub fn AudioUnitUninitialize(unit: AudioUnit) -> OSStatus;
        pub fn AudioOutputUnitStart(unit: AudioUnit) -> OSStatus;
        pub fn AudioOutputUnitStop(unit: AudioUnit) -> OSStatus;
        pub fn AudioUnitRender(
            unit: AudioUnit,
            action_flags: *mut u32,
            time_stamp: *const c_void,
            bus: u32,
            frames: u32,
            data: *mut AudioBufferList,
        ) -> OSStatus;
    }
}

/// The largest block a render can ask for (Apple's ceiling for RemoteIO:
/// 4096 frames, when the screen locks).
const MAX_FRAMES: usize = 4096;

/// The largest block the rig is handed at once: a bigger callback is run
/// in pieces of this. What a phone's effects are prepared for (memory goes
/// with it — a neural amp model's every layer is sized by it), so a rig on
/// iOS prepares for exactly this and is never handed more. 256 — a phone's
/// usual I/O buffer, so a callback is still one piece: at 1024 the NAM
/// buffers of a profile's prepared patches alone held 330 MB.
pub const MAX_PROCESS_FRAMES: usize = 256;

/// `EngineStats::stream_state` while the unit runs, as the other backends
/// report it.
const STATE_STREAMING: i32 = 3;

/// The audio session's view of the route, read once at start.
struct Route {
    rate: f64,
    in_channels: usize,
    out_channels: usize,
    input_available: bool,
    input_is_builtin: bool,
    in_latency: f64,
    out_latency: f64,
    io_buffer: f64,
}

unsafe fn session() -> *mut AnyObject {
    unsafe { msg_send![class!(AVAudioSession), sharedInstance] }
}

/// Ask the session for a block size and rate (it may not grant either).
fn prefer(frames: u32, rate: u32) {
    unsafe {
        let s = session();
        let null = ptr::null_mut::<*mut AnyObject>();
        if rate > 0 {
            let _: Bool = msg_send![s, setPreferredSampleRate: f64::from(rate), error: null];
        }
        if frames > 0 {
            let r = if rate > 0 { f64::from(rate) } else { 48_000.0 };
            let _: Bool = msg_send![s, setPreferredIOBufferDuration: f64::from(frames) / r, error: null];
        }
    }
}

fn route() -> Route {
    unsafe {
        let s = session();
        let rate: f64 = msg_send![s, sampleRate];
        let in_ch: isize = msg_send![s, inputNumberOfChannels];
        let out_ch: isize = msg_send![s, outputNumberOfChannels];
        let available: Bool = msg_send![s, isInputAvailable];
        let in_latency: f64 = msg_send![s, inputLatency];
        let out_latency: f64 = msg_send![s, outputLatency];
        let io_buffer: f64 = msg_send![s, IOBufferDuration];
        // Whether the route's input is the phone's own microphone.
        let r: *mut AnyObject = msg_send![s, currentRoute];
        let mut builtin = false;
        if !r.is_null() {
            let inputs: *mut AnyObject = msg_send![r, inputs];
            if !inputs.is_null() {
                let n: usize = msg_send![inputs, count];
                for i in 0..n {
                    let port: *mut AnyObject = msg_send![inputs, objectAtIndex: i];
                    let t: *mut AnyObject = msg_send![port, portType];
                    if !t.is_null() {
                        let c: *const std::os::raw::c_char = msg_send![t, UTF8String];
                        if !c.is_null() && std::ffi::CStr::from_ptr(c).to_bytes() == b"MicrophoneBuiltIn" {
                            builtin = true;
                        }
                    }
                }
            }
        }
        Route {
            rate,
            in_channels: in_ch.max(0) as usize,
            out_channels: out_ch.max(0) as usize,
            input_available: available.as_bool(),
            input_is_builtin: builtin,
            in_latency,
            out_latency,
            io_buffer,
        }
    }
}

/// What the input did, for the input report: counted on the render thread
/// (atomics only), read and logged every five seconds by a plain thread.
#[derive(Default)]
struct InputProbe {
    renders_ok: std::sync::atomic::AtomicU64,
    renders_failed: std::sync::atomic::AtomicU64,
    /// Blocks whose processing panicked (caught; played as silence).
    panics: std::sync::atomic::AtomicU64,
    last_status: std::sync::atomic::AtomicI32,
    /// Peak |sample| per hardware channel since the last report, as `f32`
    /// bits (a positive float's bits order as the float does).
    peaks: [std::sync::atomic::AtomicU32; PROBED_CHANNELS],
}

/// How many input channels the report shows.
const PROBED_CHANNELS: usize = 8;

/// Every five seconds, what the input did — on from start to drop. A guitar
/// that never reaches the rig shows here as renders failing (the status
/// says why) or as peaks at zero on every channel: the interface sends
/// nothing to the app.
fn input_reporter(probe: Arc<InputProbe>, stop: Arc<std::sync::atomic::AtomicBool>, input: bool, hw_in: usize) {
    let _ = std::thread::Builder::new().name("ios-input-report".into()).spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            for _ in 0..50 {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let db = |bits: u32| {
                let p = f32::from_bits(bits);
                if p > 0.0 { (20.0 * p.log10()).max(-120.0) } else { -120.0 }
            };
            let peaks: Vec<String> = probe.peaks[..hw_in.min(PROBED_CHANNELS)]
                .iter()
                .map(|p| format!("{:.1}", db(p.swap(0, Ordering::Relaxed))))
                .collect();
            let panics = probe.panics.load(Ordering::Relaxed);
            if panics > 0 {
                tracing::error!(audio.panics = panics, "ios duplex: the audio processing panicked — those blocks played silence, the app kept running");
            }
            tracing::info!(
                audio.input = input,
                audio.hw_in = hw_in,
                audio.panics = panics,
                audio.renders_ok = probe.renders_ok.load(Ordering::Relaxed),
                audio.renders_failed = probe.renders_failed.load(Ordering::Relaxed),
                audio.last_status = probe.last_status.load(Ordering::Relaxed),
                audio.peaks_db = %peaks.join(" "),
                "ios duplex: input"
            );
        }
    });
}

/// Everything the render callback touches, boxed so its address (the
/// callback's ref-con) is stable. Only the render thread uses it.
struct IoState {
    unit: ffi::AudioUnit,
    /// Whether the input bus is on (an input the caller wants, and may have).
    input: bool,
    /// Hardware input channels (the input list's buffers), and how many of
    /// them the caller sees.
    hw_in: usize,
    want_in: usize,
    /// Per-channel scratch, `MAX_FRAMES` each: every hardware input channel
    /// (AudioUnitRender fills them all), the caller's outputs.
    in_bufs: Vec<Vec<f32>>,
    out_bufs: Vec<Vec<f32>>,
    /// The `AudioBufferList` the input is rendered into: one buffer per
    /// hardware input channel, pointing at `in_bufs`. Raw bytes, sized for
    /// `hw_in` buffers, `u64`-aligned.
    in_list: Vec<u64>,
    in_slices: Vec<&'static [f32]>,
    out_slices: Vec<&'static mut [f32]>,
    /// A zeroed channel for inputs past the hardware's.
    silence: Vec<f32>,
    process: ProcessFn,
    stats: Arc<EngineStats>,
    rate: u32,
    /// Consecutive failed input renders (logged once per run of them).
    render_failures: u64,
    probe: Arc<InputProbe>,
}

/// The render callback, as CoreAudio calls it: [`render_block`] inside
/// `catch_unwind`. A panic must not unwind out of an `extern "C"` function
/// (that aborts the process — the app gone mid-song), so one anywhere in the
/// rig's processing becomes this block's silence, counted, and the next
/// block plays on.
unsafe extern "C" fn render(
    ref_con: *mut c_void,
    action_flags: *mut u32,
    time_stamp: *const c_void,
    bus: u32,
    frames: u32,
    data: *mut ffi::AudioBufferList,
) -> ffi::OSStatus {
    let rendered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: as `render_block` requires — CoreAudio's arguments.
        unsafe { render_block(ref_con, action_flags, time_stamp, bus, frames, data) }
    }));
    match rendered {
        Ok(status) => status,
        Err(_) => {
            // SAFETY: the same output list and state as above; only the
            // buffers' samples and an atomic counter are touched.
            unsafe {
                silence(data, frames as usize);
                let st: &IoState = &*ref_con.cast::<IoState>();
                st.probe.panics.fetch_add(1, Ordering::Relaxed);
            }
            0
        }
    }
}

/// Zero every output buffer for `n` frames.
unsafe fn silence(data: *mut ffi::AudioBufferList, n: usize) {
    if data.is_null() {
        return;
    }
    unsafe {
        for b in 0..(*data).number_buffers as usize {
            let buf = ffi::AudioBufferList::buffer(data, b);
            let dst = (*buf).data.cast::<f32>();
            if !dst.is_null() {
                let len = ((*buf).data_byte_size as usize / 4).min(n);
                std::slice::from_raw_parts_mut(dst, len).fill(0.0);
            }
        }
    }
}

/// One block: pull the input, run the rig, write the output.
unsafe fn render_block(
    ref_con: *mut c_void,
    action_flags: *mut u32,
    time_stamp: *const c_void,
    _bus: u32,
    frames: u32,
    data: *mut ffi::AudioBufferList,
) -> ffi::OSStatus {
    // SAFETY: `ref_con` is the boxed `IoState` registered with the unit,
    // touched only from this render thread while the unit runs; `data` is
    // the output buffer list for this cycle.
    unsafe {
        let t0 = Instant::now();
        let st = &mut *ref_con.cast::<IoState>();
        let n = frames as usize;
        st.stats.calls.fetch_add(1, Ordering::Relaxed);
        st.stats.block_frames.store(frames, Ordering::Relaxed);
        if n == 0 || n > MAX_FRAMES {
            return 0;
        }

        // Pull this cycle's input into our own buffers.
        if st.input {
            let list = st.in_list.as_mut_ptr().cast::<ffi::AudioBufferList>();
            (*list).number_buffers = st.hw_in as u32;
            for c in 0..st.hw_in {
                let b = ffi::AudioBufferList::buffer(list, c);
                (*b).number_channels = 1;
                (*b).data_byte_size = (n * 4) as u32;
                (*b).data = st.in_bufs[c].as_mut_ptr().cast();
            }
            let status = ffi::AudioUnitRender(st.unit, action_flags, time_stamp, ffi::INPUT_BUS, frames, list);
            st.probe.last_status.store(status, Ordering::Relaxed);
            if status != 0 {
                st.probe.renders_failed.fetch_add(1, Ordering::Relaxed);
                for buf in &mut st.in_bufs {
                    buf[..n].fill(0.0);
                }
                if st.render_failures == 0 {
                    // Off the realtime path in spirit: once per run of
                    // failures, not per block.
                    tracing::warn!(audio.status = status, "ios duplex: input render failed");
                }
                st.render_failures += 1;
                st.stats.xruns.fetch_add(1, Ordering::Relaxed);
            } else {
                st.render_failures = 0;
                st.probe.renders_ok.fetch_add(1, Ordering::Relaxed);
                for (c, peak) in st.probe.peaks.iter().enumerate().take(st.hw_in) {
                    let p = st.in_bufs[c][..n].iter().fold(0.0f32, |m, s| m.max(s.abs()));
                    peak.fetch_max(p.to_bits(), Ordering::Relaxed);
                }
            }
        }

        for buf in &mut st.out_bufs {
            buf[..n].fill(0.0);
        }
        // The rig runs in pieces of at most `MAX_PROCESS_FRAMES`: what it
        // was prepared for. iOS can hand a callback up to 4096 frames (the
        // screen locked); the rig's blocks are sized for less.
        let mut at = 0;
        while at < n {
            let len = (n - at).min(MAX_PROCESS_FRAMES);
            let span = at..at + len;
            st.in_slices.clear();
            for c in 0..st.want_in {
                let src: &[f32] = if st.input && c < st.hw_in { &st.in_bufs[c][span.clone()] } else { &st.silence[..len] };
                st.in_slices.push(std::mem::transmute::<&[f32], &'static [f32]>(src));
            }
            st.out_slices.clear();
            for buf in &mut st.out_bufs {
                st.out_slices
                    .push(std::mem::transmute::<&mut [f32], &'static mut [f32]>(&mut buf[span.clone()]));
            }
            {
                let inputs: &[&[f32]] = std::mem::transmute(&st.in_slices[..]);
                let outputs: &mut [&mut [f32]] = std::mem::transmute(&mut st.out_slices[..]);
                let mut block = ProcessBlock { inputs, outputs, frames: len };
                (st.process)(&mut block);
            }
            st.in_slices.clear();
            st.out_slices.clear();
            at += len;
        }

        // Our outputs into the device's channels (non-interleaved: one
        // buffer per channel); channels past ours stay silent.
        if !data.is_null() {
            for b in 0..(*data).number_buffers as usize {
                let buf = ffi::AudioBufferList::buffer(data, b);
                let dst = (*buf).data.cast::<f32>();
                if dst.is_null() {
                    continue;
                }
                let len = ((*buf).data_byte_size as usize / 4).min(n);
                let dst = std::slice::from_raw_parts_mut(dst, len);
                match st.out_bufs.get(b) {
                    Some(src) => dst.copy_from_slice(&src[..len]),
                    None => dst.fill(0.0),
                }
            }
        }

        st.stats
            .record_render_at(t0.elapsed().as_nanos() as u64, st.rate);
        0
    }
}

/// The float, non-interleaved format at `rate` with `channels`.
fn float_format(rate: f64, channels: usize) -> ffi::AudioStreamBasicDescription {
    ffi::AudioStreamBasicDescription {
        sample_rate: rate,
        format_id: ffi::kAudioFormatLinearPCM,
        format_flags: ffi::kAudioFormatFlagIsFloat | ffi::kAudioFormatFlagIsPacked | ffi::kAudioFormatFlagIsNonInterleaved,
        bytes_per_packet: 4,
        frames_per_packet: 1,
        bytes_per_frame: 4,
        channels_per_frame: channels as u32,
        bits_per_channel: 32,
        reserved: 0,
    }
}

/// Set a property, saying which one failed.
unsafe fn set<T>(unit: ffi::AudioUnit, id: u32, scope: u32, element: u32, value: &T, what: &str) -> Result<(), String> {
    let status = unsafe {
        ffi::AudioUnitSetProperty(unit, id, scope, element, (value as *const T).cast(), std::mem::size_of::<T>() as u32)
    };
    if status == 0 { Ok(()) } else { Err(format!("RemoteIO: {what} failed (OSStatus {status})")) }
}

/// A live RemoteIO duplex unit. Dropping it stops audio.
pub struct RemoteIoBackend {
    unit: ffi::AudioUnit,
    /// Ends the input report thread.
    report_stop: Arc<std::sync::atomic::AtomicBool>,
    _state: Box<IoState>,
    stats: Arc<EngineStats>,
    sample_rate: u32,
    latency: (u32, u32),
    node_name: String,
}

// SAFETY: the unit and the boxed state are owned by this handle; the render
// thread reaches the state only through the pointer the unit holds, and drop
// stops and disposes the unit before freeing it.
unsafe impl Send for RemoteIoBackend {}

impl DuplexBackend for RemoteIoBackend {
    fn start(cfg: DuplexConfig, process: ProcessFn) -> Result<Self, String> {
        let _ = crate::duplex::clock_ns();
        if let Some(name) = cfg.input_device.as_deref().filter(|n| !n.is_empty()) {
            if !crate::ios_session::prefer_input(name) {
                tracing::warn!(audio.input_device = name, "ios duplex: no session input by that name — the route's own");
            }
        }
        match cfg.latency {
            Some((frames, rate)) => prefer(frames, rate),
            None => prefer(cfg.buffer.unwrap_or(0), 0),
        }
        let r = route();
        let rate = if r.rate > 0.0 { r.rate } else { 48_000.0 };

        // The phone's own microphone is no guitar input (mic → amp → speaker
        // is feedback) unless the caller allows it.
        let mut input = cfg.inputs > 0 && r.input_available && r.in_channels > 0;
        // `FTS_ALLOW_BUILTIN_MIC=1`: take the built-in microphone as the
        // input anyway — the simulator's only input (the Mac's), so the
        // whole input path can be run there.
        let allow_mic = cfg.allow_builtin_mic || std::env::var_os("FTS_ALLOW_BUILTIN_MIC").is_some();
        if input && r.input_is_builtin && !allow_mic {
            tracing::warn!("ios duplex: the route's input is the built-in microphone — opened without input");
            input = false;
        }
        let hw_in = if input { r.in_channels } else { 0 };
        let hw_out = r.out_channels.max(1);

        let desc = ffi::AudioComponentDescription {
            component_type: ffi::kAudioUnitType_Output,
            component_sub_type: ffi::kAudioUnitSubType_RemoteIO,
            component_manufacturer: ffi::kAudioUnitManufacturer_Apple,
            component_flags: 0,
            component_flags_mask: 0,
        };
        let mut unit: ffi::AudioUnit = ptr::null_mut();
        // SAFETY: plain AudioToolbox calls on a unit this function owns until
        // it is handed to the returned backend (or disposed on error).
        unsafe {
            let comp = ffi::AudioComponentFindNext(ptr::null_mut(), &desc);
            if comp.is_null() {
                return Err("RemoteIO: no I/O audio unit".into());
            }
            let status = ffi::AudioComponentInstanceNew(comp, &mut unit);
            if status != 0 || unit.is_null() {
                return Err(format!("RemoteIO: instance failed (OSStatus {status})"));
            }
        }
        let dispose = |unit: ffi::AudioUnit, e: String| {
            // SAFETY: the unit was created above and never started.
            unsafe { ffi::AudioComponentInstanceDispose(unit) };
            e
        };

        let stats = Arc::new(EngineStats::default());
        let mut state = Box::new(IoState {
            unit,
            input,
            hw_in,
            want_in: cfg.inputs,
            in_bufs: vec![vec![0.0; MAX_FRAMES]; hw_in],
            out_bufs: vec![vec![0.0; MAX_FRAMES]; cfg.outputs],
            // Header (8 bytes: count + padding) + 16 bytes per buffer.
            in_list: vec![0u64; 1 + 2 * hw_in.max(1)],
            in_slices: Vec::with_capacity(cfg.inputs),
            out_slices: Vec::with_capacity(cfg.outputs),
            silence: vec![0.0; MAX_FRAMES],
            process,
            stats: stats.clone(),
            rate: rate.round() as u32,
            render_failures: 0,
            probe: Arc::new(InputProbe::default()),
        });
        let probe = state.probe.clone();

        // SAFETY: setting up the unit created above; `state` outlives it.
        let configured = unsafe {
            (|| -> Result<(), String> {
                let on: u32 = 1;
                let off: u32 = 0;
                set(unit, ffi::kAudioOutputUnitProperty_EnableIO, ffi::kAudioUnitScope_Input, ffi::INPUT_BUS, if input { &on } else { &off }, "enable input")?;
                set(unit, ffi::kAudioOutputUnitProperty_EnableIO, ffi::kAudioUnitScope_Output, ffi::OUTPUT_BUS, &on, "enable output")?;
                let max = MAX_FRAMES as u32;
                set(unit, ffi::kAudioUnitProperty_MaximumFramesPerSlice, ffi::kAudioUnitScope_Global, 0, &max, "max frames")?;
                // What we give the output bus, and what we take from the
                // input bus: float, one buffer per channel, at the session's
                // rate, every channel the route has.
                set(unit, ffi::kAudioUnitProperty_StreamFormat, ffi::kAudioUnitScope_Input, ffi::OUTPUT_BUS, &float_format(rate, hw_out), "output format")?;
                if input {
                    set(unit, ffi::kAudioUnitProperty_StreamFormat, ffi::kAudioUnitScope_Output, ffi::INPUT_BUS, &float_format(rate, hw_in), "input format")?;
                }
                let callback = ffi::AURenderCallbackStruct {
                    input_proc: render,
                    input_proc_ref_con: (&mut *state as *mut IoState).cast(),
                };
                set(unit, ffi::kAudioUnitProperty_SetRenderCallback, ffi::kAudioUnitScope_Input, ffi::OUTPUT_BUS, &callback, "render callback")?;
                let status = ffi::AudioUnitInitialize(unit);
                if status != 0 {
                    return Err(format!("RemoteIO: initialize failed (OSStatus {status})"));
                }
                let status = ffi::AudioOutputUnitStart(unit);
                if status != 0 {
                    ffi::AudioUnitUninitialize(unit);
                    return Err(format!("RemoteIO: start failed (OSStatus {status})"));
                }
                Ok(())
            })()
        };
        if let Err(e) = configured {
            return Err(dispose(unit, e));
        }
        stats.stream_state.store(STATE_STREAMING, Ordering::Relaxed);
        let report_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        input_reporter(probe, report_stop.clone(), input, hw_in);

        let frames = |s: f64| (s * rate).round() as u32;
        let latency = (frames(r.in_latency + r.io_buffer), frames(r.out_latency + r.io_buffer));
        tracing::info!(
            audio.rate = rate,
            audio.hw_in = hw_in,
            audio.hw_out = hw_out,
            audio.input = input,
            audio.inputs = cfg.inputs,
            audio.outputs = cfg.outputs,
            audio.io_ms = r.io_buffer * 1000.0,
            audio.latency_in = latency.0,
            audio.latency_out = latency.1,
            "ios duplex: RemoteIO running"
        );
        Ok(Self {
            unit,
            report_stop,
            _state: state,
            stats,
            sample_rate: rate.round() as u32,
            latency,
            node_name: cfg.name,
        })
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn stats(&self) -> Arc<EngineStats> {
        self.stats.clone()
    }

    fn node_name(&self) -> &str {
        &self.node_name
    }

    fn latency_frames(&self) -> Option<(u32, u32)> {
        Some(self.latency)
    }
}

impl Drop for RemoteIoBackend {
    fn drop(&mut self) {
        // SAFETY: stop the render thread before the state it uses is freed.
        unsafe {
            ffi::AudioOutputUnitStop(self.unit);
            ffi::AudioUnitUninitialize(self.unit);
            ffi::AudioComponentInstanceDispose(self.unit);
        }
        self.stats.stream_state.store(0, Ordering::Relaxed);
        self.report_stop.store(true, Ordering::Relaxed);
    }
}
