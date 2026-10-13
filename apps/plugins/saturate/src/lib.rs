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
//! one now, and [`saturate::profiles::apply`] is the only thing that knows how
//! a knob reaches the DSP — which is also why the editor's curve is the
//! engine's curve rather than a drawing of one.
//!
//! Engine: [`saturate::preamp::ClassAPreamp`] for the nonlinearity, plus
//! [`saturate::digital::DigitalStage`] on the wet path for the digital
//! family (there is no transfer curve that produces an alias, so quantisation
//! and rate reduction cannot be a shaper).

// `audio::prelude` re-exports nice-plug's, plus the Dioxus editor
// pieces — one import rather than two that overlap.
use audio::prelude::*;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use saturate::bluebox::Bluebox;
use saturate::deluxe::Deluxe;
use saturate::digital::DigitalStage;
use saturate::emphasis::EmphasisEq;
use saturate::preamp::ClassAPreamp;
use saturate::ui::params::{SatParams, SatUiState};

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
    /// The emphasis / de-emphasis EQ pair (`fx.sat.emphasis`): applied
    /// around the whole wet path, per channel like the preamps.
    emph: [EmphasisEq; 2],
    /// The measured models. Stereo engines (Deluxe's detector hears the
    /// sum of both channels), built for the session's rate in `activate`
    /// because their filters are designed there — never on the audio
    /// thread.
    deluxe: Deluxe,
    bluebox: Bluebox,
    /// The latency last reported to the host: Bluebox's oversampler delays
    /// its wet path (and its dry, to match); the other engines are
    /// zero-latency.
    latency: u32,
}

impl Default for FtsSaturate {
    fn default() -> Self {
        Self {
            params: Arc::new(SatParams::default()),
            ui_state: Arc::new(SatUiState::default()),
            // The editor sizes itself — see saturate::ui::control_view.
            editor_state: DioxusState::new(|| {
                (
                    saturate::ui::control_view::EDITOR_W,
                    saturate::ui::control_view::EDITOR_H,
                )
            })
            .with_resize_hint(saturate::ui::control_view::resize_hint()),
            pre: [ClassAPreamp::new(48_000.0), ClassAPreamp::new(48_000.0)],
            quantiser: [DigitalStage::new(), DigitalStage::new()],
            emph: [EmphasisEq::new(48_000.0), EmphasisEq::new(48_000.0)],
            deluxe: Deluxe::new(48_000.0),
            bluebox: Bluebox::new(48_000.0),
            latency: 0,
        }
    }
}

impl FtsSaturate {
    /// The latency `engine` adds, in samples.
    fn latency_of(&self, engine: saturate::profiles::Engine) -> u32 {
        match engine {
            saturate::profiles::Engine::Bluebox { .. } => u32::try_from(self.bluebox.latency()).unwrap_or(0),
            _ => 0,
        }
    }

    /// Point both channels at the resolved profile.
    ///
    /// Resolved through the *persisted id* rather than the automatable index,
    /// so growing the rail never repoints an old session at a different
    /// circuit — same contract the reverb and the compressor keep.
    fn sync_params(&mut self) -> (bool, saturate::profiles::Engine) {
        let profile = self.params.resolved_profile();
        let controls = saturate::profiles::Controls {
            drive: self.params.drive.value(),
            bias: self.params.bias.value(),
            sag: self.params.sag.value(),
            tilt: self.params.tilt.value(),
            character_a: self.params.character_a.value(),
            character_b: self.params.character_b.value(),
            mix: self.params.mix.value(),
        };
        // The emphasis bands, designed once per block (setters early-out on
        // unchanged values inside the biquad design's own cheapness — the
        // whole table is a handful of arithmetic).
        let bands = std::array::from_fn(|i| self.params.emph[i].to_band());
        for emph in &mut self.emph {
            if *emph.bands() != bands {
                emph.set_bands(&bands);
            }
        }
        // The makeup calibrates at the level the shaper actually sees — the
        // emphasis raises it exactly like the tilt does
        // (`fx.sat.emphasis.makeup`). Before `apply`, which refreshes makeup.
        let sigma_gain = self.emph[0].sigma_gain();
        for (pre, quantiser) in self.pre.iter_mut().zip(self.quantiser.iter_mut()) {
            pre.set_emphasis_sigma_gain(sigma_gain);
            saturate::profiles::apply(profile, &controls, pre, quantiser);
        }
        // The measured models take the same knobs through their own mapping.
        match profile.engine {
            saturate::profiles::Engine::Deluxe => self.deluxe.set(saturate::profiles::deluxe_params(&controls)),
            saturate::profiles::Engine::Bluebox { sat_freq, alt_tube } => {
                self.bluebox.set(saturate::profiles::bluebox_params(&controls, sat_freq, alt_tube));
            }
            saturate::profiles::Engine::Preamp => {}
        }
        (profile.voicing.digital && !self.quantiser[0].is_transparent(), profile.engine)
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

    type Editor = audio::nice_plug_dioxus::editor::DioxusEditor;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        create_dioxus_editor_with_state(
            self.editor_state.clone(),
            Arc::new(saturate::ui::control_view::SatUi {
                params: self.params.clone(),
                state: self.ui_state.clone(),
            }),
            saturate::ui::control_view::App,
        )
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl ActivateContext<Self>,
    ) -> bool {
        for (pre, quantiser) in self.pre.iter_mut().zip(self.quantiser.iter_mut()) {
            pre.set_sample_rate(buffer_config.sample_rate);
            pre.reset();
            quantiser.reset();
        }
        for emph in &mut self.emph {
            emph.set_sample_rate(buffer_config.sample_rate);
        }
        self.deluxe = Deluxe::new(f64::from(buffer_config.sample_rate));
        self.bluebox = Bluebox::new(f64::from(buffer_config.sample_rate));
        // Land the settings before the first block, so the tilt filters and
        // the sag ballistics start on the real values rather than ramping
        // out of a default nobody chose.
        let (_, engine) = self.sync_params();
        self.latency = self.latency_of(engine);
        context.set_latency_samples(self.latency);
        true
    }

    fn reset(&mut self) {
        for (pre, quantiser) in self.pre.iter_mut().zip(self.quantiser.iter_mut()) {
            pre.reset();
            quantiser.reset();
        }
        for emph in &mut self.emph {
            emph.reset();
        }
        self.deluxe.reset();
        self.bluebox.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let (crushing, engine) = self.sync_params();
        let latency = self.latency_of(engine);
        if latency != self.latency {
            self.latency = latency;
            context.set_latency_samples(latency);
        }
        let mix = self.params.mix.value();
        let output = 10.0f32.powf(self.params.output.value() / 20.0);

        let mut input_peak = 0.0f32;
        let mut output_peak = 0.0f32;

        if engine != saturate::profiles::Engine::Preamp {
            // A measured model is a whole stereo algorithm with its own mix
            // law (handed over in `sync_params`); the emphasis pair still
            // wraps what it distorts, and Output stays ours.
            for mut frame in buffer.iter_samples() {
                let mut io = [0.0f32; 2];
                for ((sample, slot), emph) in frame.iter_mut().zip(io.iter_mut()).zip(self.emph.iter_mut()) {
                    input_peak = input_peak.max(sample.abs());
                    *slot = emph.pre(*sample);
                }
                let [l, r] = io;
                let wet_pair: [f64; 2] = match engine {
                    saturate::profiles::Engine::Deluxe => self.deluxe.process(f64::from(l), f64::from(r)),
                    _ => self.bluebox.process(f64::from(l), f64::from(r)),
                }
                .into();
                for ((sample, wet), emph) in frame.iter_mut().zip(wet_pair).zip(self.emph.iter_mut()) {
                    let out = emph.post(to_f32(wet)) * output;
                    output_peak = output_peak.max(out.abs());
                    *sample = out;
                }
            }
            store_peak(&self.ui_state.input_db, input_peak);
            store_peak(&self.ui_state.out_db, output_peak);
            return ProcessStatus::Normal;
        }

        for mut frame in buffer.iter_samples() {
            for (c, sample) in frame.iter_mut().enumerate() {
                let ch = c.min(1);
                let dry = *sample;
                input_peak = input_peak.max(dry.abs());

                // Emphasis in, the stage, the quantiser, de-emphasis out,
                // and only THEN the mix (`fx.sat.emphasis`): the EQ pair
                // wraps the whole wet path so it shapes what distorts —
                // including what the bitcrusher chews on — and is net-flat
                // around it. A flat EQ short-circuits to identity, keeping
                // the default path bit-exact.
                let emphasized = self.emph[ch].pre(dry);
                let mut wet = self.pre[ch].process_wet(ch, emphasized);
                if crushing {
                    wet = self.quantiser[ch].process(ch, wet);
                }
                wet = self.emph[ch].post(wet);
                let out = (wet - dry).mul_add(mix, dry) * output;
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

/// The measured models run in f64; the host's buffer is f32.
#[expect(clippy::as_conversions, clippy::cast_possible_truncation, reason = "narrowing to the host's f32 buffer is the point")]
const fn to_f32(x: f64) -> f32 {
    x as f32
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

#[cfg(feature = "export")]
nice_export_clap!(FtsSaturate);
#[cfg(feature = "export")]
nice_export_vst3!(FtsSaturate);

#[cfg(test)]
mod tests {
    use super::*;

    /// The plugin runs whatever the rail says, with no table of its own. If
    /// a `ModelParam`-shaped thing ever comes back, this is what catches the
    /// first symptom: a profile that plays as something else.
    #[test]
    fn every_profile_on_the_rail_voices_the_engine_it_names() {
        for (index, profile) in saturate::profiles::PROFILES.iter().enumerate() {
            let plugin = {
                let mut p = FtsSaturate::default();
                p.params.store_profile_id(index);
                p.sync_params();
                p
            };
            assert_eq!(
                plugin.pre[0].positive, profile.voicing.positive,
                "{}",
                profile.id
            );
            assert_eq!(
                plugin.pre[0].negative, profile.voicing.negative,
                "{}",
                profile.id
            );
            assert_eq!(
                plugin.pre[0].positive, plugin.pre[1].positive,
                "{} voices its two channels differently",
                profile.id,
            );
        }
    }

    /// Silence in, silence out — on every profile, the measured models
    /// included, and the ones with a bias
    /// on them. The DC blocker is what makes that true, and a saturator that
    /// hums at rest is unusable however good it sounds when played.
    #[test]
    fn no_profile_makes_a_sound_out_of_nothing() {
        for index in 0..saturate::profiles::PROFILES.len() {
            let mut plugin = FtsSaturate::default();
            plugin.params.store_profile_id(index);
            let (crushing, engine) = plugin.sync_params();
            let mut worst = 0.0f32;
            for i in 0..48_000 {
                let y = match engine {
                    saturate::profiles::Engine::Deluxe => to_f32(plugin.deluxe.process(0.0, 0.0).0),
                    saturate::profiles::Engine::Bluebox { .. } => to_f32(plugin.bluebox.process(0.0, 0.0).0),
                    saturate::profiles::Engine::Preamp => {
                        let wet = plugin.pre[0].process_wet(0, 0.0);
                        if crushing { plugin.quantiser[0].process(0, wet) } else { wet }
                    }
                };
                if i > 24_000 {
                    worst = worst.max(y.abs());
                }
            }
            let id = saturate::profiles::PROFILES[index].id;
            assert!(worst < 1.0e-3, "{id} idles at {worst}");
        }
    }
}
