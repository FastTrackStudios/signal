//! What you sat down to do.
//!
//! [`crate::demo::Scene`] names *what a fixture demonstrates* — "Q
//! zones", "Channel conflict", "Mixed density", "Guitar Pro import".
//! That is exactly right for the screenshot suite, which exists to catch
//! a regression in one behaviour and needs a fixture per behaviour. It is
//! the wrong list to hand a person, because none of those is a thing
//! anyone sets out to do.
//!
//! A workflow is the other axis: a job, with the surface and the material
//! it needs already chosen. "Tune a vocal" rather than "PitchedAudio mode
//! plus a phrase with drift". The scenes stay underneath as the material
//! — deleting them would throw away the coverage the shots give us — and
//! this is the list that gets shown.
//!
//! ## The two that are stubs
//!
//! [`Workflow::TwoVocalParts`] and [`Workflow::VocalSync`] are named here
//! before they are finished, deliberately. A workflow that exists in the
//! list and opens thin material is a visible gap someone can close; one
//! that exists only in a plan is a gap nobody can see. Both say what they
//! are missing in [`Workflow::note`].

use expression_editor_core::{Editor, Mode, Viewport};

use crate::demo::{self, Scene};

/// A job the editor is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Workflow {
    /// A single sung or played line, with its expression.
    Melody,
    /// Simultaneous notes, where the chord readout is the point.
    Chords,
    /// Per-note bend, pressure and timbre.
    Mpe,
    /// Kit lanes, two-handed pieces, flams.
    Drums,
    /// A string roll: frets, bends, techniques.
    Guitar,
    /// A lead with harmony parts under it, all carrying lyrics.
    Harmonies,
    /// Two singers, each with their own lyric line.
    TwoVocalParts,
    /// Melodyne's job: an audio vocal whose pitch wants correcting.
    VocalTuning,
    /// A vocal with no pitch worth editing — consonants, breaths,
    /// percussive delivery. Hits on bands rather than notes on a scale.
    UnpitchedVocals,
    /// Lining a sung take up against a reference — the lyric-sync job.
    VocalSync,
}

impl Workflow {
    pub const ALL: [Workflow; 10] = [
        Workflow::Melody,
        Workflow::Chords,
        Workflow::Mpe,
        Workflow::Drums,
        Workflow::Guitar,
        Workflow::Harmonies,
        Workflow::TwoVocalParts,
        Workflow::VocalTuning,
        Workflow::UnpitchedVocals,
        Workflow::VocalSync,
    ];

    /// The name in the list.
    ///
    /// Nouns, and no "Edit" prefix. Every one of these is a task, so the
    /// verb is true of all of them and distinguishes none — it would cost
    /// width in a narrow list to say nothing.
    pub fn label(&self) -> &'static str {
        match self {
            Workflow::Melody => "Melody",
            Workflow::Chords => "Chords",
            Workflow::Mpe => "MPE",
            Workflow::Drums => "Drums",
            Workflow::Guitar => "Guitar",
            Workflow::Harmonies => "Lyrics & harmonies",
            Workflow::TwoVocalParts => "Two vocal parts",
            Workflow::VocalTuning => "Vocal tuning",
            Workflow::UnpitchedVocals => "Unpitched vocals",
            Workflow::VocalSync => "Vocal sync",
        }
    }

    /// The name on the command line and in the chooser's arg.
    pub fn slug(&self) -> &'static str {
        match self {
            Workflow::Melody => "melody",
            Workflow::Chords => "chords",
            Workflow::Mpe => "mpe",
            Workflow::Drums => "drums",
            Workflow::Guitar => "guitar",
            Workflow::Harmonies => "harmonies",
            Workflow::TwoVocalParts => "two-vocals",
            Workflow::VocalTuning => "vocal-tuning",
            Workflow::UnpitchedVocals => "unpitched-vocals",
            Workflow::VocalSync => "vocal-sync",
        }
    }

    /// Look one up by [`Workflow::slug`].
    pub fn by_slug(name: &str) -> Option<Self> {
        Workflow::ALL.into_iter().find(|w| w.slug() == name)
    }

    /// The surface this job wants.
    pub fn mode(&self) -> Mode {
        match self {
            Workflow::Melody | Workflow::Chords => Mode::Midi,
            Workflow::Mpe => Mode::Mpe,
            Workflow::Drums => Mode::Drums,
            Workflow::Guitar => Mode::Guitar,
            Workflow::Harmonies | Workflow::TwoVocalParts | Workflow::VocalSync => Mode::Vocals,
            Workflow::VocalTuning => Mode::PitchedAudio,
            Workflow::UnpitchedVocals => Mode::UnpitchedAudio,
        }
    }

    /// The material it opens with.
    fn scene(&self) -> Scene {
        match self {
            Workflow::Melody => Scene::Phrase,
            // Held notes across several rows: the only fixture that
            // actually sounds more than one pitch at a time, which is
            // what a chord readout needs.
            Workflow::Chords => Scene::Held,
            Workflow::Mpe => Scene::AllDimensions,
            // The FTS kit, with two-handed pieces and grace notes —
            // more of what drum editing actually involves than a plain
            // groove.
            Workflow::Drums => Scene::Flams,
            Workflow::Guitar => Scene::Guitar,
            Workflow::Harmonies | Workflow::TwoVocalParts | Workflow::VocalSync => Scene::Lyrics,
            // The same sung phrase, read as analysed audio rather than
            // as MIDI — which is exactly the difference the tuning job
            // turns on.
            Workflow::VocalTuning => Scene::Phrase,
            Workflow::UnpitchedVocals => Scene::Percussive,
        }
    }

    /// What is still missing, for the ones that are not finished.
    ///
    /// Shown next to the name rather than kept in a plan, so the gap is
    /// visible to whoever opens it.
    pub fn note(&self) -> Option<&'static str> {
        match self {
            Workflow::TwoVocalParts => Some("one part so far"),
            Workflow::VocalSync => Some("no reference track yet"),
            _ => None,
        }
    }

    /// Open it.
    pub fn editor(&self, viewport: Viewport) -> Editor {
        let mut ed = demo::editor(self.scene(), viewport);
        // The scene sets a mode to suit its material; the workflow
        // overrides it, because the same phrase is a different job read
        // as MIDI and read as audio — which is the whole distinction
        // between `Melody` and `VocalTuning`.
        ed.set_mode(self.mode());
        ed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every workflow opens, in the mode it claims.
    ///
    /// The claim worth pinning: a workflow is a *job with its surface
    /// already chosen*, so one that opened in some other mode would be
    /// exactly as useless as the mode list it replaced.
    #[test]
    fn every_workflow_opens_in_its_own_mode() {
        for w in Workflow::ALL {
            let ed = w.editor(Viewport::new(900.0, 480.0));
            assert_eq!(ed.mode, w.mode(), "{} opened in {:?}", w.label(), ed.mode);
        }
    }

    /// And the two vocal jobs that share a mode are still distinguished
    /// by it being *stated*, not inherited from the material.
    #[test]
    fn the_same_material_serves_two_jobs() {
        let vp = Viewport::new(900.0, 480.0);
        let melody = Workflow::Melody.editor(vp);
        let tuning = Workflow::VocalTuning.editor(vp);
        assert_eq!(melody.mode, Mode::Midi);
        assert_eq!(tuning.mode, Mode::PitchedAudio);
        assert_eq!(
            melody.doc.notes.len(),
            tuning.doc.notes.len(),
            "the same phrase, read two ways"
        );
    }

    #[test]
    fn the_unfinished_ones_say_so() {
        let noted: Vec<_> = Workflow::ALL
            .iter()
            .filter(|w| w.note().is_some())
            .collect();
        assert_eq!(noted.len(), 2, "only the stubs carry a note");
    }
}
