//! The MIDI **attach lifecycle** — one helper per pattern, so no rig
//! re-derives it.
//!
//! Every rig that plays from hardware MIDI runs the same dance: map a stored
//! port name to a [`PortSelector`], **drop the old handle before opening the
//! new one**, wire the monitor tap into the live-MIDI sink, and re-run the
//! whole thing when the port set changes (hot-plug). That choreography was
//! copy-pasted across the rigs — with drift — so it lives here now:
//!
//! - [`reattach`] — the full drop→select→open→store lifecycle for a rig that
//!   holds a [`MidiInput`](midicore_midir::MidiInput) handle.
//! - [`tap_sink`] / [`tap_sink_transformed`] — the monitor-tap →
//!   live-MIDI-sink wiring handed to `MidiInput::open`.
//! - [`rescan_stream`] — hot-plug reopen for a polled
//!   [`MidiStream`](midicore_midir::MidiStream) (drop-first, omni).
//!
//! ## Why drop-before-open is an invariant
//!
//! Opening the replacement connection while the old one is still alive
//! doubles ALSA-seq client usage per swap and can exhaust the kernel's queue
//! pool (seen live as `snd_seq_alloc_named_queue` OOM panics inside midir).
//! Every helper here releases the old OS clients *first*.

// r[impl primitives.midi.attach]

use std::sync::{Arc, Mutex};

use midicore_proto::{MidiEvent, PortSelector, TimedEvent};

use crate::monitor::MidiMonitor;
use crate::selector_for;

/// Attach (or re-attach) a rig's MIDI input, encoding the whole lifecycle:
///
/// 1. `drop_old` runs **first**, releasing the previous handle's OS clients
///    (the ALSA-seq queue-exhaustion invariant — see module docs).
/// 2. The stored `port` name maps to a selector via [`selector_for`]
///    (name → `NameContains`, none → `All` omni).
/// 3. `open` performs the rig-specific open (typically
///    `rig.attach_midi(sel)`), under whatever locking the rig needs.
///    `Ok(None)` means "nothing to attach to" (rig not running) — a silent
///    no-op, matching every call site's behavior.
/// 4. `store(handle)` stows the new handle; success/failure is logged
///    uniformly under `rig` (e.g. `"keys rig"`).
///
/// Returns `true` when a new handle was attached.
pub fn reattach<H>(
    rig: &str,
    port: Option<&str>,
    drop_old: impl FnOnce(),
    open: impl FnOnce(PortSelector) -> eyre::Result<Option<H>>,
    store: impl FnOnce(H),
) -> bool {
    // Drop the old handle BEFORE opening the new one (see module docs).
    drop_old();
    let sel = selector_for(port);
    match open(sel) {
        Ok(Some(handle)) => {
            store(handle);
            let which = port
                .filter(|p| !p.is_empty())
                .unwrap_or("omni (all inputs)");
            tracing::info!(port = %which, "{rig}: MIDI attached");
            true
        }
        Ok(None) => false,
        Err(e) => {
            tracing::error!("{rig}: MIDI attach failed: {e}");
            false
        }
    }
}

/// The monitor-tap → live-MIDI-sink wiring: build a `MidiInput::open` sink
/// that records every event into `monitor` (so a UI can confirm MIDI is
/// arriving), then forwards it full-fidelity to `forward` (the rig's
/// live-MIDI entry, e.g. `daw.push_live_midi`).
pub fn tap_sink<F>(monitor: MidiMonitor, forward: F) -> impl Fn(TimedEvent) + Send + Clone + 'static
where
    F: Fn(MidiEvent) + Send + Clone + 'static,
{
    move |t: TimedEvent| {
        monitor.record(&t.event);
        forward(t.event);
    }
}

/// Like [`tap_sink`], but runs every event through `transform` first,
/// forwarding each produced event (this is where a drum-map converter
/// inserts). The monitor taps the *original* (pre-transform) event so a UI
/// shows what the hardware actually sent.
///
/// `transform` is shared behind a mutex (the midir sink is `Fn + Clone`) and
/// runs on the MIDI callback thread — keep it allocation-light and lock-free
/// of the audio thread.
pub fn tap_sink_transformed<T, F>(
    monitor: MidiMonitor,
    transform: T,
    forward: F,
) -> impl Fn(TimedEvent) + Send + Clone + 'static
where
    T: FnMut(MidiEvent) -> Vec<MidiEvent> + Send + 'static,
    F: Fn(MidiEvent) + Send + Clone + 'static,
{
    let transform = Arc::new(Mutex::new(transform));
    move |t: TimedEvent| {
        monitor.record(&t.event);
        let outs = match transform.lock() {
            Ok(mut f) => f(t.event),
            Err(_) => return,
        };
        for ev in outs {
            forward(ev);
        }
    }
}

/// Hot-plug rescan for a polled omni [`MidiStream`](midicore_midir::MidiStream):
/// when the stream is missing or the input-port set changed since
/// `known_ports`, drop the old stream **first** (see module docs), then reopen
/// across all ports. Call at hot-plug cadence from a pump loop; a mid-set
/// replug comes back on its own.
pub fn rescan_stream(
    stream: &mut Option<midicore_midir::MidiStream>,
    known_ports: &mut Vec<String>,
) {
    let ports = midicore_midir::input_ports();
    if stream.is_some() && ports == *known_ports {
        return;
    }
    // Release the old stream's OS clients BEFORE opening anew.
    *stream = None;
    *stream = if ports.is_empty() {
        None
    } else {
        midicore_midir::MidiStream::open_all()
    };
    match &stream {
        Some(s) => tracing::info!("MIDI capture active on {:?}", s.opened),
        None if ports.is_empty() && !known_ports.is_empty() => {
            tracing::warn!("all MIDI inputs gone — waiting for replug")
        }
        None => {}
    }
    *known_ports = ports;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// The lifecycle order is the whole point: drop the old handle, *then*
    /// open, then store.
    #[test]
    fn reattach_drops_before_opening() {
        let log = RefCell::new(Vec::new());
        let attached = reattach(
            "test rig",
            Some("mioXM"),
            || log.borrow_mut().push("drop"),
            |sel| {
                assert_eq!(sel, PortSelector::NameContains("mioXM".into()));
                log.borrow_mut().push("open");
                Ok(Some(42u32))
            },
            |h| {
                assert_eq!(h, 42);
                log.borrow_mut().push("store");
            },
        );
        assert!(attached);
        assert_eq!(*log.borrow(), ["drop", "open", "store"]);
    }

    /// No stored name → omni (`All`); `Ok(None)` from open is a silent no-op
    /// (rig not running) but the old handle is still dropped.
    #[test]
    fn reattach_maps_missing_port_to_omni_and_tolerates_no_rig() {
        let dropped = RefCell::new(false);
        let attached = reattach(
            "test rig",
            None,
            || *dropped.borrow_mut() = true,
            |sel| {
                assert_eq!(sel, PortSelector::All);
                Ok(None::<u32>)
            },
            |_| panic!("nothing to store"),
        );
        assert!(!attached);
        assert!(*dropped.borrow());
    }

    #[test]
    fn reattach_logs_but_survives_open_errors() {
        let attached = reattach(
            "test rig",
            Some("gone"),
            || {},
            |_| Err::<Option<u32>, _>(eyre::eyre!("port vanished")),
            |_| panic!("nothing to store"),
        );
        assert!(!attached);
    }

    #[test]
    fn tap_sink_records_then_forwards() {
        let monitor = MidiMonitor::new();
        let forwarded = Arc::new(Mutex::new(Vec::new()));
        let fwd = forwarded.clone();
        let sink = tap_sink(monitor.clone(), move |ev| fwd.lock().unwrap().push(ev));
        let ev = MidiEvent::ControlChange {
            channel: midicore_proto::Channel::new(0),
            controller: midicore_proto::ControllerNumber::new(101),
            value: midicore_proto::ControllerValue::new(127),
        };
        sink(TimedEvent {
            timestamp_us: 0,
            event: ev.clone(),
        });
        assert_eq!(monitor.count(), 1);
        assert_eq!(*forwarded.lock().unwrap(), vec![ev]);
    }

    /// The transform multiplies/rewrites events; the monitor sees the
    /// original, the sink sees the transformed set.
    #[test]
    fn tap_sink_transformed_taps_original_forwards_converted() {
        let monitor = MidiMonitor::new();
        let forwarded = Arc::new(Mutex::new(Vec::new()));
        let fwd = forwarded.clone();
        let original = MidiEvent::ControlChange {
            channel: midicore_proto::Channel::new(0),
            controller: midicore_proto::ControllerNumber::new(1),
            value: midicore_proto::ControllerValue::new(64),
        };
        let converted = MidiEvent::ControlChange {
            channel: midicore_proto::Channel::new(9),
            controller: midicore_proto::ControllerNumber::new(2),
            value: midicore_proto::ControllerValue::new(65),
        };
        let conv = converted.clone();
        let sink = tap_sink_transformed(
            monitor.clone(),
            move |_| vec![conv.clone(), conv.clone()],
            move |ev| fwd.lock().unwrap().push(ev),
        );
        sink(TimedEvent {
            timestamp_us: 0,
            event: original.clone(),
        });
        assert_eq!(monitor.recent(), vec![original]);
        assert_eq!(
            *forwarded.lock().unwrap(),
            vec![converted.clone(), converted]
        );
    }
}
