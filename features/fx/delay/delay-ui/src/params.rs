//! The delay's parameters, and the two pieces of editor state a session has
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
pub struct DelayUiState {
    /// Wet output level in dB, for the repeat display.
    pub wet_db: AtomicF32,
    /// Input level in dB, so a face can show what is feeding it.
    pub input_db: AtomicF32,
}



#[derive(Params)]
pub struct DelayParams {
    /// Which of the seven families' profiles is active, as an index into
    /// [`delay_profiles::PROFILES`]. A parameter because the space is worth
    /// automating; see [`Self::profile_id`] for what a session actually saves.
    #[id = "profile"]
    pub profile: IntParam,

    /// Left delay time. The right side follows it when `link` is on.
    #[id = "time_l"]
    pub time_l: FloatParam,

    #[id = "time_r"]
    pub time_r: FloatParam,

    /// Right time follows left. Off is where ping-pong and stereo spread
    /// come from.
    #[id = "link"]
    pub link: BoolParam,

    #[id = "feedback"]
    pub feedback: FloatParam,

    #[id = "tone"]
    pub tone: FloatParam,

    /// How hard the repeats are driven into whatever the medium is. Silent on
    /// a clean digital line and the whole character of a tape.
    #[id = "drive"]
    pub drive: FloatParam,

    /// Slow pitch drift, and fast. A tape has both; a digital delay has
    /// neither unless you ask.
    #[id = "wow"]
    pub wow: FloatParam,

    #[id = "flutter"]
    pub flutter: FloatParam,

    /// Repeats duck out of the way of the input.
    #[id = "duck"]
    pub duck: FloatParam,

    #[id = "mix"]
    pub mix: FloatParam,

    /// The engine's own two controls.
    ///
    /// Every engine in `delay-dsp` reads them as something different — the
    /// pitch shifter's interval, the multi-tap's spread, the spectral tilt.
    /// The panel names them for what they do on that machine; see
    /// `faces::character_legends`.
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
}

impl Default for DelayParams {
    fn default() -> Self {
        Self {
            profile: IntParam::new(
                "Space",
                delay_profiles::profile_index("digital").unwrap_or(0) as i32,
                IntRange::Linear {
                    min: 0,
                    max: (delay_profiles::PROFILES.len() - 1) as i32,
                },
            )
            .with_value_to_string(Arc::new(|v| {
                delay_profiles::PROFILES
                    .get(v.max(0) as usize)
                    .map(|p| p.name.to_string())
                    .unwrap_or_else(|| "—".to_string())
            })),

            time_l: FloatParam::new(
                "Time L",
                375.0,
                FloatRange::Skewed { min: 1.0, max: 4000.0, factor: FloatRange::skew_factor(-1.6) },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            time_r: FloatParam::new(
                "Time R",
                375.0,
                FloatRange::Skewed { min: 1.0, max: 4000.0, factor: FloatRange::skew_factor(-1.6) },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            link: BoolParam::new("Link", true),

            feedback: FloatParam::new("Feedback", 0.35, FloatRange::Linear { min: 0.0, max: 1.1 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            tone: FloatParam::new("Tone", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            drive: FloatParam::new("Drive", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            wow: FloatParam::new("Wow", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            flutter: FloatParam::new("Flutter", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            duck: FloatParam::new("Duck", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            mix: FloatParam::new("Mix", 0.35, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            character_a: FloatParam::new("Character A", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            character_b: FloatParam::new("Character B", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            profile_id: parking_lot::RwLock::new(String::new()),
            editor_form: parking_lot::RwLock::new(String::new()),
        }
    }
}

impl DelayParams {
    /// The profile index the editor should be showing.
    ///
    /// The persisted id wins when this build still has it; otherwise the
    /// index, which is what a pre-id session has.
    pub fn resolved_profile_index(&self) -> usize {
        let id = self.profile_id.read();
        delay_profiles::profile_index(&id)
            .unwrap_or_else(|| (self.profile.value().max(0) as usize).min(delay_profiles::PROFILES.len() - 1))
    }

    /// The active profile.
    pub fn resolved_profile(&self) -> &'static delay_profiles::Profile {
        &delay_profiles::PROFILES[self.resolved_profile_index()]
    }

    /// Record the id for `index` — call wherever the profile changes.
    pub fn store_profile_id(&self, index: usize) {
        let id = delay_profiles::PROFILES
            .get(index)
            .map(|p| p.id)
            .unwrap_or("digital");
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
        let params = DelayParams::default();
        params.store_profile_id(delay_profiles::profile_index("oilcan").unwrap());
        assert_eq!(params.resolved_profile().id, "oilcan");

        // A profile this build does not have — a project from a newer
        // version — falls back to the index rather than guessing.
        *params.profile_id.write() = "gravity_well".to_string();
        assert_eq!(
            params.resolved_profile_index(),
            params.profile.value() as usize,
        );
    }
}
