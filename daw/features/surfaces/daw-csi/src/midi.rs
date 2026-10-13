//! Hardware MIDI transport for the surface driver (midir-backed:
//! ALSA on Linux, CoreMIDI on macOS, WinMM on Windows).
//!
//! The driver owns one [`SurfacePort`] — an opened input+output pair
//! found by case-insensitive substring match on the port name
//! ("x-touch" finds "X-Touch X-TOUCH MIDI 1"). Input bytes arrive on
//! a tokio channel (midir's callback runs on an OS thread; the
//! unbounded sender is the thread-safe hop into async land).

use eyre::{WrapErr, eyre};
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use tokio::sync::mpsc;

/// One surface's opened MIDI in/out pair.
pub struct SurfacePort {
    /// Held for its Drop — closing the connection stops the callback.
    _input: MidiInputConnection<()>,
    output: MidiOutputConnection,
    /// Raw incoming MIDI messages (already split by midir).
    pub rx: mpsc::UnboundedReceiver<Vec<u8>>,
    /// The matched port name (for logs / UI).
    pub name: String,
}

impl SurfacePort {
    /// Enumerate and open the first input+output pair whose port name
    /// contains `device_match` (case-insensitive).
    pub fn open(device_match: &str) -> eyre::Result<Self> {
        let needle = device_match.to_lowercase();

        let input = MidiInput::new("daw-csi-in").wrap_err("create MIDI input")?;
        let in_port = input
            .ports()
            .into_iter()
            .find(|p| {
                input
                    .port_name(p)
                    .map(|n| n.to_lowercase().contains(&needle))
                    .unwrap_or(false)
            })
            .ok_or_else(|| eyre!("no MIDI input matching '{device_match}'"))?;
        let name = input.port_name(&in_port).unwrap_or_default();

        let (tx, rx) = mpsc::unbounded_channel();
        let connection = input
            .connect(
                &in_port,
                "daw-csi",
                move |_timestamp, bytes, _| {
                    // OS-thread callback → async driver. Unbounded:
                    // surface gestures are low-rate, never block here.
                    let _ = tx.send(bytes.to_vec());
                },
                (),
            )
            .map_err(|e| eyre!("connect MIDI input: {e}"))?;

        let output = MidiOutput::new("daw-csi-out").wrap_err("create MIDI output")?;
        let out_port = output
            .ports()
            .into_iter()
            .find(|p| {
                output
                    .port_name(p)
                    .map(|n| n.to_lowercase().contains(&needle))
                    .unwrap_or(false)
            })
            .ok_or_else(|| eyre!("no MIDI output matching '{device_match}'"))?;
        let output = output
            .connect(&out_port, "daw-csi")
            .map_err(|e| eyre!("connect MIDI output: {e}"))?;

        tracing::info!(port = %name, "daw-csi: surface connected");
        Ok(Self {
            _input: connection,
            output,
            rx,
            name,
        })
    }

    /// List all MIDI input port names (for `--list` style UX).
    pub fn list_inputs() -> Vec<String> {
        MidiInput::new("daw-csi-enum")
            .map(|m| {
                m.ports()
                    .iter()
                    .filter_map(|p| m.port_name(p).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Send one raw MIDI message (short or sysex).
    pub fn send(&mut self, bytes: &[u8]) {
        if let Err(e) = self.output.send(bytes) {
            tracing::warn!("daw-csi: MIDI send failed: {e}");
        }
    }
}
