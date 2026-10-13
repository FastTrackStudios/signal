//! The taxonomy kind every template-created folder and track carries.
//!
//! A scene's selectors match **kinds, never names** (spec #48, "Scenes as
//! data"): a user may rename `Process` to `Parallel` and the Drum FX scene
//! must still find it. So the builder writes each track's kind into its
//! ext-state (REAPER `P_EXT`, the `EXT` line of a track chunk) under
//! [`KIND_KEY`], and [`read_kinds`] is how it comes back out of a project
//! file.

use std::fmt;

/// The ext-state key the kind is stored under. Namespaced `FTS:` so it
/// sits beside other extensions' data without a collision.
pub const KIND_KEY: &str = "FTS:kind";

/// The ext-state key the template group path is stored under, for the
/// tracks that stand for a template group (`Drums/Drum Kit/Kick`).
pub const GROUP_KEY: &str = "FTS:group";

/// What a template-created track *is*, independent of what it is named.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// An instrument folder standing for a template group: `Drum Kit`,
    /// `Bass`, `Electric`, `Keys`, `Vocals`.
    Group,
    /// One sound source: `Kick`, `Snare`, `Tom 1`, the bass `Guitar`.
    Piece,
    /// The folder that sums a piece's close mics.
    Sum,
    /// A close mic or a direct source under a piece: `In`, `Out`, `Top`,
    /// `T1`, `DI`, `Amp`, `Close`.
    Source,
    /// The drum's trigger: what says when it was hit — a recorded piezo
    /// (audio) or the kit's own notes (MIDI). It keys the drum's gates and
    /// drives its samples; it is never heard.
    Trigger,
    /// The drum's samples: notes from its trigger's hits, played by a
    /// sampler and blended on the piece — beside the `Trig` that drives
    /// it. A track, or a folder when there is more than one sample.
    Sample,
    /// A one-note fundamental blended under the mics.
    Fundamental,
    /// The kick's sub, a one-note track beside its Sum.
    Sub,
    /// A reverb return, or the folder holding a bank of them.
    Verb,
    /// The kit's parallel processing folder.
    Process,
    /// A balance group of parallel compressors.
    Compress,
    /// An effects folder or one of its units: `Inst FX`, `Delay`, `Mod`.
    Fx,
    /// A send return that is not a reverb: a delay, a widener, a pitch.
    Return,
    /// A part played on its own track: `Rhythm`, `Lead`, `Piano`, `Pad`,
    /// `Doubles`.
    Part,
    /// A bus: a track with no items, unarmed, in the bus tree.
    Bus,
    /// The root of the bus tree — the folder every stem sums into.
    MixBus,
    /// The Guide folder: the count and the pulse the band tracks to.
    /// `flow.scenes.guide-folder`.
    Guide,
    /// The Keyflow folder: the song's knowledge as MIDI items.
    /// `flow.scenes.keyflow-folder`.
    Keyflow,
    /// A performer's headphone bus. `flow.scenes.performer-headphones`.
    Headphones,
    /// A VCA: a fader that controls other faders and carries no audio
    /// of its own.
    ///
    /// A kind rather than the word "VCA" in the name, because the
    /// language VCAs are named for what they control (`VOX EN`) and a
    /// classifier that went by the name would miss them — and because
    /// a VCA that reaches no bus is FINISHED, while an ordinary track
    /// that reaches none is work for a human. The two must not look
    /// alike to the organizer.
    Vca,
}

impl Kind {
    /// Every kind, in declaration order.
    pub const ALL: [Self; 20] = [
        Self::Group,
        Self::Piece,
        Self::Sum,
        Self::Source,
        Self::Trigger,
        Self::Sample,
        Self::Fundamental,
        Self::Sub,
        Self::Verb,
        Self::Process,
        Self::Compress,
        Self::Fx,
        Self::Return,
        Self::Part,
        Self::Bus,
        Self::MixBus,
        Self::Guide,
        Self::Keyflow,
        Self::Headphones,
        Self::Vca,
    ];

    /// The stable string written to ext-state.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Group => "group",
            Self::Piece => "piece",
            Self::Sum => "sum",
            Self::Source => "source",
            Self::Trigger => "trigger",
            Self::Sample => "sample",
            Self::Fundamental => "fundamental",
            Self::Sub => "sub",
            Self::Verb => "verb",
            Self::Process => "process",
            Self::Compress => "compress",
            Self::Fx => "fx",
            Self::Return => "return",
            Self::Part => "part",
            Self::Bus => "bus",
            Self::MixBus => "mix-bus",
            Self::Guide => "guide",
            Self::Keyflow => "keyflow",
            Self::Headphones => "headphones",
            Self::Vca => "vca",
        }
    }

    /// The kind a stored string names, if it names one.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One track's ext-state as read back from a project file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackExt {
    /// The track's GUID, braces included, as written on its `<TRACK` line.
    pub guid: String,
    /// The track's name.
    pub name: String,
    /// The kind, when the track carries a parseable one.
    pub kind: Option<Kind>,
    /// The template group path, when the track stands for a template group.
    pub group: Option<String>,
}

/// Every track in a project file with what its ext-state says it is.
///
/// A line scan of the `<TRACK` chunks rather than a typed parse:
/// `dawfile-reaper` writes `EXT` lines but does not read them back yet,
/// and the scene engine that will consume kinds through the facade is a
/// later ticket. Tracks are returned in project order; a track with no
/// `EXT FTS:kind` line has `kind: None`, which is how a test tells a
/// template-created track from one a user added.
#[must_use]
pub fn read_kinds(rpp: &str) -> Vec<TrackExt> {
    let mut out: Vec<TrackExt> = Vec::new();
    let mut depth = 0_usize;
    let mut current: Option<TrackExt> = None;
    for raw in rpp.lines() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix("<TRACK") {
            if depth == 1 {
                current = Some(TrackExt {
                    guid: rest.trim().to_string(),
                    name: String::new(),
                    kind: None,
                    group: None,
                });
            }
            depth = depth.saturating_add(1);
        } else if line.starts_with('<') {
            depth = depth.saturating_add(1);
        } else if line == ">" {
            depth = depth.saturating_sub(1);
            if depth == 1 {
                if let Some(track) = current.take() {
                    out.push(track);
                }
            }
        } else if depth == 2 {
            if let Some(track) = current.as_mut() {
                if let Some(name) = line.strip_prefix("NAME ") {
                    track.name = name.trim().trim_matches('"').to_string();
                } else if let Some(rest) = line.strip_prefix("EXT ") {
                    if let Some((key, value)) = rest.split_once(' ') {
                        if key == KIND_KEY {
                            track.kind = Kind::parse(value.trim());
                        } else if key == GROUP_KEY {
                            track.group = Some(value.trim().to_string());
                        }
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_round_trips_through_its_string() {
        for kind in Kind::ALL {
            assert_eq!(Kind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(Kind::parse("kick"), None, "a name is not a kind");
    }

    #[test]
    fn kinds_read_back_from_track_chunks_and_a_plain_track_has_none() {
        let rpp = "<REAPER_PROJECT 0.1 \"7.0\" 0\n  <TRACK {A}\n    NAME \"Kick\"\n    EXT FTS:kind piece\n    EXT FTS:group Drums/Drum Kit/Kick\n    <ITEM\n      NAME \"Kick 1\"\n    >\n  >\n  <TRACK {B}\n    NAME \"Talkback\"\n  >\n>\n";
        let tracks = read_kinds(rpp);
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks.first().map(|t| t.kind), Some(Some(Kind::Piece)));
        assert_eq!(
            tracks.first().and_then(|t| t.group.as_deref()),
            Some("Drums/Drum Kit/Kick")
        );
        assert_eq!(tracks.first().map(|t| t.guid.as_str()), Some("{A}"));
        // The negative control: a track nobody tagged reads as untagged
        // rather than as some default kind.
        assert_eq!(tracks.get(1).map(|t| t.kind), Some(None));
        assert_eq!(tracks.get(1).map(|t| t.name.as_str()), Some("Talkback"));
    }
}
