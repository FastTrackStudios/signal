//! Shared meter/status **pump** for rig backends.
//!
//! Every rig backend runs one background thread that, at a fixed cadence,
//! publishes per-tick telemetry (meters + recent MIDI), publishes full status
//! only when the transport running-state flips, and periodically rescans MIDI
//! ports for hot-plug. That loop scaffold was copy-pasted per rig — and drifted
//! (one rig missed the hot-plug rescan, another the once-start guard, each
//! declared its own interval constant). This is the one scaffold; a rig
//! implements [`MeterPumpSource`] with only its own publishing.
//!
//! (The guitar rig keeps its own richer pump — per-tick `catch_unwind`
//! survival, debounced auto-saves, footswitch CC mapping — which is business
//! logic, not this scaffold.)

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Cadence of the pump loop (~30 Hz).
pub const PUMP_INTERVAL: Duration = Duration::from_millis(33);

/// Hot-plug rescan cadence, in ticks (~2 s at [`PUMP_INTERVAL`]).
const PORT_SCAN_TICKS: u32 = 60;

/// The rig-specific hooks the shared pump loop drives. `Clone + Send` because
/// the pump owns a clone of the source on its thread.
pub trait MeterPumpSource: Clone + Send + 'static {
    /// True while the audio engine is open/running.
    fn is_running(&self) -> bool;

    /// The once-start guard, so the pump spawns at most one thread per source.
    fn pump_started(&self) -> &AtomicBool;

    /// Run once per tick, before the running check (e.g. decay a light guide).
    /// Default: nothing.
    fn on_tick(&self) {}

    /// Publish full state on a transport running-state edge (started/stopped).
    fn on_running_edge(&self, running: bool);

    /// Publish per-tick telemetry (meters, recent MIDI) while running.
    fn on_running_tick(&self);

    /// Sorted MIDI input port names, for hot-plug detection. Default: no
    /// hot-plug (an always-empty set never fires `on_midi_ports_changed`).
    fn midi_ports(&self) -> Vec<String> {
        Vec::new()
    }

    /// The MIDI port set changed *while running* — re-attach. Default: nothing.
    fn on_midi_ports_changed(&self, _ports: &[String]) {}
}

/// Spawn the meter/status pump for `source` on a named thread. Idempotent: the
/// source's [`pump_started`](MeterPumpSource::pump_started) guard ensures at
/// most one pump per source.
pub fn spawn_meter_pump<S: MeterPumpSource>(name: &'static str, source: S) {
    if source.pump_started().swap(true, Ordering::SeqCst) {
        return; // already running
    }
    let spawned = std::thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            let mut last_running = false;
            let mut known_ports = sorted(source.midi_ports());
            let mut tick: u32 = 0;
            loop {
                std::thread::sleep(PUMP_INTERVAL);
                source.on_tick();
                let running = source.is_running();

                // Hot-plug: rescan periodically; on a change re-attach (only
                // while running — a stopped rig has nothing to attach to).
                tick = tick.wrapping_add(1);
                if tick % PORT_SCAN_TICKS == 0 {
                    let now = sorted(source.midi_ports());
                    if now != known_ports {
                        known_ports = now;
                        if running {
                            source.on_midi_ports_changed(&known_ports);
                        }
                    }
                }

                // Transport transitions are rare — full status only on the edge.
                if running != last_running {
                    last_running = running;
                    source.on_running_edge(running);
                }
                if !running {
                    continue;
                }
                source.on_running_tick();
            }
        });
    if let Err(e) = spawned {
        tracing::error!("meter pump '{name}': spawn failed: {e}");
    }
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}
