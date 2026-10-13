//! Native PipeWire MIDI input backend for `midicore` — **one graph node, one
//! port**, and no JACK shim.
//!
//! # Why this exists
//!
//! The midir backend ([`midicore_midir`]) is one OS client *per connection*:
//! that is midir's API, not a choice we make. Opening 23 hardware ports
//! therefore produced 23 JACK clients — 23 PipeWire graph nodes — for what is
//! conceptually a single application listening to the keyboards. Worse, its
//! `input_ports()` builds a whole client just to enumerate, so the hot-plug
//! pump created and destroyed a client ~2.5 times a second, forever. Measured
//! on the live rig: 3002 client lifecycles in ten minutes.
//!
//! Talking to PipeWire directly removes both problems at the source:
//!
//! - **One node.** A single `pw::stream` node (named by the caller — "Signal"
//!   for the rig) with one MIDI input port. Every selected hardware source is
//!   *linked* into that one port, so the graph shows one box no matter how
//!   many keyboards are plugged in.
//! - **No enumeration churn.** Ports are discovered from the registry, which
//!   pushes add/remove events. Nothing is created to ask what exists, and
//!   hot-plug needs no polling at all.
//!
//! # The tradeoff, stated plainly
//!
//! Merging every source into one port means an event no longer carries the
//! device it came from. Device *selection* therefore happens by choosing
//! which sources are linked ([`MidiInput::set_selector`]) rather than by
//! filtering events after the fact. That is a deliberate trade for the single
//! node — see the `pw_filter` note at the bottom of this file for the shape
//! that would keep per-source tagging, if it is ever wanted back.
//!
//! # Threading
//!
//! A PipeWire main loop is not `Send`, so the whole client lives on its own
//! thread; the handle talks to it over a `pw::channel`. `sink` is called on
//! that loop's realtime data thread — keep it cheap and non-blocking, exactly
//! as with midir.
#![cfg(target_os = "linux")]

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use eyre::eyre;
use pipewire as pw;
use pw::spa;
use spa::param::format::{MediaSubtype, MediaType};
use spa::pod::Pod;

use midicore_proto::{
    BackendError, Direction, InputBackend, InputConfig, MaybeSend, MidiEvent, PortId, PortInfo,
    PortSelector, TimedEvent,
};

/// Handshake between the caller and the loop thread: `None` while the stream
/// is still connecting, then the outcome, once.
type Ready = Arc<(Mutex<Option<Result<(), String>>>, std::sync::Condvar)>;

/// The node name used when a caller does not pick one.
pub const DEFAULT_NODE_NAME: &str = "Signal";

// ── Port discovery ──────────────────────────────────────────────────────────

/// A MIDI source port in the graph, as the rest of the world names it.
///
/// The name is `"{node}:{port}"` — byte-identical to what `pw-link` prints and
/// to what the midir/JACK backend reported, so a port name stored in a rig
/// preset keeps working across the backend swap.
#[derive(Clone, Debug)]
struct SourcePort {
    global_id: u32,
    node_id: u32,
    port_name: String,
}

impl SourcePort {
    fn full_name(&self, nodes: &HashMap<u32, String>) -> String {
        match nodes.get(&self.node_id) {
            Some(node) => format!("{node}:{}", self.port_name),
            // A port can arrive before its node does; name it as best we can
            // rather than dropping it, and the next registry event fixes it.
            None => self.port_name.clone(),
        }
    }
}

/// Does this port's props describe a MIDI *output* (i.e. a source we can
/// listen to)? PipeWire spells the MIDI DSP format several ways across
/// versions ("8 bit raw midi", "32 bit raw UMP"), so match on the substring
/// rather than an exact string that a point release can change under us.
fn is_midi_source(props: &spa::utils::dict::DictRef) -> bool {
    let dsp = props.get("format.dsp").unwrap_or_default();
    let is_midi = dsp.contains("midi") || dsp.contains("UMP");
    let is_out = props.get("port.direction") == Some("out");
    is_midi && is_out
}

/// Names of every MIDI input port available to open, sorted.
///
/// Unlike the midir backend this creates **no node** — it is a registry
/// roundtrip on a throwaway connection, which is why the hot-plug pump can
/// call it without the client churn that made enumeration itself unreliable.
pub fn input_ports() -> Vec<String> {
    match enumerate() {
        Ok(mut ports) => {
            ports.sort();
            ports
        }
        Err(e) => {
            tracing::warn!("midicore-pipewire: enumerate failed: {e}");
            Vec::new()
        }
    }
}

/// Available MIDI input ports as [`PortInfo`] records (id = port name).
pub fn input_devices() -> Vec<PortInfo> {
    input_ports()
        .into_iter()
        .map(|name| PortInfo {
            id: PortId(name.clone()),
            name,
            direction: Direction::Input,
            virtual_port: false,
        })
        .collect()
}

fn enumerate() -> eyre::Result<Vec<String>> {
    init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

    let nodes: Rc<RefCell<HashMap<u32, String>>> = Rc::new(RefCell::new(HashMap::new()));
    let ports: Rc<RefCell<Vec<SourcePort>>> = Rc::new(RefCell::new(Vec::new()));

    let _listener = {
        let (nodes, ports) = (nodes.clone(), ports.clone());
        registry
            .add_listener_local()
            .global(move |g| collect_global(g, &nodes, &ports))
            .register()
    };

    // One roundtrip: the registry replays every existing global before the
    // sync completes, so when `done` fires we have seen everything.
    let done = Rc::new(Cell::new(false));
    let pending = core.sync(0)?;
    let _core_listener = {
        let (done, mainloop) = (done.clone(), mainloop.clone());
        core.add_listener_local()
            .done(move |id, seq| {
                if id == pw::core::PW_ID_CORE && seq == pending {
                    done.set(true);
                    mainloop.quit();
                }
            })
            .register()
    };
    mainloop.run();

    let nodes = nodes.borrow();
    let ports = ports.borrow();
    let names = ports.iter().map(|p| p.full_name(&nodes)).collect();
    Ok(names)
}

use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Fold one registry global into the node-name / source-port maps.
fn collect_global(
    g: &pw::registry::GlobalObject<&spa::utils::dict::DictRef>,
    nodes: &Rc<RefCell<HashMap<u32, String>>>,
    ports: &Rc<RefCell<Vec<SourcePort>>>,
) {
    let Some(props) = g.props else { return };
    match g.type_ {
        pw::types::ObjectType::Node => {
            if let Some(name) = props.get("node.name") {
                nodes.borrow_mut().insert(g.id, name.to_string());
            }
        }
        pw::types::ObjectType::Port => {
            if !is_midi_source(props) {
                return;
            }
            let (Some(node_id), Some(port_name)) = (props.get("node.id"), props.get("port.name"))
            else {
                return;
            };
            let Ok(node_id) = node_id.parse::<u32>() else {
                return;
            };
            ports.borrow_mut().push(SourcePort {
                global_id: g.id,
                node_id,
                port_name: port_name.to_string(),
            });
        }
        _ => {}
    }
}

// ── The open input ──────────────────────────────────────────────────────────

/// Commands sent from the handle to the PipeWire loop thread.
enum Cmd {
    /// Change which sources are linked into our port. A *set* of selectors,
    /// unioned: several rigs share this one node, and each names the devices
    /// it wants independently.
    Select(Vec<PortSelector>),
    /// Tear down the loop.
    Quit,
}

/// An open MIDI input: one PipeWire node with one merged MIDI port, plus the
/// links feeding it. Drop to remove the node and every link it owns.
pub struct MidiInput {
    tx: pw::channel::Sender<Cmd>,
    /// Source ports currently linked, kept fresh by the loop thread.
    linked: Arc<RwLock<Vec<String>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl MidiInput {
    /// Open a MIDI input node named [`DEFAULT_NODE_NAME`].
    pub fn open<F>(selector: PortSelector, sink: F) -> eyre::Result<Self>
    where
        F: Fn(TimedEvent) + Send + 'static,
    {
        Self::open_named(DEFAULT_NODE_NAME, selector, sink)
    }

    /// Open a MIDI input node called `node_name`.
    ///
    /// Returns once the loop thread has connected its stream, so a caller that
    /// immediately asks for [`Self::ports`] sees a settled answer rather than
    /// an empty one.
    pub fn open_named<F>(node_name: &str, selector: PortSelector, sink: F) -> eyre::Result<Self>
    where
        F: Fn(TimedEvent) + Send + 'static,
    {
        Self::open_selecting(node_name, vec![selector], sink)
    }

    /// Open a node called `node_name` linked to the union of `selectors` from
    /// its first reconcile — no window where it links a narrower or wider set.
    fn open_selecting<F>(
        node_name: &str,
        selectors: Vec<PortSelector>,
        sink: F,
    ) -> eyre::Result<Self>
    where
        F: Fn(TimedEvent) + Send + 'static,
    {
        init();
        let (tx, rx) = pw::channel::channel();
        let linked = Arc::new(RwLock::new(Vec::new()));
        let ready: Ready = Arc::new((Mutex::new(None), std::sync::Condvar::new()));

        let thread = {
            let (node_name, linked, ready) = (node_name.to_string(), linked.clone(), ready.clone());
            std::thread::Builder::new()
                .name("midicore-pw".into())
                .spawn(move || {
                    let outcome = run_loop(&node_name, selectors, sink, rx, &linked, &ready);
                    if let Err(e) = outcome {
                        tracing::error!("midicore-pipewire: loop stopped: {e}");
                        signal_ready(&ready, Err(e.to_string()));
                    }
                })?
        };

        // Wait for the stream to connect (or fail) before handing the caller a
        // node they cannot yet query.
        let (lock, cv) = &*ready;
        let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
        while state.is_none() {
            let (s, timeout) = cv
                .wait_timeout(state, std::time::Duration::from_secs(5))
                .unwrap_or_else(|e| e.into_inner());
            state = s;
            if timeout.timed_out() {
                break;
            }
        }
        match state.clone() {
            Some(Err(e)) => Err(eyre!("open PipeWire MIDI node '{node_name}': {e}")),
            _ => Ok(Self {
                tx,
                linked,
                thread: Some(thread),
            }),
        }
    }

    /// The source ports currently linked into this node.
    pub fn ports(&self) -> Vec<String> {
        self.linked
            .read()
            .map(|p| p.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
    }

    /// Change which sources feed this node. Links are reconciled — only the
    /// difference is created or destroyed, so a selector that has not actually
    /// changed disturbs nothing.
    pub fn set_selector(&self, selector: PortSelector) {
        self.set_selectors(vec![selector]);
    }

    /// Link the union of several selectors.
    ///
    /// One node serves every rig in the process, so "what should be linked" is
    /// the union of what each rig asked for, not any single rig's answer. An
    /// empty list links nothing — that is a rig-less process, not an error.
    pub fn set_selectors(&self, selectors: Vec<PortSelector>) {
        let _ = self.tx.send(Cmd::Select(selectors));
    }
}

impl Drop for MidiInput {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl InputBackend for MidiInput {
    const NAME: &'static str = "pipewire";

    fn sources() -> Vec<PortInfo> {
        input_devices()
    }

    fn open<F>(config: InputConfig, sink: F) -> Result<Self, BackendError>
    where
        F: Fn(TimedEvent) + MaybeSend + 'static,
    {
        Self::open_selecting(&config.name, config.selectors, sink)
            .map_err(|e| BackendError(e.to_string()))
    }

    fn select(&self, selectors: Vec<PortSelector>) {
        self.set_selectors(selectors);
    }

    fn connected(&self) -> Vec<PortInfo> {
        let mut names = self.ports();
        names.sort();
        names
            .into_iter()
            .map(|name| PortInfo {
                id: PortId(name.clone()),
                name,
                direction: Direction::Input,
                virtual_port: false,
            })
            .collect()
    }
}

fn signal_ready(ready: &Ready, r: Result<(), String>) {
    let (lock, cv) = &**ready;
    if let Ok(mut guard) = lock.lock() {
        if guard.is_none() {
            *guard = Some(r);
            cv.notify_all();
        }
    }
}

/// Does `selector` want `port`? Mirrors the midir backend's matching rules so
/// a stored port name behaves identically on either backend.
fn wants(selector: &PortSelector, port: &str) -> bool {
    // A virtual port is a node other apps connect *to*; `matches` never
    // selects an existing source for one, so there is nothing to link.
    selector.matches(port)
}

fn run_loop<F>(
    node_name: &str,
    selectors: Vec<PortSelector>,
    sink: F,
    rx: pw::channel::Receiver<Cmd>,
    linked: &Arc<RwLock<Vec<String>>>,
    ready: &Ready,
) -> eyre::Result<()>
where
    F: Fn(TimedEvent) + Send + 'static,
{
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

    let mut props = pw::properties::properties! {
        *pw::keys::MEDIA_TYPE => "Midi",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Music",
        *pw::keys::NODE_NAME => node_name,
        *pw::keys::NODE_DESCRIPTION => node_name,
    };
    // Without a driver this node is never scheduled: it has no output of its
    // own to pull it into a graph cycle.
    props.insert("node.want-driver", "true");

    let stream = pw::stream::StreamRc::new(core.clone(), node_name, props)?;
    let started = Instant::now();

    let _stream_listener = stream
        .add_local_listener_with_user_data(())
        // The stream reaching `Streaming` is the difference between a node
        // that is merely present in the graph and one that is being fed. It
        // is the first thing to look at when links exist but nothing arrives.
        .state_changed(|_, _, old, new| {
            tracing::info!(
                midi.state.from = ?old,
                midi.state.to = ?new,
                "midicore-pipewire: stream state"
            );
        })
        .process(move |stream, _| {
            let Some(mut buf) = stream.dequeue_buffer() else {
                return;
            };
            let Some(data) = buf.datas_mut().first_mut() else {
                return;
            };
            let size = data.chunk().size() as usize;
            if size == 0 {
                return;
            }
            let Some(bytes) = data.data() else { return };
            let end = size.min(bytes.len());
            let ts = started.elapsed().as_micros() as u64;
            for_each_message(&bytes[..end], |raw| {
                if let Ok((event, _)) = MidiEvent::decode(raw) {
                    sink(TimedEvent {
                        timestamp_us: ts,
                        event,
                    });
                }
            });
        })
        .register()?;

    // MIDI is `application/control` in SPA's format vocabulary.
    let obj = spa::pod::object! {
        spa::utils::SpaTypes::ObjectParamFormat,
        spa::param::ParamType::EnumFormat,
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaType,
            Id,
            MediaType::Application
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaSubtype,
            Id,
            MediaSubtype::Control
        ),
    };
    let values: Vec<u8> = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(obj),
    )
    .map_err(|e| eyre!("serialize MIDI format pod: {e}"))?
    .0
    .into_inner();
    let mut params = [Pod::from_bytes(&values).ok_or_else(|| eyre!("bad format pod"))?];

    // No AUTOCONNECT: the session manager has no notion of "every MIDI
    // keyboard", and asking for one lands the node in an error state when no
    // default MIDI source exists. We make the links ourselves.
    stream.connect(
        spa::utils::Direction::Input,
        None,
        pw::stream::StreamFlags::MAP_BUFFERS | pw::stream::StreamFlags::RT_PROCESS,
        &mut params,
    )?;

    // ── Graph state, all owned by this thread ───────────────────────────────
    let state = Rc::new(RefCell::new(GraphState {
        selectors,
        nodes: HashMap::new(),
        sources: HashMap::new(),
        in_ports: HashMap::new(),
        links: HashMap::new(),
    }));

    let dirty = Rc::new(Cell::new(true));

    let _registry_listener = {
        let (state, dirty) = (state.clone(), dirty.clone());
        let removed = state.clone();
        registry
            .add_listener_local()
            .global({
                let (state, dirty) = (state.clone(), dirty.clone());
                move |g| {
                    if track_global(g, &state) {
                        dirty.set(true);
                    }
                }
            })
            .global_remove({
                let dirty = dirty.clone();
                move |id| {
                    let mut s = removed.borrow_mut();
                    let had = s.sources.remove(&id).is_some();
                    s.nodes.remove(&id);
                    s.in_ports.remove(&id);
                    s.links.retain(|_, link_id| *link_id != id);
                    if had {
                        dirty.set(true);
                    }
                }
            })
            .register()
    };

    let _rx = rx.attach(mainloop.loop_(), {
        let (state, dirty, mainloop) = (state.clone(), dirty.clone(), mainloop.clone());
        move |cmd| match cmd {
            Cmd::Select(sel) => {
                state.borrow_mut().selectors = sel;
                dirty.set(true);
            }
            Cmd::Quit => mainloop.quit(),
        }
    });

    // Reconcile on a slow timer as well as on events: link creation is
    // asynchronous, and our own port only appears in the registry a moment
    // after `connect`, so a purely event-driven reconcile can settle one pass
    // too early.
    let _timer = {
        let (state, dirty, core, linked) =
            (state.clone(), dirty.clone(), core.clone(), linked.clone());
        let stream = stream.clone();
        let timer = mainloop.loop_().add_timer(move |_| {
            if dirty.replace(false) {
                reconcile(&state, &core, &stream, &linked);
            }
        });
        timer
            .update_timer(
                Some(std::time::Duration::from_millis(200)),
                Some(std::time::Duration::from_millis(200)),
            )
            .into_result()
            .map_err(|e| eyre!("start reconcile timer: {e}"))?;
        timer
    };

    signal_ready(ready, Ok(()));
    mainloop.run();
    Ok(())
}

/// Everything the loop thread knows about the graph.
struct GraphState {
    /// Unioned: a source is linked if *any* selector wants it.
    selectors: Vec<PortSelector>,
    /// node global id → node.name
    nodes: HashMap<u32, String>,
    /// port global id → source port
    sources: HashMap<u32, SourcePort>,
    /// Every MIDI *input* port in the graph: global id → owning node id. Our
    /// own port is in here, but it can be announced before the stream reports
    /// its node id, so which one is ours is resolved at reconcile time rather
    /// than when the global arrives.
    in_ports: HashMap<u32, u32>,
    /// source port global id → link global id
    links: HashMap<u32, u32>,
}

/// Fold a registry global into [`GraphState`]. Returns whether the graph
/// changed in a way that needs a reconcile.
fn track_global(
    g: &pw::registry::GlobalObject<&spa::utils::dict::DictRef>,
    state: &Rc<RefCell<GraphState>>,
) -> bool {
    let Some(props) = g.props else { return false };
    let mut s = state.borrow_mut();
    match g.type_ {
        pw::types::ObjectType::Node => {
            let Some(name) = props.get("node.name") else {
                return false;
            };
            s.nodes.insert(g.id, name.to_string());
            true
        }
        pw::types::ObjectType::Port => {
            let Some(node_id) = props.get("node.id").and_then(|n| n.parse::<u32>().ok()) else {
                return false;
            };
            // Our own input port is among these; `reconcile` picks it out
            // once the stream can tell us which node id is ours.
            if props.get("port.direction") == Some("in") {
                s.in_ports.insert(g.id, node_id);
                return true;
            }
            if !is_midi_source(props) {
                return false;
            }
            let Some(port_name) = props.get("port.name") else {
                return false;
            };
            s.sources.insert(
                g.id,
                SourcePort {
                    global_id: g.id,
                    node_id,
                    port_name: port_name.to_string(),
                },
            );
            true
        }
        _ => false,
    }
}

/// Create the links the selector asks for, destroy the ones it no longer does.
fn reconcile(
    state: &Rc<RefCell<GraphState>>,
    core: &pw::core::CoreRc,
    stream: &pw::stream::StreamRc,
    linked: &Arc<RwLock<Vec<String>>>,
) {
    let mut s = state.borrow_mut();
    // The stream is the authority on which node is ours; it only knows once
    // the server has answered `connect`.
    let our_node = stream.node_id();
    if our_node == pw::constants::ID_ANY {
        return;
    }
    let Some(our_port) = s
        .in_ports
        .iter()
        .find(|(_, node)| **node == our_node)
        .map(|(port, _)| *port)
    else {
        return; // Our port has not been announced yet; the next tick retries.
    };

    let wanted: Vec<(u32, String)> = {
        let nodes = &s.nodes;
        s.sources
            .values()
            .map(|p| (p.global_id, p.full_name(nodes)))
            .filter(|(_, name)| s.selectors.iter().any(|sel| wants(sel, name)))
            .collect()
    };

    // Drop links whose source is gone or no longer wanted. The proxy is
    // dropped with the entry, which destroys the link.
    let keep: std::collections::HashSet<u32> = wanted.iter().map(|(id, _)| *id).collect();
    s.links.retain(|src, _| keep.contains(src));

    for (src_id, name) in &wanted {
        if s.links.contains_key(src_id) {
            continue;
        }
        let props = pw::properties::properties! {
            "link.output.port" => src_id.to_string(),
            "link.input.port" => our_port.to_string(),
            // Die with us: a lingering link would keep feeding a node that no
            // longer exists.
            "object.linger" => "false",
        };
        match core.create_object::<pw::link::Link>("link-factory", &props) {
            Ok(link) => {
                // The proxy must outlive the link; PipeWire destroys the link
                // when its proxy drops. `into_raw`-style leak is deliberate —
                // the whole client goes away on Drop, taking its links.
                std::mem::forget(link);
                s.links.insert(*src_id, u32::MAX);
                tracing::info!(midi.port = %name, "midicore-pipewire: linked");
            }
            Err(e) => tracing::warn!(midi.port = %name, "midicore-pipewire: link failed: {e}"),
        }
    }

    let names: Vec<String> = wanted.into_iter().map(|(_, n)| n).collect();
    if let Ok(mut l) = linked.write() {
        *l = names;
    }
}

// ── SPA control decoding ────────────────────────────────────────────────────

/// Walk a MIDI buffer and hand each message's raw bytes to `f`.
///
/// A PipeWire MIDI buffer is a SPA POD **Sequence** of `spa_pod_control`
/// entries. Each entry is either `SPA_CONTROL_Midi` (raw MIDI 1.0 bytes) or,
/// on PipeWire 1.6+, `SPA_CONTROL_UMP` (32-bit Universal MIDI Packet words).
/// A stream that negotiated `application/control` gets the former; the UMP arm
/// is here because the daemon is free to hand us either.
fn for_each_message(raw: &[u8], mut f: impl FnMut(&[u8])) {
    // SAFETY: `raw` is the mapped buffer PipeWire handed us, whose contents it
    // guarantees to be a well-formed POD of `chunk.size` bytes. Every read
    // below is bounded by that length.
    unsafe {
        if raw.len() < std::mem::size_of::<libspa_sys::spa_pod>() {
            return;
        }
        let pod = raw.as_ptr() as *const libspa_sys::spa_pod;
        if (*pod).type_ != libspa_sys::SPA_TYPE_Sequence {
            return;
        }
        let total = std::mem::size_of::<libspa_sys::spa_pod>() + (*pod).size as usize;
        if total > raw.len() {
            return;
        }
        let end = raw.as_ptr().add(total);

        let seq = pod as *const libspa_sys::spa_pod_sequence;
        let mut c = (&(*seq).body as *const libspa_sys::spa_pod_sequence_body).add(1)
            as *const libspa_sys::spa_pod_control;

        while (c as *const u8).add(std::mem::size_of::<libspa_sys::spa_pod_control>()) <= end {
            let value = &(*c).value as *const libspa_sys::spa_pod;
            let vsize = (*value).size as usize;
            let vdata = (value as *const u8).add(std::mem::size_of::<libspa_sys::spa_pod>());
            if vdata.add(vsize) > end {
                return;
            }
            let bytes = std::slice::from_raw_parts(vdata, vsize);
            match (*c).type_ {
                libspa_sys::SPA_CONTROL_Midi => f(bytes),
                libspa_sys::SPA_CONTROL_UMP => {
                    if let Some((buf, n)) = ump_to_midi1(bytes) {
                        f(&buf[..n]);
                    }
                }
                _ => {}
            }
            // Entries are 8-byte aligned.
            let step = std::mem::size_of::<libspa_sys::spa_pod_control>() + ((vsize + 7) & !7);
            c = (c as *const u8).add(step) as *const libspa_sys::spa_pod_control;
        }
    }
}

/// Translate a UMP word to MIDI 1.0 bytes.
///
/// Only message type `0x2` (MIDI 1.0 channel voice) is translated: that is
/// every note, CC, bend and aftertouch a controller sends. Types we do not
/// translate — UMP-native MIDI 2.0 voice messages, sysex chunks — are dropped
/// rather than mistranslated, because a wrong note is worse than a missing
/// one on stage.
fn ump_to_midi1(bytes: &[u8]) -> Option<([u8; 3], usize)> {
    if bytes.len() < 4 {
        return None;
    }
    let w = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if (w >> 28) & 0xf != 0x2 {
        return None;
    }
    let status = ((w >> 16) & 0xff) as u8;
    let d1 = ((w >> 8) & 0x7f) as u8;
    let d2 = (w & 0x7f) as u8;
    Some(match status & 0xf0 {
        // Program change and channel pressure are two bytes on the wire.
        0xc0 | 0xd0 => ([status, d1, 0], 2),
        _ => ([status, d1, d2], 3),
    })
}

/// `pw::init` is process-global and must run before any other call.
fn init() {
    static ONCE: AtomicBool = AtomicBool::new(false);
    static LOCK: Mutex<()> = Mutex::new(());
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if !ONCE.swap(true, Ordering::SeqCst) {
        pw::init();
    }
}

// A note on the road not taken: `pw_filter` gives one node with *many* named
// ports, which would keep per-source tagging (an event would still know which
// keyboard sent it) while still showing a single box in the graph. It has no
// safe Rust wrapper in pipewire-rs 0.10, so it would mean hand-written FFI
// against `pw_filter_*`. The single merged port here is the deliberate
// simpler choice; that is the shape to reach for if per-rig device routing is
// ever wanted back.

#[cfg(test)]
mod tests {
    use super::{ump_to_midi1, wants};
    use midicore_proto::{PortId, PortSelector};

    const S88: &str = "Midi-Bridge:KONTROL S88 MK3: Main (capture)";

    #[test]
    fn omni_wants_every_device() {
        assert!(wants(&PortSelector::All, S88));
        assert!(wants(&PortSelector::NameContains(String::new()), S88));
    }

    /// A stored port name is matched as a case-insensitive substring, the
    /// same rule the midir backend used — so a rig preset written against
    /// the old backend selects the same device on this one.
    #[test]
    fn a_named_selector_matches_a_substring_case_insensitively() {
        assert!(wants(
            &PortSelector::NameContains("kontrol s88".into()),
            S88
        ));
        assert!(!wants(&PortSelector::NameContains("mioXM".into()), S88));
    }

    #[test]
    fn an_id_selector_matches_the_whole_name_only() {
        assert!(wants(&PortSelector::Id(PortId(S88.into())), S88));
        assert!(!wants(&PortSelector::Id(PortId("KONTROL".into())), S88));
    }

    /// A virtual port is a node other apps connect *to*; there is nothing on
    /// the far end for us to link, so it must never pull a device in.
    #[test]
    fn a_virtual_selector_links_nothing() {
        assert!(!wants(&PortSelector::Virtual("Signal".into()), S88));
    }

    /// UMP message type 0x2 is MIDI 1.0 channel voice — note on, at full
    /// velocity, on channel 1.
    #[test]
    fn ump_channel_voice_becomes_three_midi_bytes() {
        // group 0, type 2, status 0x90, note 60, velocity 100
        let word = 0x2090_3C64u32.to_be_bytes();
        assert_eq!(ump_to_midi1(&word), Some(([0x90, 60, 100], 3)));
    }

    /// Program change and channel pressure are two bytes on the wire; a third
    /// would be read as a spurious status byte by anything downstream.
    #[test]
    fn ump_two_byte_messages_report_a_length_of_two() {
        let pc = 0x20C0_0500u32.to_be_bytes();
        assert_eq!(ump_to_midi1(&pc), Some(([0xC0, 5, 0], 2)));
        let pressure = 0x20D0_4000u32.to_be_bytes();
        assert_eq!(ump_to_midi1(&pressure), Some(([0xD0, 0x40, 0], 2)));
    }

    /// Anything that is not MIDI 1.0 channel voice is dropped rather than
    /// mistranslated: a wrong note is worse than a missing one on stage.
    #[test]
    fn ump_drops_what_it_cannot_faithfully_translate() {
        // Type 0x4 — MIDI 2.0 channel voice, a different word layout.
        assert_eq!(ump_to_midi1(&0x4090_3C64u32.to_be_bytes()), None);
        // Type 0x1 — system real time.
        assert_eq!(ump_to_midi1(&0x10F8_0000u32.to_be_bytes()), None);
        // A truncated word is not a message.
        assert_eq!(ump_to_midi1(&[0x20, 0x90]), None);
    }
}
