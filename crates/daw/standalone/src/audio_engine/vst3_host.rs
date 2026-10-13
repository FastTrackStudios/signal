//! VST3 plugin host (skeleton).
//!
//! Mirrors the architecture of [`crate::audio_engine::plugin_host`]
//! (the CLAP host) but talks to Steinberg's COM-based VST3 ABI via
//! the auto-generated [`vst3`] crate bindings.
//!
//! Scope of this skeleton: load a `.vst3` bundle from disk, find the
//! first audio-effect class, instantiate its `IComponent` and query
//! `IAudioProcessor`, walk the activation lifecycle (initialize →
//! setupProcessing → setActive(true) → setProcessing(true)), and run
//! `process()` per audio block. **No parameter support yet** —
//! `IEditController`, `IParameterChanges`, `IComponentHandler`, and
//! presets land in a follow-up.
//!
//! Feature-gated under `vst3-host`. Native-only (uses [`libloading`]
//! to dlopen the platform-specific shared library inside the bundle).

#![cfg(feature = "vst3-host")]
// VST3 hosting is inherently `unsafe`: dlopen + COM vtables + raw
// pointers. The unsafety is encapsulated inside this module; callers
// see safe `&mut self` methods.
#![allow(unsafe_code)]

use std::cell::RefCell;
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::ptr;

use libloading::Library;
use vst3::Steinberg::IBStream_::IStreamSeekMode_;
use vst3::Steinberg::Vst::ControllerNumbers_::{kAfterTouch, kCtrlProgramChange, kPitchBend};
use vst3::Steinberg::Vst::DataEvent_::DataTypes_ as Vst3DataTypes_;
use vst3::Steinberg::Vst::Event_::EventTypes_;
use vst3::Steinberg::Vst::NoteExpressionTypeIDs_;
use vst3::Steinberg::Vst::ProcessContext_::StatesAndFlags_ as ProcessContextFlags_;
use vst3::Steinberg::Vst::{
    AudioBusBuffers, AudioBusBuffers__type0, BusDirections_, DataEvent, Event, Event__type0,
    IAudioProcessor, IAudioProcessorTrait, IComponent, IComponent_iid, IComponentHandler,
    IComponentHandlerTrait, IComponentTrait, IConnectionPoint, IConnectionPointTrait,
    IEditController, IEditController_iid, IEditControllerTrait, IEventList, IEventListTrait,
    IHostApplication, IHostApplicationTrait, IMidiMapping, IMidiMappingTrait, IParamValueQueue,
    IParamValueQueueTrait, IParameterChanges, IParameterChangesTrait, MediaTypes_,
    NoteExpressionTypeID, NoteExpressionValue, NoteExpressionValueEvent, NoteOffEvent, NoteOnEvent,
    ParamID, ParamValue, ParameterInfo, PolyPressureEvent, ProcessContext, ProcessData,
    ProcessModes_, ProcessSetup, SpeakerArr, SpeakerArrangement, String128, SymbolicSampleSizes_,
};
use vst3::Steinberg::{
    FIDString, FUnknown, IBStream, IBStreamTrait, IPluginBaseTrait, IPluginFactory,
    IPluginFactoryTrait, PClassInfo, TUID, char8, char16, int32, int64, kInvalidArgument,
    kNoInterface, kNotImplemented, kResultFalse, kResultOk, kResultTrue, tresult,
};
use vst3::{Class, ComPtr, ComWrapper};

use crate::plugin::{PluginEvents, PluginMidiEvent};

// ── Public host handle ───────────────────────────────────────────────

/// VST3 host. Cheap value type — module entry/exit lifecycle is
/// managed by the loaded bundle, not the host handle.
#[derive(Clone, Default)]
pub struct Vst3Host {
    _private: (),
}

impl Vst3Host {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enumerate every audio-effect class inside a `.vst3` bundle.
    pub fn list_in_bundle(
        &self,
        bundle_path: &Path,
    ) -> Result<Vec<Vst3PluginDescriptor>, Vst3HostError> {
        let module = Vst3Module::open(bundle_path)?;
        let descriptors = module.list_audio_classes();
        // Keep `module` alive until after enumeration; descriptors are
        // owning Strings so it's fine to drop the module afterwards.
        drop(module);
        Ok(descriptors)
    }

    /// Instantiate the audio-effect class at `plugin_index` inside
    /// the bundle. The returned [`LoadedVst3Plugin`] keeps the
    /// underlying module loaded for the lifetime of the plugin.
    pub fn load(
        &self,
        bundle_path: &Path,
        plugin_index: usize,
    ) -> Result<LoadedVst3Plugin, Vst3HostError> {
        let module = Vst3Module::open(bundle_path)?;
        let audio_classes = module.list_audio_classes();
        let descriptor = audio_classes
            .get(plugin_index)
            .ok_or(Vst3HostError::IndexOutOfRange)?
            .clone();

        // SAFETY: `descriptor.cid` was just read from the factory
        // we still hold open via `module`; the COM call below
        // populates a fresh out-pointer.
        let component: ComPtr<IComponent> = unsafe {
            let factory = module.factory()?;
            let mut raw: *mut c_void = ptr::null_mut();
            let res = factory.createInstance(
                descriptor.cid.as_ptr() as FIDString,
                IComponent_iid.as_ptr() as FIDString,
                &mut raw,
            );
            if res != kResultOk || raw.is_null() {
                return Err(Vst3HostError::Instantiate);
            }
            ComPtr::from_raw(raw as *mut IComponent).ok_or(Vst3HostError::Instantiate)?
        };

        // Query IAudioProcessor off the same object via COM cast.
        let processor: ComPtr<IAudioProcessor> =
            component.cast().ok_or(Vst3HostError::NoAudioProcessor)?;

        // Resolve IEditController. Single-component plugins implement
        // both IComponent and IEditController on the same object —
        // a COM cast finds it. Two-component plugins (the canonical
        // VST3 style) report a separate controller class id via
        // `IComponent::getControllerClassId`, which we createInstance.
        // Missing controller is non-fatal — the plugin just won't
        // expose params.
        let controller = unsafe {
            let base = if let Some(ptr) = component.cast::<IEditController>() {
                Some((ptr, false))
            } else {
                let mut controller_cid: TUID = [0; 16];
                let res = component.getControllerClassId(&mut controller_cid);
                if res == kResultOk || res == kResultTrue {
                    let factory = module.factory()?;
                    let mut raw: *mut c_void = ptr::null_mut();
                    let res = factory.createInstance(
                        controller_cid.as_ptr() as FIDString,
                        IEditController_iid.as_ptr() as FIDString,
                        &mut raw,
                    );
                    if res == kResultOk && !raw.is_null() {
                        ComPtr::from_raw(raw as *mut IEditController).map(|p| (p, true))
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            base.map(|(ptr, separate)| {
                // IMidiMapping is queryable off the controller.
                let midi_mapping = ptr.cast::<IMidiMapping>();
                ControllerHandle {
                    ptr,
                    separate,
                    initialized: false,
                    midi_mapping,
                }
            })
        };

        Ok(LoadedVst3Plugin {
            descriptor,
            activation: None,
            initialized: false,
            controller,
            processor,
            component,
            _component_handler: ComWrapper::new(HostComponentHandler),
            _host_app: ComWrapper::new(HostApplication),
            _module: module,
            output_buses: 1,
        })
    }
}

// ── Module loader ────────────────────────────────────────────────────

struct Vst3Module {
    // Drop order: factory (COM) → exit_fn (calls into library) →
    // _lib (unloads library). See LoadedVst3Plugin for the rationale.
    factory: ComPtr<IPluginFactory>,
    exit_fn: Option<unsafe extern "system" fn() -> bool>,
    /// The `CFBundleRef` handed to `bundleEntry` on macOS; released after
    /// `bundleExit`. Null elsewhere (and for single-file bundles).
    entry_arg: *mut c_void,
    _lib: Library,
}

impl Vst3Module {
    fn open(bundle_path: &Path) -> Result<Self, Vst3HostError> {
        let lib_path = resolve_lib_path(bundle_path)?;
        // SAFETY: dlopen of an arbitrary path is fundamentally
        // unsafe; the user picked the bundle, the host contract
        // accepts the unsafety. See header comment.
        let lib = unsafe { Library::new(&lib_path) }.map_err(|_| Vst3HostError::BundleLoad)?;

        // Module entry: `bundleEntry` (macOS), `ModuleEntry` (Linux),
        // `InitDll` (Windows). Errors-out only if the symbol exists
        // *and* the call returned false.
        //
        // macOS plugins get their own `CFBundleRef` — many use it to find
        // resources in the bundle, and crash or refuse to load on null.
        let entry_arg = module_entry_arg(bundle_path);
        unsafe {
            if let Ok(entry) = lib.get::<unsafe extern "system" fn(*mut c_void) -> bool>(
                module_entry_symbol().as_bytes(),
            ) && !entry(entry_arg)
            {
                release_module_entry_arg(entry_arg);
                return Err(Vst3HostError::ModuleEntry);
            }
        }

        // Resolve GetPluginFactory.
        let factory: Result<ComPtr<IPluginFactory>, Vst3HostError> = unsafe {
            let get_factory = lib
                .get::<unsafe extern "system" fn() -> *mut IPluginFactory>(b"GetPluginFactory")
                .map_err(|_| Vst3HostError::NoFactory)?;
            let raw = get_factory();
            ComPtr::from_raw(raw).ok_or(Vst3HostError::NoFactory)
        };
        let factory = match factory {
            Ok(factory) => factory,
            Err(err) => {
                release_module_entry_arg(entry_arg);
                return Err(err);
            }
        };

        // Stash the exit function pointer before we move `lib` into
        // the struct. We can't keep the libloading::Symbol around
        // because it borrows from `lib`.
        let exit_fn = unsafe {
            lib.get::<unsafe extern "system" fn() -> bool>(module_exit_symbol().as_bytes())
                .ok()
                .map(|s| *s)
        };

        Ok(Self {
            _lib: lib,
            factory,
            exit_fn,
            entry_arg,
        })
    }

    fn factory(&self) -> Result<&ComPtr<IPluginFactory>, Vst3HostError> {
        Ok(&self.factory)
    }

    fn list_audio_classes(&self) -> Vec<Vst3PluginDescriptor> {
        let mut out = Vec::new();
        let count = unsafe { self.factory.countClasses() };
        for i in 0..count {
            let mut info = PClassInfo {
                cid: [0; 16],
                cardinality: 0,
                category: [0; 32],
                name: [0; 64],
            };
            let res = unsafe { self.factory.getClassInfo(i, &mut info) };
            if res != kResultOk {
                continue;
            }
            let category = char8_array_to_string(&info.category);
            if category != "Audio Module Class" {
                continue;
            }
            out.push(Vst3PluginDescriptor {
                cid: info.cid,
                name: char8_array_to_string(&info.name),
                category,
                vendor: String::new(),
                version: String::new(),
            });
        }
        out
    }
}

impl Drop for Vst3Module {
    fn drop(&mut self) {
        // The factory must be released before `_lib` is dropped,
        // which happens automatically via field-drop order
        // (factory: ComPtr is declared before _lib). Call ModuleExit
        // before releasing the library handle.
        if let Some(exit) = self.exit_fn.take() {
            // SAFETY: symbol resolved at load time; bundle is still
            // mapped because `_lib` is dropped after this Drop body.
            unsafe {
                let _ = exit();
            }
        }
        release_module_entry_arg(self.entry_arg);
    }
}

// ── Loaded plugin ────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Vst3PluginDescriptor {
    pub cid: TUID,
    pub name: String,
    pub category: String,
    pub vendor: String,
    pub version: String,
}

pub struct LoadedVst3Plugin {
    descriptor: Vst3PluginDescriptor,
    // Field drop order matters: COM objects must Release before the
    // library is unloaded, otherwise the vtable's Release thunk
    // calls into an unmapped page (SIGSEGV on plugin teardown, seen
    // with Pianoteq + Atlas which call back into their .so during
    // Release). Rust drops fields in declaration order, so list
    // activation → controller → processor → component → _host_app
    // → _component_handler → _module so the bundle stays mapped
    // until every COM ref is gone.
    activation: Option<ActivationGuard>,
    /// Stereo output buses to run (main + aux), set before `prepare`
    /// (see [`LoadedVst3Plugin::set_output_buses`]); 1 by default.
    output_buses: u32,
    /// `IPluginBase::initialize` has run on the component (and a separate
    /// controller). Tracked apart from `activation` because state and
    /// parameter calls are legal — and required to be preceded by
    /// initialize — on a plugin the audio engine has not prepared yet.
    initialized: bool,
    controller: Option<ControllerHandle>,
    processor: ComPtr<IAudioProcessor>,
    component: ComPtr<IComponent>,
    /// Host-side IComponentHandler installed on the controller after
    /// initialize(). Lives for the plugin's lifetime so the COM ref
    /// the controller holds stays valid.
    _component_handler: ComWrapper<HostComponentHandler>,
    /// Host-side IHostApplication passed as the context to
    /// `initialize()`. Some plugins keep a pointer to this — must
    /// outlive `terminate()`.
    _host_app: ComWrapper<HostApplication>,
    /// Keep the bundle open for the lifetime of the COM objects.
    _module: Vst3Module,
}

/// Holds the controller pointer + whether we instantiated it
/// separately (and thus need to `terminate()` it ourselves).
struct ControllerHandle {
    ptr: ComPtr<IEditController>,
    separate: bool,
    initialized: bool,
    /// Optional `IMidiMapping` queried off the controller. When
    /// present we translate CC / pitch-bend / program-change /
    /// channel-aftertouch events into `IParameterChanges` points
    /// (VST3's canonical MIDI-CC routing). When absent the host
    /// falls back to dropping these events — VST3 has no equivalent
    /// of CLAP's raw-MIDI-bytes shortcut.
    midi_mapping: Option<ComPtr<IMidiMapping>>,
}

struct ActivationGuard {
    sample_rate: f64,
    block_size: u32,
    /// Whether bus 0 in each direction exists + was activated. Pure
    /// effects have both; instruments typically lack an input bus;
    /// pure analyzers / meters may lack an output bus.
    has_input_bus: bool,
    has_output_bus: bool,
    /// Scratch L/R buffers for de-interleaved input. Reused per block.
    scratch_l: Vec<f32>,
    scratch_r: Vec<f32>,
    /// Pointer arrays for `AudioBusBuffers::channelBuffers32`.
    /// Allocated once at prepare time, reused per block. The pointers
    /// inside are reseated each block to the current scratch slices.
    in_ptrs: [*mut f32; 2],
    out_ptrs: [*mut f32; 2],
    /// Aux output buses (1..): their L/R buffers and pointer arrays,
    /// refilled by every `process_block` (read with `aux_output`).
    aux_bufs: Vec<[Vec<f32>; 2]>,
    aux_ptrs: Vec<[*mut f32; 2]>,
    processing_started: bool,
    active: bool,
    /// Host-implemented IEventList that the plugin reads MIDI input
    /// from each block. Refilled by `process_block`. Kept alive for
    /// the activation's lifetime so we don't pay COM-allocation cost
    /// per audio callback.
    event_list_owner: ComWrapper<HostEventList>,
    event_list_ptr: ComPtr<IEventList>,
    /// Host-implemented IParameterChanges, drained + refilled per
    /// block from `PluginEvents::params`.
    param_changes_owner: ComWrapper<HostParameterChanges>,
    param_changes_ptr: ComPtr<IParameterChanges>,
    /// Sample-rate / tempo / transport state passed to the plugin
    /// each block. Required (non-null) by many instruments — without
    /// it MT-PowerDrumKit etc. dereference null and segfault inside
    /// their own process().
    process_context: ProcessContext,
}

/// Host-side IEventList implementation. The plugin queries
/// `getEventCount` / `getEvent` once per process() call; we refill
/// `events` from the host's MIDI stream just before invoking the
/// plugin so plugin reads see a fresh snapshot.
struct HostEventList {
    events: RefCell<Vec<Event>>,
    /// SysEx payloads. `DataEvent::bytes` is a borrowed `*const u8`
    /// the plugin reads during `process()`; we keep the owning Vec
    /// here so the pointer stays valid for the whole call. Refilled
    /// each block alongside `events`.
    sysex_bufs: RefCell<Vec<Vec<u8>>>,
}

impl HostEventList {
    fn new() -> Self {
        Self {
            events: RefCell::new(Vec::with_capacity(64)),
            sysex_bufs: RefCell::new(Vec::with_capacity(4)),
        }
    }
}

impl Class for HostEventList {
    type Interfaces = (IEventList,);
}

impl IEventListTrait for HostEventList {
    unsafe fn getEventCount(&self) -> int32 {
        self.events.borrow().len() as int32
    }

    unsafe fn getEvent(&self, index: int32, e: *mut Event) -> tresult {
        if e.is_null() || index < 0 {
            return kInvalidArgument;
        }
        let events = self.events.borrow();
        let Some(src) = events.get(index as usize) else {
            return kInvalidArgument;
        };
        unsafe { *e = *src };
        kResultOk
    }

    unsafe fn addEvent(&self, e: *mut Event) -> tresult {
        // We accept output events from the plugin (rare for instruments)
        // but don't currently forward them to the host's MIDI stream.
        if e.is_null() {
            return kInvalidArgument;
        }
        self.events.borrow_mut().push(unsafe { *e });
        kResultOk
    }
}

// ── HostParamValueQueue / HostParameterChanges ───────────────────────
//
// VST3 routes parameter automation through `IParameterChanges`, which
// owns a set of `IParamValueQueue`s (one per parameter). Each queue is
// a sorted list of (sample_offset, normalized_value) points spanning
// the current block. The plugin walks the changes at process() time
// and applies sample-accurate ramps internally.
//
// Our host build accepts (param_id, plain_value) pairs at sample 0
// from the renderer and emits one point per parameter. Multi-point
// automation is a thin extension of this code (push more points into
// the queue from per-sample envelope evaluation).

struct HostParamValueQueue {
    id: RefCell<ParamID>,
    /// (sampleOffset, normalizedValue) pairs.
    points: RefCell<Vec<(int32, ParamValue)>>,
}

impl HostParamValueQueue {
    fn new() -> Self {
        Self {
            id: RefCell::new(0),
            points: RefCell::new(Vec::with_capacity(4)),
        }
    }

    fn reset(&self, id: ParamID) {
        *self.id.borrow_mut() = id;
        self.points.borrow_mut().clear();
    }
}

impl Class for HostParamValueQueue {
    type Interfaces = (IParamValueQueue,);
}

impl IParamValueQueueTrait for HostParamValueQueue {
    unsafe fn getParameterId(&self) -> ParamID {
        *self.id.borrow()
    }

    unsafe fn getPointCount(&self) -> int32 {
        self.points.borrow().len() as int32
    }

    unsafe fn getPoint(
        &self,
        index: int32,
        sample_offset: *mut int32,
        value: *mut ParamValue,
    ) -> tresult {
        if index < 0 || sample_offset.is_null() || value.is_null() {
            return kInvalidArgument;
        }
        let points = self.points.borrow();
        let Some(&(off, val)) = points.get(index as usize) else {
            return kInvalidArgument;
        };
        unsafe {
            *sample_offset = off;
            *value = val;
        }
        kResultOk
    }

    unsafe fn addPoint(
        &self,
        sample_offset: int32,
        value: ParamValue,
        index: *mut int32,
    ) -> tresult {
        let mut points = self.points.borrow_mut();
        // Insert sorted; VST3 spec says points are ordered by offset.
        let pos = points
            .iter()
            .position(|&(o, _)| o > sample_offset)
            .unwrap_or(points.len());
        points.insert(pos, (sample_offset, value));
        if !index.is_null() {
            unsafe { *index = pos as int32 };
        }
        kResultOk
    }
}

struct HostParameterChanges {
    /// One queue per parameter touched this block. Reused across
    /// blocks: `reset()` clears the queues; `push()` appends a new
    /// one or returns an existing queue for the requested id.
    queues: RefCell<Vec<ComWrapper<HostParamValueQueue>>>,
    /// Active queue count this block (≤ queues.len(); the rest are
    /// kept around so we don't realloc when a future block re-uses
    /// them).
    used: RefCell<usize>,
}

impl HostParameterChanges {
    fn new() -> Self {
        Self {
            queues: RefCell::new(Vec::with_capacity(16)),
            used: RefCell::new(0),
        }
    }

    /// Discard the previous block's contents. Keeps the underlying
    /// queue allocations alive for reuse.
    fn reset(&self) {
        *self.used.borrow_mut() = 0;
    }

    /// Append a `(param_id, normalized_value)` change at sample 0.
    fn push_point(&self, id: ParamID, normalized_value: ParamValue) {
        let mut used = self.used.borrow_mut();
        let mut queues = self.queues.borrow_mut();
        // Reuse an existing queue (next slot) or grow.
        if *used >= queues.len() {
            queues.push(ComWrapper::new(HostParamValueQueue::new()));
        }
        let q = &queues[*used];
        q.reset(id);
        // SAFETY: COM call into our own implementation; the queue
        // lives as long as `queues` does.
        unsafe {
            // ignore return; addPoint never fails in our impl
            let mut _index: int32 = 0;
            let _ = (**q).addPoint(0, normalized_value, &mut _index);
        }
        *used += 1;
    }
}

impl Class for HostParameterChanges {
    type Interfaces = (IParameterChanges,);
}

impl IParameterChangesTrait for HostParameterChanges {
    unsafe fn getParameterCount(&self) -> int32 {
        *self.used.borrow() as int32
    }

    unsafe fn getParameterData(&self, index: int32) -> *mut IParamValueQueue {
        if index < 0 {
            return ptr::null_mut();
        }
        let used = *self.used.borrow();
        if (index as usize) >= used {
            return ptr::null_mut();
        }
        let queues = self.queues.borrow();
        match queues[index as usize].to_com_ptr::<IParamValueQueue>() {
            Some(p) => p.as_ptr(),
            None => ptr::null_mut(),
        }
    }

    unsafe fn addParameterData(
        &self,
        id: *const ParamID,
        index: *mut int32,
    ) -> *mut IParamValueQueue {
        if id.is_null() {
            return ptr::null_mut();
        }
        let id = unsafe { *id };
        let mut used = self.used.borrow_mut();
        let mut queues = self.queues.borrow_mut();
        if *used >= queues.len() {
            queues.push(ComWrapper::new(HostParamValueQueue::new()));
        }
        let q = &queues[*used];
        q.reset(id);
        let slot = *used as int32;
        *used += 1;
        if !index.is_null() {
            unsafe { *index = slot };
        }
        match q.to_com_ptr::<IParamValueQueue>() {
            Some(p) => p.as_ptr(),
            None => ptr::null_mut(),
        }
    }
}

// ── HostComponentHandler ─────────────────────────────────────────────
//
// Minimal IComponentHandler: the controller calls this when the user
// edits a parameter from its own UI (`beginEdit` / `performEdit` /
// `endEdit`) or when the plugin needs to be reconfigured
// (`restartComponent`). The skeleton just acknowledges — proper
// integration with the renderer's automation system lands when we
// wire the controller's edits back into envelope writes.

struct HostComponentHandler;

impl Class for HostComponentHandler {
    type Interfaces = (IComponentHandler,);
}

impl IComponentHandlerTrait for HostComponentHandler {
    unsafe fn beginEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }
    unsafe fn performEdit(&self, _id: ParamID, _value: ParamValue) -> tresult {
        kResultOk
    }
    unsafe fn endEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }
    unsafe fn restartComponent(&self, _flags: int32) -> tresult {
        kResultOk
    }
}

// ── HostApplication ──────────────────────────────────────────────────
//
// Passed as the host-context (`*mut FUnknown`) into
// `IPluginBase::initialize`. Some plugins (notably JUCE-based ones)
// hard-require a non-null host application or refuse to initialize.
// We provide a name and a minimal createInstance that returns
// kNoInterface — plugins that ask for IMessage / IAttributeList don't
// crash; they fall back to non-message paths.

struct HostApplication;

impl Class for HostApplication {
    type Interfaces = (IHostApplication,);
}

impl IHostApplicationTrait for HostApplication {
    unsafe fn getName(&self, name: *mut String128) -> tresult {
        if name.is_null() {
            return kInvalidArgument;
        }
        let bytes: Vec<u16> = "daw-standalone"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            let dst = (*name).as_mut_ptr();
            let n = bytes.len().min(128);
            for i in 0..n {
                *dst.add(i) = bytes[i];
            }
        }
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        _cid: *mut TUID,
        _iid: *mut TUID,
        _obj: *mut *mut std::ffi::c_void,
    ) -> tresult {
        kNoInterface
    }
}

// ── MemoryStream (IBStream) ─────────────────────────────────────────
//
// Backs `component.getState(stream)` → `controller.setComponentState(
// stream)`. Required by two-component plugins so the controller has
// the same parameter defaults as the audio component when it boots.

struct MemoryStream {
    buf: RefCell<Vec<u8>>,
    pos: RefCell<usize>,
}

impl MemoryStream {
    fn new() -> Self {
        Self {
            buf: RefCell::new(Vec::new()),
            pos: RefCell::new(0),
        }
    }
}

impl Class for MemoryStream {
    type Interfaces = (IBStream,);
}

impl IBStreamTrait for MemoryStream {
    unsafe fn read(
        &self,
        buffer: *mut std::ffi::c_void,
        num_bytes: int32,
        num_bytes_read: *mut int32,
    ) -> tresult {
        if buffer.is_null() || num_bytes < 0 {
            return kInvalidArgument;
        }
        let buf = self.buf.borrow();
        let mut pos = self.pos.borrow_mut();
        let want = num_bytes as usize;
        let have = buf.len().saturating_sub(*pos);
        let n = want.min(have);
        if n > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(buf.as_ptr().add(*pos), buffer as *mut u8, n);
            }
            *pos += n;
        }
        if !num_bytes_read.is_null() {
            unsafe { *num_bytes_read = n as int32 };
        }
        kResultOk
    }

    unsafe fn write(
        &self,
        buffer: *mut std::ffi::c_void,
        num_bytes: int32,
        num_bytes_written: *mut int32,
    ) -> tresult {
        if buffer.is_null() || num_bytes < 0 {
            return kInvalidArgument;
        }
        let n = num_bytes as usize;
        let mut buf = self.buf.borrow_mut();
        let mut pos = self.pos.borrow_mut();
        if *pos + n > buf.len() {
            buf.resize(*pos + n, 0);
        }
        unsafe {
            std::ptr::copy_nonoverlapping(buffer as *const u8, buf.as_mut_ptr().add(*pos), n);
        }
        *pos += n;
        if !num_bytes_written.is_null() {
            unsafe { *num_bytes_written = n as int32 };
        }
        kResultOk
    }

    unsafe fn seek(&self, target: int64, mode: int32, result: *mut int64) -> tresult {
        let mut pos = self.pos.borrow_mut();
        let buf_len = self.buf.borrow().len() as i64;
        let new_pos: i64 = match mode {
            m if m == IStreamSeekMode_::kIBSeekSet as i32 => target,
            m if m == IStreamSeekMode_::kIBSeekCur as i32 => *pos as i64 + target,
            m if m == IStreamSeekMode_::kIBSeekEnd as i32 => buf_len + target,
            _ => return kInvalidArgument,
        };
        if new_pos < 0 {
            return kInvalidArgument;
        }
        *pos = new_pos as usize;
        if !result.is_null() {
            unsafe { *result = new_pos };
        }
        kResultOk
    }

    unsafe fn tell(&self, pos: *mut int64) -> tresult {
        if pos.is_null() {
            return kInvalidArgument;
        }
        unsafe { *pos = *self.pos.borrow() as int64 };
        kResultOk
    }
}

impl LoadedVst3Plugin {
    pub fn descriptor(&self) -> &Vst3PluginDescriptor {
        &self.descriptor
    }

    /// `IPluginBase::initialize` the component (and a separate
    /// controller), wire the controller up, and sync its state.
    /// Idempotent; undone by [`Self::terminate`].
    fn ensure_initialized(&mut self) -> Result<(), Vst3HostError> {
        if self.initialized {
            return Ok(());
        }
        unsafe {
            // 1. IPluginBase::initialize on the component, with our
            // HostApplication as the context. Some plugins (notably
            // JUCE-based ones) require a non-null context with a
            // valid IHostApplication. Cast our ComWrapper to
            // *mut FUnknown — the plugin queryInterfaces upward.
            let host_ctx: *mut FUnknown = self
                ._host_app
                .to_com_ptr::<FUnknown>()
                .map(|p| p.as_ptr())
                .unwrap_or(ptr::null_mut());
            let res = self.component.initialize(host_ctx);
            if res != kResultOk && res != kResultTrue {
                return Err(Vst3HostError::Initialize);
            }
            if let Some(c) = self.controller.as_mut() {
                if c.separate {
                    let res = c.ptr.initialize(host_ctx);
                    if res == kResultOk || res == kResultTrue {
                        c.initialized = true;
                    }
                }
                // Install the host component handler so the
                // controller can route user edits / restart requests
                // somewhere instead of choking on a null handler.
                if let Some(handler_ptr) = self._component_handler.to_com_ptr::<IComponentHandler>()
                {
                    let _ = c.ptr.setComponentHandler(handler_ptr.as_ptr());
                }
                // Connect the controller and component via
                // IConnectionPoint (two-component plugins use this
                // to exchange state messages). Best-effort — many
                // single-component plugins return errors here.
                if c.separate {
                    if let (Some(comp_cp), Some(ctrl_cp)) = (
                        self.component.cast::<IConnectionPoint>(),
                        c.ptr.cast::<IConnectionPoint>(),
                    ) {
                        let _ = comp_cp.connect(ctrl_cp.as_ptr());
                        let _ = ctrl_cp.connect(comp_cp.as_ptr());
                    }
                    // Sync component state → controller so the
                    // controller's parameter defaults match the
                    // component's runtime state. Critical for many
                    // synths (MT-PowerDrumKit etc.) to come up in a
                    // valid configuration.
                    let stream = ComWrapper::new(MemoryStream::new());
                    if let Some(stream_ptr) = stream.to_com_ptr::<IBStream>() {
                        let raw = stream_ptr.as_ptr();
                        let res = self.component.getState(raw);
                        if res == kResultOk || res == kResultTrue {
                            // Rewind to read from the top.
                            let mut _out: int64 = 0;
                            let _ = stream_ptr.seek(
                                0,
                                IStreamSeekMode_::kIBSeekSet as int32,
                                &mut _out,
                            );
                            let _ = c.ptr.setComponentState(raw);
                        }
                    }
                }
            }
        }
        self.initialized = true;
        Ok(())
    }

    /// Activate the plugin: initialize → setupProcessing →
    /// setActive(true) → setProcessing(true). Subsequent
    /// `process_block` calls reuse the activated state.
    pub fn prepare(&mut self, sample_rate: f64, block_size: u32) -> Result<(), Vst3HostError> {
        if self.activation.is_some() {
            self.deactivate();
        }

        self.ensure_initialized()?;

        unsafe {
            // 2. setupProcessing.
            let mut setup = ProcessSetup {
                processMode: ProcessModes_::kRealtime as i32,
                symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
                maxSamplesPerBlock: block_size as i32,
                sampleRate: sample_rate,
            };
            let res = self.processor.setupProcessing(&mut setup);
            if res != kResultOk && res != kResultTrue {
                self.terminate();
                return Err(Vst3HostError::SetupProcessing);
            }

            // 3. Activate audio bus 0 in each direction *if it exists*.
            // Instruments (e.g. MT-PowerDrumKit) report 0 input audio
            // buses — activating + then passing a non-null inputs
            // pointer would crash inside process().
            let in_count = self
                .component
                .getBusCount(MediaTypes_::kAudio as i32, BusDirections_::kInput as i32);
            let out_count = self
                .component
                .getBusCount(MediaTypes_::kAudio as i32, BusDirections_::kOutput as i32);
            let has_input_bus = in_count > 0;
            let has_output_bus = out_count > 0;
            if has_input_bus {
                let _ = self.component.activateBus(
                    MediaTypes_::kAudio as i32,
                    BusDirections_::kInput as i32,
                    0,
                    1,
                );
            }
            // Bus 0, plus any aux buses asked for (multi-out
            // instruments: Kontakt routes each multi slot to a pair).
            let out_buses = if has_output_bus {
                self.output_buses.clamp(1, out_count.max(1) as u32)
            } else {
                0
            };
            for bus in 0..out_buses {
                let _ = self.component.activateBus(
                    MediaTypes_::kAudio as i32,
                    BusDirections_::kOutput as i32,
                    bus as i32,
                    1,
                );
            }

            // 3a. Declare the bus channel arrangement. JUCE-based
            // plugins (MT-PowerDrumKit, Atlas, …) refuse to run
            // process() until they've been told stereo input/output
            // via setBusArrangements — the default arrangement is
            // implementation-defined and they often skip allocating
            // their internal channel scratch otherwise.
            let mut in_arr: SpeakerArrangement = SpeakerArr::kStereo;
            let mut out_arr: Vec<SpeakerArrangement> =
                vec![SpeakerArr::kStereo; out_buses.max(1) as usize];
            let in_ptr: *mut SpeakerArrangement = if has_input_bus {
                &mut in_arr
            } else {
                ptr::null_mut()
            };
            let out_ptr: *mut SpeakerArrangement = if has_output_bus {
                out_arr.as_mut_ptr()
            } else {
                ptr::null_mut()
            };
            let in_count: int32 = if has_input_bus { 1 } else { 0 };
            let out_count: int32 = out_buses as int32;
            let _ = self
                .processor
                .setBusArrangements(in_ptr, in_count, out_ptr, out_count);

            // 4. setActive(true).
            let res = self.component.setActive(1);
            if res != kResultOk && res != kResultTrue {
                self.terminate();
                return Err(Vst3HostError::Activate);
            }

            // 5. setProcessing(true).
            let res = self.processor.setProcessing(1);
            if res != kResultOk && res != kResultTrue && res != kNotImplemented {
                let _ = self.component.setActive(0);
                self.terminate();
                return Err(Vst3HostError::StartProcessing);
            }
        }

        let scratch_l = vec![0.0f32; block_size as usize];
        let scratch_r = vec![0.0f32; block_size as usize];
        // Borrow-checker dance: the unsafe block above can't easily
        // return the bus flags through the `?`/early-return paths.
        // Re-query here (cheap; just a vtable call).
        let (has_input_bus, has_output_bus) = unsafe {
            (
                self.component
                    .getBusCount(MediaTypes_::kAudio as i32, BusDirections_::kInput as i32)
                    > 0,
                self.component
                    .getBusCount(MediaTypes_::kAudio as i32, BusDirections_::kOutput as i32)
                    > 0,
            )
        };

        // Pre-build the host-side MIDI event list + parameter
        // changes objects. We hold both the owner (ComWrapper) and a
        // typed ComPtr so process_block doesn't re-query per block.
        let event_list_owner = ComWrapper::new(HostEventList::new());
        let event_list_ptr = event_list_owner
            .to_com_ptr::<IEventList>()
            .ok_or(Vst3HostError::Initialize)?;
        let param_changes_owner = ComWrapper::new(HostParameterChanges::new());
        let param_changes_ptr = param_changes_owner
            .to_com_ptr::<IParameterChanges>()
            .ok_or(Vst3HostError::Initialize)?;

        // Sensible ProcessContext defaults. Mark tempo + time-sig
        // valid so plugins that read those (drum kits keyed to BPM,
        // tempo-sync'd modulators) get usable values. `state` carries
        // both validity flags and playing/recording bits — we leave
        // playing off; tests render offline blocks.
        let process_context = ProcessContext {
            state: (ProcessContextFlags_::kTempoValid
                | ProcessContextFlags_::kTimeSigValid
                | ProcessContextFlags_::kProjectTimeMusicValid),
            sampleRate: sample_rate,
            projectTimeSamples: 0,
            systemTime: 0,
            continousTimeSamples: 0,
            projectTimeMusic: 0.0,
            barPositionMusic: 0.0,
            cycleStartMusic: 0.0,
            cycleEndMusic: 0.0,
            tempo: 120.0,
            timeSigNumerator: 4,
            timeSigDenominator: 4,
            chord: unsafe { std::mem::zeroed() },
            smpteOffsetSubframes: 0,
            frameRate: unsafe { std::mem::zeroed() },
            samplesToNextClock: 0,
        };

        let aux_count = if has_output_bus {
            // Main + aux buses actually activated above.
            let n = unsafe {
                self.component
                    .getBusCount(MediaTypes_::kAudio as i32, BusDirections_::kOutput as i32)
            };
            self.output_buses.clamp(1, n.max(1) as u32)
        } else {
            1
        };
        self.activation = Some(ActivationGuard {
            sample_rate,
            block_size,
            has_input_bus,
            has_output_bus,
            scratch_l,
            scratch_r,
            in_ptrs: [ptr::null_mut(); 2],
            out_ptrs: [ptr::null_mut(); 2],
            aux_bufs: (1..aux_count)
                .map(|_| [vec![0.0; block_size as usize], vec![0.0; block_size as usize]])
                .collect(),
            aux_ptrs: vec![[ptr::null_mut(); 2]; aux_count.saturating_sub(1) as usize],
            processing_started: true,
            active: true,
            event_list_owner,
            event_list_ptr,
            param_changes_owner,
            param_changes_ptr,
            process_context,
        });
        Ok(())
    }

    pub fn is_prepared(&self) -> bool {
        self.activation.is_some()
    }

    pub fn sample_rate(&self) -> Option<f64> {
        self.activation.as_ref().map(|a| a.sample_rate)
    }

    pub fn block_size(&self) -> Option<u32> {
        self.activation.as_ref().map(|a| a.block_size)
    }

    /// Run `n` stereo output buses (main + `n − 1` aux) from the next
    /// `prepare`: a multi-out instrument routes its parts to them. Read an
    /// aux bus after each `process_block` with [`Self::aux_output`].
    pub fn set_output_buses(&mut self, n: u32) {
        self.output_buses = n.max(1);
    }

    /// The last block's samples on output bus `bus` (≥ 1), `frames` long.
    #[must_use]
    pub fn aux_output(&self, bus: usize, frames: usize) -> Option<(&[f32], &[f32])> {
        let b = self.activation.as_ref()?.aux_bufs.get(bus.checked_sub(1)?)?;
        let n = frames.min(b[0].len());
        Some((&b[0][..n], &b[1][..n]))
    }

    /// Set the tempo the plugin sees (the process context's, for
    /// tempo-synced LFOs, envelopes and delays). No-op before activation.
    pub fn set_tempo(&mut self, bpm: f64) {
        if let Some(act) = self.activation.as_mut() {
            act.process_context.tempo = bpm.max(1.0);
        }
    }

    /// Process one block of stereo audio. `events.midi` is delivered
    /// to the plugin via a host-implemented `IEventList` (required
    /// for VST3i instrument plugins to make sound). `events.params`
    /// is not yet plumbed — `IParameterChanges` lands in a follow-up.
    pub fn process_block(
        &mut self,
        in_l: &[f32],
        in_r: &[f32],
        out_l: &mut [f32],
        out_r: &mut [f32],
        events: &PluginEvents<'_>,
    ) -> Result<(), Vst3HostError> {
        let Some(act) = self.activation.as_mut() else {
            return Err(Vst3HostError::NotActivated);
        };

        // Refill the host event list. Note on/off + poly pressure
        // are first-class VST3 Events; CC / pitch-bend / program-
        // change / channel-aftertouch route through IMidiMapping
        // into IParameterChanges below. SysEx becomes a kDataEvent
        // pointing into a per-block buffer we own. Per-note
        // expressions become kNoteExpressionValueEvent.
        {
            let mut buf = act.event_list_owner.events.borrow_mut();
            let mut bufs = act.event_list_owner.sysex_bufs.borrow_mut();
            buf.clear();
            bufs.clear();
            for ev in events.midi {
                if let daw_proto::MidiEvent::SysEx(ref data) = ev.message {
                    // Park the bytes in a stable slot and emit a
                    // DataEvent referencing them. The Vec inside
                    // `bufs` must not move — pre-reserved at
                    // activation, but if we exceed capacity it will
                    // realloc and invalidate pointers; the bufs
                    // capacity is grown ahead of pushes here.
                    if bufs.len() == bufs.capacity() {
                        bufs.reserve(8);
                    }
                    bufs.push(data.clone());
                    let owned = bufs.last().unwrap();
                    buf.push(Event {
                        busIndex: 0,
                        sampleOffset: ev.offset as i32,
                        ppqPosition: 0.0,
                        flags: 0,
                        r#type: EventTypes_::kDataEvent as u16,
                        __field0: Event__type0 {
                            data: DataEvent {
                                size: owned.len() as u32,
                                r#type: Vst3DataTypes_::kMidiSysEx,
                                bytes: owned.as_ptr(),
                            },
                        },
                    });
                    continue;
                }
                if let Some(vst_event) = midi_to_vst3_event(ev) {
                    buf.push(vst_event);
                }
            }
            for ne in events.note_expressions {
                if let Some(typ) = note_expression_dim_to_vst3(ne.dimension) {
                    buf.push(Event {
                        busIndex: 0,
                        sampleOffset: ne.offset as i32,
                        ppqPosition: 0.0,
                        flags: 0,
                        r#type: EventTypes_::kNoteExpressionValueEvent as u16,
                        __field0: Event__type0 {
                            noteExpressionValue: NoteExpressionValueEvent {
                                typeId: typ as NoteExpressionTypeID,
                                noteId: -1,
                                value: ne.value as NoteExpressionValue,
                            },
                        },
                    });
                }
            }
        }

        // Refill the parameter-changes queue with (1) explicit
        // param events from the renderer and (2) the IMidiMapping
        // translation of MIDI CCs / pitch-bend / aftertouch /
        // program change. VST3 routes these as parameter automation
        // — the canonical pattern is documented in the VST3 SDK.
        act.param_changes_owner.reset();
        if let Some(c) = self.controller.as_ref() {
            for &(id, plain) in events.params {
                let normalized = unsafe { c.ptr.plainParamToNormalized(id, plain) };
                act.param_changes_owner.push_point(id, normalized);
            }
            if let Some(mm) = c.midi_mapping.as_ref() {
                for ev in events.midi {
                    if let Some((ctrl_num, channel, normalized)) =
                        midi_to_ctrl_assignment(&ev.message)
                    {
                        let mut param_id: ParamID = 0;
                        let res = unsafe {
                            mm.getMidiControllerAssignment(
                                0,
                                channel as i16,
                                ctrl_num,
                                &mut param_id,
                            )
                        };
                        if res == kResultOk || res == kResultTrue {
                            act.param_changes_owner.push_point(param_id, normalized);
                        }
                    }
                }
            }
        }
        let frames = in_l.len().min(in_r.len()).min(out_l.len()).min(out_r.len());
        if frames > act.block_size as usize {
            return Err(Vst3HostError::BlockTooLarge);
        }

        // Copy inputs into mutable scratch (VST3 takes non-const
        // pointers; plugins are expected to treat input as read-only).
        act.scratch_l[..frames].copy_from_slice(&in_l[..frames]);
        act.scratch_r[..frames].copy_from_slice(&in_r[..frames]);

        act.in_ptrs[0] = act.scratch_l.as_mut_ptr();
        act.in_ptrs[1] = act.scratch_r.as_mut_ptr();
        act.out_ptrs[0] = out_l.as_mut_ptr();
        act.out_ptrs[1] = out_r.as_mut_ptr();

        let mut in_bus = AudioBusBuffers {
            numChannels: 2,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: act.in_ptrs.as_mut_ptr(),
            },
        };
        let out_bus = AudioBusBuffers {
            numChannels: 2,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: act.out_ptrs.as_mut_ptr(),
            },
        };

        // Effects have an input bus, instruments don't. Pass a null
        // `inputs` + `numInputs=0` for the latter — otherwise the
        // plugin will deref a non-existent input bus.
        let (inputs_ptr, num_inputs): (*mut AudioBusBuffers, i32) = if act.has_input_bus {
            (&mut in_bus, 1)
        } else {
            (ptr::null_mut(), 0)
        };
        // Main bus first, then the aux buses into their own buffers.
        let mut out_buses = vec![out_bus];
        for (bufs, ptrs) in act.aux_bufs.iter_mut().zip(act.aux_ptrs.iter_mut()) {
            ptrs[0] = bufs[0].as_mut_ptr();
            ptrs[1] = bufs[1].as_mut_ptr();
            out_buses.push(AudioBusBuffers {
                numChannels: 2,
                silenceFlags: 0,
                __field0: AudioBusBuffers__type0 {
                    channelBuffers32: ptrs.as_mut_ptr(),
                },
            });
        }
        let (outputs_ptr, num_outputs): (*mut AudioBusBuffers, i32) = if act.has_output_bus {
            (out_buses.as_mut_ptr(), out_buses.len() as i32)
        } else {
            (ptr::null_mut(), 0)
        };

        let mut data = ProcessData {
            processMode: ProcessModes_::kRealtime as i32,
            symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
            numSamples: frames as i32,
            numInputs: num_inputs,
            numOutputs: num_outputs,
            inputs: inputs_ptr,
            outputs: outputs_ptr,
            inputParameterChanges: act.param_changes_ptr.as_ptr(),
            outputParameterChanges: ptr::null_mut(),
            inputEvents: act.event_list_ptr.as_ptr(),
            outputEvents: ptr::null_mut(),
            processContext: &mut act.process_context,
        };

        // If the plugin has no output bus, zero the caller's buffers
        // (silence) so they don't see uninitialized scratch noise.
        if !act.has_output_bus {
            out_l[..frames].fill(0.0);
            out_r[..frames].fill(0.0);
        }

        let res = unsafe { self.processor.process(&mut data) };
        if res != kResultOk && res != kResultTrue {
            return Err(Vst3HostError::Process);
        }
        // Advance transport for the next block so tempo-sync'd
        // plugins see a monotonically increasing clock.
        act.process_context.projectTimeSamples += frames as i64;
        act.process_context.continousTimeSamples += frames as i64;
        act.process_context.projectTimeMusic +=
            frames as f64 * act.process_context.tempo / (60.0 * act.sample_rate);
        Ok(())
    }

    /// Save plugin state to a daw-internal byte blob. Format:
    ///
    /// ```text
    /// "DAW3"           — 4-byte magic (host format marker)
    /// u32 LE           — component state length
    /// N bytes          — component state stream
    /// u32 LE           — controller state length
    /// M bytes          — controller state stream
    /// ```
    ///
    /// Same blob round-trips through [`Self::load_state`].
    pub fn save_state(&mut self) -> Result<Vec<u8>, Vst3HostError> {
        self.ensure_initialized()?;
        let comp_state = self.read_state_from(StateOwner::Component)?;
        let ctrl_state = self
            .controller
            .as_ref()
            .map(|_| self.read_state_from(StateOwner::Controller))
            .transpose()?
            .unwrap_or_default();
        let mut out = Vec::with_capacity(8 + comp_state.len() + ctrl_state.len());
        out.extend_from_slice(b"DAW3");
        out.extend_from_slice(&(comp_state.len() as u32).to_le_bytes());
        out.extend_from_slice(&comp_state);
        out.extend_from_slice(&(ctrl_state.len() as u32).to_le_bytes());
        out.extend_from_slice(&ctrl_state);
        Ok(out)
    }

    /// Restore plugin state from a blob produced by
    /// [`Self::save_state`]. To restore from a REAPER-format chunk,
    /// run it through [`crate::rpp_state::reaper_vst3_to_daw_state`]
    /// first.
    pub fn load_state(&mut self, state: &[u8]) -> Result<(), Vst3HostError> {
        // Parse the wrapper.
        if state.len() < 8 || &state[..4] != b"DAW3" {
            return Err(Vst3HostError::BadStateBlob);
        }
        let comp_len = u32::from_le_bytes(state[4..8].try_into().unwrap()) as usize;
        if state.len() < 8 + comp_len + 4 {
            return Err(Vst3HostError::BadStateBlob);
        }
        let comp_state = &state[8..8 + comp_len];
        let ctrl_len_off = 8 + comp_len;
        let ctrl_len = u32::from_le_bytes(
            state[ctrl_len_off..ctrl_len_off + 4]
                .try_into()
                .map_err(|_| Vst3HostError::BadStateBlob)?,
        ) as usize;
        if state.len() < ctrl_len_off + 4 + ctrl_len {
            return Err(Vst3HostError::BadStateBlob);
        }
        let ctrl_state = &state[ctrl_len_off + 4..ctrl_len_off + 4 + ctrl_len];
        self.ensure_initialized()?;

        // Apply to the audio component (IComponent::setState).
        unsafe {
            let stream = ComWrapper::new(MemoryStream::new());
            stream.buf.borrow_mut().extend_from_slice(comp_state);
            if let Some(stream_ptr) = stream.to_com_ptr::<IBStream>() {
                let raw = stream_ptr.as_ptr();
                let res = self.component.setState(raw);
                if res != kResultOk && res != kResultTrue {
                    return Err(Vst3HostError::SetState);
                }
                // Also push the component state through the
                // controller so its UI reflects the loaded values.
                if let Some(c) = self.controller.as_mut() {
                    let mut _o: int64 = 0;
                    let _ = stream_ptr.seek(0, IStreamSeekMode_::kIBSeekSet as int32, &mut _o);
                    let _ = c.ptr.setComponentState(raw);
                }
            }
        }
        // Apply controller-only state (parameter values, UI prefs).
        if !ctrl_state.is_empty()
            && let Some(c) = self.controller.as_mut()
        {
            unsafe {
                let stream = ComWrapper::new(MemoryStream::new());
                stream.buf.borrow_mut().extend_from_slice(ctrl_state);
                if let Some(stream_ptr) = stream.to_com_ptr::<IBStream>() {
                    let _ = c.ptr.setState(stream_ptr.as_ptr());
                }
            }
        }
        Ok(())
    }

    fn read_state_from(&self, owner: StateOwner) -> Result<Vec<u8>, Vst3HostError> {
        let stream = ComWrapper::new(MemoryStream::new());
        let stream_ptr = stream
            .to_com_ptr::<IBStream>()
            .ok_or(Vst3HostError::SaveState)?;
        let raw = stream_ptr.as_ptr();
        unsafe {
            let res = match owner {
                StateOwner::Component => self.component.getState(raw),
                StateOwner::Controller => self
                    .controller
                    .as_ref()
                    .map(|c| c.ptr.getState(raw))
                    .unwrap_or(kNotImplemented),
            };
            // kNotImplemented + kResultFalse both mean "no state to
            // persist" (utility plugins, controllers that defer to
            // the component). Treat as empty rather than failing.
            if res != kResultOk
                && res != kResultTrue
                && res != kResultFalse
                && res != kNotImplemented
            {
                return Err(Vst3HostError::SaveState);
            }
        }
        Ok(stream.buf.borrow().clone())
    }

    /// Reverse of [`prepare`]: setProcessing(false) → setActive(false)
    /// → terminate(). Idempotent.
    pub fn deactivate(&mut self) {
        let Some(mut act) = self.activation.take() else {
            return;
        };
        unsafe {
            if act.processing_started {
                let _ = self.processor.setProcessing(0);
                act.processing_started = false;
            }
            if act.active {
                let _ = self.component.setActive(0);
                act.active = false;
            }
        }
        self.terminate();
    }

    /// Reverse of [`Self::ensure_initialized`]: detach and disconnect the
    /// controller, then `terminate()` both halves. Idempotent.
    fn terminate(&mut self) {
        if !self.initialized {
            return;
        }
        unsafe {
            if let Some(c) = self.controller.as_mut() {
                // Detach the host-installed component handler
                // before terminating so the controller doesn't
                // call back into our wrapper after Drop.
                let _ = c.ptr.setComponentHandler(ptr::null_mut());
                // Disconnect the connection points (best-effort).
                if c.separate {
                    if let (Some(comp_cp), Some(ctrl_cp)) = (
                        self.component.cast::<IConnectionPoint>(),
                        c.ptr.cast::<IConnectionPoint>(),
                    ) {
                        let _ = comp_cp.disconnect(ctrl_cp.as_ptr());
                        let _ = ctrl_cp.disconnect(comp_cp.as_ptr());
                    }
                    if c.initialized {
                        let _ = c.ptr.terminate();
                        c.initialized = false;
                    }
                }
            }
            let _ = self.component.terminate();
        }
        self.initialized = false;
    }

    // ── Parameter access (via IEditController) ─────────────────────

    /// All parameters this plugin exposes. Empty vec if the plugin
    /// has no controller or `getParameterCount` returned 0.
    pub fn params(&mut self) -> Vec<Vst3ParamInfo> {
        if self.ensure_initialized().is_err() {
            return Vec::new();
        }
        let Some(c) = self.controller.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        unsafe {
            let count = c.ptr.getParameterCount();
            out.reserve(count.max(0) as usize);
            for i in 0..count {
                let mut info = ParameterInfo {
                    id: 0,
                    title: [0; 128],
                    shortTitle: [0; 128],
                    units: [0; 128],
                    stepCount: 0,
                    defaultNormalizedValue: 0.0,
                    unitId: 0,
                    flags: 0,
                };
                let res = c.ptr.getParameterInfo(i, &mut info);
                if res != kResultOk && res != kResultTrue {
                    continue;
                }
                let title = utf16_array_to_string(&info.title);
                let units = utf16_array_to_string(&info.units);
                // Convert the default from normalized (0..1) to plain
                // for parity with the CLAP host's reported defaults.
                let default_plain = c
                    .ptr
                    .normalizedParamToPlain(info.id, info.defaultNormalizedValue);
                // VST3 has no explicit min/max range — for non-stepped
                // params, min=plain(0.0), max=plain(1.0).
                let min_plain = c.ptr.normalizedParamToPlain(info.id, 0.0);
                let max_plain = c.ptr.normalizedParamToPlain(info.id, 1.0);
                out.push(Vst3ParamInfo {
                    id: info.id,
                    name: title,
                    units,
                    min: min_plain,
                    max: max_plain,
                    default: default_plain,
                    step_count: info.stepCount,
                    flags: info.flags,
                });
            }
        }
        out
    }

    /// Current plain (de-normalized) value of a parameter.
    pub fn param_value(&mut self, id: u32) -> Option<f64> {
        if self.ensure_initialized().is_err() {
            return None;
        }
        let c = self.controller.as_ref()?;
        unsafe {
            let n = c.ptr.getParamNormalized(id);
            Some(c.ptr.normalizedParamToPlain(id, n))
        }
    }

    /// Format a plain-value parameter as the plugin would display
    /// it (e.g. `"-12.0 dB"`).
    pub fn value_to_text(&mut self, id: u32, plain_value: f64) -> Option<String> {
        if self.ensure_initialized().is_err() {
            return None;
        }
        let c = self.controller.as_ref()?;
        unsafe {
            let normalized = c.ptr.plainParamToNormalized(id, plain_value);
            let mut buf = [0u16; 128];
            let res = c.ptr.getParamStringByValue(id, normalized, &mut buf);
            if res != kResultOk && res != kResultTrue {
                return None;
            }
            Some(utf16_array_to_string(&buf))
        }
    }

    /// Parse a display string back to a plain parameter value.
    pub fn text_to_value(&mut self, id: u32, text: &str) -> Option<f64> {
        if self.ensure_initialized().is_err() {
            return None;
        }
        let c = self.controller.as_ref()?;
        unsafe {
            let mut wide: Vec<u16> = text.encode_utf16().collect();
            wide.push(0);
            let mut normalized: f64 = 0.0;
            let res = c
                .ptr
                .getParamValueByString(id, wide.as_mut_ptr(), &mut normalized);
            if res != kResultOk && res != kResultTrue {
                return None;
            }
            Some(c.ptr.normalizedParamToPlain(id, normalized))
        }
    }
}

/// One VST3 parameter as the host sees it (plain-valued).
#[derive(Clone, Debug)]
pub struct Vst3ParamInfo {
    pub id: ParamID,
    pub name: String,
    pub units: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    /// `0` = continuous; `1` = toggle; `n>1` = `n+1`-position discrete.
    pub step_count: i32,
    /// Raw `ParameterFlags` bitmask (kCanAutomate, kIsBypass, etc).
    pub flags: i32,
}

impl Drop for LoadedVst3Plugin {
    fn drop(&mut self) {
        self.deactivate();
        // Initialized for a state/param call but never prepared.
        self.terminate();
    }
}

// ── Send wrapper + PluginInstance impl ───────────────────────────────
//
// VST3 COM smart pointers are `!Send` for the same reason as clack:
// the spec ties some methods to a specific thread. Our project
// serializes all access through the `Mutex<HashMap<..>>` that owns
// plugin instances, so wrapping with [`ThreadSerialized`] is sound.

pub use crate::plugin::ThreadSerialized;

pub type SendableVst3Plugin = ThreadSerialized<LoadedVst3Plugin>;

impl LoadedVst3Plugin {
    /// Wrap for storage in the cross-thread plugin map. The plugin
    /// must only ever be touched from one thread at a time afterwards.
    pub fn into_send(self) -> SendableVst3Plugin {
        // SAFETY: caller serializes access (see module docs).
        unsafe { ThreadSerialized::new(self) }
    }
}

impl crate::plugin::PluginInstance for SendableVst3Plugin {
    fn descriptor(&self) -> crate::plugin::PluginDescriptor {
        let d = &self.descriptor;
        crate::plugin::PluginDescriptor {
            id: tuid_to_hex(&d.cid),
            name: d.name.clone(),
            vendor: d.vendor.clone(),
            version: d.version.clone(),
            format: crate::plugin::PluginFormat::Vst3,
        }
    }
    fn params(&mut self) -> Vec<crate::plugin::PluginParamInfo> {
        LoadedVst3Plugin::params(self)
            .into_iter()
            .map(|p| crate::plugin::PluginParamInfo {
                id: p.id,
                name: p.name,
                min: p.min,
                max: p.max,
                default: p.default,
            })
            .collect()
    }
    fn param_value(&mut self, id: u32) -> Option<f64> {
        LoadedVst3Plugin::param_value(self, id)
    }
    fn value_to_text(&mut self, id: u32, v: f64) -> Option<String> {
        LoadedVst3Plugin::value_to_text(self, id, v)
    }
    fn text_to_value(&mut self, id: u32, t: &str) -> Option<f64> {
        LoadedVst3Plugin::text_to_value(self, id, t)
    }
    fn latency(&mut self) -> u32 {
        // SAFETY: COM call on a still-live IAudioProcessor.
        unsafe { self.processor.getLatencySamples() }
    }
    fn prepare(&mut self, sr: f64, bs: u32) -> Result<(), crate::plugin::PluginError> {
        LoadedVst3Plugin::prepare(self, sr, bs).map_err(map_err)
    }
    fn is_prepared(&self) -> bool {
        LoadedVst3Plugin::is_prepared(self)
    }
    fn process_block(
        &mut self,
        il: &[f32],
        ir: &[f32],
        ol: &mut [f32],
        or: &mut [f32],
        ev: &crate::plugin::PluginEvents<'_>,
    ) -> Result<(), crate::plugin::PluginError> {
        LoadedVst3Plugin::process_block(self, il, ir, ol, or, ev).map_err(map_err)
    }
    fn deactivate(&mut self) {
        LoadedVst3Plugin::deactivate(self)
    }
    fn load_state(&mut self, state: &[u8]) -> Result<(), crate::plugin::PluginError> {
        LoadedVst3Plugin::load_state(self, state).map_err(map_err)
    }
    fn save_state(&mut self) -> Result<Vec<u8>, crate::plugin::PluginError> {
        LoadedVst3Plugin::save_state(self).map_err(map_err)
    }
}

fn map_err(e: Vst3HostError) -> crate::plugin::PluginError {
    use crate::plugin::PluginError as P;
    match e {
        Vst3HostError::BundleLoad => P::LoadFailed("vst3 bundle load".into()),
        Vst3HostError::UnknownPlatform => P::LoadFailed("vst3 platform unknown".into()),
        Vst3HostError::BundleLayout => P::LoadFailed("vst3 bundle layout".into()),
        Vst3HostError::NoFactory => P::LoadFailed("vst3 GetPluginFactory missing".into()),
        Vst3HostError::ModuleEntry => P::LoadFailed("vst3 module entry returned false".into()),
        Vst3HostError::IndexOutOfRange => P::LoadFailed("vst3 plugin index out of range".into()),
        Vst3HostError::NoAudioProcessor => {
            P::LoadFailed("vst3 plugin lacks IAudioProcessor".into())
        }
        Vst3HostError::Instantiate => P::LoadFailed("vst3 createInstance failed".into()),
        Vst3HostError::Initialize => P::ActivateFailed("vst3 initialize failed".into()),
        Vst3HostError::SetupProcessing => P::ActivateFailed("vst3 setupProcessing failed".into()),
        Vst3HostError::Activate => P::ActivateFailed("vst3 setActive(true) failed".into()),
        Vst3HostError::StartProcessing => {
            P::ActivateFailed("vst3 setProcessing(true) failed".into())
        }
        Vst3HostError::NotActivated => P::NotActivated,
        Vst3HostError::BlockTooLarge => P::BlockTooLarge,
        Vst3HostError::Process => P::ProcessFailed("vst3 process failed".into()),
        Vst3HostError::BadStateBlob => P::LoadFailed("vst3 state blob malformed".into()),
        Vst3HostError::SetState => P::LoadFailed("vst3 setState failed".into()),
        Vst3HostError::SaveState => P::LoadFailed("vst3 getState failed".into()),
    }
}

#[derive(Copy, Clone)]
enum StateOwner {
    Component,
    Controller,
}

#[derive(Debug, thiserror::Error)]
pub enum Vst3HostError {
    #[error("failed to load .vst3 bundle")]
    BundleLoad,
    #[error("running platform not supported by this build")]
    UnknownPlatform,
    #[error("vst3 bundle layout malformed (no platform .so/.dylib/.dll)")]
    BundleLayout,
    #[error("bundle does not export GetPluginFactory")]
    NoFactory,
    #[error("module entry function returned false")]
    ModuleEntry,
    #[error("plugin index out of range")]
    IndexOutOfRange,
    #[error("plugin does not implement IAudioProcessor")]
    NoAudioProcessor,
    #[error("plugin createInstance failed")]
    Instantiate,
    #[error("IPluginBase::initialize failed")]
    Initialize,
    #[error("IAudioProcessor::setupProcessing failed")]
    SetupProcessing,
    #[error("IComponent::setActive(true) failed")]
    Activate,
    #[error("IAudioProcessor::setProcessing(true) failed")]
    StartProcessing,
    #[error("process() called before prepare()")]
    NotActivated,
    #[error("block exceeds prepared maxSamplesPerBlock")]
    BlockTooLarge,
    #[error("IAudioProcessor::process() returned error")]
    Process,
    #[error("plugin state blob is malformed or has wrong magic")]
    BadStateBlob,
    #[error("IComponent::setState / IEditController::setState failed")]
    SetState,
    #[error("IComponent::getState / IEditController::getState failed")]
    SaveState,
}

// ── Bundle layout + platform helpers ─────────────────────────────────

fn resolve_lib_path(bundle: &Path) -> Result<PathBuf, Vst3HostError> {
    // Single-file form (rare on Linux/macOS, common on Windows when
    // the .vst3 is just a renamed DLL): use the bundle path directly.
    if bundle.is_file() {
        return Ok(bundle.to_path_buf());
    }
    if !bundle.is_dir() {
        return Err(Vst3HostError::BundleLoad);
    }
    let contents = bundle.join("Contents");
    if !contents.is_dir() {
        return Err(Vst3HostError::BundleLayout);
    }
    // Bundle name (without trailing .vst3) — Steinberg's spec says
    // the library inside is named after the bundle stem.
    let stem = bundle
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or(Vst3HostError::BundleLayout)?;

    #[cfg(target_os = "linux")]
    let candidates: &[(&str, &str)] = &[
        ("x86_64-linux", "so"),
        ("aarch64-linux", "so"),
        ("armv7-linux", "so"),
        ("i386-linux", "so"),
    ];
    #[cfg(target_os = "macos")]
    let candidates: &[(&str, &str)] = &[("MacOS", "")];
    #[cfg(target_os = "windows")]
    let candidates: &[(&str, &str)] = &[
        ("x86_64-win", "vst3"),
        ("x86-win", "vst3"),
        ("aarch64-win", "vst3"),
    ];
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let candidates: &[(&str, &str)] = &[];

    if candidates.is_empty() {
        return Err(Vst3HostError::UnknownPlatform);
    }

    for (subdir, ext) in candidates {
        let dir = contents.join(subdir);
        if !dir.is_dir() {
            continue;
        }
        let path = if ext.is_empty() {
            dir.join(stem)
        } else {
            dir.join(format!("{stem}.{ext}"))
        };
        if path.is_file() {
            return Ok(path);
        }
        // Fall back to first file in the platform dir (some bundles
        // ship a differently-named library).
        if let Ok(mut entries) = std::fs::read_dir(&dir)
            && let Some(Ok(entry)) = entries.next()
        {
            let p = entry.path();
            if p.is_file() {
                return Ok(p);
            }
        }
    }
    Err(Vst3HostError::BundleLayout)
}

/// The argument for the module entry call. On macOS that is a
/// `CFBundleRef` for the `.vst3` bundle (VST3 SDK `bundleEntry`);
/// everywhere else the host passes null.
#[cfg(target_os = "macos")]
fn module_entry_arg(bundle: &Path) -> *mut c_void {
    use std::os::unix::ffi::OsStrExt;

    if !bundle.is_dir() {
        return ptr::null_mut();
    }
    let bytes = bundle.as_os_str().as_bytes();
    // SAFETY: plain CoreFoundation create calls; each returned object is
    // either null or owned by us (the Create rule). The URL is released
    // here, the bundle in `release_module_entry_arg`.
    unsafe {
        let url = cf::CFURLCreateFromFileSystemRepresentation(
            ptr::null(),
            bytes.as_ptr(),
            bytes.len() as isize,
            true,
        );
        if url.is_null() {
            return ptr::null_mut();
        }
        let bundle = cf::CFBundleCreate(ptr::null(), url);
        cf::CFRelease(url);
        bundle
    }
}

#[cfg(not(target_os = "macos"))]
fn module_entry_arg(_bundle: &Path) -> *mut c_void {
    ptr::null_mut()
}

fn release_module_entry_arg(arg: *mut c_void) {
    #[cfg(target_os = "macos")]
    if !arg.is_null() {
        // SAFETY: `arg` came from CFBundleCreate and is released once.
        unsafe { cf::CFRelease(arg) };
    }
    #[cfg(not(target_os = "macos"))]
    let _ = arg;
}

#[cfg(target_os = "macos")]
mod cf {
    use std::ffi::c_void;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        pub fn CFURLCreateFromFileSystemRepresentation(
            allocator: *const c_void,
            buffer: *const u8,
            buf_len: isize,
            is_directory: bool,
        ) -> *mut c_void;
        pub fn CFBundleCreate(allocator: *const c_void, bundle_url: *mut c_void) -> *mut c_void;
        pub fn CFRelease(cf: *mut c_void);
    }
}

fn module_entry_symbol() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "ModuleEntry\0"
    }
    #[cfg(target_os = "macos")]
    {
        "bundleEntry\0"
    }
    #[cfg(target_os = "windows")]
    {
        "InitDll\0"
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        "ModuleEntry\0"
    }
}

fn module_exit_symbol() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "ModuleExit\0"
    }
    #[cfg(target_os = "macos")]
    {
        "bundleExit\0"
    }
    #[cfg(target_os = "windows")]
    {
        "ExitDll\0"
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        "ModuleExit\0"
    }
}

/// Translate a single `PluginMidiEvent` to a VST3 `Event` for the
/// host event list. Returns `None` for messages that don't have a
/// first-class VST3 representation in this skeleton (CC, pitch bend,
/// program change — handled via IParameterChanges in the follow-up).
fn midi_to_vst3_event(ev: &PluginMidiEvent) -> Option<Event> {
    use daw_proto::MidiEvent;
    let sample_offset = ev.offset as i32;
    match ev.message {
        MidiEvent::NoteOn {
            channel,
            key,
            velocity,
        } => Some(Event {
            busIndex: 0,
            sampleOffset: sample_offset,
            ppqPosition: 0.0,
            flags: 0,
            r#type: EventTypes_::kNoteOnEvent as u16,
            __field0: Event__type0 {
                noteOn: NoteOnEvent {
                    channel: channel.index() as i16,
                    pitch: key.get() as i16,
                    tuning: 0.0,
                    velocity: velocity.get() as f32 / 127.0,
                    length: 0,
                    noteId: -1,
                },
            },
        }),
        MidiEvent::NoteOff {
            channel,
            key,
            velocity,
        } => Some(Event {
            busIndex: 0,
            sampleOffset: sample_offset,
            ppqPosition: 0.0,
            flags: 0,
            r#type: EventTypes_::kNoteOffEvent as u16,
            __field0: Event__type0 {
                noteOff: NoteOffEvent {
                    channel: channel.index() as i16,
                    pitch: key.get() as i16,
                    velocity: velocity.get() as f32 / 127.0,
                    noteId: -1,
                    tuning: 0.0,
                },
            },
        }),
        MidiEvent::PolyAftertouch {
            channel,
            key,
            pressure,
        } => Some(Event {
            busIndex: 0,
            sampleOffset: sample_offset,
            ppqPosition: 0.0,
            flags: 0,
            r#type: EventTypes_::kPolyPressureEvent as u16,
            __field0: Event__type0 {
                polyPressure: PolyPressureEvent {
                    channel: channel.index() as i16,
                    pitch: key.get() as i16,
                    pressure: pressure.get() as f32 / 127.0,
                    noteId: -1,
                },
            },
        }),
        // CC / PitchBend / ProgramChange / ChannelPressure are
        // routed via IMidiMapping → IParameterChanges by the caller,
        // not as IEventList entries. SysEx has no VST3-canonical
        // path; we drop it (only the IEventList carries notes and
        // poly-pressure in VST3, and the IDataEvent type is rare
        // enough to defer).
        _ => None,
    }
}

/// Map our cross-format `NoteExpressionDim` onto VST3's
/// `NoteExpressionTypeIDs`. Returns `None` only for dimensions that
/// don't have a VST3-standard slot (currently every dim except
/// `Pressure` maps directly; Pressure has no standard ID so we
/// allocate it inside the `kCustomStart` range so plugins that
/// declare a custom pressure expression can pick it up).
fn note_expression_dim_to_vst3(dim: daw_proto::midi::NoteExpressionDim) -> Option<u32> {
    use NoteExpressionTypeIDs_ as V;
    use daw_proto::midi::NoteExpressionDim;
    Some(match dim {
        NoteExpressionDim::Volume => V::kVolumeTypeID,
        NoteExpressionDim::Pan => V::kPanTypeID,
        NoteExpressionDim::Tuning => V::kTuningTypeID,
        NoteExpressionDim::Vibrato => V::kVibratoTypeID,
        NoteExpressionDim::Expression => V::kExpressionTypeID,
        NoteExpressionDim::Brightness => V::kBrightnessTypeID,
        NoteExpressionDim::Pressure => V::kCustomStart,
    })
}

/// Translate a non-note MIDI message into the (controller_number,
/// channel, normalized_value) tuple that `IMidiMapping::
/// getMidiControllerAssignment` consumes. Returns `None` for events
/// that have no IMidiMapping path (notes — they go through
/// IEventList; SysEx — VST3 has no first-class transport for it).
fn midi_to_ctrl_assignment(message: &daw_proto::MidiEvent) -> Option<(i16, u8, f64)> {
    use daw_proto::MidiEvent;
    match *message {
        MidiEvent::ControlChange {
            channel,
            controller,
            value,
        } => Some((
            controller.get() as i16,
            channel.index(),
            value.get() as f64 / 127.0,
        )),
        MidiEvent::PitchBend { channel, bend } => {
            // 14-bit unsigned 0..16383 (center 8192) → normalized 0..1.
            Some((
                kPitchBend as i16,
                channel.index(),
                bend.get() as f64 / 16383.0,
            ))
        }
        MidiEvent::ProgramChange { channel, program } => Some((
            kCtrlProgramChange as i16,
            channel.index(),
            program.get() as f64 / 127.0,
        )),
        MidiEvent::ChannelPressure { channel, pressure } => Some((
            kAfterTouch as i16,
            channel.index(),
            pressure.get() as f64 / 127.0,
        )),
        _ => None,
    }
}

fn utf16_array_to_string(arr: &[char16]) -> String {
    let end = arr.iter().position(|&c| c == 0).unwrap_or(arr.len());
    String::from_utf16_lossy(&arr[..end])
}

fn char8_array_to_string(arr: &[char8]) -> String {
    let bytes: Vec<u8> = arr
        .iter()
        .take_while(|&&b| (b as u8) != 0)
        .map(|&b| b as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn tuid_to_hex(cid: &TUID) -> String {
    // VST3 class IDs are 16-byte arrays; render as 32-char hex.
    let mut s = String::with_capacity(32);
    for b in cid {
        s.push_str(&format!("{:02X}", *b as u8));
    }
    s
}
