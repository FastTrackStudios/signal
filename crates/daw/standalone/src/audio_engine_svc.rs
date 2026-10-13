//! `impl AudioEngine for Standalone` — stub.

use daw_proto::{AudioEngine, AudioEngineState, AudioInputInfo, AudioLatency, DawResult};

use crate::sync::Standalone;

impl AudioEngine for Standalone {
    fn state(&self) -> AudioEngineState {
        AudioEngineState::default()
    }
    fn latency(&self) -> AudioLatency {
        AudioLatency::default()
    }
    fn output_latency_seconds(&self) -> f64 {
        0.0
    }
    fn is_running(&self) -> bool {
        false
    }
    fn inputs(&self) -> AudioInputInfo {
        AudioInputInfo::default()
    }
    fn init(&self) -> DawResult<()> {
        Ok(())
    }
    fn quit(&self) -> DawResult<()> {
        Ok(())
    }
}
