//! FTS Saturate — CLAP/VST3 saturation plugin.
//!
//! Five circuits on a rail — Tube, Tape, Transformer, Transistor, Digital —
//! and nine profiles inside them. What each one *is* lives in
//! [`saturate_profiles`]: the per-side shaper pair, the operating point, the
//! sag and its ballistics, the skew that makes the even harmonics, and which
//! band meets the knee first. Nothing about a circuit lives in this file.
//!
//! That is deliberate and it is the point of the crate split. The plugin
//! shell used to carry its own `ModelParam` table — Preamp/Tube/Tape/
//! Transformer/Console/Fuzz — which named nearly the same circuits as the
//! rail did, mapped them to slightly different settings, and drifted from it.
//! Two vocabularies for one choice is a bug that ships as a sound. There is
//! one now, and [`saturate_profiles::apply`] is the only thing that knows how
//! a knob reaches the DSP — which is also why the editor's curve is the
//! engine's curve rather than a drawing of one.
//!
//! Engine: [`saturate_dsp::preamp::ClassAPreamp`] for the nonlinearity, plus
//! [`saturate_dsp::digital::DigitalStage`] on the wet path for the digital
//! family (there is no transfer curve that produces an alias, so quantisation
//! and rate reduction cannot be a shaper).

// `audiocore_core::prelude` re-exports nice-plug's, plus the Dioxus editor
// pieces — one import rather than two that overlap.
use audiocore_core::prelude::*;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use saturate::digital::DigitalStage;
use saturate::preamp::ClassAPreamp;
use saturate_ui::params::{SatParams, SatUiState};

const PLUGIN_NAME: &str = "FTS Saturate";

// ── Plugin ────────────────────────────────────────────────────────────────

pub struct FtsSaturate {
    params: Arc<SatParams>,
    ui_state: Arc<SatUiState>,
    editor_state: Arc<DioxusState>,
    /// One stage per channel: the preamp's sag envelope, DC blocker and tilt
    /// filters are all per-channel state, so the two sides must not share.
    pre: [ClassAPreamp; 2],
    /// …and one quantiser per channel, for the same reason (the hold is
    /// state) plus one more: a shared dither sequence would collapse into the
    /// middle of the stereo image instead of sitting behind it.
    quantiser: [DigitalStage; 2],
}

impl Default for FtsSaturate {
    fn default() -> Self {
        Self {
            params: Arc::new(SatParams::default()),
            ui_state: Arc::new(SatUiState::default()),
            // The editor sizes itself — see saturate_ui::control_view.
            editor_state: DioxusState::new(|| {
                (
                    saturate_ui::control_view::EDITOR_W,
                    saturate_ui::control_view::EDITOR_H,
                )
            })
            .with_resize_hint(saturate_ui::control_view::resize_hint()),
            pre: [ClassAPreamp::new(48_000.0), ClassAPreamp::new(48_000.0)],
            quantiser: [DigitalStage::new(), DigitalStage::new()],
        }
    }
}

impl FtsSaturate {
    /// Point both channels at the resolved profile.
    ///
    /// Resolved through the *persisted id* rather than the automatable index,
    /// so growing the rail never repoints an old session at a different
    /// circuit — same contract the reverb and the compressor keep.
    fn sync_params(&mut self) -> bool {
        let profile = self.params.resolved_profile();
        let controls = saturate_profiles::Controls {
            drive: self.params.drive.value(),
            bias: self.params.bias.value(),
            sag: self.params.sag.value(),
            tilt: self.params.tilt.value(),
            character_a: self.params.character_a.value(),
            character_b: self.params.character_b.value(),
            mix: self.params.mix.value(),
        };
        for (pre, quantiser) in self.pre.iter_mut().zip(self.quantiser.iter_mut()) {
            saturate_profiles::apply(profile, &controls, pre, quantiser);
        }
        profile.voicing.digital && !self.quantiser[0].is_transparent()
    }
}

impl Plugin for FtsSaturate {
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
            Arc::new(saturate_ui::control_view::SatUi {
                params: self.params.clone(),
                state: self.ui_state.clone(),
            }),
            saturate_ui::control_view::App,
        )
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        for (pre, quantiser) in self.pre.iter_mut().zip(self.quantiser.iter_mut()) {
            pre.set_sample_rate(buffer_config.sample_rate);
            pre.reset();
            quantiser.reset();
        }
        // Land the settings before the first block, so the tilt filters and
        // the sag ballistics start on the real values rather than ramping
        // out of a default nobody chose.
        self.sync_params();
        true
    }

    fn reset(&mut self) {
        for (pre, quantiser) in self.pre.iter_mut().zip(self.quantiser.iter_mut()) {
            pre.reset();
            quantiser.reset();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let crushing = self.sync_params();
        let mix = self.params.mix.value();
        let output = 10.0f32.powf(self.params.output.value() / 20.0);

        let mut input_peak = 0.0f32;
        let mut output_peak = 0.0f32;

        for mut frame in buffer.iter_samples() {
            for (c, sample) in frame.iter_mut().enumerate() {
                let ch = c.min(1);
                let dry = *sample;
                input_peak = input_peak.max(dry.abs());

                // The stage alone, then the quantiser, and only THEN the
                // mix: a bitcrusher blended in after its own dry/wet would
                // be crushing a signal that had already been un-crushed.
                let mut wet = self.pre[ch].process_wet(ch, dry);
                if crushing {
                    wet = self.quantiser[ch].process(ch, wet);
                }
                let out = (dry + (wet - dry) * mix) * output;
                output_peak = output_peak.max(out.abs());
                *sample = out;
            }
        }

        // Meter ballistics: instant attack, a slow fall the editor can
        // read at whatever rate it happens to draw at.
        store_peak(&self.ui_state.input_db, input_peak);
        store_peak(&self.ui_state.out_db, output_peak);

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

impl ClapPlugin for FtsSaturate {
    const CLAP_ID: &'static str = "com.fasttrackstudio.saturate";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Five circuits: tube, tape, transformer, transistor, and digital");
    const CLAP_MANUAL_URL: Option<&'static str> = None;
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Distortion,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for FtsSaturate {
    const VST3_CLASS_ID: [u8; 16] = *b"FtsSaturatePlg01";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nice_export_clap!(FtsSaturate);
nice_export_vst3!(FtsSaturate);

#[cfg(test)]
mod tests {
    use super::*;

    /// The plugin runs whatever the rail says, with no table of its own. If
    /// a `ModelParam`-shaped thing ever comes back, this is what catches the
    /// first symptom: a profile that plays as something else.
    #[test]
    fn every_profile_on_the_rail_voices_the_engine_it_names() {
        for (index, profile) in saturate_profiles::PROFILES.iter().enumerate() {
            let plugin = {
                let mut p = FtsSaturate::default();
                p.params.store_profile_id(index);
                p.sync_params();
                p
            };
            assert_eq!(plugin.pre[0].positive, profile.voicing.positive, "{}", profile.id);
            assert_eq!(plugin.pre[0].negative, profile.voicing.negative, "{}", profile.id);
            assert_eq!(
                plugin.pre[0].positive, plugin.pre[1].positive,
                "{} voices its two channels differently",
                profile.id,
            );
        }
    }

    /// Silence in, silence out — on all nine, including the ones with a bias
    /// on them. The DC blocker is what makes that true, and a saturator that
    /// hums at rest is unusable however good it sounds when played.
    #[test]
    fn no_profile_makes_a_sound_out_of_nothing() {
        for index in 0..saturate_profiles::PROFILES.len() {
            let mut plugin = FtsSaturate::default();
            plugin.params.store_profile_id(index);
            let crushing = plugin.sync_params();
            let mut worst = 0.0f32;
            for i in 0..48_000 {
                let wet = plugin.pre[0].process_wet(0, 0.0);
                let y = if crushing {
                    plugin.quantiser[0].process(0, wet)
                } else {
                    wet
                };
                if i > 24_000 {
                    worst = worst.max(y.abs());
                }
            }
            let id = saturate_profiles::PROFILES[index].id;
            assert!(worst < 1.0e-3, "{id} idles at {worst}");
        }
    }
}
