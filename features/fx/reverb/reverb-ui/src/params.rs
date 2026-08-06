//! The reverb's parameters, and the two pieces of editor state a session has
//! to remember.
//!
//! Lives here rather than in the plugin shell for the same reason the
//! compressor's do: the editor is what constrains them. The profile is a
//! parameter so a host can automate the space; the profile *id* is persisted
//! state so growing the list cannot repoint an old project at a different
//! reverb.

use std::sync::Arc;

use atomic_float::AtomicF32;
use nice_plug::prelude::*;

/// Live values the editor reads and the audio thread writes.
///
/// Plain atomics, not a channel: the editor samples them when it draws, and a
/// frame that misses an update simply draws the next one.
#[derive(Default)]
pub struct ReverbUiState {
    /// Wet output level in dB, for the tail display.
    pub tail_db: AtomicF32,
    /// Input level in dB, so a face can show what is feeding it.
    pub input_db: AtomicF32,
}

#[derive(Params)]
pub struct ReverbParams {
    /// Which of the seven families' profiles is active, as an index into
    /// [`reverb_profiles::PROFILES`]. A parameter because the space is worth
    /// automating; see [`Self::profile_id`] for what a session actually saves.
    #[id = "profile"]
    pub profile: IntParam,

    #[id = "decay"]
    pub decay: FloatParam,

    #[id = "size"]
    pub size: FloatParam,

    #[id = "predelay"]
    pub predelay: FloatParam,

    #[id = "damping"]
    pub damping: FloatParam,

    #[id = "tone"]
    pub tone: FloatParam,

    #[id = "width"]
    pub width: FloatParam,

    #[id = "mix"]
    pub mix: FloatParam,

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

impl Default for ReverbParams {
    fn default() -> Self {
        Self {
            profile: IntParam::new(
                "Space",
                reverb_profiles::profile_index("hall_concert").unwrap_or(0) as i32,
                IntRange::Linear {
                    min: 0,
                    max: (reverb_profiles::PROFILES.len() - 1) as i32,
                },
            )
            .with_value_to_string(Arc::new(|v| {
                reverb_profiles::PROFILES
                    .get(v.max(0) as usize)
                    .map(|p| p.name.to_string())
                    .unwrap_or_else(|| "—".to_string())
            })),

            decay: FloatParam::new("Decay", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            size: FloatParam::new("Size", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            predelay: FloatParam::new(
                "Pre-Delay",
                20.0,
                FloatRange::Skewed {
                    min: 0.0,
                    max: 250.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            damping: FloatParam::new("Damping", 0.3, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            tone: FloatParam::new("Tone", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            width: FloatParam::new("Width", 1.0, FloatRange::Linear { min: 0.0, max: 2.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            mix: FloatParam::new("Mix", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            profile_id: parking_lot::RwLock::new(String::new()),
            editor_form: parking_lot::RwLock::new(String::new()),
        }
    }
}

impl ReverbParams {
    /// The profile index the editor should be showing.
    ///
    /// The persisted id wins when this build still has it; otherwise the
    /// index, which is what a pre-id session has.
    pub fn resolved_profile_index(&self) -> usize {
        let id = self.profile_id.read();
        reverb_profiles::profile_index(&id)
            .unwrap_or_else(|| (self.profile.value().max(0) as usize).min(reverb_profiles::PROFILES.len() - 1))
    }

    /// The active profile.
    pub fn resolved_profile(&self) -> &'static reverb_profiles::Profile {
        &reverb_profiles::PROFILES[self.resolved_profile_index()]
    }

    /// Record the id for `index` — call wherever the profile changes.
    pub fn store_profile_id(&self, index: usize) {
        let id = reverb_profiles::PROFILES
            .get(index)
            .map(|p| p.id)
            .unwrap_or("hall_concert");
        *self.profile_id.write() = id.to_string();
    }

    pub fn resolved_editor_form(&self) -> fts_ui_audio::EditorForm {
        fts_ui_audio::EditorForm::from_id(&self.editor_form.read()).unwrap_or_default()
    }

    pub fn store_editor_form(&self, form: fts_ui_audio::EditorForm) {
        *self.editor_form.write() = form.id().to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_restores_from_the_id_and_not_the_number() {
        let params = ReverbParams::default();
        params.store_profile_id(reverb_profiles::profile_index("spring_vintage").unwrap());
        assert_eq!(params.resolved_profile().id, "spring_vintage");

        // A profile this build does not have — a project from a newer
        // version — falls back to the index rather than guessing.
        *params.profile_id.write() = "gravity_well".to_string();
        assert_eq!(
            params.resolved_profile_index(),
            params.profile.value() as usize,
        );
    }
}
