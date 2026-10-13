//! Native CoreMIDI input backend for `midicore` — **one client, one input
//! port**, every selected source connected to it.
//!
//! # Why not midir
//!
//! midir opens one CoreMIDI client *per connection*: that is its API. CoreMIDI
//! itself is built the other way round — a client owns an input port and any
//! number of sources are *connected* to that port with
//! `MIDIPortConnectSource`. That is exactly the shape the PipeWire backend
//! builds by linking sources into one node, so this backend is the same idea
//! in CoreMIDI's own terms: selection is connect/disconnect on one port, and
//! nothing is torn down to change which keyboards feed the rig.
//!
//! # Port names
//!
//! A source is named by its `kMIDIPropertyDisplayName` — byte-identical to
//! what midir reported on macOS — so a port name stored in a rig preset keeps
//! selecting the same device after the swap.
//!
//! # Hot-plug
//!
//! CoreMIDI tells a process about devices coming and going through a
//! CFRunLoop: the client's notification port is serviced there, and until it
//! is, the process's own view of the source list does not change. The owning
//! thread therefore *runs* its run loop, in [`RESCAN`] slices, and re-reads
//! the source list after each — so a pedal switched on after the rig opened
//! is seen and connected. (It used to sleep instead, and read the same stale
//! list forever: a device that was not there at launch never arrived.) A
//! setup change also ends the slice early, so a plug-in is picked up at once.
//! Unlike midir-over-JACK, enumerating CoreMIDI sources creates nothing, so
//! re-reading costs no client churn.
//!
//! A source that is *offline* counts as absent. A Bluetooth pedal switched
//! off does not leave the source list — its endpoint stays, under the same
//! unique id, marked offline — so a list that only tracked presence saw no
//! change, kept its (now dead) connection, and never connected again when
//! the pedal came back: the footswitch went silent until the app restarted.
//! Treated as gone while offline, it is disconnected when it drops and gets
//! a fresh connection when it returns.
//!
//! # Threading
//!
//! The client, its port and any virtual destinations live on one thread,
//! which the handle drives over a channel. `sink` is called on CoreMIDI's own
//! high-priority receive thread — keep it cheap and non-blocking.
#![cfg(target_os = "macos")]

use std::collections::{BTreeMap, HashMap};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use coremidi::{Client, InputPort, PacketList, Source, Sources, VirtualDestination};
use midicore_proto::{
    decode_all, BackendError, Direction, InputBackend, InputConfig, MaybeSend, PortId, PortInfo,
    PortSelector, TimedEvent,
};

/// Make this crate's CoreMIDI home thread the process's first contact with
/// CoreMIDI. Call it as early as possible — first thing in `main`.
///
/// CoreMIDI keeps a process's view of the devices current through the run
/// loop of the thread that first connected the process to the MIDI server.
/// When that thread never runs a run loop — any worker that happened to
/// enumerate ports first — the source list freezes at launch: a pedal
/// switched on later never appears, in any client, on any thread. The home
/// thread connects first and then runs its run loop for the life of the
/// process, so the list stays live whoever asks. Every entry point of this
/// crate calls it; calling it from `main` also wins against anything else in
/// the process (midir, say) that might otherwise touch CoreMIDI first.
pub fn init() {
    static HOME: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    HOME.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<()>(1);
        let spawned = std::thread::Builder::new()
            .name("midicore-coremidi-home".into())
            .spawn(move || {
                // Connecting to the MIDI server: the first CoreMIDI call.
                let client = Client::new_with_notifications(
                    "midicore-home",
                    |_n: &coremidi::Notification| {},
                );
                let _ = tx.send(());
                let _client = client;
                loop {
                    let result = unsafe {
                        core_foundation::runloop::CFRunLoop::run_in_mode(
                            core_foundation::runloop::kCFRunLoopDefaultMode,
                            Duration::from_secs(60),
                            false,
                        )
                    };
                    if matches!(
                        result,
                        core_foundation::runloop::CFRunLoopRunResult::Finished
                    ) {
                        std::thread::sleep(RESCAN);
                    }
                }
            });
        if spawned.is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(5));
        }
    });
}

/// Sends to CoreMIDI destinations by name — for driving a controller's
/// LEDs from the rig's state. One client and output port for the process;
/// the destination is looked up on every send, so a device that drops out
/// and comes back (a Bluetooth pedal switched off and on) needs no
/// reconnecting: the next send simply finds it again.
pub struct CoreMidiOutput {
    port: coremidi::OutputPort,
    _client: Client,
}

impl CoreMidiOutput {
    /// # Errors
    /// When CoreMIDI cannot make a client or output port.
    pub fn open(name: &str) -> Result<Self, BackendError> {
        init();
        let client = Client::new(name)
            .map_err(|s| BackendError(format!("create CoreMIDI client '{name}': OSStatus {s}")))?;
        let port = client
            .output_port(name)
            .map_err(|s| BackendError(format!("create CoreMIDI output port: OSStatus {s}")))?;
        Ok(Self {
            port,
            _client: client,
        })
    }

    /// Send `bytes` (one or more complete short messages) to every
    /// destination whose name contains `device` (case-insensitive). Returns
    /// how many destinations it went to — 0 when the device is not there.
    pub fn send_to(&self, device: &str, bytes: &[u8]) -> usize {
        let want = device.to_lowercase();
        let mut sent = 0;
        for dest in coremidi::Destinations {
            let name = dest
                .display_name()
                .or_else(|| dest.name())
                .unwrap_or_default();
            if name.to_lowercase().contains(&want) {
                let packets = coremidi::PacketBuffer::new(0, bytes);
                if self.port.send(&dest, &packets).is_ok() {
                    sent += 1;
                }
            }
        }
        sent
    }
}

/// How often the owning thread re-reads the source list.
pub const RESCAN: Duration = Duration::from_millis(200);

/// Every CoreMIDI source right now: unique id → display name.
///
/// The unique id is what survives a rename or a reorder, so it is what the
/// connection bookkeeping keys on; the name is what selectors match.
fn scan() -> BTreeMap<u32, String> {
    scan_all()
        .into_iter()
        .filter(|(_, (_, offline))| !offline)
        .map(|(id, (name, _))| (id, name))
        .collect()
}

/// Every CoreMIDI source, online or not: unique id → (display name, offline).
fn scan_all() -> BTreeMap<u32, (String, bool)> {
    Sources
        .into_iter()
        .filter_map(|s| {
            let id = s.unique_id()?;
            let name = s.display_name().or_else(|| s.name())?;
            // The property inherits from the entity and device, so a device
            // that dropped reads offline here too. Unreadable = online.
            let offline = s
                .get_property(&coremidi::Properties::offline())
                .unwrap_or(false);
            Some((id, (name, offline)))
        })
        .collect()
}

fn source_info(name: String, virtual_port: bool) -> PortInfo {
    PortInfo {
        id: PortId(name.clone()),
        name,
        direction: Direction::Input,
        virtual_port,
    }
}

enum Cmd {
    Select(Vec<PortSelector>),
    Quit,
}

/// An open CoreMIDI input. Drop to disconnect every source and dispose the
/// port.
pub struct CoreMidiInput {
    tx: mpsc::Sender<Cmd>,
    connected: Arc<RwLock<Vec<PortInfo>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// The sink, shared by the input port and every virtual destination. Behind a
/// mutex because the caller's sink is only `Send`; every callback runs on
/// CoreMIDI's single receive thread, so the lock is never contended.
type Sink = Arc<Mutex<dyn Fn(TimedEvent) + Send>>;

fn deliver(sink: &Sink, started: Instant, packets: &PacketList) {
    let timestamp_us = started.elapsed().as_micros() as u64;
    let sink = sink
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for packet in packets.iter() {
        decode_all(packet.data(), |event| {
            sink(TimedEvent {
                timestamp_us,
                event,
            });
        });
    }
}

impl InputBackend for CoreMidiInput {
    const NAME: &'static str = "coremidi";

    fn sources() -> Vec<PortInfo> {
        init();
        let mut names: Vec<String> = scan().into_values().collect();
        names.sort();
        names.into_iter().map(|n| source_info(n, false)).collect()
    }

    fn open<F>(config: InputConfig, sink: F) -> Result<Self, BackendError>
    where
        F: Fn(TimedEvent) + MaybeSend + 'static,
    {
        init();
        let sink: Sink = Arc::new(Mutex::new(sink));
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<(), BackendError>>(1);
        let connected = Arc::new(RwLock::new(Vec::new()));

        let thread = {
            let connected = connected.clone();
            std::thread::Builder::new()
                .name("midicore-coremidi".into())
                .spawn(move || run(config, sink, rx, &ready_tx, &connected))
                .map_err(|e| BackendError(format!("spawn CoreMIDI thread: {e}")))?
        };

        // Wait until the client and port exist, so a caller that asks for
        // `connected()` straight away gets a settled answer.
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Err(e)) => Err(e),
            Ok(Ok(())) | Err(_) => Ok(Self {
                tx,
                connected,
                thread: Some(thread),
            }),
        }
    }

    fn select(&self, selectors: Vec<PortSelector>) {
        let _ = self.tx.send(Cmd::Select(selectors));
    }

    fn connected(&self) -> Vec<PortInfo> {
        self.connected
            .read()
            .map(|c| c.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
    }
}

impl Drop for CoreMidiInput {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Everything the owning thread holds. Dropping it disconnects and disposes.
struct State {
    selectors: Vec<PortSelector>,
    /// The last source list reconciled against, to skip no-change rescans.
    seen: BTreeMap<u32, String>,
    /// Connected sources: unique id → name.
    linked: BTreeMap<u32, String>,
    /// Virtual destinations created for `PortSelector::Virtual`, by name.
    virtuals: HashMap<String, VirtualDestination>,
}

fn run(
    config: InputConfig,
    sink: Sink,
    rx: mpsc::Receiver<Cmd>,
    ready: &mpsc::SyncSender<Result<(), BackendError>>,
    connected: &RwLock<Vec<PortInfo>>,
) {
    let started = Instant::now();
    // NOTE: coremidi 0.9 does not dispose a `Client` on drop, so each open
    // leaves a (cheap, idle) client registered until the process exits. The
    // intended use is one long-lived input per process.
    // A setup change wakes the loop below early (and, by existing, puts the
    // notification port on this thread's run loop).
    let changed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let client = match Client::new_with_notifications(&config.name, {
        let changed = changed.clone();
        move |_n: &coremidi::Notification| {
            changed.store(true, std::sync::atomic::Ordering::Relaxed);
            core_foundation::runloop::CFRunLoop::get_current().stop();
        }
    }) {
        Ok(c) => c,
        Err(status) => {
            let _ = ready.send(Err(BackendError(format!(
                "create CoreMIDI client '{}': OSStatus {status}",
                config.name
            ))));
            return;
        }
    };
    let port = {
        let sink = sink.clone();
        match client.input_port(&config.name, move |packets| {
            deliver(&sink, started, packets)
        }) {
            Ok(p) => p,
            Err(status) => {
                let _ = ready.send(Err(BackendError(format!(
                    "create CoreMIDI input port: OSStatus {status}"
                ))));
                return;
            }
        }
    };

    let mut state = State {
        selectors: config.selectors,
        seen: BTreeMap::new(),
        linked: BTreeMap::new(),
        virtuals: HashMap::new(),
    };
    reconcile(
        &mut state,
        scan(),
        &client,
        &port,
        &sink,
        started,
        connected,
    );
    let _ = ready.send(Ok(()));

    'outer: loop {
        // Service CoreMIDI's notifications for a slice (returning early on a
        // setup change); with nothing scheduled on the run loop, wait the
        // slice out instead of spinning.
        let started_slice = Instant::now();
        let result = unsafe {
            core_foundation::runloop::CFRunLoop::run_in_mode(
                core_foundation::runloop::kCFRunLoopDefaultMode,
                RESCAN,
                false,
            )
        };
        if matches!(
            result,
            core_foundation::runloop::CFRunLoopRunResult::Finished
        ) {
            std::thread::sleep(RESCAN.saturating_sub(started_slice.elapsed()));
        }
        let mut dirty = changed.swap(false, std::sync::atomic::Ordering::Relaxed);
        loop {
            match rx.try_recv() {
                Ok(Cmd::Select(selectors)) => {
                    dirty |= state.selectors != selectors;
                    state.selectors = selectors;
                }
                Ok(Cmd::Quit) | Err(mpsc::TryRecvError::Disconnected) => break 'outer,
                Err(mpsc::TryRecvError::Empty) => break,
            }
        }
        let now = scan();
        if dirty || now != state.seen {
            reconcile(&mut state, now, &client, &port, &sink, started, connected);
        }
    }

    for id in state.linked.keys() {
        if let Some(source) = Source::from_unique_id(*id) {
            let _ = port.disconnect_source(&source);
        }
    }
}

/// Connect what the selectors want, disconnect what they no longer do, and
/// create or drop virtual destinations to match.
fn reconcile(
    state: &mut State,
    now: BTreeMap<u32, String>,
    client: &Client,
    port: &InputPort,
    sink: &Sink,
    started: Instant,
    connected: &RwLock<Vec<PortInfo>>,
) {
    let wanted: BTreeMap<u32, String> = now
        .iter()
        .filter(|(_, name)| state.selectors.iter().any(|s| s.matches(name)))
        .map(|(id, name)| (*id, name.clone()))
        .collect();

    // A source that vanished is already gone from CoreMIDI; only one that is
    // still present but no longer wanted needs an explicit disconnect.
    state.linked.retain(|id, name| {
        if wanted.contains_key(id) {
            return true;
        }
        if let Some(source) = Source::from_unique_id(*id) {
            let _ = port.disconnect_source(&source);
            tracing::info!(midi.port = %name, "midicore-coremidi: disconnected");
        }
        false
    });

    for (id, name) in &wanted {
        if state.linked.contains_key(id) {
            continue;
        }
        let Some(source) = Source::from_unique_id(*id) else {
            continue;
        };
        match port.connect_source(&source) {
            Ok(()) => {
                tracing::info!(midi.port = %name, "midicore-coremidi: connected");
                state.linked.insert(*id, name.clone());
            }
            Err(status) => tracing::warn!(
                midi.port = %name,
                midi.os_status = status,
                "midicore-coremidi: connect failed"
            ),
        }
    }

    let virtual_names: Vec<&String> = state
        .selectors
        .iter()
        .filter_map(|s| match s {
            PortSelector::Virtual(name) => Some(name),
            _ => None,
        })
        .collect();
    state
        .virtuals
        .retain(|name, _| virtual_names.contains(&name));
    for name in virtual_names {
        if state.virtuals.contains_key(name) {
            continue;
        }
        let sink = sink.clone();
        match client.virtual_destination(name, move |packets| deliver(&sink, started, packets)) {
            Ok(dest) => {
                tracing::info!(midi.port = %name, "midicore-coremidi: created virtual input");
                state.virtuals.insert(name.clone(), dest);
            }
            Err(status) => tracing::warn!(
                midi.port = %name,
                midi.os_status = status,
                "midicore-coremidi: virtual input failed"
            ),
        }
    }

    let mut infos: Vec<PortInfo> = state
        .linked
        .values()
        .map(|n| source_info(n.clone(), false))
        .chain(state.virtuals.keys().map(|n| source_info(n.clone(), true)))
        .collect();
    infos.sort_by(|a, b| a.name.cmp(&b.name));
    if let Ok(mut c) = connected.write() {
        *c = infos;
    }
    state.seen = now;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Enumeration needs no client and must never fail — on a Mac with no
    /// MIDI hardware it is simply empty.
    #[test]
    fn sources_enumerate_without_opening_anything() {
        let sources = CoreMidiInput::sources();
        assert!(sources.windows(2).all(|w| w[0].name <= w[1].name));
        assert!(sources.iter().all(|p| p.id.0 == p.name && !p.virtual_port));
    }

    /// Opening with nothing selected connects nothing, and a virtual selector
    /// creates a port other apps can see and send to.
    #[test]
    fn a_virtual_selector_creates_a_port_and_dropping_removes_it() {
        let _serial = coremidi_lock();
        let name = format!("midicore-test-{}", std::process::id());
        let input = CoreMidiInput::open(
            InputConfig::new("midicore-test").selecting(vec![PortSelector::Virtual(name.clone())]),
            |_| {},
        )
        .expect("CoreMIDI reachable");
        let connected = input.connected();
        assert_eq!(connected.len(), 1);
        assert_eq!(connected[0].name, name);
        assert!(connected[0].virtual_port);

        input.select(Vec::new());
        std::thread::sleep(RESCAN * 2);
        assert!(input.connected().is_empty());
    }

    /// End to end through CoreMIDI: a virtual *source* we create is selected
    /// by name, and bytes sent on it arrive in the sink decoded.
    #[test]
    fn a_selected_source_delivers_decoded_events() {
        let _serial = coremidi_lock();
        let client = Client::new("midicore-test-sender").expect("client");
        let name = format!("midicore-test-src-{}", std::process::id());
        let source = client.virtual_source(&name).expect("virtual source");

        let (tx, rx) = mpsc::channel();
        let input = CoreMidiInput::open(
            InputConfig::new("midicore-test")
                .selecting(vec![PortSelector::NameContains(name.clone())]),
            move |ev| {
                let _ = tx.send(ev.event);
            },
        )
        .expect("CoreMIDI reachable");
        assert_eq!(input.ports_named(), vec![name.clone()]);

        let packets = coremidi::PacketBuffer::new(0, &[0x90, 60, 100]);
        source.received(&packets).expect("send");
        let event = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("event arrives");
        assert!(matches!(event, midicore_proto::MidiEvent::NoteOn { .. }));
    }

    /// A source that goes offline and comes back — a Bluetooth pedal
    /// switched off and on — is disconnected while it is away and delivers
    /// again once it returns, without the input being reopened.
    #[test]
    fn a_source_that_goes_offline_and_returns_plays_again() {
        let _serial = coremidi_lock();
        let client = Client::new("midicore-test-offline-sender").expect("client");
        let name = format!("midicore-test-offline-{}", std::process::id());
        let source = client.virtual_source(&name).expect("virtual source");

        let (tx, rx) = mpsc::channel();
        let input = CoreMidiInput::open(
            InputConfig::new("midicore-test")
                .selecting(vec![PortSelector::NameContains(name.clone())]),
            move |ev| {
                let _ = tx.send(ev.event);
            },
        )
        .expect("CoreMIDI reachable");
        assert_eq!(input.ports_named(), vec![name.clone()]);

        source
            .set_property(&coremidi::Properties::offline(), true)
            .expect("mark offline");
        assert!(
            within(|| input.ports_named().is_empty()),
            "offline: not connected"
        );

        source
            .set_property(&coremidi::Properties::offline(), false)
            .expect("mark online");
        assert!(
            within(|| input.ports_named() == vec![name.clone()]),
            "back online: connected again"
        );

        let packets = coremidi::PacketBuffer::new(0, &[0xB0, 64, 127]);
        source.received(&packets).expect("send");
        let event = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("event arrives after the return");
        assert!(matches!(
            event,
            midicore_proto::MidiEvent::ControlChange { .. }
        ));
    }

    /// The tests that create CoreMIDI clients and endpoints run one at a
    /// time: side by side in one process, one test's endpoints were seen to
    /// vanish from the source list mid-test.
    fn coremidi_lock() -> std::sync::MutexGuard<'static, ()> {
        init();
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Bytes sent to a destination by name arrive: a virtual destination
    /// stands in for the pedal.
    #[test]
    fn an_output_sends_to_a_destination_by_name() {
        let _serial = coremidi_lock();
        let name = format!("midicore-test-dest-{}", std::process::id());
        let (tx, rx) = mpsc::channel();
        let client = Client::new("midicore-test-receiver").expect("client");
        let _dest = client
            .virtual_destination(&name, move |packets: &PacketList| {
                for p in packets.iter() {
                    let _ = tx.send(p.data().to_vec());
                }
            })
            .expect("virtual destination");
        let out = CoreMidiOutput::open("midicore-test-out").expect("output");
        assert!(
            within(|| out.send_to(&name, &[0x90, 2, 127]) == 1),
            "the destination is found"
        );
        let got = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("bytes arrive");
        assert_eq!(got, vec![0x90, 2, 127]);
        assert_eq!(out.send_to("no-such-device", &[0x80, 2, 0]), 0);
    }

    /// Whether `cond` holds within 3 s — CoreMIDI applies a property change
    /// asynchronously, so a fixed wait is a race.
    fn within(cond: impl Fn() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if cond() {
                return true;
            }
            std::thread::sleep(RESCAN / 2);
        }
        cond()
    }

    /// Run as a child process by the next test (`MIDICORE_CHILD_SOURCE`
    /// set): publish a virtual source from *another process*, for a while.
    /// A no-op in a normal run.
    #[test]
    fn child_publishes_a_source() {
        let Ok(name) = std::env::var("MIDICORE_CHILD_SOURCE") else {
            return;
        };
        let client = Client::new("midicore-test-child").expect("client");
        let _source = client.virtual_source(&name).expect("virtual source");
        std::thread::sleep(Duration::from_secs(4));
    }

    /// A device that appears after the input opened — a pedal switched on
    /// once the rig is up — is connected. It lives in another process, as
    /// hardware does: CoreMIDI only tells a process about devices coming
    /// and going through a run loop, and a thread that only slept between
    /// rescans read the same stale source list forever.
    #[test]
    fn a_source_appearing_later_in_another_process_is_connected() {
        let _serial = coremidi_lock();
        let name = format!("midicore-test-later-{}", std::process::id());
        let input = CoreMidiInput::open(
            InputConfig::new("midicore-test")
                .selecting(vec![PortSelector::NameContains(name.clone())]),
            |_| {},
        )
        .expect("CoreMIDI reachable");
        assert!(input.ports_named().is_empty());
        let mut child = std::process::Command::new(std::env::current_exe().expect("test binary"))
            .args(["tests::child_publishes_a_source", "--exact", "--nocapture"])
            .env("MIDICORE_CHILD_SOURCE", &name)
            .spawn()
            .expect("spawn the child");
        let seen = within(|| input.ports_named() == vec![name.clone()]);
        let _ = child.wait();
        assert!(seen, "a source that appeared later was connected");
    }

    impl CoreMidiInput {
        fn ports_named(&self) -> Vec<String> {
            self.connected().into_iter().map(|p| p.name).collect()
        }
    }
}
