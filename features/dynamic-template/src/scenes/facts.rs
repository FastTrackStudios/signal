//! What the engine knows about a track: the input side of the seam.
//!
//! A [`Fact`] is a track as the taxonomy sees it — its kind, its place
//! in the template's group tree, the dimensions parsed out of its name.
//! It is deliberately NOT a `daw_proto::Track`: the engine resolves
//! against sessions read from REAPER, from a project file and from the
//! golden fixture's own tree, and each of those is an adapter that
//! builds facts. The engine never learns which one it is talking to.

use crate::golden_session::Kind;

/// One step of a track's taxonomy path: an enclosing folder, or the
/// track itself when it is one.
///
/// Three ways to say the same folder, because that is what makes a
/// selector survive a rename. `kind` is what the template made it,
/// `template` is the group it stands for, and `name` is what it is
/// called today — a selector segment matches if it equals any of them,
/// which is how `("process" "compress")` finds a Process folder a user
/// renamed to Parallel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Segment {
    /// The taxonomy kind the template wrote into ext-state.
    pub kind: Option<Kind>,
    /// The last element of the template group path this folder stands
    /// for (`Drums/Drum Kit/Kick` → `Kick`).
    pub template: Option<String>,
    /// What the folder is called.
    pub name: String,
}

impl Segment {
    /// A segment for a folder that only has a name.
    #[must_use]
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            kind: None,
            template: None,
            name: name.into(),
        }
    }

    /// The same segment, carrying a kind.
    #[must_use]
    pub const fn of(mut self, kind: Kind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// The same segment, standing for a template group.
    #[must_use]
    pub fn standing_for(mut self, template: impl Into<String>) -> Self {
        self.template = Some(template.into());
        self
    }

    /// Whether a selector's path segment names this folder.
    #[must_use]
    pub fn answers_to(&self, want: &str) -> bool {
        if self.name.eq_ignore_ascii_case(want) {
            return true;
        }
        if self
            .template
            .as_deref()
            .is_some_and(|t| t.eq_ignore_ascii_case(want))
        {
            return true;
        }
        self.kind
            .is_some_and(|k| k.as_str().eq_ignore_ascii_case(want))
    }
}

/// One track, as the scene engine sees it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fact {
    /// The track's stable identity.
    pub guid: String,
    /// What it is called.
    pub name: String,
    /// Its position in the project's track list.
    pub index: u32,
    /// How deep it sits, 0 at the top.
    pub depth: u32,
    /// Whether it is a folder parent.
    pub is_folder: bool,
    /// What the template made it, when it says.
    pub kind: Option<Kind>,
    /// Its taxonomy path: every enclosing folder outermost first, then
    /// the track itself when it is a folder. A selector's `group` is a
    /// PREFIX of this.
    pub path: Vec<Segment>,
    /// The Performer dimension.
    pub performer: Option<String>,
    /// The Layer dimension.
    pub layer: Option<String>,
    /// The Channel dimension.
    pub channel: Option<String>,
    /// The multi-mic dimension.
    pub multi_mic: Option<String>,
    /// The Arrangement dimension.
    pub arrangement: Option<String>,
    /// The Language dimension (`flow.vocals.language`): read from the
    /// track's own name, or inherited from the nearest enclosing
    /// Language folder — see `super::adapt::Walk`.
    pub language: Option<super::language::Language>,
    /// One half of a stereo pair — see the engine invariant in
    /// [`super::resolve`].
    pub pair_half: bool,
    /// In the mixer and not the track panel (the drums' Process): never a
    /// row on the arrangement, whatever a scene says.
    pub mixer_only: bool,
}

impl Fact {
    /// A leaf with a name, for tests and for sessions with no taxonomy.
    #[must_use]
    pub fn leaf(guid: &str, name: &str, index: u32, depth: u32) -> Self {
        Self {
            guid: guid.to_owned(),
            name: name.to_owned(),
            index,
            depth,
            ..Self::default()
        }
    }

    /// The same, as a folder.
    #[must_use]
    pub fn folder(guid: &str, name: &str, index: u32, depth: u32) -> Self {
        Self {
            is_folder: true,
            ..Self::leaf(guid, name, index, depth)
        }
    }

    /// The same, carrying a kind.
    #[must_use]
    pub const fn of(mut self, kind: Kind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// The same, at a taxonomy path.
    #[must_use]
    pub fn at(mut self, path: Vec<Segment>) -> Self {
        self.path = path;
        self
    }

    /// The same, carrying a language.
    #[must_use]
    pub const fn speaking(mut self, language: super::language::Language) -> Self {
        self.language = Some(language);
        self
    }

    /// The grouping key `Rank::TopmostPerInstrument` counts by: the
    /// taxonomy path, so every mic of one piece shares it and two
    /// pieces do not.
    #[must_use]
    pub fn group_key(&self) -> String {
        if self.path.is_empty() {
            return self.name.clone();
        }
        self.path
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join("/")
    }
}

/// Build the taxonomy path of a track from the folders above it.
///
/// `stack` is the enclosing folders outermost first. A folder that
/// stands for a template group restarts the path at that group's own
/// path, because a template group IS an absolute address: `Rhodes` is
/// `Keys/Electric/Rhodes` wherever the user filed it.
#[must_use]
pub fn path_of(
    stack: &[Segment],
    own: Option<&Segment>,
    template: Option<&[String]>,
) -> Vec<Segment> {
    if let Some(full) = template {
        let mut out: Vec<Segment> = full
            .iter()
            .map(|name| Segment::named(name.clone()))
            .collect();
        if let (Some(last), Some(own)) = (out.last_mut(), own) {
            last.kind = own.kind;
            last.template = Some(last.name.clone());
        }
        return out;
    }
    let mut out = stack.to_vec();
    if let Some(own) = own {
        out.push(own.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three ways to name one folder, which is what a rename
    /// survives on: the kind the template wrote, the group it stands
    /// for, and what it is called today.
    #[test]
    fn a_segment_answers_to_its_kind_its_group_and_its_name() {
        let seg = Segment::named("Parallel")
            .of(Kind::Process)
            .standing_for("Process");
        assert!(seg.answers_to("Parallel"), "its name today");
        assert!(seg.answers_to("process"), "the kind the template wrote");
        assert!(seg.answers_to("Process"), "the group it stands for");
        assert!(!seg.answers_to("Compress"));
    }

    /// A template group is an absolute address, so a folder standing for
    /// one restarts the path rather than hanging off wherever it sits.
    #[test]
    fn a_template_group_restarts_the_path() {
        let stack = vec![Segment::named("Keys")];
        let own = Segment::named("Rhodes").of(Kind::Part);
        let path = path_of(
            &stack,
            Some(&own),
            Some(&[
                "Keys".to_owned(),
                "Electric".to_owned(),
                "Rhodes".to_owned(),
            ]),
        );
        let names: Vec<&str> = path.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Keys", "Electric", "Rhodes"]);
        assert_eq!(path.last().and_then(|s| s.kind), Some(Kind::Part));
    }

    /// Without one, the path is simply where the track sits.
    #[test]
    fn without_a_template_group_the_path_is_the_tree() {
        let stack = vec![Segment::named("Process").of(Kind::Process)];
        let own = Segment::named("Compress").of(Kind::Compress);
        let path = path_of(&stack, Some(&own), None);
        assert_eq!(path.len(), 2);
        assert!(path[1].answers_to("compress"));
    }
}
