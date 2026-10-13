//! Reading REAPER's `<KEYSIG>` block.
//!
//! REAPER's key signature is not reachable through the extension API — it
//! lives in the project chunk:
//!
//! ```text
//! <KEYSIG
//!   0 0 1 0xAB5
//!   4 1 1 0xAB5
//!   8 1 -1 0xAB5
//! >
//! ```
//!
//! Four fields per line: **measure**, **root**, **accidental**, **scale
//! mask**.
//!
//! The field that isn't obvious is `accidental`. It does *not* modify the
//! root — it chooses how the key is spelled. Root is already a pitch
//! class 0-11, so root `1` with accidental `+1` is C♯ major and the same
//! root with `-1` is D♭ major: identical pitches, different signature.
//! The fixture this module is tested against was built to demonstrate
//! exactly that, pairing 4/8 (C♯/D♭) and 16/20 (D♯/E♭).
//!
//! `accidental` takes `0` as well as ±1 — `0` means spell it naturally.
//! That matters at the edges of the circle: root `11` with `0` is B
//! major, and the same root with `-1` is C♭ major. Those two cases come
//! from a Cockos forum thread (t=287164) rather than the fixture, which
//! never uses root 11.
//!
//! `scale_mask` is a 12-bit set with the root at bit 0, so `0xAB5` —
//! bits 0,2,4,5,7,9,11 — is the major scale.
//!
//! The write side lives in [`crate::scaffold`]; this is the reader.

/// One key-signature change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeySig {
    /// Measure the change takes effect, 0-based.
    pub measure: u32,
    /// Root pitch class, 0-11.
    pub root: u8,
    /// How to spell it: negative for flats, zero for naturals, positive
    /// for sharps. Not an offset — the root already carries the pitch.
    pub accidental: i8,
    /// 12-bit scale set, root at bit 0.
    pub scale_mask: u32,
}

/// Major scale: bits 0,2,4,5,7,9,11.
pub const SCALE_MASK_MAJOR: u32 = 0xAB5;

const SHARP_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];
/// Flat spellings. Note 4 and 11: in a flat key those are F♭ and C♭, not
/// E and B — the naive table returns the natural name and gets C♭ major
/// wrong.
const FLAT_NAMES: [&str; 12] = [
    "C", "Db", "D", "Eb", "Fb", "F", "Gb", "G", "Ab", "A", "Bb", "Cb",
];

impl KeySig {
    /// The root spelled the way this signature asks for.
    pub fn root_name(&self) -> &'static str {
        let table = if self.accidental < 0 {
            &FLAT_NAMES
        } else {
            &SHARP_NAMES
        };
        table[usize::from(self.root % 12)]
    }

    /// Semitones of the scale above the root.
    pub fn scale_degrees(&self) -> Vec<u8> {
        (0..12u8)
            .filter(|i| self.scale_mask >> i & 1 == 1)
            .collect()
    }

    /// Whether the mask is the major scale.
    pub fn is_major(&self) -> bool {
        self.scale_mask == SCALE_MASK_MAJOR
    }

    /// Human-readable, e.g. `"Db major"`.
    pub fn display(&self) -> String {
        let quality = if self.is_major() { "major" } else { "scale" };
        format!("{} {quality}", self.root_name())
    }
}

/// Pull every key signature out of a project chunk.
///
/// Returns an empty vec when the project has none — which is the common
/// case, since REAPER only writes the block once a key is set.
pub fn parse(chunk: &str) -> Vec<KeySig> {
    let mut out = Vec::new();
    let mut inside = false;

    for line in chunk.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<KEYSIG") {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        if trimmed == ">" {
            break;
        }
        if let Some(sig) = parse_line(trimmed) {
            out.push(sig);
        }
    }
    out
}

fn parse_line(line: &str) -> Option<KeySig> {
    let mut parts = line.split_whitespace();
    let measure = parts.next()?.parse().ok()?;
    let root = parts.next()?.parse().ok()?;
    let accidental = parts.next()?.parse().ok()?;
    let mask_text = parts.next()?;
    let scale_mask = mask_text
        .strip_prefix("0x")
        .or_else(|| mask_text.strip_prefix("0X"))
        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
        .or_else(|| mask_text.parse().ok())?;

    Some(KeySig {
        measure,
        root,
        accidental,
        scale_mask,
    })
}

/// The line ending a chunk uses.
///
/// REAPER writes CRLF on every platform. A rewriter that assumes `\n`
/// finds nothing, changes nothing, and reports success — so this is
/// detected rather than assumed.
fn line_ending(chunk: &str) -> &'static str {
    if chunk.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Render key signatures as the `<KEYSIG>` block REAPER writes.
///
/// Empty input yields an empty string — REAPER omits the block entirely
/// when a project has none, and writing an empty one back would be a
/// gratuitous difference.
pub fn render(sigs: &[KeySig]) -> String {
    render_with(sigs, "\n")
}

fn render_with(sigs: &[KeySig], eol: &str) -> String {
    if sigs.is_empty() {
        return String::new();
    }
    let mut out = format!("  <KEYSIG{eol}");
    for sig in sigs {
        out.push_str(&format!(
            "    {} {} {} 0x{:X}{eol}",
            sig.measure, sig.root, sig.accidental, sig.scale_mask
        ));
    }
    out.push_str(&format!("  >{eol}"));
    out
}

/// Replace a project chunk's `<KEYSIG>` block with `sigs`.
///
/// Splices rather than rewrites: every other line is passed through
/// untouched, because this runs over a real project file and the only
/// safe edit is the smallest one. Where there is no block yet, the new
/// one goes just before the first `<TRACK`, which is where REAPER puts
/// it. Line endings are preserved.
pub fn splice(chunk: &str, sigs: &[KeySig]) -> String {
    let eol = line_ending(chunk);
    let mut out: Vec<String> = Vec::new();
    let mut inside = false;
    let mut wrote = false;

    let block_lines = |sigs: &[KeySig]| -> Vec<String> {
        if sigs.is_empty() {
            return Vec::new();
        }
        let mut v = vec!["  <KEYSIG".to_string()];
        for sig in sigs {
            v.push(format!(
                "    {} {} {} 0x{:X}",
                sig.measure, sig.root, sig.accidental, sig.scale_mask
            ));
        }
        v.push("  >".to_string());
        v
    };

    for line in chunk.split(eol) {
        if !inside && line.trim_start().starts_with("<KEYSIG") {
            inside = true;
            out.extend(block_lines(sigs));
            wrote = true;
            continue;
        }
        if inside {
            // Between `<KEYSIG` and its closing `>` are the old
            // signatures; they go.
            if line.trim() == ">" {
                inside = false;
            }
            continue;
        }
        if !wrote && !sigs.is_empty() && line.trim_start().starts_with("<TRACK") {
            out.extend(block_lines(sigs));
            wrote = true;
        }
        out.push(line.to_string());
    }

    if !wrote && !sigs.is_empty() {
        out.extend(block_lines(sigs));
    }
    out.join(eol)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real project, built to exercise key signatures — ten changes
    /// including deliberate enharmonic pairs.
    const FIXTURE: &str = include_str!("../tests/fixtures/key-signatures.RPP");

    #[test]
    fn reads_every_change_from_a_real_project() {
        let sigs = parse(FIXTURE);
        assert_eq!(sigs.len(), 10, "the fixture has ten key changes");
        assert_eq!(sigs[0].measure, 0);
        assert_eq!(sigs.last().expect("last").measure, 60);
    }

    /// The point of the format: root is a pitch class and `accidental`
    /// picks the spelling. Same root, opposite sign, different key.
    #[test]
    fn accidental_selects_spelling_not_pitch() {
        let sigs = parse(FIXTURE);
        let sharp = sigs.iter().find(|s| s.measure == 4).expect("measure 4");
        let flat = sigs.iter().find(|s| s.measure == 8).expect("measure 8");

        assert_eq!(sharp.root, flat.root, "same pitch class");
        assert_eq!(sharp.root_name(), "C#");
        assert_eq!(flat.root_name(), "Db");
    }

    /// The other enharmonic pair in the fixture.
    #[test]
    fn d_sharp_and_e_flat_share_a_root() {
        let sigs = parse(FIXTURE);
        let sharp = sigs.iter().find(|s| s.measure == 16).expect("measure 16");
        let flat = sigs.iter().find(|s| s.measure == 20).expect("measure 20");
        assert_eq!(sharp.root, 3);
        assert_eq!(sharp.root_name(), "D#");
        assert_eq!(flat.root_name(), "Eb");
    }

    #[test]
    fn the_whole_fixture_decodes_to_the_expected_keys() {
        let actual: Vec<String> = parse(FIXTURE).iter().map(KeySig::display).collect();
        assert_eq!(
            actual,
            [
                "C major", "C# major", "Db major", "D major", "D# major", "Eb major", "E major",
                "F major", "Ab major", "E major",
            ]
        );
    }

    #[test]
    fn mask_ab5_is_the_major_scale() {
        let sig = parse(FIXTURE).into_iter().next().expect("first");
        assert!(sig.is_major());
        assert_eq!(sig.scale_degrees(), vec![0, 2, 4, 5, 7, 9, 11]);
    }

    /// A project with no key set has no block, and that must read as
    /// "none" rather than failing.
    #[test]
    fn a_project_without_a_block_yields_nothing() {
        assert!(parse("<REAPER_PROJECT 0.1\n  TEMPO 120 4 4\n>").is_empty());
    }

    /// Parsing must stop at the block's closing `>` and not wander into
    /// the tracks that follow.
    #[test]
    fn stops_at_the_end_of_the_block() {
        let chunk = "<KEYSIG\n  0 7 1 0xAB5\n>\n<TRACK\n  0 0 0 0x0\n>";
        let sigs = parse(chunk);
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0].root_name(), "G");
    }
}

#[cfg(test)]
mod forum_cases {
    use super::*;

    /// From the Cockos thread that documents this format (t=287164):
    /// root 11 spelled flat is C♭ major, spelled natural it's B major.
    /// The fixture has no root 11, so a naive flat table (…, "Bb", "B")
    /// passes every fixture test and still gets this wrong.
    #[test]
    fn root_eleven_spells_c_flat_or_b() {
        let c_flat = parse("<KEYSIG\n  0 11 -1 0xAB5\n>");
        assert_eq!(c_flat[0].root_name(), "Cb");
        assert_eq!(c_flat[0].display(), "Cb major");

        let b = parse("<KEYSIG\n  0 11 0 0xAB5\n>");
        assert_eq!(b[0].root_name(), "B");
        assert_eq!(b[0].display(), "B major");
    }

    /// Zero is a valid accidental, not just ±1.
    #[test]
    fn natural_spelling_is_accepted() {
        let sigs = parse("<KEYSIG\n  0 7 0 0xAB5\n>");
        assert_eq!(sigs[0].accidental, 0);
        assert_eq!(sigs[0].root_name(), "G");
    }
}

#[cfg(test)]
mod write_tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/key-signatures.RPP");

    /// Rendering what we parsed must reproduce REAPER's own block — the
    /// property that makes writing into a real project safe.
    #[test]
    fn render_round_trips_the_fixture() {
        let sigs = parse(FIXTURE);
        let rendered = render(&sigs);
        assert_eq!(parse(&rendered), sigs);
        for line in ["0 0 1 0xAB5", "8 1 -1 0xAB5", "60 4 1 0xAB5"] {
            assert!(rendered.contains(line), "missing `{line}` in\n{rendered}");
        }
    }

    /// Splicing must change the KEYSIG block and nothing else. A project
    /// file is someone's work; an edit that reformats it is not
    /// acceptable even if it parses.
    #[test]
    fn splice_touches_only_the_block() {
        let replacement = [KeySig {
            measure: 0,
            root: 7,
            accidental: 1,
            scale_mask: SCALE_MASK_MAJOR,
        }];
        let out = splice(FIXTURE, &replacement);

        assert_eq!(parse(&out), replacement, "the block is replaced");
        // Everything outside the block survives untouched.
        for marker in [
            "<REAPER_PROJECT",
            "<TRACK",
            "IMGRESOURCEFLAGS",
            "<TEMPOENVEX",
        ] {
            assert_eq!(
                out.matches(marker).count(),
                FIXTURE.matches(marker).count(),
                "{marker} count changed"
            );
        }
        assert_eq!(out.matches("<KEYSIG").count(), 1, "exactly one block");
    }

    /// A project with no key signatures gains a block in the right place.
    #[test]
    fn splice_inserts_when_absent() {
        let chunk = "<REAPER_PROJECT 0.1\n  TEMPO 120 4 4\n  <TRACK\n  >\n>";
        let out = splice(
            chunk,
            &[KeySig {
                measure: 0,
                root: 5,
                accidental: -1,
                scale_mask: SCALE_MASK_MAJOR,
            }],
        );
        assert_eq!(parse(&out)[0].root_name(), "F");
        assert!(
            out.find("<KEYSIG").expect("block") < out.find("<TRACK").expect("track"),
            "the block belongs before the tracks"
        );
    }

    /// Clearing removes the block rather than leaving an empty one.
    #[test]
    fn splicing_nothing_removes_the_block() {
        let out = splice(FIXTURE, &[]);
        assert!(!out.contains("<KEYSIG"), "no empty block left behind");
        assert!(out.contains("<TRACK"), "the rest survives");
    }
}
