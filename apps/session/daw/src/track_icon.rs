//! A track's icon: what it is, at a glance — a mic, a guitar, a kit.
//!
//! Most are Phosphor's regular weight (`icons/phosphor`, MIT, its licence
//! beside the files): refined line icons, one weight, on a 256 grid. The
//! band instruments Phosphor does not draw — a kit, a bass, strings,
//! brass — are drawn here in its manner, the same grid and a stroke of
//! its weight with round ends, so they sit with the rest rather than
//! beside them.
//!
//! Which icon is read off the track's name ([`Icon::of`]): the names a
//! session's tracks already carry — "Kick", "EG 1", "BGVs" — say what the
//! track is, and a folder named for its section ("Drums") takes that
//! section's icon.

use std::sync::OnceLock;

use anyrender::PaintScene;
use vello::kurbo::{Affine, BezPath, Cap, Join, Rect, Stroke};
use vello::peniko::{Color, Fill};

/// A track's icon.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Vocal,
    Choir,
    Guitar,
    Bass,
    Drums,
    Keys,
    Synth,
    Strings,
    Brass,
    Click,
    Guide,
    Lyrics,
    Midi,
    Folder,
    Returns,
    Audio,
}

impl Icon {
    const ALL: [Self; 16] = [
        Self::Vocal,
        Self::Choir,
        Self::Guitar,
        Self::Bass,
        Self::Drums,
        Self::Keys,
        Self::Synth,
        Self::Strings,
        Self::Brass,
        Self::Click,
        Self::Guide,
        Self::Lyrics,
        Self::Midi,
        Self::Folder,
        Self::Returns,
        Self::Audio,
    ];

    /// The icon a track's name says, a folder's if the name says nothing,
    /// a waveform otherwise.
    ///
    /// Words, not substrings, for the short ones: "EG" is a guitar, and
    /// "Keg" is not.
    #[must_use]
    pub fn of(name: &str, is_folder: bool) -> Self {
        let lower = name.to_lowercase();
        let words: Vec<&str> = lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .collect();
        let word = |w: &[&str]| words.iter().any(|each| w.contains(each));
        let has = |parts: &[&str]| parts.iter().any(|p| lower.contains(p));
        // Most specific first: "Synth Bass" is a synth's bass, and reads
        // as the bass it plays; "Lead Vox" is a vocal, not a synth lead.
        if has(&["click", "metronome"]) {
            Self::Click
        } else if has(&["guide", "cue", "count"]) {
            Self::Guide
        } else if has(&["lyric"]) {
            Self::Lyrics
        } else if has(&["keyflow", "chord", "midi"]) {
            Self::Midi
        } else if has(&["bgv", "choir", "harmon", "backing"]) {
            Self::Choir
        } else if has(&["vocal", "vox", "voice", "sing"]) || word(&["ld", "lv"]) {
            Self::Vocal
        } else if has(&["bass"]) {
            Self::Bass
        } else if has(&[
            "drum", "kick", "snare", "tom", "hat", "cymbal", "overhead", "perc", "shaker", "tamb",
            "conga", "clap", "kit",
        ]) || word(&["oh", "hh", "kik", "sn"])
        {
            Self::Drums
        } else if has(&["piano", "keys", "rhodes", "wurl", "organ", "b3"]) {
            Self::Keys
        } else if has(&["guitar", "gtr", "electric", "acoustic"]) || word(&["eg", "ag"]) {
            Self::Guitar
        } else if has(&["synth", "pad", "arp"]) || word(&["lead", "sub"]) {
            Self::Synth
        } else if has(&["string", "violin", "viola", "cello", "orch"]) {
            Self::Strings
        } else if has(&["brass", "horn", "trumpet", "trombone", "sax"]) {
            Self::Brass
        } else if has(&["reverb", "delay", "return", "fx"]) {
            Self::Returns
        } else if is_folder {
            Self::Folder
        } else {
            Self::Audio
        }
    }

    /// How it is drawn.
    const fn shape(self) -> Shape {
        match self {
            Self::Vocal => Shape::Phosphor(include_str!("../icons/phosphor/microphone-stage.svg")),
            Self::Choir => Shape::Phosphor(include_str!("../icons/phosphor/users-three.svg")),
            Self::Guitar => Shape::Phosphor(include_str!("../icons/phosphor/guitar.svg")),
            Self::Keys => Shape::Phosphor(include_str!("../icons/phosphor/piano-keys.svg")),
            Self::Synth => Shape::Phosphor(include_str!("../icons/phosphor/waveform.svg")),
            Self::Click => Shape::Phosphor(include_str!("../icons/phosphor/metronome.svg")),
            Self::Guide => Shape::Phosphor(include_str!("../icons/phosphor/megaphone-simple.svg")),
            Self::Lyrics => Shape::Phosphor(include_str!("../icons/phosphor/quotes.svg")),
            Self::Midi => Shape::Phosphor(include_str!("../icons/phosphor/music-notes.svg")),
            Self::Folder => Shape::Phosphor(include_str!("../icons/phosphor/folder-simple.svg")),
            Self::Returns => {
                Shape::Phosphor(include_str!("../icons/phosphor/sliders-horizontal.svg"))
            }
            Self::Audio => Shape::Phosphor(include_str!("../icons/phosphor/headphones.svg")),
            // A snare, its head an ellipse, and the two sticks over it.
            Self::Drums => Shape::Drawn(&[
                "M40,108 A88,28 0 1,0 216,108 A88,28 0 1,0 40,108 Z",
                "M40,108 V172 A88,28 0 0,0 216,172 V108",
                "M64,28 L116,90",
                "M192,28 L140,90",
            ]),
            // A bass: its body low and left, a long neck up to a headstock
            // with its four pegs.
            Self::Bass => Shape::Drawn(&[
                "M44,176 A52,48 0 1,0 148,176 A52,48 0 1,0 44,176 Z",
                "M126,148 L212,62",
                "M204,54 L230,28",
                "M200,42 L192,34",
                "M212,30 L204,22",
                "M218,66 L226,74",
                "M230,54 L238,62",
            ]),
            // A violin: its waisted body, the neck and scroll, the bridge.
            Self::Strings => Shape::Drawn(&[
                "M128,72 C98,72 88,96 98,118 C70,128 72,204 128,216 C184,204 186,128 158,118 C168,96 158,72 128,72 Z",
                "M128,72 V28",
                "M116,170 H140",
            ]),
            // A trumpet: mouthpiece, the lead pipe, three valves, the bell.
            Self::Brass => Shape::Drawn(&[
                "M28,116 V148",
                "M28,132 H168",
                "M168,132 L224,92 V172 Z",
                "M84,96 V132",
                "M110,96 V132",
                "M136,96 V132",
                "M84,132 C84,176 168,176 168,148",
            ]),
        }
    }
}

/// How an icon is drawn: Phosphor's own SVG, whose one path is filled; or
/// paths of ours, stroked at Phosphor's regular weight.
enum Shape {
    Phosphor(&'static str),
    Drawn(&'static [&'static str]),
}

/// Phosphor's regular weight: a 16-unit line on its 256 grid.
const WEIGHT: f64 = 16.0;
/// The grid the icons are drawn on.
const GRID: f64 = 256.0;

/// Each icon's paths, parsed once.
fn paths(icon: Icon) -> &'static [BezPath] {
    static PARSED: OnceLock<Vec<Vec<BezPath>>> = OnceLock::new();
    let all = PARSED.get_or_init(|| {
        Icon::ALL
            .iter()
            .map(|icon| match icon.shape() {
                Shape::Phosphor(svg) => path_data(svg)
                    .filter_map(|d| BezPath::from_svg(d).ok())
                    .collect(),
                Shape::Drawn(ds) => ds
                    .iter()
                    .filter_map(|d| BezPath::from_svg(d).ok())
                    .collect(),
            })
            .collect()
    });
    let index = Icon::ALL.iter().position(|i| *i == icon).unwrap_or(0);
    all.get(index).map_or(&[], Vec::as_slice)
}

/// The `d` of every `<path>` in an SVG.
fn path_data(svg: &str) -> impl Iterator<Item = &str> {
    svg.split("<path").skip(1).filter_map(|element| {
        let start = element.find(" d=\"")? + 4;
        let rest = element.get(start..)?;
        rest.get(..rest.find('"')?)
    })
}

/// Draw `icon` in `color`, filling the square `at`.
pub fn paint(scene: &mut impl PaintScene, icon: Icon, at: Rect, color: Color) {
    let side = at.width().min(at.height());
    let scale = side / GRID;
    let place = Affine::translate((
        at.x0 + (at.width() - side) / 2.0,
        at.y0 + (at.height() - side) / 2.0,
    )) * Affine::scale(scale);
    let drawn = matches!(icon.shape(), Shape::Drawn(_));
    let line = Stroke::new(WEIGHT)
        .with_caps(Cap::Round)
        .with_join(Join::Round);
    for path in paths(icon) {
        if drawn {
            scene.stroke(&line, place, color, None, path);
        } else {
            scene.fill(Fill::NonZero, place, color, None, path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Icon;

    #[test]
    fn a_session_s_track_names_read_as_their_instruments() {
        for (name, folder, icon) in [
            ("Click", false, Icon::Click),
            ("Count", false, Icon::Guide),
            ("Lead Vox", false, Icon::Vocal),
            ("BGVs", true, Icon::Choir),
            ("Drums", true, Icon::Drums),
            ("Kick In", false, Icon::Drums),
            ("OH", false, Icon::Drums),
            ("Synth Bass", false, Icon::Bass),
            ("EG 1", false, Icon::Guitar),
            ("AG", false, Icon::Guitar),
            ("Piano", false, Icon::Keys),
            ("Electric", true, Icon::Guitar),
            ("Electric Piano", false, Icon::Keys),
            ("Pad", false, Icon::Synth),
            ("Strings", true, Icon::Strings),
            ("Keyflow", false, Icon::Midi),
            ("Lyrics", false, Icon::Lyrics),
            ("Band", true, Icon::Folder),
            ("Bounce 3", false, Icon::Audio),
        ] {
            assert_eq!(Icon::of(name, folder), icon, "{name}");
        }
    }

    #[test]
    fn no_icon_has_a_point_that_is_not_a_number() {
        use vello::kurbo::PathEl;
        for icon in Icon::ALL {
            for path in super::paths(icon) {
                for el in path.elements() {
                    let points: Vec<vello::kurbo::Point> = match *el {
                        PathEl::MoveTo(p) | PathEl::LineTo(p) => vec![p],
                        PathEl::QuadTo(a, b) => vec![a, b],
                        PathEl::CurveTo(a, b, c) => vec![a, b, c],
                        PathEl::ClosePath => vec![],
                    };
                    assert!(
                        points.iter().all(|p| p.x.is_finite() && p.y.is_finite()),
                        "{icon:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_icon_has_something_to_draw() {
        for icon in Icon::ALL {
            assert!(!super::paths(icon).is_empty(), "{icon:?}");
        }
    }
}
