//! What a scene *is*: the table, its rules, and what a rule does.
//!
//! Every type here derives [`Facet`] from day one, because the styx step
//! that follows (spec #48, "Scenes as data") is a loader and nothing
//! else: the same table, read from a file instead of compiled in. The
//! round-trip test in this module is what keeps that promise honest.

use facet::Facet;

use super::selector::Selector;

/// How much room a track gets on a surface.
///
/// A CLASS rather than a pixel count, because the same scene is applied
/// to two panels and to screens from 1080p to a 5120 ultrawide. A rule
/// says "this track is something you are working on" and the surface
/// decides what that comes to — see [`super::surface::SurfaceTables`],
/// which is the one place a class becomes a number.
#[derive(Facet, Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum Size {
    /// As small as the surface allows. Present, not read.
    Minimum,
    /// Enough for the controls that identify and route it: a bus.
    #[default]
    Compact,
    /// The surface's normal size — REAPER's own.
    Normal,
    /// Enough to work on: the processing is visible and legible.
    Working,
    /// Everything the surface can give one track. For editing a
    /// plugin's parameters rather than for mixing.
    Focus,
}

impl Size {
    /// Every size, smallest first — the order the pixel tables are in.
    pub const ALL: [Self; 5] = [
        Self::Minimum,
        Self::Compact,
        Self::Normal,
        Self::Working,
        Self::Focus,
    ];

    /// Its index into a per-surface pixel table.
    #[must_use]
    pub const fn slot(self) -> usize {
        match self {
            Self::Minimum => 0,
            Self::Compact => 1,
            Self::Normal => 2,
            Self::Working => 3,
            Self::Focus => 4,
        }
    }
}

/// What a scene does to a folder and to what it holds.
///
/// One enum for both surfaces, because a fold is a fact about the tree
/// rather than about a panel: REAPER's `I_FOLDERCOMPACT` and the
/// window's own folder state are two renderings of the same decision.
#[derive(Facet, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum Fold {
    /// The folder and its rows.
    #[default]
    Open,
    /// REAPER's half-compact: the folder small, its rows still there.
    Small,
    /// The folder stays as one row; the rows inside it go.
    Collapsed,
    /// The folder and everything in it go.
    Hidden,
    /// The folder's own row goes; the rows inside it stay, lifted to its
    /// depth — a piece's Sum while editing, where the mics are the rows
    /// that matter and the folder summing them is not.
    Lifted,
}

impl Fold {
    /// Whether the rows beneath a folder in this state survive.
    #[must_use]
    pub const fn keeps_children(self) -> bool {
        matches!(self, Self::Open | Self::Small | Self::Lifted)
    }

    /// REAPER's `I_FOLDERCOMPACT` value for a folder in this state.
    ///
    /// `Hidden` has none — a hidden folder is not compacted, it is not
    /// shown at all, which is a different REAPER field.
    #[must_use]
    pub const fn compact(self) -> Option<i32> {
        match self {
            // REAPER cannot drop a folder's row and keep its tracks: a
            // lifted folder shows open there.
            Self::Open | Self::Lifted => Some(0),
            Self::Small => Some(1),
            Self::Collapsed => Some(2),
            Self::Hidden => None,
        }
    }
}

/// What a rule does to the tracks it matches.
///
/// Both fields are optional, and an absent one does not change what an
/// earlier rule decided. That independence is what makes rules
/// composable rather than an all-or-nothing overwrite: a rule that
/// widens a track should not have to restate whether it is folded, and
/// a scene that rails every bus must not thereby unfold the Guide
/// folder the common prelude closed.
#[derive(Facet, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[facet(rename_all = "kebab-case")]
pub struct Effect {
    /// How much room the track gets.
    #[facet(default)]
    pub size: Option<Size>,
    /// What happens to the folder and its rows. Meaningless on a leaf
    /// beyond `Hidden`, which drops the row.
    #[facet(default)]
    pub fold: Option<Fold>,
}

impl Effect {
    /// A size, and nothing said about folding.
    #[must_use]
    pub const fn at(size: Size) -> Self {
        Self {
            size: Some(size),
            fold: None,
        }
    }

    /// The same effect, folded.
    #[must_use]
    pub const fn folded(self, fold: Fold) -> Self {
        Self {
            fold: Some(fold),
            ..self
        }
    }

    /// Gone, with whatever it holds. Says nothing about size, because a
    /// row that is not drawn has none.
    #[must_use]
    pub const fn hidden() -> Self {
        Self {
            size: None,
            fold: Some(Fold::Hidden),
        }
    }

    /// This effect over an earlier one: what it states wins, what it
    /// leaves out survives.
    #[must_use]
    pub fn over(self, earlier: Resolved) -> Resolved {
        Resolved {
            size: self.size.unwrap_or(earlier.size),
            fold: self.fold.unwrap_or(earlier.fold),
        }
    }
}

/// What a track ends up with once every rule has had its say.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Resolved {
    /// How much room it gets.
    pub size: Size,
    /// What happens to the rows beneath it.
    pub fold: Fold,
}

impl From<Effect> for Resolved {
    /// The scene's `default` as a starting point: what it does not say,
    /// the engine's own floor says.
    fn from(effect: Effect) -> Self {
        Self {
            size: effect.size.unwrap_or_default(),
            fold: effect.fold.unwrap_or_default(),
        }
    }
}

/// One rule: which tracks, and what happens to them.
///
/// `effect` is what both surfaces do; `arrange` and `mixer` override it
/// for one surface only, which is how a scene can rail a track in the
/// mixer and still give its row a waveform's worth of height.
#[derive(Facet, Debug, Clone, Default, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
pub struct Rule {
    /// Which tracks.
    #[facet(default)]
    pub selector: Selector,
    /// What happens to them on both surfaces.
    #[facet(default)]
    pub effect: Effect,
    /// The arrangement's own answer, when it differs.
    #[facet(default)]
    pub arrange: Option<Effect>,
    /// The mixer's own answer, when it differs.
    #[facet(default)]
    pub mixer: Option<Effect>,
}

impl Rule {
    /// A rule that does one thing to everything the selector matches.
    #[must_use]
    pub const fn new(selector: Selector, effect: Effect) -> Self {
        Self {
            selector,
            effect,
            arrange: None,
            mixer: None,
        }
    }

    /// What this rule does on one surface.
    #[must_use]
    pub fn for_surface(&self, surface: Surface) -> Effect {
        match surface {
            Surface::Arrange => self.arrange.unwrap_or(self.effect),
            Surface::Mixer => self.mixer.unwrap_or(self.effect),
        }
    }
}

/// Which panel a scene is being resolved for.
#[derive(Facet, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum Surface {
    /// The arrangement's track panel: rows, so heights.
    #[default]
    Arrange,
    /// The mixer: strips, so widths.
    Mixer,
}

/// Who a scene is for.
///
/// `flow.scenes.two-audiences`: a flow has views for two audiences where
/// they differ — the engineer's full view, with every track the flow
/// touches, and the player's overview, which is what the person
/// performing needs to see and nothing else.
#[derive(Facet, Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum Audience {
    /// Every track the flow touches.
    #[default]
    Engineer,
    /// What the person performing needs to see, and nothing else.
    Player,
}

impl Audience {
    /// Every audience, in declaration order.
    pub const ALL: [Self; 2] = [Self::Engineer, Self::Player];

    /// The stable slug.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Engineer => "engineer",
            Self::Player => "player",
        }
    }
}

/// How a scene's rows are grouped.
///
/// `flow.scenes.performer-order`: tracking sorts by performer, editing
/// and mixing by arrangement. The performer grouping is a VIEW — the
/// project's own folders stay arrangement-sorted — so it is a property
/// of the scene rather than of the session. Emitting the header rows is
/// #51; the variant and this field exist so that ticket only adds the
/// emitting.
#[derive(Facet, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
pub enum GroupBy {
    /// The project's own order: every rhythm guitar together.
    #[default]
    Arrangement,
    /// One header row per performer under each instrument folder.
    Performer,
}

/// A scene: which tracks are shown, how large, and which folders are
/// folded, for one moment of a session.
///
/// Not a list of track ids — a rule resolved against the session's own
/// taxonomy, so it fits a real session whose shape differs from the
/// reference. Rules are ordered and **last match wins**; `default`
/// covers everything no rule matched.
#[derive(Facet, Debug, Clone, Default, PartialEq, Eq)]
#[facet(rename_all = "kebab-case")]
pub struct Scene {
    /// What the window prints.
    pub name: String,
    /// The stable id, `<instrument>-<flow>`.
    pub slug: String,
    /// What a rail can print: three or four letters. A rail is
    /// forty-four pixels wide, and "Lead Vocal FX Edit" is not a label
    /// - deriving one from the name would be guesswork, so it is said.
    pub short: String,
    /// Which instrument the scene is about — the first half of its slug,
    /// and the middle term of the follow-mode triple.
    pub instrument: String,
    /// The DAW modes this scene is the default for, by slug. Empty means
    /// recall-only: the number keys reach it, no mode opens it.
    #[facet(default)]
    pub modes: Vec<String>,
    /// Who it is for.
    #[facet(default)]
    pub audience: Audience,
    /// How its rows are grouped.
    #[facet(default)]
    pub group_by: GroupBy,
    /// The flow rule ids this scene implements, so its render fixture is
    /// their `r[verify]` and the coverage check finds it.
    #[facet(default)]
    pub spec: Vec<String>,
    /// What happens to a track no rule matched.
    #[facet(default)]
    pub default: Effect,
    /// The rules, in order. Last match wins.
    #[facet(default)]
    pub rules: Vec<Rule>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenes::selector::{Rank, Role};

    /// The whole reason every type here derives Facet: the styx step is
    /// a loader and nothing else, which is only true if the table
    /// survives a round trip through a serialiser today.
    #[test]
    fn a_scene_round_trips_through_facet() {
        let scene = Scene {
            name: "Drum Tracking".to_owned(),
            slug: "drum-tracking".to_owned(),
            short: "Trk".to_owned(),
            instrument: "drums".to_owned(),
            modes: vec!["record".to_owned()],
            audience: Audience::Player,
            group_by: GroupBy::Performer,
            spec: vec!["flow.drums.tracking.full".to_owned()],
            default: Effect::at(Size::Compact),
            rules: vec![Rule {
                selector: Selector {
                    group: vec!["Drums".to_owned(), "Drum Kit".to_owned()],
                    kind: Some("source".to_owned()),
                    role: Role::Leaf,
                    rank: Rank::TopmostPerInstrument,
                    performer: Some("Cody".to_owned()),
                    layer: Some("Main".to_owned()),
                    channel: Some("L".to_owned()),
                    multi_mic: Some("Amp A 57".to_owned()),
                    arrangement: Some("Rhythm".to_owned()),
                    language: Some(crate::scenes::Language::En),
                    name: Some("Talkback".to_owned()),
                    depth: None,
                },
                effect: Effect::at(Size::Working).folded(Fold::Collapsed),
                arrange: Some(Effect::at(Size::Focus)),
                mixer: Some(Effect::hidden()),
            }],
        };
        let json = facet_json::to_string(&scene).expect("a scene serialises");
        let back: Scene = facet_json::from_str(&json).expect("a scene comes back");
        assert_eq!(back, scene);
    }

    /// Every fold either keeps the rows under it or does not, and the
    /// two that do not are the two that mean something different in
    /// REAPER's own field.
    #[test]
    fn a_fold_says_what_happens_to_the_rows_beneath_it() {
        assert!(Fold::Open.keeps_children());
        assert!(Fold::Small.keeps_children());
        assert!(!Fold::Collapsed.keeps_children());
        assert!(!Fold::Hidden.keeps_children());
        assert_eq!(Fold::Open.compact(), Some(0));
        assert_eq!(Fold::Small.compact(), Some(1));
        assert_eq!(Fold::Collapsed.compact(), Some(2));
        assert_eq!(Fold::Hidden.compact(), None, "hidden is not a compact");
    }

    /// A rule with no per-surface override says the same thing twice;
    /// one with an override says two different things.
    #[test]
    fn a_surface_override_replaces_the_shared_effect() {
        let both = Rule::new(Selector::default(), Effect::at(Size::Working));
        assert_eq!(both.for_surface(Surface::Arrange), both.effect);
        assert_eq!(both.for_surface(Surface::Mixer), both.effect);
        let split = Rule {
            mixer: Some(Effect::at(Size::Minimum)),
            ..both
        };
        assert_eq!(
            split.for_surface(Surface::Arrange).size,
            Some(Size::Working)
        );
        assert_eq!(split.for_surface(Surface::Mixer).size, Some(Size::Minimum));
    }
}
