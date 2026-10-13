//! Audio engine service — state, latency, devices, init/quit.
//!
//! Global to the DAW instance — no `ProjectContext`.

use super::{AudioEngineState, AudioInputInfo, AudioLatency};
use crate::DawResult;

#[architect::rpc]
pub trait AudioEngine {
    fn state(&self) -> AudioEngineState;
    fn latency(&self) -> AudioLatency;
    /// Convenience: just the output latency in seconds (0.0 when the
    /// engine isn't running). Useful for visual sync compensation.
    fn output_latency_seconds(&self) -> f64;
    fn is_running(&self) -> bool;
    fn inputs(&self) -> AudioInputInfo;

    /// Open all audio + MIDI devices.
    fn init(&self) -> DawResult<()>;
    /// Close all audio + MIDI devices.
    fn quit(&self) -> DawResult<()>;
}
