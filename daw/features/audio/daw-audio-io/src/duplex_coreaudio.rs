//! macOS duplex backend — a CoreAudio HAL **IOProc** registered directly on
//! the device, so one realtime callback receives the capture and playback
//! buffers of the same IO cycle. No ring, no AUHAL converter: the macOS
//! equivalent of the PipeWire `pw_filter` backend (and what JUCE,
//! SuperCollider and RtAudio do).
//!
//! Input and output on **one** device (the normal case: an audio interface)
//! run the IOProc on that device. On **two** devices a private aggregate
//! device is created — output device as the clock, drift compensation on
//! the input — and the IOProc runs on the aggregate; it is destroyed when
//! the backend drops. For a live rig, one interface is always better: an
//! aggregate adds both devices' safety offsets and the drift resampler.
//!
//! The HAL IO thread is already realtime and already in the device's audio
//! workgroup; the `process` closure runs on it directly. (Any DSP worker
//! threads added later must join `kAudioDevicePropertyIOThreadOSWorkgroup`.)
//!
//! The IOProc sees the device's own stream layout: one `AudioBuffer` per
//! stream, each interleaving `mNumberChannels` channels. Multi-stream
//! devices (UA Apollo, Dante) are flattened into one channel index space in
//! stream order, which is what `DuplexConfig::inputs`/`outputs` index.

use std::ffi::{CStr, CString, c_char, c_void};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use crate::duplex::{DuplexBackend, DuplexConfig, EngineStats, ProcessBlock, ProcessFn};

// ── CoreAudio / CoreFoundation FFI ────────────────────────────────────────

mod ffi {
    use std::ffi::{c_char, c_void};

    pub type OSStatus = i32;
    pub type AudioObjectID = u32;
    pub type CFTypeRef = *const c_void;

    pub const fn fourcc(s: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*s)
    }

    pub const SYSTEM_OBJECT: AudioObjectID = 1;
    pub const UNKNOWN_OBJECT: AudioObjectID = 0;

    pub const SCOPE_GLOBAL: u32 = fourcc(b"glob");
    pub const SCOPE_INPUT: u32 = fourcc(b"inpt");
    pub const SCOPE_OUTPUT: u32 = fourcc(b"outp");
    pub const ELEMENT_MAIN: u32 = 0;

    pub const HW_DEVICES: u32 = fourcc(b"dev#");
    pub const HW_DEFAULT_INPUT: u32 = fourcc(b"dIn ");
    pub const HW_DEFAULT_OUTPUT: u32 = fourcc(b"dOut");
    pub const OBJECT_NAME: u32 = fourcc(b"lnam");
    pub const DEVICE_UID: u32 = fourcc(b"uid ");
    pub const DEVICE_TRANSPORT_TYPE: u32 = fourcc(b"tran");
    pub const TRANSPORT_BUILT_IN: u32 = fourcc(b"bltn");
    pub const DEVICE_IS_ALIVE: u32 = fourcc(b"livn");
    pub const DEVICE_STREAM_CONFIGURATION: u32 = fourcc(b"slay");
    pub const DEVICE_STREAMS: u32 = fourcc(b"stm#");
    pub const DEVICE_NOMINAL_SAMPLE_RATE: u32 = fourcc(b"nsrt");
    pub const DEVICE_BUFFER_FRAME_SIZE: u32 = fourcc(b"fsiz");
    pub const DEVICE_BUFFER_FRAME_SIZE_RANGE: u32 = fourcc(b"fsz#");
    pub const DEVICE_LATENCY: u32 = fourcc(b"ltnc");
    pub const DEVICE_SAFETY_OFFSET: u32 = fourcc(b"saft");
    pub const DEVICE_PROCESSOR_OVERLOAD: u32 = fourcc(b"ovrl");
    pub const DEVICE_IO_THREAD_WORKGROUP: u32 = fourcc(b"oswg");
    pub const STREAM_LATENCY: u32 = fourcc(b"ltnc");
    pub const STREAM_VIRTUAL_FORMAT: u32 = fourcc(b"sfmt");

    pub const FORMAT_LINEAR_PCM: u32 = fourcc(b"lpcm");
    pub const FORMAT_FLAG_IS_FLOAT: u32 = 1;

    pub const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
    pub const CF_NUMBER_SINT32: isize = 3;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct PropertyAddress {
        pub selector: u32,
        pub scope: u32,
        pub element: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct ValueRange {
        pub minimum: f64,
        pub maximum: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct StreamBasicDescription {
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

    /// Variable-length: `buffers` really holds `number_buffers` entries.
    #[repr(C)]
    pub struct AudioBufferList {
        pub number_buffers: u32,
        pub buffers: [AudioBuffer; 1],
    }

    impl AudioBufferList {
        /// The `i`-th buffer. Caller guarantees `i < number_buffers`.
        pub unsafe fn buffer(this: *const Self, i: usize) -> *const AudioBuffer {
            unsafe { (&raw const (*this).buffers).cast::<AudioBuffer>().add(i) }
        }
    }

    /// Opaque; only passed through.
    #[repr(C)]
    pub struct AudioTimeStamp {
        _private: [u8; 0],
    }

    pub type IOProc = unsafe extern "C" fn(
        device: AudioObjectID,
        now: *const AudioTimeStamp,
        input: *const AudioBufferList,
        input_time: *const AudioTimeStamp,
        output: *mut AudioBufferList,
        output_time: *const AudioTimeStamp,
        client: *mut c_void,
    ) -> OSStatus;
    pub type IOProcID = *mut c_void;

    pub type PropertyListener = unsafe extern "C" fn(
        object: AudioObjectID,
        count: u32,
        addresses: *const PropertyAddress,
        client: *mut c_void,
    ) -> OSStatus;

    #[link(name = "CoreAudio", kind = "framework")]
    unsafe extern "C" {
        pub fn AudioObjectGetPropertyDataSize(
            object: AudioObjectID,
            address: *const PropertyAddress,
            qualifier_size: u32,
            qualifier: *const c_void,
            out_size: *mut u32,
        ) -> OSStatus;
        pub fn AudioObjectGetPropertyData(
            object: AudioObjectID,
            address: *const PropertyAddress,
            qualifier_size: u32,
            qualifier: *const c_void,
            io_size: *mut u32,
            out_data: *mut c_void,
        ) -> OSStatus;
        pub fn AudioObjectSetPropertyData(
            object: AudioObjectID,
            address: *const PropertyAddress,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: u32,
            data: *const c_void,
        ) -> OSStatus;
        pub fn AudioObjectAddPropertyListener(
            object: AudioObjectID,
            address: *const PropertyAddress,
            listener: PropertyListener,
            client: *mut c_void,
        ) -> OSStatus;
        pub fn AudioObjectRemovePropertyListener(
            object: AudioObjectID,
            address: *const PropertyAddress,
            listener: PropertyListener,
            client: *mut c_void,
        ) -> OSStatus;
        pub fn AudioDeviceCreateIOProcID(
            device: AudioObjectID,
            proc_: IOProc,
            client: *mut c_void,
            out_id: *mut IOProcID,
        ) -> OSStatus;
        pub fn AudioDeviceDestroyIOProcID(device: AudioObjectID, id: IOProcID) -> OSStatus;
        pub fn AudioDeviceStart(device: AudioObjectID, id: IOProcID) -> OSStatus;
        pub fn AudioDeviceStop(device: AudioObjectID, id: IOProcID) -> OSStatus;
        pub fn AudioHardwareCreateAggregateDevice(
            description: CFTypeRef,
            out_device: *mut AudioObjectID,
        ) -> OSStatus;
        pub fn AudioHardwareDestroyAggregateDevice(device: AudioObjectID) -> OSStatus;
    }

    /// Opaque CF callback tables; only their addresses are used.
    #[repr(C)]
    pub struct CFCallBacks {
        _private: [u8; 0],
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        pub static kCFTypeDictionaryKeyCallBacks: CFCallBacks;
        pub static kCFTypeDictionaryValueCallBacks: CFCallBacks;
        pub static kCFTypeArrayCallBacks: CFCallBacks;
        pub fn CFRelease(cf: CFTypeRef);
        pub fn CFStringCreateWithCString(
            alloc: CFTypeRef,
            s: *const c_char,
            encoding: u32,
        ) -> CFTypeRef;
        pub fn CFStringGetCString(
            s: CFTypeRef,
            buffer: *mut c_char,
            size: isize,
            encoding: u32,
        ) -> bool;
        pub fn CFNumberCreate(alloc: CFTypeRef, kind: isize, value: *const c_void) -> CFTypeRef;
        pub fn CFDictionaryCreateMutable(
            alloc: CFTypeRef,
            capacity: isize,
            keys: *const CFCallBacks,
            values: *const CFCallBacks,
        ) -> CFTypeRef;
        pub fn CFDictionarySetValue(dict: CFTypeRef, key: CFTypeRef, value: CFTypeRef);
        pub fn CFArrayCreateMutable(
            alloc: CFTypeRef,
            capacity: isize,
            callbacks: *const CFCallBacks,
        ) -> CFTypeRef;
        pub fn CFArrayAppendValue(array: CFTypeRef, value: CFTypeRef);
    }
}

use ffi::AudioObjectID;

// ── Property helpers ──────────────────────────────────────────────────────

fn address(selector: u32, scope: u32) -> ffi::PropertyAddress {
    ffi::PropertyAddress {
        selector,
        scope,
        element: ffi::ELEMENT_MAIN,
    }
}

fn check(status: ffi::OSStatus, what: &str) -> Result<(), String> {
    if status == 0 {
        Ok(())
    } else {
        let code = status as u32;
        let tag = code.to_be_bytes();
        let fourcc = if tag.iter().all(|b| b.is_ascii_graphic() || *b == b' ') {
            format!(" '{}'", String::from_utf8_lossy(&tag))
        } else {
            String::new()
        };
        Err(format!("CoreAudio {what} failed: {status}{fourcc}"))
    }
}

/// Read a fixed-size property.
fn get<T: Copy + Default>(object: AudioObjectID, selector: u32, scope: u32) -> Result<T, String> {
    let addr = address(selector, scope);
    let mut value = T::default();
    let mut size = size_of::<T>() as u32;
    // SAFETY: `value` is a `T` of `size` bytes; the HAL writes at most that.
    let status = unsafe {
        ffi::AudioObjectGetPropertyData(
            object,
            &addr,
            0,
            ptr::null(),
            &mut size,
            (&raw mut value).cast(),
        )
    };
    check(status, "get property")?;
    Ok(value)
}

fn set<T: Copy>(object: AudioObjectID, selector: u32, scope: u32, value: T) -> Result<(), String> {
    let addr = address(selector, scope);
    // SAFETY: `value` is a `T` and we pass its exact size.
    let status = unsafe {
        ffi::AudioObjectSetPropertyData(
            object,
            &addr,
            0,
            ptr::null(),
            size_of::<T>() as u32,
            (&raw const value).cast(),
        )
    };
    check(status, "set property")
}

/// Read a variable-size property as raw bytes (u64-aligned).
fn get_bytes(object: AudioObjectID, selector: u32, scope: u32) -> Result<Vec<u64>, String> {
    let addr = address(selector, scope);
    let mut size = 0u32;
    // SAFETY: size query only.
    let status =
        unsafe { ffi::AudioObjectGetPropertyDataSize(object, &addr, 0, ptr::null(), &mut size) };
    check(status, "get property size")?;
    let mut buf = vec![0u64; (size as usize).div_ceil(8).max(1)];
    // SAFETY: `buf` holds at least `size` bytes.
    let status = unsafe {
        ffi::AudioObjectGetPropertyData(
            object,
            &addr,
            0,
            ptr::null(),
            &mut size,
            buf.as_mut_ptr().cast(),
        )
    };
    check(status, "get property")?;
    Ok(buf)
}

fn get_ids(object: AudioObjectID, selector: u32, scope: u32) -> Vec<AudioObjectID> {
    let Ok(bytes) = get_bytes(object, selector, scope) else {
        return Vec::new();
    };
    let addr = address(selector, scope);
    let mut size = 0u32;
    // SAFETY: size query only.
    unsafe { ffi::AudioObjectGetPropertyDataSize(object, &addr, 0, ptr::null(), &mut size) };
    let count = size as usize / size_of::<AudioObjectID>();
    // SAFETY: the HAL wrote `count` ids into `bytes`.
    unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<AudioObjectID>(), count) }.to_vec()
}

/// A CFString property as a Rust string (the HAL returns a +1 reference).
fn get_string(object: AudioObjectID, selector: u32) -> Option<String> {
    let s: usize = get(object, selector, ffi::SCOPE_GLOBAL).ok()?;
    let s = s as ffi::CFTypeRef;
    if s.is_null() {
        return None;
    }
    let mut buf = [0 as c_char; 1024];
    // SAFETY: `s` is a CFString we own; `buf` is writable for its length.
    let ok = unsafe {
        let ok = ffi::CFStringGetCString(
            s,
            buf.as_mut_ptr(),
            buf.len() as isize,
            ffi::CF_STRING_ENCODING_UTF8,
        );
        ffi::CFRelease(s);
        ok
    };
    // SAFETY: CFStringGetCString NUL-terminated `buf` when it returned true.
    ok.then(|| {
        unsafe { CStr::from_ptr(buf.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    })
}

/// Channel count per stream (`AudioBuffer`) for `scope`, in stream order —
/// exactly the layout the IOProc's buffer lists arrive in.
fn stream_channels(device: AudioObjectID, scope: u32) -> Vec<u32> {
    let Ok(bytes) = get_bytes(device, ffi::DEVICE_STREAM_CONFIGURATION, scope) else {
        return Vec::new();
    };
    let list = bytes.as_ptr().cast::<ffi::AudioBufferList>();
    // SAFETY: the HAL filled a well-formed AudioBufferList into `bytes`.
    unsafe {
        (0..(*list).number_buffers as usize)
            .map(|i| (*ffi::AudioBufferList::buffer(list, i)).number_channels)
            .collect()
    }
}

fn channel_count(device: AudioObjectID, scope: u32) -> usize {
    stream_channels(device, scope).iter().sum::<u32>() as usize
}

/// Every stream of `device` in `scope` delivers 32-bit float to an IOProc.
fn streams_are_f32(device: AudioObjectID, scope: u32) -> Result<(), String> {
    for stream in get_ids(device, ffi::DEVICE_STREAMS, scope) {
        let fmt: ffi::StreamBasicDescription =
            get(stream, ffi::STREAM_VIRTUAL_FORMAT, ffi::SCOPE_GLOBAL)?;
        let is_f32 = fmt.format_id == ffi::FORMAT_LINEAR_PCM
            && fmt.format_flags & ffi::FORMAT_FLAG_IS_FLOAT != 0
            && fmt.bits_per_channel == 32;
        if !is_f32 {
            return Err(format!(
                "device stream format is not 32-bit float ({} bits, flags {:#x})",
                fmt.bits_per_channel, fmt.format_flags
            ));
        }
    }
    Ok(())
}

/// Frames of latency in `scope`: device latency + safety offset + the
/// first stream's latency (converter). One buffer is added by the caller.
fn scope_latency(device: AudioObjectID, scope: u32) -> u32 {
    let device_latency: u32 = get(device, ffi::DEVICE_LATENCY, scope).unwrap_or(0);
    let safety: u32 = get(device, ffi::DEVICE_SAFETY_OFFSET, scope).unwrap_or(0);
    let stream_latency = get_ids(device, ffi::DEVICE_STREAMS, scope)
        .first()
        .and_then(|&s| get::<u32>(s, ffi::STREAM_LATENCY, ffi::SCOPE_GLOBAL).ok())
        .unwrap_or(0);
    tracing::debug!(
        scope = %String::from_utf8_lossy(&scope.to_be_bytes()),
        device_latency,
        safety,
        stream_latency,
        "coreaudio duplex: latency parts (frames)"
    );
    device_latency + safety + stream_latency
}

/// Is this device built into the machine (the MacBook's own mic)?
fn is_builtin(device: AudioObjectID) -> bool {
    get::<u32>(device, ffi::DEVICE_TRANSPORT_TYPE, ffi::SCOPE_GLOBAL)
        .is_ok_and(|t| t == ffi::TRANSPORT_BUILT_IN)
}

/// Whether the CoreAudio input called exactly `name` is built in, or `None`
/// when CoreAudio has no input by that name (the caller falls back to
/// matching the name). See [`crate::input_guard`].
pub(crate) fn input_is_builtin(name: &str) -> Option<bool> {
    get_ids(ffi::SYSTEM_OBJECT, ffi::HW_DEVICES, ffi::SCOPE_GLOBAL)
        .into_iter()
        .filter(|&d| channel_count(d, ffi::SCOPE_INPUT) > 0)
        .find(|&d| get_string(d, ffi::OBJECT_NAME).is_some_and(|n| n == name))
        .map(is_builtin)
}

/// Resolve a device by name substring (with channels in `scope`), or the
/// system default for that direction.
fn find_device(name: Option<&str>, scope: u32) -> Result<AudioObjectID, String> {
    let input = scope == ffi::SCOPE_INPUT;
    if let Some(name) = name.filter(|n| !n.is_empty()) {
        let hit = get_ids(ffi::SYSTEM_OBJECT, ffi::HW_DEVICES, ffi::SCOPE_GLOBAL)
            .into_iter()
            .filter(|&d| channel_count(d, scope) > 0)
            .find(|&d| get_string(d, ffi::OBJECT_NAME).is_some_and(|n| n.contains(name)));
        if let Some(d) = hit {
            return Ok(d);
        }
        tracing::warn!(
            device = name,
            "coreaudio duplex: named {} device not found; using default",
            if input { "input" } else { "output" }
        );
    }
    let selector = if input {
        ffi::HW_DEFAULT_INPUT
    } else {
        ffi::HW_DEFAULT_OUTPUT
    };
    let id: AudioObjectID = get(ffi::SYSTEM_OBJECT, selector, ffi::SCOPE_GLOBAL)?;
    if id == ffi::UNKNOWN_OBJECT {
        return Err(format!(
            "no default audio {} device",
            if input { "input" } else { "output" }
        ));
    }
    Ok(id)
}

/// Set the device's nominal rate and wait (bounded) for it to take effect —
/// the change is asynchronous and the IO cycle must not start mid-switch.
fn set_rate(device: AudioObjectID, rate: u32) -> Result<(), String> {
    let current: f64 = get(device, ffi::DEVICE_NOMINAL_SAMPLE_RATE, ffi::SCOPE_GLOBAL)?;
    if (current - rate as f64).abs() < 0.5 {
        return Ok(());
    }
    set(
        device,
        ffi::DEVICE_NOMINAL_SAMPLE_RATE,
        ffi::SCOPE_GLOBAL,
        rate as f64,
    )
    .map_err(|e| format!("{e} (setting {rate} Hz)"))?;
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        let now: f64 = get(device, ffi::DEVICE_NOMINAL_SAMPLE_RATE, ffi::SCOPE_GLOBAL)?;
        if (now - rate as f64).abs() < 0.5 {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err(format!("device did not switch to {rate} Hz"))
}

// ── Aggregate device ──────────────────────────────────────────────────────

/// Owned CF object, released on drop.
struct Cf(ffi::CFTypeRef);

impl Drop for Cf {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: we hold one reference.
            unsafe { ffi::CFRelease(self.0) };
        }
    }
}

fn cf_str(s: &str) -> Cf {
    let c = CString::new(s).unwrap_or_default();
    // SAFETY: plain create; +1 reference owned by the returned `Cf`.
    Cf(unsafe {
        ffi::CFStringCreateWithCString(ptr::null(), c.as_ptr(), ffi::CF_STRING_ENCODING_UTF8)
    })
}

fn cf_i32(v: i32) -> Cf {
    // SAFETY: plain create from a local.
    Cf(unsafe { ffi::CFNumberCreate(ptr::null(), ffi::CF_NUMBER_SINT32, (&raw const v).cast()) })
}

fn cf_dict() -> Cf {
    // SAFETY: plain create with the standard CFType callbacks.
    Cf(unsafe {
        ffi::CFDictionaryCreateMutable(
            ptr::null(),
            0,
            &ffi::kCFTypeDictionaryKeyCallBacks,
            &ffi::kCFTypeDictionaryValueCallBacks,
        )
    })
}

fn dict_set(dict: &Cf, key: &str, value: &Cf) {
    let key = cf_str(key);
    // SAFETY: all three are live CF objects; the dictionary retains them.
    unsafe { ffi::CFDictionarySetValue(dict.0, key.0, value.0) };
}

/// A private aggregate of `input` + `output`: output is the clock, the
/// input is drift-compensated. Invisible to other apps; destroyed with the
/// backend.
fn create_aggregate(
    name: &str,
    input: AudioObjectID,
    output: AudioObjectID,
) -> Result<AudioObjectID, String> {
    let in_uid = get_string(input, ffi::DEVICE_UID).ok_or("input device has no UID")?;
    let out_uid = get_string(output, ffi::DEVICE_UID).ok_or("output device has no UID")?;
    let uid = format!(
        "com.fasttrackstudio.duplex.{}.{}",
        std::process::id(),
        AGGREGATE_SEQ.fetch_add(1, Ordering::Relaxed)
    );

    let sub = |uid: &str, drift: bool| {
        let d = cf_dict();
        dict_set(&d, "uid", &cf_str(uid));
        dict_set(&d, "drift", &cf_i32(drift as i32));
        d
    };
    let in_sub = sub(&in_uid, true);
    let out_sub = sub(&out_uid, false);
    // SAFETY: plain create; the array retains what is appended.
    let subs =
        Cf(unsafe { ffi::CFArrayCreateMutable(ptr::null(), 2, &ffi::kCFTypeArrayCallBacks) });
    // Order matters: the input device's channels come first in the
    // aggregate, then the output device's (see the offsets in `start`).
    unsafe {
        ffi::CFArrayAppendValue(subs.0, in_sub.0);
        ffi::CFArrayAppendValue(subs.0, out_sub.0);
    }

    let desc = cf_dict();
    dict_set(&desc, "uid", &cf_str(&uid));
    dict_set(&desc, "name", &cf_str(name));
    dict_set(&desc, "private", &cf_i32(1));
    dict_set(&desc, "stacked", &cf_i32(0));
    dict_set(&desc, "master", &cf_str(&out_uid));
    dict_set(&desc, "subdevices", &subs);

    let mut id: AudioObjectID = 0;
    // SAFETY: `desc` is a well-formed aggregate description dictionary.
    let status = unsafe { ffi::AudioHardwareCreateAggregateDevice(desc.0, &mut id) };
    check(status, "create aggregate device")?;
    // The aggregate publishes its streams asynchronously; wait for them.
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if channel_count(id, ffi::SCOPE_OUTPUT) > 0 {
            return Ok(id);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    // SAFETY: we created it.
    unsafe { ffi::AudioHardwareDestroyAggregateDevice(id) };
    Err("aggregate device never published its streams".into())
}

static AGGREGATE_SEQ: AtomicU32 = AtomicU32::new(0);

// ── Realtime state + IOProc ───────────────────────────────────────────────

/// Where a flat channel index lives in the IOProc's buffer list:
/// `(buffer, channel within that interleaved buffer)`.
type Slot = Option<(usize, usize)>;

/// Flatten a stream layout into per-channel slots, starting `offset`
/// channels in, for `count` channels.
fn channel_slots(streams: &[u32], offset: usize, count: usize) -> Vec<Slot> {
    let flat: Vec<(usize, usize)> = streams
        .iter()
        .enumerate()
        .flat_map(|(b, &n)| (0..n as usize).map(move |k| (b, k)))
        .collect();
    (0..count).map(|c| flat.get(offset + c).copied()).collect()
}

struct IoState {
    in_slots: Vec<Slot>,
    out_slots: Vec<Slot>,
    /// De-interleaved per-channel scratch, `max_frames` each.
    in_bufs: Vec<Vec<f32>>,
    out_bufs: Vec<Vec<f32>>,
    /// Reused per-block slice views (capacity reserved → no RT allocation).
    in_slices: Vec<&'static [f32]>,
    out_slices: Vec<&'static mut [f32]>,
    max_frames: usize,
    process: ProcessFn,
    stats: Arc<EngineStats>,
    rate: u32,
}

/// Zero every output buffer (the HAL does not guarantee it).
unsafe fn silence(output: *mut ffi::AudioBufferList) {
    if output.is_null() {
        return;
    }
    unsafe {
        for i in 0..(*output).number_buffers as usize {
            let b = ffi::AudioBufferList::buffer(output, i);
            if !(*b).data.is_null() {
                ptr::write_bytes((*b).data.cast::<u8>(), 0, (*b).data_byte_size as usize);
            }
        }
    }
}

/// Frames in this cycle, from whichever side has a buffer.
unsafe fn cycle_frames(list: *const ffi::AudioBufferList) -> Option<usize> {
    unsafe {
        if list.is_null() || (*list).number_buffers == 0 {
            return None;
        }
        let b = ffi::AudioBufferList::buffer(list, 0);
        let ch = (*b).number_channels.max(1) as usize;
        Some((*b).data_byte_size as usize / (4 * ch))
    }
}

unsafe extern "C" fn io_proc(
    _device: AudioObjectID,
    _now: *const ffi::AudioTimeStamp,
    input: *const ffi::AudioBufferList,
    _input_time: *const ffi::AudioTimeStamp,
    output: *mut ffi::AudioBufferList,
    _output_time: *const ffi::AudioTimeStamp,
    client: *mut c_void,
) -> ffi::OSStatus {
    // SAFETY: `client` is the boxed `IoState` handed to
    // AudioDeviceCreateIOProcID, only touched from this (single) IO thread
    // while the proc is registered. Buffer lists are valid for this call.
    unsafe {
        let t0 = Instant::now();
        let st = &mut *client.cast::<IoState>();
        silence(output);
        let Some(n) = cycle_frames(output).or_else(|| cycle_frames(input)) else {
            return 0;
        };
        st.stats.calls.fetch_add(1, Ordering::Relaxed);
        st.stats.block_frames.store(n as u32, Ordering::Relaxed);
        if n == 0 || n > st.max_frames {
            return 0;
        }

        // De-interleave the capture channels we were asked for.
        for (c, slot) in st.in_slots.iter().enumerate() {
            let dst = &mut st.in_bufs[c][..n];
            match *slot {
                Some((b, k)) if !input.is_null() && b < (*input).number_buffers as usize => {
                    let buf = ffi::AudioBufferList::buffer(input, b);
                    let stride = (*buf).number_channels as usize;
                    let src = (*buf).data.cast::<f32>();
                    if src.is_null() || (*buf).data_byte_size as usize / 4 < n * stride {
                        dst.fill(0.0);
                    } else {
                        for (f, d) in dst.iter_mut().enumerate() {
                            *d = *src.add(f * stride + k);
                        }
                    }
                }
                _ => dst.fill(0.0),
            }
        }
        for buf in &mut st.out_bufs {
            buf[..n].fill(0.0);
        }

        st.in_slices.clear();
        for buf in &st.in_bufs {
            st.in_slices
                .push(std::mem::transmute::<&[f32], &'static [f32]>(&buf[..n]));
        }
        st.out_slices.clear();
        for buf in &mut st.out_bufs {
            st.out_slices
                .push(std::mem::transmute::<&mut [f32], &'static mut [f32]>(
                    &mut buf[..n],
                ));
        }
        {
            let inputs: &[&[f32]] = std::mem::transmute(&st.in_slices[..]);
            let outputs: &mut [&mut [f32]] = std::mem::transmute(&mut st.out_slices[..]);
            let mut block = ProcessBlock {
                inputs,
                outputs,
                frames: n,
            };
            (st.process)(&mut block);
        }
        st.in_slices.clear();
        st.out_slices.clear();

        // Interleave the playback channels back into the device streams.
        if !output.is_null() {
            for (c, slot) in st.out_slots.iter().enumerate() {
                let Some((b, k)) = *slot else { continue };
                if b >= (*output).number_buffers as usize {
                    continue;
                }
                let buf = ffi::AudioBufferList::buffer(output, b);
                let stride = (*buf).number_channels as usize;
                let dst = (*buf).data.cast::<f32>();
                if dst.is_null() || (*buf).data_byte_size as usize / 4 < n * stride {
                    continue;
                }
                for (f, &s) in st.out_bufs[c][..n].iter().enumerate() {
                    *dst.add(f * stride + k) = s;
                }
            }
        }

        st.stats
            .record_render_at(t0.elapsed().as_nanos() as u64, st.rate);
        0
    }
}

/// HAL listener: processor overloads count as xruns; a dead device marks
/// the stream errored. Runs on a HAL notification thread, never the IOProc.
unsafe extern "C" fn on_property(
    _object: AudioObjectID,
    count: u32,
    addresses: *const ffi::PropertyAddress,
    client: *mut c_void,
) -> ffi::OSStatus {
    // SAFETY: `client` is the `Arc<EngineStats>` pointer registered in
    // `start`, kept alive until the listeners are removed in `drop`.
    unsafe {
        let stats = &*client.cast::<EngineStats>();
        for i in 0..count as usize {
            match (*addresses.add(i)).selector {
                ffi::DEVICE_PROCESSOR_OVERLOAD => {
                    stats.xruns.fetch_add(1, Ordering::Relaxed);
                    let frames = stats.block_frames.load(Ordering::Relaxed);
                    stats
                        .drops
                        .push(crate::duplex::DropKind::DeviceOverload, 0, 0, frames);
                }
                ffi::DEVICE_IS_ALIVE => {
                    stats.stream_state.store(STATE_ERROR, Ordering::Relaxed);
                    tracing::error!("coreaudio duplex: device is gone");
                }
                _ => {}
            }
        }
    }
    0
}

/// `EngineStats::stream_state` values, matching the PipeWire backend's
/// `pw_filter_state` so owners can watch either the same way.
const STATE_ERROR: i32 = -1;
const STATE_STREAMING: i32 = 3;

const LISTENED: [u32; 2] = [ffi::DEVICE_PROCESSOR_OVERLOAD, ffi::DEVICE_IS_ALIVE];

// ── Backend ───────────────────────────────────────────────────────────────

/// A live CoreAudio duplex IOProc. Dropping it stops audio.
pub struct CoreAudioBackend {
    /// The device the IOProc runs on (the aggregate, when one was made).
    device: AudioObjectID,
    proc_id: ffi::IOProcID,
    /// Private aggregate we created, destroyed on drop.
    aggregate: Option<AudioObjectID>,
    // Boxed so its address (the IOProc's client data) is stable.
    _state: Box<IoState>,
    stats: Arc<EngineStats>,
    /// `Arc::into_raw` of `stats`, registered with the HAL listeners.
    listener_client: *const EngineStats,
    sample_rate: u32,
    latency: (u32, u32),
    node_name: String,
}

// SAFETY: raw HAL ids/pointers owned by this handle; the IOProc touches
// `_state` only through the pointer the HAL holds, which drop unregisters
// before freeing.
unsafe impl Send for CoreAudioBackend {}

impl DuplexBackend for CoreAudioBackend {
    fn start(cfg: DuplexConfig, process: ProcessFn) -> Result<Self, String> {
        // The drop log's clock starts here, not on the realtime thread's
        // first drop.
        let _ = crate::duplex::clock_ns();
        let output = if cfg.outputs > 0 {
            Some(find_device(
                cfg.output_device.as_deref(),
                ffi::SCOPE_OUTPUT,
            )?)
        } else {
            None
        };
        let input = if cfg.inputs > 0 {
            let id = find_device(cfg.input_device.as_deref(), ffi::SCOPE_INPUT)?;
            if is_builtin(id) {
                let name = get_string(id, ffi::OBJECT_NAME).unwrap_or_default();
                // Transport type is exact here; the guard's name matching is
                // only the fallback for names CoreAudio does not know.
                crate::input_guard::check_input_known(&name, true, cfg.allow_builtin_mic)?;
            }
            Some(id)
        } else {
            None
        };
        let describe = |d: Option<AudioObjectID>| {
            d.and_then(|d| get_string(d, ffi::OBJECT_NAME))
                .unwrap_or_else(|| "-".into())
        };
        let (in_name, out_name) = (describe(input), describe(output));

        // Rate goes on the physical devices before any aggregate is built,
        // so the aggregate inherits it rather than resampling.
        if let Some((_, rate)) = cfg.latency {
            for d in [input, output].into_iter().flatten() {
                set_rate(d, rate)?;
            }
        }

        // One device, or a private aggregate of two.
        let (device, aggregate, in_offset, out_offset) = match (input, output) {
            (Some(i), Some(o)) if i != o => {
                let agg = create_aggregate(&cfg.name, i, o)?;
                // Aggregate channels run input device first, then output
                // device: our outputs start after the input device's own.
                (agg, Some(agg), 0, channel_count(i, ffi::SCOPE_OUTPUT))
            }
            (Some(d), _) | (None, Some(d)) => (d, None, 0, 0),
            (None, None) => return Err("duplex config has no inputs and no outputs".into()),
        };
        let fail = |e: String| {
            if let Some(agg) = aggregate {
                // SAFETY: we created it.
                unsafe { ffi::AudioHardwareDestroyAggregateDevice(agg) };
            }
            e
        };

        streams_are_f32(device, ffi::SCOPE_INPUT).map_err(fail)?;
        streams_are_f32(device, ffi::SCOPE_OUTPUT).map_err(fail)?;

        // Buffer: clamp into the device's range (another client may still
        // force it smaller — the HAL runs at the smallest request).
        let range: ffi::ValueRange = get(
            device,
            ffi::DEVICE_BUFFER_FRAME_SIZE_RANGE,
            ffi::SCOPE_GLOBAL,
        )
        .map_err(fail)?;
        if let Some(frames) = cfg.latency.map(|(f, _)| f).or(cfg.buffer) {
            let clamped = (frames as f64).clamp(range.minimum, range.maximum) as u32;
            if clamped != frames {
                tracing::warn!(
                    requested = frames,
                    granted = clamped,
                    "coreaudio duplex: buffer outside the device range {}..={}",
                    range.minimum,
                    range.maximum
                );
            }
            let current: u32 =
                get(device, ffi::DEVICE_BUFFER_FRAME_SIZE, ffi::SCOPE_GLOBAL).unwrap_or(u32::MAX);
            if cfg.buffer_lower_only && current <= clamped {
                tracing::info!(
                    requested = clamped,
                    current,
                    "coreaudio duplex: keeping the process's smaller block (lower-only)"
                );
            } else {
                set(
                    device,
                    ffi::DEVICE_BUFFER_FRAME_SIZE,
                    ffi::SCOPE_GLOBAL,
                    clamped,
                )
                .map_err(fail)?;
            }
        }
        let buffer: u32 =
            get(device, ffi::DEVICE_BUFFER_FRAME_SIZE, ffi::SCOPE_GLOBAL).map_err(fail)?;
        let rate: f64 =
            get(device, ffi::DEVICE_NOMINAL_SAMPLE_RATE, ffi::SCOPE_GLOBAL).map_err(fail)?;
        let rate = rate.round() as u32;

        let in_streams = stream_channels(device, ffi::SCOPE_INPUT);
        let out_streams = stream_channels(device, ffi::SCOPE_OUTPUT);
        let in_slots = channel_slots(&in_streams, in_offset, cfg.inputs);
        let out_slots = channel_slots(&out_streams, out_offset, cfg.outputs);
        if in_slots.iter().any(Option::is_none) || out_slots.iter().any(Option::is_none) {
            tracing::warn!(
                inputs = cfg.inputs,
                outputs = cfg.outputs,
                device_inputs = in_streams.iter().sum::<u32>(),
                device_outputs = out_streams.iter().sum::<u32>(),
                "coreaudio duplex: more channels requested than the device has; extras are silent"
            );
        }

        // Scratch sized for the largest block the device can deliver.
        let max_frames = (range.maximum as usize)
            .max(buffer as usize)
            .clamp(1, 16_384);
        let stats = Arc::new(EngineStats::default());
        let mut state = Box::new(IoState {
            in_slots,
            out_slots,
            in_bufs: vec![vec![0.0; max_frames]; cfg.inputs],
            out_bufs: vec![vec![0.0; max_frames]; cfg.outputs],
            in_slices: Vec::with_capacity(cfg.inputs),
            out_slices: Vec::with_capacity(cfg.outputs),
            max_frames,
            process,
            stats: stats.clone(),
            rate,
        });

        let listener_client = Arc::into_raw(stats.clone());
        for selector in LISTENED {
            let addr = address(selector, ffi::SCOPE_GLOBAL);
            // SAFETY: `listener_client` stays alive until drop removes these.
            unsafe {
                ffi::AudioObjectAddPropertyListener(
                    device,
                    &addr,
                    on_property,
                    listener_client.cast_mut().cast(),
                )
            };
        }
        let remove_listeners = || {
            for selector in LISTENED {
                let addr = address(selector, ffi::SCOPE_GLOBAL);
                // SAFETY: same registration as above.
                unsafe {
                    ffi::AudioObjectRemovePropertyListener(
                        device,
                        &addr,
                        on_property,
                        listener_client.cast_mut().cast(),
                    )
                };
            }
            // SAFETY: balances the `Arc::into_raw` above.
            drop(unsafe { Arc::from_raw(listener_client) });
        };

        let mut proc_id: ffi::IOProcID = ptr::null_mut();
        let client = (&mut *state as *mut IoState).cast::<c_void>();
        // SAFETY: `state` is boxed and owned by the returned backend, which
        // stops and destroys the proc before freeing it.
        let status =
            unsafe { ffi::AudioDeviceCreateIOProcID(device, io_proc, client, &mut proc_id) };
        if let Err(e) = check(status, "create IOProc") {
            remove_listeners();
            return Err(fail(e));
        }
        // SAFETY: proc registered above.
        let status = unsafe { ffi::AudioDeviceStart(device, proc_id) };
        if let Err(e) = check(status, "start device") {
            // SAFETY: registered above, never started.
            unsafe { ffi::AudioDeviceDestroyIOProcID(device, proc_id) };
            remove_listeners();
            return Err(fail(e));
        }
        stats.stream_state.store(STATE_STREAMING, Ordering::Relaxed);
        // Render workers join the IO thread's workgroup (see `rt_workers`).
        // The property hands over a retained `os_workgroup_t`.
        let workgroup = get::<usize>(device, ffi::DEVICE_IO_THREAD_WORKGROUP, ffi::SCOPE_GLOBAL)
            .unwrap_or(0) as *mut c_void;
        crate::rt_workers::publish(
            workgroup,
            u64::from(buffer) * 1_000_000_000 / u64::from(rate.max(1)),
        );

        let latency = (
            scope_latency(device, ffi::SCOPE_INPUT) + buffer,
            scope_latency(device, ffi::SCOPE_OUTPUT) + buffer,
        );
        tracing::info!(
            input = %in_name,
            output = %out_name,
            aggregate = aggregate.is_some(),
            rate,
            buffer,
            input_latency = latency.0,
            output_latency = latency.1,
            round_trip_ms = (latency.0 + latency.1) as f64 * 1000.0 / rate.max(1) as f64,
            "coreaudio duplex: started"
        );

        Ok(Self {
            device,
            proc_id,
            aggregate,
            _state: state,
            stats,
            listener_client,
            sample_rate: rate,
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

impl Drop for CoreAudioBackend {
    fn drop(&mut self) {
        // SAFETY: tear down in reverse: stop the IO cycle (synchronous from
        // a non-IO thread), unregister the proc, drop listeners, then the
        // aggregate. `_state` is freed after this body, once nothing can
        // call into it.
        unsafe {
            ffi::AudioDeviceStop(self.device, self.proc_id);
            ffi::AudioDeviceDestroyIOProcID(self.device, self.proc_id);
            for selector in LISTENED {
                let addr = address(selector, ffi::SCOPE_GLOBAL);
                ffi::AudioObjectRemovePropertyListener(
                    self.device,
                    &addr,
                    on_property,
                    self.listener_client.cast_mut().cast(),
                );
            }
            drop(Arc::from_raw(self.listener_client));
            if let Some(agg) = self.aggregate {
                ffi::AudioHardwareDestroyAggregateDevice(agg);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::channel_slots;

    /// Every input CoreAudio reports as built in (a MacBook's own mic) is
    /// refused by default and allowed by the override; nothing else is
    /// touched. Vacuous on a Mac with no built-in input.
    #[test]
    fn builtin_inputs_are_refused_unless_allowed() {
        use super::{channel_count, ffi, get_ids, get_string, is_builtin};
        use crate::input_guard::check_input;
        for d in get_ids(ffi::SYSTEM_OBJECT, ffi::HW_DEVICES, ffi::SCOPE_GLOBAL) {
            if channel_count(d, ffi::SCOPE_INPUT) == 0 {
                continue;
            }
            let Some(name) = get_string(d, ffi::OBJECT_NAME) else {
                continue;
            };
            if is_builtin(d) {
                assert!(check_input(&name, false).is_err(), "{name} was allowed");
                assert!(
                    check_input(&name, true).is_ok(),
                    "{name} ignored the override"
                );
            }
        }
    }

    #[test]
    fn channels_flatten_across_multi_stream_devices() {
        // Two stereo streams then a mono one (UA Apollo / Dante style).
        let streams = [2, 2, 1];
        assert_eq!(
            channel_slots(&streams, 0, 5),
            vec![
                Some((0, 0)),
                Some((0, 1)),
                Some((1, 0)),
                Some((1, 1)),
                Some((2, 0))
            ]
        );
        // Offset (aggregate: outputs after the input device's) and overflow.
        assert_eq!(
            channel_slots(&streams, 3, 3),
            vec![Some((1, 1)), Some((2, 0)), None]
        );
    }

    #[test]
    fn fourcc_matches_the_headers() {
        assert_eq!(super::ffi::fourcc(b"glob"), 0x676c_6f62);
        assert_eq!(super::ffi::SCOPE_OUTPUT, 0x6f75_7470);
    }
}
