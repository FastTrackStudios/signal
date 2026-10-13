//! Scaffold a fresh REAPER project from a structured spec.
//!
//! Pure RPP construction — no keyflow/DAW deps — so the keyflow CLI (and
//! anything else) can generate a `.rpp` offline. Consumers build a
//! [`ScaffoldSpec`] (tracks with folder nesting, section regions, key
//! signatures) and get back RPP text via [`build_scaffold_rpp`].
//!
//! Tracks and regions are serialized through the tested builder/`RppSerialize`
//! path; the full project header and the `<KEYSIG>` block (which the base
//! serializer doesn't model) are injected. See
//! `crates/keyflow/docs/REAPER_SCAFFOLD_FORMAT.md` for the format.

use crate::builder::{ReaperProjectBuilder, TrackBuilder};
use crate::types::RppSerialize;

/// A key signature at a measure. `root` is pitch class 0–11 (0=C), `accidental`
/// is the spelling preference (`1` sharp-ward, `-1` flat-ward), `scale_mask` is
/// the 12-bit chromatic scale mask (`0xAB5` = major).
#[derive(Debug, Clone)]
pub struct KeySigSpec {
    pub measure: u32,
    pub root: u8,
    pub accidental: i8,
    pub scale_mask: u32,
}

/// Major-scale mask (C D E F G A B).
pub const SCALE_MASK_MAJOR: u32 = 0xAB5;

/// Where a track sits in the folder nesting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderRole {
    /// Opens a folder (this track is the parent).
    Start,
    /// A normal child track.
    Child,
    /// The last child — closes the folder.
    End,
}

#[derive(Debug, Clone)]
pub struct TrackSpec {
    pub name: String,
    pub color: Option<u32>,
    pub folder: FolderRole,
}

/// A song section laid out on the seconds timeline → a REAPER region.
#[derive(Debug, Clone)]
pub struct SectionSpec {
    pub name: String,
    pub start_seconds: f64,
    pub end_seconds: f64,
}

#[derive(Debug, Clone)]
pub struct ScaffoldSpec {
    pub timestamp: i64,
    pub bpm: f64,
    pub time_sig: (i32, i32),
    pub tracks: Vec<TrackSpec>,
    pub sections: Vec<SectionSpec>,
    pub key_sigs: Vec<KeySigSpec>,
}

/// Serialize a [`ScaffoldSpec`] to REAPER `.rpp` text.
pub fn build_scaffold_rpp(spec: &ScaffoldSpec) -> String {
    let mut pb = ReaperProjectBuilder::new()
        .timestamp(spec.timestamp)
        .tempo_with_time_sig(spec.bpm, spec.time_sig.0, spec.time_sig.1)
        .sample_rate(48_000);

    for track in &spec.tracks {
        let mut tb = TrackBuilder::new(track.name.clone());
        if let Some(color) = track.color {
            tb = tb.color(color);
        }
        tb = match track.folder {
            FolderRole::Start => tb.folder_start(),
            FolderRole::End => tb.folder_end(1),
            FolderRole::Child => tb,
        };
        pb = pb.add_track(tb.build());
    }

    let project = pb.build();
    let base = project.to_rpp_string();

    // The base serializer emits a minimal header (RIPPLE/SAMPLERATE/TEMPO) +
    // tracks. Inject the fuller header fields, the <KEYSIG> block, and the
    // section regions right after the `<REAPER_PROJECT …` version line.
    // (Regions are emitted directly — the generic marker serializer writes a
    // `B`-type marker with a truncated end line, not the `… 1 R {guid}` region
    // pair REAPER needs.)
    let inject = format!(
        "{}{}{}",
        header_fields(),
        keysig_block(&spec.key_sigs),
        regions_block(&spec.sections),
    );
    match base.find('\n') {
        Some(nl) => {
            let mut out = String::with_capacity(base.len() + inject.len());
            out.push_str(&base[..=nl]);
            out.push_str(&inject);
            out.push_str(&base[nl + 1..]);
            out
        }
        None => base,
    }
}

/// Standard project-header fields (2-space indent) that the base serializer
/// omits, terminated by an empty `<PROJBAY>`. Kept close to a real REAPER 7.x
/// project so the file opens cleanly.
fn header_fields() -> String {
    "  GROUPOVERRIDE 0 0 0\n\
     \x20 AUTOXFADE 1\n\
     \x20 ENVATTACH 3\n\
     \x20 POOLEDENVATTACH 0\n\
     \x20 MIXERUIFLAGS 11 48\n\
     \x20 PEAKGAIN 1\n\
     \x20 FEEDBACK 0\n\
     \x20 PANLAW 1\n\
     \x20 PROJOFFS 0 0 0\n\
     \x20 MAXPROJLEN 0 0\n\
     \x20 GRID 3199 8 1 8 1 0 0 0\n\
     \x20 TIMEMODE 1 5 -1 30 0 0 -1\n\
     \x20 PANMODE 3\n\
     \x20 PANLAWFLAGS 3\n\
     \x20 CURSOR 0\n\
     \x20 ZOOM 100 0 0\n\
     \x20 VZOOMEX 6 0\n\
     \x20 USE_REC_CFG 0\n\
     \x20 RECMODE 1\n\
     \x20 LOOP 0\n\
     \x20 LOOPGRAN 0 4\n\
     \x20 RECORD_PATH \"\" \"\"\n\
     \x20 RENDER_FILE \"\"\n\
     \x20 RENDER_PATTERN \"\"\n\
     \x20 TIMELOCKMODE 1\n\
     \x20 TEMPOENVLOCKMODE 1\n\
     \x20 ITEMMIX 1\n\
     \x20 DEFPITCHMODE 589824 0\n\
     \x20 TAKELANE 1\n\
     \x20 <PROJBAY\n\
     \x20 >\n"
        .to_string()
}

/// Section regions as REAPER region-marker pairs (`… 1 R {guid}`), the format
/// REAPER actually reads (see `setlist_rpp.rs`). Empty `{}` guids are filled by
/// REAPER on load; regions pair start↔end by matching id.
fn regions_block(sections: &[SectionSpec]) -> String {
    let mut out = String::new();
    for (i, section) in sections.iter().enumerate() {
        let id = i + 1;
        let name = crate::rpp_tree::RToken::to_safe_string(&section.name);
        out.push_str(&format!(
            "  MARKER {id} {} {name} 1 0 1 R {{}} 0\n",
            section.start_seconds
        ));
        out.push_str(&format!(
            "  MARKER {id} {} \"\" 1 0 1 R {{}} 0\n",
            section.end_seconds
        ));
    }
    out
}

/// The `<KEYSIG>` block, or empty if there are no key signatures.
fn keysig_block(key_sigs: &[KeySigSpec]) -> String {
    if key_sigs.is_empty() {
        return String::new();
    }
    let mut out = String::from("  <KEYSIG\n");
    for ks in key_sigs {
        out.push_str(&format!(
            "    {} {} {} 0x{:X}\n",
            ks.measure, ks.root, ks.accidental, ks.scale_mask
        ));
    }
    out.push_str("  >\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffold_has_header_folder_tracks_regions_keysig() {
        let spec = ScaffoldSpec {
            timestamp: 1_700_000_000,
            bpm: 120.0,
            time_sig: (4, 4),
            tracks: vec![
                TrackSpec {
                    name: "Keyflow".into(),
                    color: None,
                    folder: FolderRole::Start,
                },
                TrackSpec {
                    name: "KEY".into(),
                    color: None,
                    folder: FolderRole::Child,
                },
                TrackSpec {
                    name: "CHORD".into(),
                    color: None,
                    folder: FolderRole::Child,
                },
                TrackSpec {
                    name: "MELODY".into(),
                    color: None,
                    folder: FolderRole::Child,
                },
                TrackSpec {
                    name: "SCALE".into(),
                    color: None,
                    folder: FolderRole::End,
                },
            ],
            sections: vec![
                SectionSpec {
                    name: "IN".into(),
                    start_seconds: 0.0,
                    end_seconds: 8.0,
                },
                SectionSpec {
                    name: "VS".into(),
                    start_seconds: 8.0,
                    end_seconds: 24.0,
                },
            ],
            key_sigs: vec![
                KeySigSpec {
                    measure: 0,
                    root: 0,
                    accidental: 1,
                    scale_mask: SCALE_MASK_MAJOR,
                },
                KeySigSpec {
                    measure: 8,
                    root: 1,
                    accidental: -1,
                    scale_mask: SCALE_MASK_MAJOR,
                },
            ],
        };

        let rpp = build_scaffold_rpp(&spec);

        assert!(rpp.starts_with("<REAPER_PROJECT 0.1"));
        assert!(rpp.contains("PANMODE 3"), "full header injected");
        assert!(rpp.contains("<PROJBAY"));
        assert!(rpp.contains("<KEYSIG"));
        assert!(rpp.contains("0 0 1 0xAB5"));
        assert!(rpp.contains("8 1 -1 0xAB5"));
        assert!(rpp.contains("TEMPO 120 4 4"));
        assert!(rpp.contains("NAME Keyflow"));
        assert!(rpp.contains("NAME SCALE"));
        // Folder open (ISBUS 1 1) + close (ISBUS 2 -1).
        assert!(rpp.contains("ISBUS 1 1"), "folder opens");
        assert!(rpp.contains("ISBUS 2 -1"), "folder closes");
        // Two regions in the `… 1 R {guid}` pair format.
        assert!(rpp.contains("MARKER 1 0 IN 1 0 1 R {} 0"));
        assert!(rpp.contains("MARKER 2 8 VS 1 0 1 R {} 0"));
        // Ends with the project chunk close.
        assert!(rpp.trim_end().ends_with('>'));
    }
}
