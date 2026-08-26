//! The saturator's parameters, and the two pieces of editor state a session has
//! to remember.
//!
//! Lives here rather than in the plugin shell for the same reason the
//! compressor's do: the editor is what constrains them. The profile is a
//! parameter so a host can automate the space; the profile *id* is persisted
//! state so growing the list cannot repoint an old project at a different
//! delay.

use std::sync::Arc;

use atomic_float::AtomicF32;
use nice_plug::prelude::*;

/// Live values the editor reads and the audio thread writes, plus the
/// editor's side of IR loading.
///
/// The meters are plain atomics rather than a channel: the editor samples them
/// when it draws, and a frame that misses an update simply draws the next one.
#[derive(Default)]
pub struct SatUiState {
    /// Output level in dB, for the meter.
    pub out_db: AtomicF32,
    /// Input level in dB, so a face can show what is feeding it.
    pub input_db: AtomicF32,
}

/// **Noon is the circuit as designed.**
///
/// Every control here except `drive`, `mix` and `output` is a *trim* around
/// the active profile's voicing rather than an absolute value: 0.5 means "a
/// Triode, as a Triode is", and turning a knob is always a statement about
/// this circuit. That is what lets one set of nine parameters serve nine
/// machines without a knob meaning something different on each — see
/// [`saturate_profiles::Controls`], which is where they land.
#[derive(Params)]
pub struct SatParams {
    /// Which of the seven families' profiles is active, as an index into
    /// [`saturate_profiles::PROFILES`]. A parameter because the space is worth
    /// automating; see [`Self::profile_id`] for what a session actually saves.
    #[id = "profile"]
    pub profile: IntParam,

    /// How hard the signal is pushed into the circuit. On every one of the
    /// five, which is why it is a control and not a family.
    #[id = "drive"]
    pub drive: FloatParam,

    /// Where the circuit is biased. On an asymmetric stage this is what
    /// decides how much of the even-harmonic content you get; on a symmetric
    /// one it does almost nothing, and the panel only offers it where it
    /// means something.
    #[id = "q_point"]
    pub bias: FloatParam,

    /// Supply sag: the circuit ducking under a transient and recovering.
    #[id = "sag"]
    pub sag: FloatParam,

    /// Pre-emphasis into the stage — drive the top end harder than the
    /// bottom, which is how a transformer is usually flattered.
    #[id = "tilt"]
    pub tilt: FloatParam,

    #[id = "mix"]
    pub mix: FloatParam,

    #[id = "output"]
    pub output: FloatParam,

    /// The circuit's own two controls; the panel names them per profile.
    #[id = "chara"]
    pub character_a: FloatParam,

    #[id = "charb"]
    pub character_b: FloatParam,

    /// What a session restores from.
    ///
    /// The index is not stable: adding a family, or another plate, renumbers
    /// everything after it, and a project saved last year would open on
    /// whatever now sits at that number. The id is stable, so it is what gets
    /// written down. Same contract as the compressor's.
    #[persist = "profile_id"]
    pub profile_id: parking_lot::RwLock<String>,

    /// The editor's form factor, persisted by id.
    #[persist = "editor_form"]
    pub editor_form: parking_lot::RwLock<String>,

    // ── Appended (never reorder anything above) ────────────────────────
    /// The emphasis / de-emphasis EQ (`fx.sat.emphasis`,
    /// docs/spec/fx/embedded-eq.md): six bands applied before the stage and
    /// exactly inverted after it, so the curve chooses what distorts. Ids
    /// come out `eshape_1`…`eq_6`.
    #[nested(array, group = "Emphasis")]
    pub emph: [EmphBandParams; saturate_dsp::emphasis::BANDS],
}

/// One emphasis band (`fx.embed-eq.band-params`). Bell / Low Shelf / High
/// Shelf only — cut shapes have no inverse (`fx.sat.emphasis.mirror`). A
/// band at 0 dB is idle and costs nothing.
#[derive(Params)]
pub struct EmphBandParams {
    #[id = "eshape"]
    pub shape: IntParam,
    #[id = "efreq"]
    pub freq_hz: FloatParam,
    #[id = "egain"]
    pub gain_db: FloatParam,
    #[id = "eq"]
    pub q: FloatParam,
}

/// Emphasis shape labels, in `eshape` value order
/// (`saturate_dsp::emphasis::EmphShape`).
pub const EMPH_SHAPE_LABELS: &[&str] = &["Bell", "Low Shelf", "High Shelf"];

impl EmphBandParams {
    fn new(default_freq: f32) -> Self {
        Self {
            shape: IntParam::new("Emph Shape", 0, IntRange::Linear { min: 0, max: 2 })
                .with_value_to_string(Arc::new(|v| {
                    EMPH_SHAPE_LABELS
                        .get(v.max(0) as usize)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| v.to_string())
                })),
            freq_hz: FloatParam::new(
                "Emph Freq",
                default_freq,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 20_000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
            .with_string_to_value(formatters::s2v_f32_hz_then_khz()),
            gain_db: FloatParam::new(
                "Emph Gain",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            q: FloatParam::new(
                "Emph Q",
                0.707,
                FloatRange::Skewed {
                    min: 0.1,
                    max: 18.0,
                    factor: FloatRange::skew_factor(-1.5),
                },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
        }
    }

    /// This band as the DSP sees it.
    pub fn to_band(&self) -> saturate_dsp::emphasis::EmphBand {
        saturate_dsp::emphasis::EmphBand {
            shape: saturate_dsp::emphasis::EmphShape::from_index(self.shape.value().max(0) as u32),
            freq_hz: self.freq_hz.value(),
            gain_db: self.gain_db.value(),
            q: self.q.value(),
        }
    }
}

/// The default emphasis band frequencies — a useful spread, all at 0 dB
/// (idle) until moved.
pub const EMPH_DEFAULT_FREQS: [f32; saturate_dsp::emphasis::BANDS] =
    [80.0, 250.0, 700.0, 1_800.0, 4_500.0, 10_000.0];

impl Default for SatParams {
    fn default() -> Self {
        Self {
            profile: IntParam::new(
                "Space",
                saturate_profiles::profile_index("triode").unwrap_or(0) as i32,
                IntRange::Linear {
                    min: 0,
                    max: (saturate_profiles::PROFILES.len() - 1) as i32,
                },
            )
            .with_value_to_string(Arc::new(|v| {
                saturate_profiles::PROFILES
                    .get(v.max(0) as usize)
                    .map(|p| p.name.to_string())
                    .unwrap_or_else(|| "—".to_string())
            })),

            drive: FloatParam::new("Drive", 0.25, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            bias: FloatParam::new("Bias", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            // Noon, like every other trim on the panel — see the type's note.
            sag: FloatParam::new("Sag", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            tilt: FloatParam::new("Tilt", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            output: FloatParam::new(
                "Output",
                0.0,
                FloatRange::Linear {
                    min: -24.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            character_a: FloatParam::new(
                "Character A",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(formatters::v2s_f32_percentage(0)),

            character_b: FloatParam::new(
                "Character B",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(formatters::v2s_f32_percentage(0)),

            profile_id: parking_lot::RwLock::new(String::new()),
            editor_form: parking_lot::RwLock::new(String::new()),
            emph: std::array::from_fn(|i| EmphBandParams::new(EMPH_DEFAULT_FREQS[i])),
        }
    }
}

impl SatParams {
    /// The profile index the editor should be showing.
    ///
    /// The persisted id wins when this build still has it; otherwise the
    /// index, which is what a pre-id session has.
    pub fn resolved_profile_index(&self) -> usize {
        let id = self.profile_id.read();
        saturate_profiles::profile_index(&id).unwrap_or_else(|| {
            (self.profile.value().max(0) as usize).min(saturate_profiles::PROFILES.len() - 1)
        })
    }

    /// The active profile.
    pub fn resolved_profile(&self) -> &'static saturate_profiles::Profile {
        &saturate_profiles::PROFILES[self.resolved_profile_index()]
    }

    /// Record the id for `index` — call wherever the profile changes.
    pub fn store_profile_id(&self, index: usize) {
        let id = saturate_profiles::PROFILES
            .get(index)
            .map(|p| p.id)
            .unwrap_or("triode");
        *self.profile_id.write() = id.to_string();
    }

    pub fn resolved_editor_form(&self) -> fts_audio_ui::EditorForm {
        fts_audio_ui::EditorForm::from_id(&self.editor_form.read()).unwrap_or_default()
    }

    pub fn store_editor_form(&self, form: fts_audio_ui::EditorForm) {
        *self.editor_form.write() = form.id().to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_restores_from_the_id_and_not_the_number() {
        let params = SatParams::default();
        params.store_profile_id(saturate_profiles::profile_index("fuzz").unwrap());
        assert_eq!(params.resolved_profile().id, "fuzz");

        // A profile this build does not have — a project from a newer
        // version — falls back to the index rather than guessing.
        *params.profile_id.write() = "gravity_well".to_string();
        assert_eq!(
            params.resolved_profile_index(),
            params.profile.value() as usize,
        );
    }
}
