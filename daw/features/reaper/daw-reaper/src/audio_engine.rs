//! `impl AudioEngine for Reaper` — sync trait + REAPER's audio C API.

use crate::safe_wrappers::audio as sw;
use daw_proto::{
    AudioEngine, AudioEngineState, AudioInputChannel, AudioInputInfo, AudioLatency, DawResult,
};
use reaper_high::Reaper;

impl AudioEngine for crate::Reaper {
    fn state(&self) -> AudioEngineState {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();
        let is_running = medium.audio_is_running();
        let is_prebuffer = medium.low().Audio_IsPreBuffer() != 0;
        let latency = get_audio_latency_internal(medium);
        AudioEngineState {
            is_running,
            is_prebuffer,
            latency,
        }
    }

    fn latency(&self) -> AudioLatency {
        let medium = Reaper::get().medium_reaper();
        get_audio_latency_internal(medium)
    }

    fn output_latency_seconds(&self) -> f64 {
        let medium = Reaper::get().medium_reaper();
        if !medium.audio_is_running() {
            return 0.0;
        }
        medium.low().GetOutputLatency()
    }

    fn is_running(&self) -> bool {
        Reaper::get().medium_reaper().audio_is_running()
    }

    fn inputs(&self) -> AudioInputInfo {
        let reaper = Reaper::get();
        let medium = reaper.medium_reaper();
        let low = medium.low();
        let device_name = sw::get_audio_device_info(low, c"IDENT_IN", 256).unwrap_or_default();
        let num_inputs = low.GetNumAudioInputs() as u32;
        let channels: Vec<AudioInputChannel> = (0..num_inputs)
            .map(|i| {
                let name = medium.get_input_channel_name(i, |cstr| {
                    cstr.map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| format!("Input {}", i + 1))
                });
                AudioInputChannel { index: i, name }
            })
            .collect();
        AudioInputInfo {
            device_name,
            channels,
        }
    }

    fn init(&self) -> DawResult<()> {
        Reaper::get().medium_reaper().low().Audio_Init();
        Ok(())
    }

    fn quit(&self) -> DawResult<()> {
        Reaper::get().medium_reaper().low().Audio_Quit();
        Ok(())
    }
}

/// MUST be called on main thread.
pub(crate) fn get_audio_latency_internal(medium: &reaper_medium::Reaper) -> AudioLatency {
    let lat_result = medium.get_input_output_latency();
    let sample_rate = get_sample_rate(medium);
    let output_seconds = if sample_rate > 0 {
        lat_result.output_latency as f64 / sample_rate as f64
    } else {
        0.0
    };
    AudioLatency {
        input_samples: lat_result.input_latency,
        output_samples: lat_result.output_latency,
        output_seconds,
        sample_rate,
    }
}

pub(crate) fn get_sample_rate(medium: &reaper_medium::Reaper) -> u32 {
    let low = medium.low();
    if let Some(srate_str) = sw::get_audio_device_info(low, c"SRATE", 64)
        && let Ok(rate) = srate_str.parse::<u32>()
        && rate > 0
    {
        return rate;
    }
    let reaper = Reaper::get();
    let project = reaper.current_project();
    let use_custom = sw::get_set_project_info(low, project.raw(), c"PROJECT_SRATE_USE", 0.0, false);
    if use_custom > 0.0 {
        let rate = sw::get_set_project_info(low, project.raw(), c"PROJECT_SRATE", 0.0, false);
        if rate > 0.0 {
            return rate as u32;
        }
    }
    44100
}
