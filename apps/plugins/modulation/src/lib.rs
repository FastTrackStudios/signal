//! FTS Modulation — CLAP/VST3 multi-mode modulation plugin.
//!
//! Five circuits on a rail — Chorus, Flanger, Vibrato, Tremolo, Wah — and
//! fifteen profiles inside them. What each one *is* lives in
//! [`modulation_profiles`]: which of the three chains it runs, which engine,
//! and where its own controls rest. Nothing about a circuit lives in this
//! file.
//!
//! That is the point of the crate split, and it is worth saying what it
//! fixed. This shell used to carry a five-value `Mode` enum, hardcode
//! `EngineType::Cubic`, and never call `set_engine` — its own doc note said
//! so. Four of the five chorus engines (BBD, Tape, Orbit, Juno) had no way to
//! be heard at all; the tremolo's groove, feel, accent and saturation and the
//! wah's resonance, stages and sensitivity were fields nothing wrote. Seven
//! parameters over about twenty-five real controls.
//!
//! [`modulation_profiles::apply`] is the only thing that knows how a knob
//! reaches the DSP — which is also why the editor's shape is the engine's own
//! movement rather than a drawing of one.

use audiocore_core::prelude::*;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use audiocore_dsp::{AudioConfig, Processor};
use modulation::chorus::chain::ChorusChain;
use modulation::trem::chain::TremChain;
use modulation::wah::chain::WahChain;
use modulation_profiles::Chain;
use modulation_ui::params::{ModParams, ModUiState};

const PLUGIN_NAME: &str = "FTS Modulation";

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsModulation {
    params: Arc<ModParams>,
    ui_state: Arc<ModUiState>,
    editor_state: Arc<DioxusState>,
    /// All three chains live for the life of the plugin: switching families
    /// is a dispatch, not a rebuild, and coming back to one finds it as you
    /// left it.
    chorus: ChorusChain,
    trem: TremChain,
    wah: WahChain,
    /// Which chain the last block ran, so a family switch resets the incoming
    /// one instead of resuming from stale state.
    current: Chain,
    sample_rate: f64,
    max_buffer_size: usize,
}

impl Default for FtsModulation {
    fn default() -> Self {
        // The modulators' clocks are set by `modulation_profiles::apply`, not
        // here — a chain configured in two places is how the editor ended up
        // drawing a still line for the tremolo while the plugin ran fine.
        Self {
            params: Arc::new(ModParams::default()),
            ui_state: Arc::new(ModUiState::default()),
            // The editor sizes itself — see modulation_ui::control_view.
            editor_state: DioxusState::new(|| {
                (
                    modulation_ui::control_view::EDITOR_W,
                    modulation_ui::control_view::EDITOR_H,
                )
            })
            .with_resize_hint(modulation_ui::control_view::resize_hint()),
            chorus: ChorusChain::new(),
            trem: TremChain::new(),
            wah: WahChain::new(),
            current: Chain::Delay,
            sample_rate: 48_000.0,
            max_buffer_size: 512,
        }
    }
}

impl FtsModulation {
    /// Point the engines at the resolved profile.
    ///
    /// Resolved through the *persisted id* rather than the automatable index,
    /// so growing the rail never repoints an old session at a different
    /// circuit — same contract the reverb and the saturator keep.
    ///
    /// Returns the chain to run this block.
    fn sync_params(&mut self) -> Chain {
        let profile = self.params.resolved_profile();
        let chain = profile.voicing.circuit.chain();

        // Reset the incoming chain on a family switch so it starts clean
        // rather than resuming a half-finished sweep.
        if chain != self.current {
            self.current = chain;
            match chain {
                Chain::Delay => self.chorus.reset(),
                Chain::Tremolo => self.trem.reset(),
                Chain::Wah => self.wah.reset(),
            }
        }

        modulation_profiles::apply(
            profile,
            &self.params.controls(),
            &mut self.chorus,
            &mut self.trem,
            &mut self.wah,
        );
        chain
    }
}

impl Plugin for FtsModulation {
    const NAME: &'static str = PLUGIN_NAME;
    const VENDOR: &'static str = "FastTrackStudio";
    const URL: &'static str = "https://fasttrackstudio.com";
    const EMAIL: &'static str = "";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    /// Audio effect: stereo in, stereo out.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    type Editor = audiocore_core::nice_plug_dioxus::editor::DioxusEditor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        create_dioxus_editor_with_state(
            self.editor_state.clone(),
            Arc::new(modulation_ui::control_view::ModUi {
                params: self.params.clone(),
                state: self.ui_state.clone(),
            }),
            modulation_ui::control_view::App,
        )
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate as f64;
        self.max_buffer_size = buffer_config.max_buffer_size as usize;
        let config = AudioConfig {
            sample_rate: self.sample_rate,
            max_buffer_size: self.max_buffer_size,
        };
        // Land the profile before the reconfigure, so each chain retunes onto
        // the real settings rather than ramping out of a default nobody chose.
        self.sync_params();
        self.chorus.update(config);
        self.trem.update(config);
        self.wah.update(config);
        true
    }

    fn reset(&mut self) {
        self.chorus.reset();
        self.trem.reset();
        self.wah.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        if buffer.channels() < 2 {
            return ProcessStatus::Normal;
        }
        let chain = self.sync_params();
        let output = 10.0f32.powf(self.params.output.value() / 20.0);

        let mut input_peak = 0.0f32;
        let mut output_peak = 0.0f32;

        // Fixed stack chunks, f32 ↔ f64. No allocation on the audio thread.
        const CHUNK: usize = 128;
        let total = buffer.samples();
        let mut offset = 0;

        while offset < total {
            let len = (offset + CHUNK).min(total) - offset;
            let mut left = [0.0f64; CHUNK];
            let mut right = [0.0f64; CHUNK];

            {
                let channels = buffer.as_slice();
                for i in 0..len {
                    let (l, r) = (channels[0][offset + i], channels[1][offset + i]);
                    input_peak = input_peak.max(l.abs().max(r.abs()));
                    left[i] = l as f64;
                    right[i] = r as f64;
                }
            }

            match chain {
                Chain::Delay => self.chorus.process(&mut left[..len], &mut right[..len]),
                Chain::Tremolo => self.trem.process(&mut left[..len], &mut right[..len]),
                Chain::Wah => self.wah.process(&mut left[..len], &mut right[..len]),
            }

            {
                let channels = buffer.as_slice();
                for i in 0..len {
                    let (l, r) = (left[i] as f32 * output, right[i] as f32 * output);
                    output_peak = output_peak.max(l.abs().max(r.abs()));
                    channels[0][offset + i] = l;
                    channels[1][offset + i] = r;
                }
            }

            offset += len;
        }

        store_peak(&self.ui_state.input_db, input_peak);
        store_peak(&self.ui_state.out_db, output_peak);
        // Where the modulator is sitting, for the panel's playhead.
        let modulated = match chain {
            Chain::Tremolo => self.trem.modulator.output(),
            Chain::Wah => self.wah.modulator.output(),
            Chain::Delay => 0.0,
        };
        self.ui_state
            .mod_value
            .store(modulated as f32, Ordering::Relaxed);

        ProcessStatus::Normal
    }
}

/// Peak-hold in dB, falling 0.3 dB a block. The editor samples this when it
/// draws; a frame that misses an update simply draws the next one.
fn store_peak(slot: &atomic_float::AtomicF32, peak: f32) {
    let db = if peak > 0.0 {
        20.0 * peak.log10()
    } else {
        -100.0
    };
    let previous = slot.load(Ordering::Relaxed);
    slot.store(
        if db > previous { db } else { previous - 0.3 },
        Ordering::Relaxed,
    );
}

impl ClapPlugin for FtsModulation {
    const CLAP_ID: &'static str = "com.fasttrackstudio.modulation";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Five circuits: chorus, flanger, vibrato, tremolo, and wah");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Chorus,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsModulation {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsModPlugin0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Modulation];
}

nice_export_clap!(FtsModulation);
nice_export_vst3!(FtsModulation);

#[cfg(test)]
mod tests {
    use super::*;
    use audiocore_core::nice_plug::params::InternalParamMut;
    use modulation_profiles::{Circuit, EngineType};

    /// The plugin runs whatever the rail says, with no table of its own.
    ///
    /// This is the regression guard for the bug the split fixed: the shell
    /// pinned `EngineType::Cubic` and never switched, so four engines were
    /// dead code. If a profile ever stops reaching its engine again, this is
    /// what notices.
    #[test]
    fn every_profile_on_the_rail_reaches_the_engine_it_names() {
        for (index, profile) in modulation_profiles::PROFILES.iter().enumerate() {
            let mut plugin = FtsModulation::default();
            plugin.params.store_profile_id(index);
            let chain = plugin.sync_params();
            assert_eq!(chain, profile.voicing.circuit.chain(), "{}", profile.id);
            if let Circuit::Delay { engine, effect } = profile.voicing.circuit {
                assert_eq!(plugin.chorus.engine, engine, "{}", profile.id);
                assert_eq!(plugin.chorus.effect_type, effect, "{}", profile.id);
            }
        }
    }

    /// …and specifically that all five chorus engines are selected by
    /// something, in the shell rather than only in the table.
    #[test]
    fn the_shell_can_select_every_chorus_engine() {
        let mut reached = Vec::new();
        for (index, _) in modulation_profiles::PROFILES.iter().enumerate() {
            let mut plugin = FtsModulation::default();
            plugin.params.store_profile_id(index);
            plugin.sync_params();
            if !reached.contains(&plugin.chorus.engine) {
                reached.push(plugin.chorus.engine);
            }
        }
        for engine in [
            EngineType::Cubic,
            EngineType::Bbd,
            EngineType::Tape,
            EngineType::Orbit,
            EngineType::Juno,
        ] {
            assert!(
                reached.contains(&engine),
                "{engine:?} unreachable from the shell"
            );
        }
    }

    /// Silence in, silence out, on all fifteen. A modulator that idles into
    /// something is unusable however good it sounds when played.
    #[test]
    fn no_profile_makes_a_sound_out_of_nothing() {
        for index in 0..modulation_profiles::PROFILES.len() {
            let mut plugin = FtsModulation::default();
            plugin.params.store_profile_id(index);
            let chain = plugin.sync_params();
            let config = AudioConfig {
                sample_rate: 48_000.0,
                max_buffer_size: 128,
            };
            plugin.chorus.update(config);
            plugin.trem.update(config);
            plugin.wah.update(config);

            let mut worst = 0.0f64;
            for block in 0..40 {
                let mut l = [0.0f64; 128];
                let mut r = [0.0f64; 128];
                match chain {
                    Chain::Delay => plugin.chorus.process(&mut l, &mut r),
                    Chain::Tremolo => plugin.trem.process(&mut l, &mut r),
                    Chain::Wah => plugin.wah.process(&mut l, &mut r),
                }
                if block > 20 {
                    for v in l.iter().chain(r.iter()) {
                        assert!(v.is_finite(), "non-finite output");
                        worst = worst.max(v.abs());
                    }
                }
            }
            let id = modulation_profiles::PROFILES[index].id;
            assert!(worst < 1.0e-6, "{id} idles at {worst}");
        }
    }

    /// Every profile passes audio without running away, at every extreme of
    /// its controls.
    #[test]
    fn nothing_explodes_at_any_setting() {
        for index in 0..modulation_profiles::PROFILES.len() {
            for end in [0.0f32, 1.0] {
                let mut plugin = FtsModulation::default();
                plugin.params.store_profile_id(index);
                for p in [
                    &plugin.params.rate,
                    &plugin.params.depth,
                    &plugin.params.mix,
                    &plugin.params.knob_a,
                    &plugin.params.knob_b,
                    &plugin.params.knob_c,
                    &plugin.params.knob_d,
                ] {
                    // The host-facing setter; a test is standing in for the
                    // host here, which is exactly what it is for. Unsafe
                    // because nice-plug expects only the host to call it —
                    // there is no aliasing hazard with a params tree this
                    // test owns outright.
                    unsafe { p._internal_set_plain_value(end) };
                }
                let chain = plugin.sync_params();
                let config = AudioConfig {
                    sample_rate: 48_000.0,
                    max_buffer_size: 128,
                };
                plugin.chorus.update(config);
                plugin.trem.update(config);
                plugin.wah.update(config);

                let mut worst = 0.0f64;
                let mut phase = 0.0f64;
                for block in 0..60 {
                    let mut l = [0.0f64; 128];
                    let mut r = [0.0f64; 128];
                    for i in 0..128 {
                        phase += 220.0 / 48_000.0;
                        let s = (phase * std::f64::consts::TAU).sin() * 0.5;
                        l[i] = s;
                        r[i] = s;
                    }
                    match chain {
                        Chain::Delay => plugin.chorus.process(&mut l, &mut r),
                        Chain::Tremolo => plugin.trem.process(&mut l, &mut r),
                        Chain::Wah => plugin.wah.process(&mut l, &mut r),
                    }
                    if block > 20 {
                        for v in l.iter().chain(r.iter()) {
                            assert!(
                                v.is_finite(),
                                "{} went non-finite at {end}",
                                modulation_profiles::PROFILES[index].id,
                            );
                            worst = worst.max(v.abs());
                        }
                    }
                }
                assert!(
                    worst < 4.0,
                    "{} reached {worst} at {end}",
                    modulation_profiles::PROFILES[index].id,
                );
            }
        }
    }
}
