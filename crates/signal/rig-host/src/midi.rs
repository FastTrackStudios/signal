//! Instrumented MIDI attach — one wide event per attempt, for every rig.
//!
//! [`midicore::attach::reattach`] owns the *lifecycle* (drop-before-open,
//! sink wiring). This owns the *telemetry*, because the interesting failure
//! is not one attach going wrong — it is the shape across attaches:
//!
//! - A rig re-attaching in a loop opens its whole port set again each time.
//!   With every rig defaulting to omni that is ~23 OS clients per rig per
//!   cycle, and a runaway was observed climbing 41 → 291 live clients until
//!   enumeration returned nothing and the engine died. Nothing in a single
//!   attach looks wrong; only `seq` and `since_last_ms` show it.
//! - An attach that opens zero ports leaves the rig running and inaudible,
//!   which reads to a player as "my keyboard is broken".
//!
//! So each attempt emits exactly one event carrying who, why, how many, and
//! how long since last — never a line per port. `trigger` is the field that
//! names the caller, because "which code path is re-attaching" was the
//! question that took a night to answer by counting PipeWire clients.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use midicore_proto::PortSelector;

/// Why an attach is happening. Distinguishing these is the whole point: a
/// storm looks identical to healthy hot-plug until you can see the trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachTrigger {
    /// The rig opened (start, preset load, rebuild).
    RigOpen,
    /// The hot-plug pump saw the port set change.
    PortsChanged,
    /// A user picked a port.
    PortSelected,
    /// The caller has not been classified yet. Still worth emitting: the
    /// sequence and gap fields detect a storm on their own, and the trigger
    /// only narrows down which path caused it.
    Unspecified,
}

impl AttachTrigger {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RigOpen => "rig_open",
            Self::PortsChanged => "ports_changed",
            Self::PortSelected => "port_selected",
            Self::Unspecified => "unspecified",
        }
    }
}

/// Per-rig attach history, so an event can report its own rate.
static HISTORY: Mutex<Option<HashMap<&'static str, (u64, Instant)>>> = Mutex::new(None);

fn bump(rig: &'static str) -> (u64, Option<u128>) {
    let Ok(mut guard) = HISTORY.lock() else {
        return (0, None);
    };
    let map = guard.get_or_insert_with(HashMap::new);
    let now = Instant::now();
    match map.get_mut(rig) {
        Some((seq, last)) => {
            *seq += 1;
            let since = now.duration_since(*last).as_millis();
            *last = now;
            (*seq, Some(since))
        }
        None => {
            map.insert(rig, (1, now));
            (1, None)
        }
    }
}

/// Attach `rig`'s MIDI and emit one wide event describing the attempt.
///
/// A thin wrapper over [`midicore::attach::reattach`] — same arguments, same
/// semantics, same return — so no rig has to re-derive either the lifecycle
/// or the instrumentation. `ports_seen` is passed in rather than enumerated
/// here: the caller already has the list, and enumerating twice is exactly
/// the kind of duplicate OS work that started the runaway.
pub fn reattach_instrumented<H>(
    rig: &'static str,
    trigger: AttachTrigger,
    port: Option<&str>,
    ports_seen: usize,
    drop_old: impl FnOnce(),
    open: impl FnOnce(PortSelector) -> eyre::Result<Option<H>>,
    store: impl FnOnce(H),
) -> bool {
    let (seq, since_last_ms) = bump(rig);
    let started = Instant::now();
    let attached = midicore::attach::reattach(rig, port, drop_old, open, store);

    let selector = if port.is_some_and(|p| !p.is_empty()) {
        "named"
    } else {
        "omni"
    };
    let outcome = if attached {
        "attached"
    } else if ports_seen == 0 {
        "no_ports"
    } else {
        "not_attached"
    };

    // ONE event per attempt. Everything a query needs is here: the trigger
    // that fired it, the sequence and gap that expose a storm, and the port
    // count that exposes a deaf rig.
    tracing::info!(
        midi.rig = rig,
        midi.trigger = trigger.as_str(),
        midi.selector = selector,
        midi.ports_seen = ports_seen,
        midi.outcome = outcome,
        midi.attach_seq = seq,
        midi.since_last_ms = since_last_ms,
        midi.elapsed_ms = started.elapsed().as_millis(),
        "midi attach"
    );

    // Alertable conditions get exactly one warn each — the allowed path
    // rides the event above and nothing else.
    if outcome == "no_ports" {
        tracing::warn!(
            midi.rig = rig,
            "MIDI attach found no input ports — the rig is running but cannot be played"
        );
    }
    if since_last_ms.is_some_and(|ms| ms < 1_000) {
        tracing::warn!(
            midi.rig = rig,
            midi.trigger = trigger.as_str(),
            midi.attach_seq = seq,
            midi.since_last_ms = since_last_ms,
            "MIDI re-attaching faster than once a second — each cycle reopens the \
             whole port set, which exhausts OS MIDI clients"
        );
    }
    attached
}

/// Instrumented [`midicore::attach::rescan_stream`] — the polled hot-plug
/// path, which reopens the whole port set whenever it fires.
///
/// This one is worth watching more closely than the attach path, because its
/// reopen condition is `ports != *known_ports`: an **order-sensitive** `Vec`
/// compare. JACK does not promise a stable enumeration order, so a port list
/// that merely *reordered* is indistinguishable from one that changed, and
/// every reorder costs a full drop-and-reopen of every port. Polled every
/// couple of seconds, that is a plausible engine-killer, so the event
/// separates the two cases explicitly: `reordered_only=true` means we paid
/// for a reopen that changed nothing.
///
/// Adds no enumeration of its own — `rescan_stream` writes the fresh list
/// back into `known_ports`, so comparing before against after is exact and
/// free. Instrumentation that doubled the OS work here would be making the
/// very problem it is measuring worse.
pub fn rescan_stream_instrumented(
    rig: &'static str,
    stream: &mut Option<midicore::midir::MidiStream>,
    known_ports: &mut Vec<String>,
) {
    let had_stream = stream.is_some();
    let before = known_ports.clone();

    // Guard a transient EMPTY enumeration, which `rescan_stream` treats as
    // "all MIDI inputs gone" and answers by dropping the live stream.
    //
    // Under load that reading is usually wrong. Opening a rig's omni set is
    // ~23 OS clients and takes seconds; while several rigs do that at once,
    // `input_ports()` has been observed returning an empty (or short) list
    // for a moment even though every device is still plugged in. Dropping on
    // that costs a full reopen, which makes the next enumeration worse — the
    // churn feeds itself.
    //
    // The rigs' own `on_midi_ports_changed` already refuses to act on an
    // empty scan; this brings the polled path in line. A genuine unplug is
    // not lost, only deferred to the next poll, where a *stable* empty list
    // still reads as empty.
    if had_stream && !before.is_empty() && midicore::pipewire::input_ports().is_empty() {
        tracing::warn!(
            midi.rig = rig,
            midi.ports_before = before.len(),
            "MIDI enumeration came back EMPTY while a stream was live — keeping the \
             existing attachment rather than dropping every port on what is almost \
             certainly a transient scan failure"
        );
        return;
    }

    midicore::attach::rescan_stream(stream, known_ports);

    // `rescan_stream` reopens exactly when the stream was absent or the list
    // differs, and it writes the fresh list back — so this reproduces its
    // decision without asking the OS anything a second time.
    let changed = before != *known_ports;
    if !changed && had_stream {
        return; // The common case: nothing happened, so say nothing.
    }

    let (mut a, mut b) = (before.clone(), known_ports.clone());
    a.sort();
    b.sort();
    let reordered_only = changed && a == b;

    let (seq, since_last_ms) = bump(rig);
    tracing::info!(
        midi.rig = rig,
        midi.trigger = AttachTrigger::PortsChanged.as_str(),
        midi.selector = "omni",
        midi.ports_seen = known_ports.len(),
        midi.ports_before = before.len(),
        midi.outcome = if stream.is_some() { "attached" } else { "no_ports" },
        midi.reordered_only = reordered_only,
        midi.had_stream = had_stream,
        midi.attach_seq = seq,
        midi.since_last_ms = since_last_ms,
        "midi rescan"
    );

    if reordered_only {
        tracing::warn!(
            midi.rig = rig,
            midi.ports_seen = known_ports.len(),
            "MIDI ports only REORDERED, yet every port was dropped and reopened — \
             the rescan compares an ordered list, so an unstable enumeration order \
             costs a full reopen cycle each poll"
        );
    }
    if since_last_ms.is_some_and(|ms| ms < 1_000) {
        tracing::warn!(
            midi.rig = rig,
            midi.attach_seq = seq,
            midi.since_last_ms = since_last_ms,
            "MIDI rescan reopening faster than once a second — this exhausts OS \
             MIDI clients"
        );
    }
}
