//! `StandaloneAudioEngine` — sync audio-engine sub-handle.
//!
//! Returns whatever was last seeded via `Standalone::seed_audio_engine_state`.
//! `init` / `quit` flip the `is_running` flag in the cached state so callers
//! can observe lifecycle transitions.

use daw_proto::{AudioEngineState, AudioInputInfo, AudioLatency, DawResult, sync::AudioEngine};

use super::daw::Standalone;

pub struct StandaloneAudioEngine<'a> {
    daw: &'a Standalone,
}

impl<'a> StandaloneAudioEngine<'a> {
    pub(crate) fn new(daw: &'a Standalone) -> Self {
        Self { daw }
    }
}

impl<'a> AudioEngine for StandaloneAudioEngine<'a> {
    fn state(&self) -> DawResult<AudioEngineState> {
        let s = self.daw.state.lock().expect("standalone state poisoned");
        Ok(s.audio_engine.clone())
    }

    fn latency(&self) -> AudioLatency {
        let s = self.daw.state.lock().expect("standalone state poisoned");
        s.audio_latency.clone()
    }

    fn is_running(&self) -> bool {
        let s = self.daw.state.lock().expect("standalone state poisoned");
        s.audio_engine.is_running
    }

    fn inputs(&self) -> Vec<AudioInputInfo> {
        let s = self.daw.state.lock().expect("standalone state poisoned");
        s.audio_inputs.clone()
    }

    fn init(&self) -> DawResult<()> {
        let mut s = self.daw.state.lock().expect("standalone state poisoned");
        s.audio_engine.is_running = true;
        Ok(())
    }

    fn quit(&self) -> DawResult<()> {
        let mut s = self.daw.state.lock().expect("standalone state poisoned");
        s.audio_engine.is_running = false;
        Ok(())
    }
}
