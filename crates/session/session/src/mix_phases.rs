//! The mix phases — what a mixing session is doing right now.
//!
//! [`Mode::Mix`] is one of ten session modes; a phase is a state WITHIN
//! it. Where a mode says which surface you are working on, a phase says
//! which pass you are making over it, and that decides two things: what
//! the channel strip offers, and how much room each track is given.
//!
//! [`Mode`]: crate::modes::Mode
//!
//! # Why the phase drives track size
//!
//! Every processor visible at once is unusable — the point of a phase is
//! that most of them are not the question right now. The same is true of
//! tracks: a Depth pass is about sends and returns, so the close mics can
//! collapse to a band; a Rescue pass is the opposite. That is what
//! `Track::height` and `Track::width` are for, and why the phase is the
//! thing that sets them.
//!
//! # Offline processing is still shown
//!
//! Several steps are marked [`Stage::offline`]. They are not realtime
//! plugins — they are renders, and the result is written back to the
//! item. They appear in the strip anyway, because what matters to
//! someone mixing is "has this been de-clicked", not "is there a plugin
//! instance doing it". A step with no realtime equivalent still has a
//! state worth showing.
//!
//! # Where this came from
//!
//! The shipped `MIX_PHASE_*` actions had nine phases. This is the
//! reconciled set: `Staging` is absorbed into `Rescue`, which already
//! carries gain staging and the phase check; `Mix` is renamed
//! `Relational`, which is what its own description said it was
//! ("relational EQ, instrument/bus strips"); `Creative` is new; and
//! `Automate` is gone as a phase because automation is a property of
//! every phase from `Rescue` onward rather than a pass of its own.

use std::fmt;

/// One pass over a mix.
///
/// Ordered as the work is done. That order is not decoration — a phase
/// list is a method, and showing them out of sequence would suggest you
/// can reach for the reverb before the track is usable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MixPhase {
    /// Make the signal usable: gain, polarity, tuning, surgical repair.
    Rescue,
    /// Levels: gain staging, then the faders. Every track visible and
    /// detailed.
    Balance,
    /// The sound of each track on its own.
    Tone,
    /// Take off what the tone pass exposed.
    Polish,
    /// How tracks sit against EACH OTHER — the first phase that is about
    /// more than one track at a time.
    Relational,
    /// Where things are, front to back.
    Depth,
    /// The parts that are choices rather than corrections.
    Creative,
    /// Maximum collapse, bus skeleton — the mix as a shape.
    Overview,
}

impl MixPhase {
    /// Every phase, in working order.
    pub const ALL: [Self; 8] = [
        Self::Rescue,
        Self::Balance,
        Self::Tone,
        Self::Polish,
        Self::Relational,
        Self::Depth,
        Self::Creative,
        Self::Overview,
    ];

    /// Stable lowercase identifier, used in action IDs and on the wire.
    ///
    /// `relational` rather than the shipped `mix`: the action was
    /// `MIX_PHASE_MIX`, which named the mode twice and the phase not at
    /// all.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Rescue => "rescue",
            Self::Balance => "balance",
            Self::Tone => "tone",
            Self::Polish => "polish",
            Self::Relational => "relational",
            Self::Depth => "depth",
            Self::Creative => "creative",
            Self::Overview => "overview",
        }
    }

    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|phase| phase.slug() == slug)
    }

    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Rescue => "Rescue",
            Self::Balance => "Balance",
            Self::Tone => "Tone",
            Self::Polish => "Polish",
            Self::Relational => "Relational",
            Self::Depth => "Depth",
            Self::Creative => "Creative",
            Self::Overview => "Overview",
        }
    }

    /// The toolbar icon's base name in `Data/toolbar_icons`.
    ///
    /// Every phase names its own file. `Relational` used to borrow the
    /// shipped `fts_mix_mix`, which meant a button labelled Relational
    /// showed a pill reading MIX — a wrong icon, which is worse than a
    /// missing one, because a missing one falls back to the word.
    ///
    /// `fts-icons` builds these from `examples/mix.toml`; a name with
    /// nothing installed behind it simply does not draw.
    #[must_use]
    pub const fn icon(self) -> Option<&'static str> {
        match self {
            Self::Rescue => Some("fts_mix_rescue"),
            Self::Balance => Some("fts_mix_balance"),
            Self::Tone => Some("fts_mix_tone"),
            Self::Polish => Some("fts_mix_polish"),
            Self::Relational => Some("fts_mix_relational"),
            Self::Depth => Some("fts_mix_depth"),
            Self::Creative => Some("fts_mix_creative"),
            Self::Overview => Some("fts_mix_overview"),
        }
    }

    /// What this pass is for, in one line.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::Rescue => "Make the signal usable: gain, polarity, tuning, surgical repair",
            Self::Balance => "Levels: gain staging, then the faders",
            Self::Tone => "The sound of each track on its own",
            Self::Polish => "Take off what the tone pass exposed",
            Self::Relational => "How tracks sit against each other",
            Self::Depth => "Where things are, front to back",
            Self::Creative => "The parts that are choices rather than corrections",
            Self::Overview => "Maximum collapse, bus skeleton",
        }
    }

    /// The steps this phase offers, in the order they are worked.
    ///
    /// This is what the channel strip is built from: a phase's steps are
    /// its sections, and a step that is offline still gets one.
    #[must_use]
    pub const fn steps(self) -> &'static [Step] {
        match self {
            Self::Rescue => RESCUE_STEPS,
            Self::Balance => BALANCE_STEPS,
            Self::Tone => TONE_STEPS,
            Self::Polish => POLISH_STEPS,
            Self::Relational => RELATIONAL_STEPS,
            Self::Depth => DEPTH_STEPS,
            Self::Creative => CREATIVE_STEPS,
            // A view, not a pass — the only phase that offers nothing,
            // because looking at the mix is not a thing you apply.
            Self::Overview => &[],
        }
    }

    /// Whether automation is in play during this phase.
    ///
    /// Everything from `Rescue` on. `Automate` used to be a phase of its
    /// own, which put it after `Depth` and implied you do not ride
    /// anything until the end — automation is a property of the work,
    /// not a stage of it.
    #[must_use]
    pub const fn automates(self) -> bool {
        // Every phase: `Rescue` is the first, so the comparison is
        // always true. Kept as a method rather than deleted because the
        // question "does this phase automate" is one the strip asks, and
        // the answer being "yes, always" is a fact about the method
        // rather than an absence.
        matches!(
            self,
            Self::Rescue
                | Self::Balance
                | Self::Tone
                | Self::Polish
                | Self::Relational
                | Self::Depth
                | Self::Creative
                | Self::Overview
        )
    }
}

impl fmt::Display for MixPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

/// One step within a phase — a section of the channel strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Step {
    pub name: &'static str,
    /// Normally done for you, by a script rather than by hand.
    ///
    /// Not the same as [`Step::offline`], which is about HOW a step is
    /// applied. This is about who applies it: an automatic step usually
    /// arrives already complete, so the strip's job is to show its state
    /// and let it be overridden — not to present it as work waiting.
    pub automatic: bool,
    /// A render, not a plugin.
    ///
    /// The processing is applied to the item and written back, so there
    /// is no instance to open and no realtime cost. It still appears in
    /// the strip: what matters is whether the track HAS been de-clicked,
    /// and a step with no realtime equivalent still has a state.
    pub offline: bool,
}

impl Step {
    #[must_use]
    pub const fn live(name: &'static str) -> Self {
        Self {
            name,
            automatic: false,
            offline: false,
        }
    }

    #[must_use]
    pub const fn offline(name: &'static str) -> Self {
        Self {
            name,
            automatic: false,
            offline: true,
        }
    }

    /// The same step, marked as normally already done.
    #[must_use]
    pub const fn automatic(self) -> Self {
        Self {
            automatic: true,
            ..self
        }
    }
}

const RESCUE_STEPS: &[Step] = &[
    Step::offline("Phase Check"),
    Step::live("Live Tuning"),
    Step::live("Rescue EQ"),
    Step::offline("Consistency Compression"),
];

/// Staging, then the faders.
///
/// Gain staging sits here rather than in `Rescue` because it is the same
/// question the rest of this phase asks — how loud is this — where
/// `Rescue` is about whether the signal is usable at all. It is also the
/// one step that is normally already done: the scripts stage a session
/// on import, so what the strip shows is a state to CHECK rather than
/// work to do.
///
/// There is no step for the faders themselves. Every strip already has
/// one, and a section called "volume" would claim there is something to
/// add.
const BALANCE_STEPS: &[Step] = &[Step::offline("Gain Stage").automatic()];

const TONE_STEPS: &[Step] = &[
    Step::live("Compression"),
    Step::live("EQ"),
    Step::live("Saturation"),
    Step::live("Parallel Filters"),
    Step::live("Parallel Compression"),
];

const POLISH_STEPS: &[Step] = &[Step::live("De-Essing"), Step::live("Spectral Suppressors")];

const RELATIONAL_STEPS: &[Step] = &[
    Step::live("Relational EQ"),
    Step::live("Parallel Filters (Super Separators)"),
    Step::live("Side-Chain Dynamics"),
];

const DEPTH_STEPS: &[Step] = &[
    Step::live("Panning"),
    Step::live("Reverb"),
    Step::live("Delay"),
    Step::live("Ambience Layers"),
];

const CREATIVE_STEPS: &[Step] = &[
    Step::live("Delay and Verb Throws"),
    Step::live("Creative Filter Effects"),
    Step::live("Sound Design Effects"),
];

/// The steps shown while in [`Mode::Edit`], which is a MODE and not a
/// phase.
///
/// Editing happens before mixing and its strip is the offline repair
/// chain. It lives here because it is the same kind of thing — a list of
/// steps a strip is built from — and keeping it beside the phases is
/// what stops a second, differently-shaped model growing for it.
///
/// [`Mode::Edit`]: crate::modes::Mode::Edit
pub const EDIT_STEPS: &[Step] = &[
    Step::offline("De-Clip"),
    Step::offline("De-Click"),
    Step::offline("De-Essing"),
    Step::offline("Time Alignment"),
    Step::offline("Offline Tuning (Melodyne)"),
    Step::offline("Gating"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_round_trip() {
        for phase in MixPhase::ALL {
            assert_eq!(MixPhase::from_slug(phase.slug()), Some(phase));
        }
    }

    /// Every phase has a distinct slug — they become action IDs.
    #[test]
    fn slugs_are_unique() {
        let mut slugs: Vec<_> = MixPhase::ALL.iter().map(|p| p.slug()).collect();
        slugs.sort_unstable();
        let before = slugs.len();
        slugs.dedup();
        assert_eq!(slugs.len(), before);
    }

    /// Automation runs from the first phase onward, so every phase
    /// automates — the flag exists to say so explicitly rather than to
    /// carve out an exception that no longer exists.
    #[test]
    fn automation_runs_throughout() {
        assert!(MixPhase::ALL.iter().all(|p| p.automates()));
    }

    /// `Overview` is a view rather than a pass, and the only phase with
    /// nothing to apply.
    #[test]
    fn only_overview_has_no_steps() {
        for phase in MixPhase::ALL {
            assert_eq!(
                phase.steps().is_empty(),
                phase == MixPhase::Overview,
                "{phase} steps"
            );
        }
    }

    /// Gain staging is in `Balance`, not `Rescue` — same question as the
    /// faders beside it — and is the step that normally arrives done.
    #[test]
    fn staging_is_a_balance_step_and_automatic() {
        let staging = MixPhase::Balance
            .steps()
            .iter()
            .find(|step| step.name == "Gain Stage")
            .expect("Balance stages gain");
        assert!(staging.automatic);
        assert!(
            MixPhase::Rescue
                .steps()
                .iter()
                .all(|step| step.name != "Gain Stage")
        );
    }

    /// The edit chain is offline throughout — that is what makes it the
    /// edit chain rather than a mix phase.
    #[test]
    fn editing_is_offline() {
        assert!(EDIT_STEPS.iter().all(|step| step.offline));
    }
}
