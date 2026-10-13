//! Linux duplex backend — a native PipeWire `pw_filter` with capture + playback
//! ports processed in one realtime callback. No ring: input and output are the
//! same graph cycle (the EasyEffects model, in Rust via raw `pw_filter` FFI
//! since the high-level crate wraps `stream` but not `filter`).
//!
//! Lifecycle is managed through raw pointers (own `pw_thread_loop` → `pw_context`
//! → `pw_core` → `pw_filter`) so the handle is `Send` — the non-`Send` Rc
//! wrappers from the `pipewire` crate would otherwise pin it to one thread. The
//! `pw_thread_loop` runs the graph on its own OS thread; our `process` closure
//! fires on PipeWire's realtime data thread.

use std::ffi::{CStr, CString, c_void};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use libspa::sys as spa;
use pipewire::sys as pw;

use crate::duplex::{DuplexBackend, DuplexConfig, EngineStats, ProcessBlock, ProcessFn};

/// Realtime-thread state behind the filter's `process` callback.
struct DuplexState {
    in_ports: Vec<*mut c_void>,
    out_ports: Vec<*mut c_void>,
    /// Reused per-block slice views (capacity pre-reserved → no RT allocation).
    /// `'static` is a lie scoped to one callback; the buffers are valid then.
    in_slices: Vec<&'static [f32]>,
    out_slices: Vec<&'static mut [f32]>,
    /// Zero-fill for a null capture buffer / scratch sink for a null playback
    /// buffer, sized to the largest block seen.
    silence: Vec<f32>,
    sink: Vec<f32>,
    process: ProcessFn,
    stats: Arc<EngineStats>,
    /// Last seen `spa_io_clock.xrun` (accumulated xrun duration) — a rise
    /// means the graph reported an xrun since the previous block.
    last_xrun: u64,
    /// Graph rate, for turning a block's frame count into its deadline.
    rate: u32,
}

unsafe extern "C" fn on_process(data: *mut c_void, position: *mut spa::spa_io_position) {
    // SAFETY: `data` is the boxed `DuplexState` address handed to
    // `pw_filter_add_listener`; `position` is valid for the call. The DSP buffer
    // pointers are valid for this callback only, which is why the slice views
    // are rebuilt (and cleared) every block. Whole body is one unsafe scope.
    unsafe {
        let t0 = Instant::now();
        let st = &mut *(data as *mut DuplexState);
        let n = (*position).clock.duration as usize;
        st.stats.calls.fetch_add(1, Ordering::Relaxed);
        st.stats.block_frames.store(n as u32, Ordering::Relaxed);
        // Xrun accounting: the graph accumulates xrun duration in the position
        // clock; count each block where it advanced.
        let xrun = (*position).clock.xrun;
        if xrun > st.last_xrun {
            st.stats.xruns.fetch_add(1, Ordering::Relaxed);
            let frames = st.stats.block_frames.load(Ordering::Relaxed);
            st.stats
                .drops
                .push(crate::duplex::DropKind::DeviceOverload, 0, 0, frames);
        }
        st.last_xrun = xrun;
        if n == 0 {
            return;
        }
        if st.silence.len() < n {
            st.silence.resize(n, 0.0);
            st.sink.resize(n, 0.0);
        }
        for s in &mut st.silence[..n] {
            *s = 0.0;
        }

        st.in_slices.clear();
        for &p in &st.in_ports {
            let buf = pw::pw_filter_get_dsp_buffer(p, n as u32) as *const f32;
            let slice: &[f32] = if buf.is_null() {
                &st.silence[..n]
            } else {
                std::slice::from_raw_parts(buf, n)
            };
            st.in_slices
                .push(std::mem::transmute::<&[f32], &'static [f32]>(slice));
        }

        st.out_slices.clear();
        // Borrow-checker dance: collect raw out pointers first, then build &mut.
        let sink_ptr = st.sink.as_mut_ptr();
        for i in 0..st.out_ports.len() {
            let buf = pw::pw_filter_get_dsp_buffer(st.out_ports[i], n as u32) as *mut f32;
            let p = if buf.is_null() { sink_ptr } else { buf };
            let slice: &mut [f32] = std::slice::from_raw_parts_mut(p, n);
            st.out_slices
                .push(std::mem::transmute::<&mut [f32], &'static mut [f32]>(slice));
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

        st.stats
            .record_render_at(t0.elapsed().as_nanos() as u64, st.rate);
    } // unsafe
}

unsafe extern "C" fn on_state_changed(
    data: *mut c_void,
    old: pw::pw_filter_state,
    state: pw::pw_filter_state,
    error: *const std::os::raw::c_char,
) {
    // SAFETY: same user-data contract as `on_process`; fires on the
    // pw_thread_loop thread (not the RT data thread), so logging is fine.
    unsafe {
        let st = &*(data as *const DuplexState);
        st.stats.stream_state.store(state, Ordering::Relaxed);
        let name = |s: pw::pw_filter_state| -> &'static str {
            let p = pw::pw_filter_state_as_string(s);
            if p.is_null() {
                "?"
            } else {
                CStr::from_ptr(p).to_str().unwrap_or("?")
            }
        };
        let err = if error.is_null() {
            None
        } else {
            Some(CStr::from_ptr(error).to_string_lossy())
        };
        if state == pw::pw_filter_state_PW_FILTER_STATE_ERROR {
            tracing::error!(
                "duplex filter state {} -> {} (error: {})",
                name(old),
                name(state),
                err.as_deref().unwrap_or("unknown"),
            );
        } else {
            tracing::warn!("duplex filter state {} -> {}", name(old), name(state));
        }
    }
}

/// A live native-PipeWire duplex node.
pub struct PipewireBackend {
    thread_loop: *mut pw::pw_thread_loop,
    context: *mut pw::pw_context,
    core: *mut pw::pw_core,
    filter: *mut pw::pw_filter,
    // Boxed so its address (handed to the C callback as user-data) is stable.
    _state: Box<DuplexState>,
    // The hook stores a pointer into `_events`; both must outlive the filter.
    _listener: Box<spa::spa_hook>,
    _events: Box<pw::pw_filter_events>,
    stats: Arc<EngineStats>,
    sample_rate: u32,
    node_name: String,
}

// SAFETY: all fields are owned raw pointers / Arc; the pw_thread_loop runs the
// graph on its own thread and we only touch these from the owning thread (plus
// the stable boxed state address handed to the RT callback).
unsafe impl Send for PipewireBackend {}

unsafe fn pset(props: *mut pw::pw_properties, k: &CStr, v: &CStr) {
    unsafe { pw::pw_properties_set(props, k.as_ptr(), v.as_ptr()) };
}

impl DuplexBackend for PipewireBackend {
    fn start(cfg: DuplexConfig, process: ProcessFn) -> Result<Self, String> {
        let sample_rate = cfg.latency.map(|(_, r)| r).unwrap_or(48_000);
        let stats = Arc::new(EngineStats::default());

        let mut state = Box::new(DuplexState {
            in_ports: Vec::with_capacity(cfg.inputs),
            out_ports: Vec::with_capacity(cfg.outputs),
            in_slices: Vec::with_capacity(cfg.inputs),
            out_slices: Vec::with_capacity(cfg.outputs),
            silence: vec![0.0; 4096],
            sink: vec![0.0; 4096],
            process,
            stats: stats.clone(),
            last_xrun: 0,
            rate: sample_rate,
        });

        unsafe {
            pipewire::init();
            let name_c = CString::new(cfg.name.clone()).map_err(|_| "bad node name")?;
            let thread_loop = pw::pw_thread_loop_new(name_c.as_ptr(), ptr::null());
            if thread_loop.is_null() {
                return Err("pw_thread_loop_new failed".into());
            }
            // Start the loop thread, then do all PipeWire setup under its lock —
            // the connect handshakes only complete while the loop iterates.
            pw::pw_thread_loop_start(thread_loop);
            pw::pw_thread_loop_lock(thread_loop);

            let loop_ = pw::pw_thread_loop_get_loop(thread_loop);
            let context = pw::pw_context_new(loop_, ptr::null_mut(), 0);
            if context.is_null() {
                pw::pw_thread_loop_unlock(thread_loop);
                pw::pw_thread_loop_stop(thread_loop);
                pw::pw_thread_loop_destroy(thread_loop);
                return Err("pw_context_new failed".into());
            }
            let core = pw::pw_context_connect(context, ptr::null_mut(), 0);
            if core.is_null() {
                pw::pw_thread_loop_unlock(thread_loop);
                pw::pw_thread_loop_stop(thread_loop);
                pw::pw_context_destroy(context);
                pw::pw_thread_loop_destroy(thread_loop);
                return Err("pw_context_connect failed (is PipeWire running?)".into());
            }

            // Filter node.
            let props = pw::pw_properties_new(ptr::null());
            pset(props, c"media.type", c"Audio");
            pset(props, c"media.category", c"Duplex");
            pset(props, c"media.role", c"DSP");
            pset(props, c"node.name", name_c.as_c_str());
            if let Some((frames, rate)) = cfg.latency {
                let lat = CString::new(format!("{frames}/{rate}")).unwrap();
                pset(props, c"node.latency", lat.as_c_str());
            }
            let filter = pw::pw_filter_new(core, name_c.as_ptr(), props);
            if filter.is_null() {
                pw::pw_core_disconnect(core);
                pw::pw_thread_loop_unlock(thread_loop);
                pw::pw_thread_loop_stop(thread_loop);
                pw::pw_context_destroy(context);
                pw::pw_thread_loop_destroy(thread_loop);
                return Err("pw_filter_new failed".into());
            }

            let add_port = |dir: u32, name: &str| -> *mut c_void {
                // Closures don't inherit the enclosing unsafe scope.
                unsafe {
                    let p = pw::pw_properties_new(ptr::null());
                    pset(p, c"format.dsp", c"32 bit float mono audio");
                    let pn = CString::new(name).unwrap();
                    pw::pw_properties_set(p, c"port.name".as_ptr(), pn.as_ptr());
                    pw::pw_filter_add_port(
                        filter,
                        dir,
                        pw::pw_filter_port_flags_PW_FILTER_PORT_FLAG_MAP_BUFFERS,
                        0,
                        p,
                        ptr::null_mut(),
                        0,
                    )
                }
            };
            for c in 0..cfg.inputs {
                state
                    .in_ports
                    .push(add_port(spa::SPA_DIRECTION_INPUT, &format!("input_{c}")));
            }
            for c in 0..cfg.outputs {
                state
                    .out_ports
                    .push(add_port(spa::SPA_DIRECTION_OUTPUT, &format!("output_{c}")));
            }

            // RT process listener. Both the events table and the hook must
            // outlive the filter (the hook stores a pointer to `events`), so
            // they're boxed and kept in the returned struct. User-data is the
            // stable boxed-state heap address.
            let mut events: Box<pw::pw_filter_events> = Box::new(std::mem::zeroed());
            events.version = pw::PW_VERSION_FILTER_EVENTS;
            events.process = Some(on_process);
            events.state_changed = Some(on_state_changed);
            let mut listener: Box<spa::spa_hook> = Box::new(std::mem::zeroed());
            let state_ptr = (&mut *state) as *mut DuplexState as *mut c_void;
            pw::pw_filter_add_listener(filter, &mut *listener, &*events, state_ptr);

            if pw::pw_filter_connect(
                filter,
                pw::pw_filter_flags_PW_FILTER_FLAG_RT_PROCESS,
                ptr::null_mut(),
                0,
            ) != 0
            {
                pw::pw_filter_destroy(filter);
                pw::pw_core_disconnect(core);
                pw::pw_thread_loop_unlock(thread_loop);
                pw::pw_thread_loop_stop(thread_loop);
                pw::pw_context_destroy(context);
                pw::pw_thread_loop_destroy(thread_loop);
                return Err("pw_filter_connect failed".into());
            }

            pw::pw_thread_loop_unlock(thread_loop);

            Ok(Self {
                thread_loop,
                context,
                core,
                filter,
                _state: state,
                _listener: listener,
                _events: events,
                stats,
                sample_rate,
                node_name: cfg.name,
            })
        }
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
}

impl Drop for PipewireBackend {
    fn drop(&mut self) {
        unsafe {
            pw::pw_thread_loop_lock(self.thread_loop);
            pw::pw_filter_destroy(self.filter);
            pw::pw_core_disconnect(self.core);
            pw::pw_thread_loop_unlock(self.thread_loop);
            pw::pw_thread_loop_stop(self.thread_loop);
            pw::pw_context_destroy(self.context);
            pw::pw_thread_loop_destroy(self.thread_loop);
        }
    }
}
