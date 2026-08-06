//! The modulator's parameters, and the two pieces of editor state a session
//! has to remember.
//!
//! Lives here rather than in the plugin shell for the same reason the
//! saturator's do: the editor is what constrains them. The profile is a
//! parameter so a host can automate the space; the profile *id* is persisted
//! state so growing the list cannot repoint an old project at a different
//! circuit.

use std::sync::Arc;

use atomic_float::AtomicF32;
use nice_plug::prelude::*;

/// Live values the editor reads and the audio thread writes.
///
/// Plain atomics rather than a channel: the editor samples them when it
/// draws, and a frame that misses an update simply draws the next one.
#[derive(Default)]
pub struct ModUiState {
    /// Output level in dB, for the meter.
    pub out_db: AtomicF32,
    /// Input level in dB, so a face can show what is feeding it.
    pub input_db: AtomicF32,
    /// Where the modulator is in its cycle, 0..1 — the panel draws a
    /// playhead on the shape with it. Written every block, read whenever the
    /// editor happens to draw; nothing is synchronised because nothing needs
    /// to be.
    pub phase: AtomicF32,
    /// The modulator's current output, 0..1. On a wah this is where the
    /// filter is sitting, which no number on a panel has ever conveyed.
    pub mod_value: AtomicF32,
}

/// **Noon is the circuit as designed.**
///
/// The four circuit knobs are *trims* around the active profile's voicing
/// rather than absolute values: 0.5 means "a Juno, as a Juno is", and turning
/// one is always a statement about this circuit. That is what lets one set of
/// parameters serve fifteen modulators without a knob meaning something
/// different on each — see [`modulation_profiles::Controls`], where they land.
///
/// `rate`, `depth` and `mix` are absolute, because they mean the same thing
/// on every modulator ever built.
#[derive(Params)]
pub struct ModParams {
    /// Which of the five families' profiles is active, as an index into
    /// [`modulation_profiles::PROFILES`]. A parameter because the space is
    /// worth automating; see [`Self::profile_id`] for what a session saves.
    #[id = "profile"]
    pub profile: IntParam,

    /// How fast it moves. On every one of the five, which is why it is a
    /// control and not a family. Exponential — see
    /// [`modulation_profiles::rate_hz_from`].
    #[id = "rate"]
    pub rate: FloatParam,

    /// How far it moves.
    #[id = "depth"]
    pub depth: FloatParam,

    /// Dry/wet. Vibrato has no dry path at all, and its panel does not offer
    /// this rather than offering one that does nothing.
    #[id = "mix"]
    pub mix: FloatParam,

    /// Output trim.
    #[id = "output"]
    pub output: FloatParam,

    /// The circuit's own four controls; the panel names them per profile.
    #[id = "knoba"]
    pub knob_a: FloatParam,
    #[id = "knobb"]
    pub knob_b: FloatParam,
    #[id = "knobc"]
    pub knob_c: FloatParam,
    #[id = "knobd"]
    pub knob_d: FloatParam,

    /// What a session restores from.
    ///
    /// The index is not stable: adding a family renumbers everything after
    /// it, and a project saved last year would open on whatever now sits at
    /// that number. The id is stable, so it is what gets written down.
    #[persist = "profile_id"]
    pub profile_id: parking_lot::RwLock<String>,

    /// The editor's form factor, persisted by id.
    #[persist = "editor_form"]
    pub editor_form: parking_lot::RwLock<String>,
}

impl Default for ModParams {
    fn default() -> Self {
        Self {
            profile: IntParam::new(
                "Circuit",
                modulation_profiles::profile_index("juno").unwrap_or(0) as i32,
                IntRange::Linear {
                    min: 0,
                    max: (modulation_profiles::PROFILES.len() - 1) as i32,
                },
            )
            .with_value_to_string(Arc::new(|v| {
                modulation_profiles::PROFILES
                    .get(v.max(0) as usize)
                    .map(|p| p.name.to_string())
                    .unwrap_or_else(|| "—".to_string())
            })),

            // Shown in Hz, stored normalised: the taper belongs to the
            // profiles crate so the editor and the audio thread cannot
            // disagree about what 0.5 means.
            rate: FloatParam::new("Rate", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(Arc::new(|v| {
                    format!("{:.2} Hz", modulation_profiles::rate_hz_from(v))
                }))
                .with_string_to_value(Arc::new(|s| {
                    s.trim()
                        .trim_end_matches("Hz")
                        .trim()
                        .parse::<f32>()
                        .ok()
                        .map(modulation_profiles::rate_knob_from)
                })),

            depth: FloatParam::new("Depth", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            mix: FloatParam::new("Mix", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_percentage(0)),

            output: FloatParam::new("Output", 0.0, FloatRange::Linear { min: -24.0, max: 12.0 })
                .with_unit(" dB")
                .with_value_to_string(formatters::v2s_f32_rounded(1)),

            knob_a: knob("Knob A"),
            knob_b: knob("Knob B"),
            knob_c: knob("Knob C"),
            knob_d: knob("Knob D"),

            profile_id: parking_lot::RwLock::new(String::new()),
            editor_form: parking_lot::RwLock::new(String::new()),
        }
    }
}

/// A circuit knob: centred, because noon is the voicing.
fn knob(name: &str) -> FloatParam {
    FloatParam::new(name, 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
        .with_value_to_string(formatters::v2s_f32_percentage(0))
}

impl ModParams {
    /// The profile index the editor should be showing.
    ///
    /// The persisted id wins when this build still has it; otherwise the
    /// index, which is what a pre-id session has.
    pub fn resolved_profile_index(&self) -> usize {
        let id = self.profile_id.read();
        modulation_profiles::profile_index(&id).unwrap_or_else(|| {
            (self.profile.value().max(0) as usize)
                .min(modulation_profiles::PROFILES.len() - 1)
        })
    }

    /// The active profile.
    pub fn resolved_profile(&self) -> &'static modulation_profiles::Profile {
        &modulation_profiles::PROFILES[self.resolved_profile_index()]
    }

    /// Record the id for `index` — call wherever the profile changes.
    pub fn store_profile_id(&self, index: usize) {
        let id = modulation_profiles::PROFILES
            .get(index)
            .map(|p| p.id)
            .unwrap_or("juno");
        *self.profile_id.write() = id.to_string();
    }

    /// Everything the mapping needs, in one struct.
    pub fn controls(&self) -> modulation_profiles::Controls {
        modulation_profiles::Controls {
            rate: self.rate.value(),
            depth: self.depth.value(),
            mix: self.mix.value(),
            knobs: [
                self.knob_a.value(),
                self.knob_b.value(),
                self.knob_c.value(),
                self.knob_d.value(),
            ],
        }
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
        let params = ModParams::default();
        params.store_profile_id(modulation_profiles::profile_index("wah_pattern").unwrap());
        assert_eq!(params.resolved_profile().id, "wah_pattern");

        // A profile this build does not have — a project from a newer
        // version — falls back to the index rather than guessing.
        *params.profile_id.write() = "rotary".to_string();
        assert_eq!(
            params.resolved_profile_index(),
            params.profile.value() as usize,
        );
    }

    /// The rate control is stored normalised and shown in Hz, so the two
    /// conversions have to be each other's inverse or the readout lies.
    #[test]
    fn the_rate_readout_round_trips() {
        let params = ModParams::default();
        let hz = modulation_profiles::rate_hz_from(params.rate.value());
        let back = modulation_profiles::rate_knob_from(hz);
        assert!((back - params.rate.value()).abs() < 1.0e-4);
    }
}
