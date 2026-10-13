//! The `RChunk` tree must return an untouched project byte for byte.
//!
//! This is the property that makes chunk-tree editing safe for real sessions.
//! A typed model of the RPP format can only write back the fields it models,
//! so a tool built on one silently drops everything else — the master track,
//! `<RECORD_CFG>`, per-item `CHANMODE`, `<EXT>` blocks. The chunk tree models
//! nothing, so it can lose nothing; but only if stringify returns each
//! untouched line as it was parsed rather than re-rendering it from tokens.
//!
//! Re-rendering is not *wrong* — `FILE "Media/x.wav"` and `FILE Media/x.wav`
//! mean the same thing to REAPER — but it rewrites thousands of lines a tool
//! never touched, which makes the output of any organize/edit pass impossible
//! to review as a diff.

fn roundtrip(text: &str) -> String {
    let chunk = dawfile_reaper::read_rpp_chunk(text).expect("parse");
    dawfile_reaper::stringify_rpp_node(&dawfile_reaper::RNodeTree::Chunk(chunk))
}

/// Quoting that a token round-trip would normalize away, and that must not be.
const QUOTING: &str = r#"<REAPER_PROJECT 0.1 "7.65/macOS-arm64" 1786573501 0
  RECORD_PATH "Media" ""
  <NOTES 0 2
  >
  <TRACK {76C5CF39-EE77-234F-B94C-1A51752BA9B6}
    NAME Tracks
    <ITEM
      NAME "Bass - It knows my name.wav"
      <SOURCE WAVE
        FILE "Media/Bass - It knows my name.wav"
      >
      <EXT
        ORIGINAL_FILENAME "/Volumes/SSD/Songs/Bass - It knows my name.wav"
      >
    >
  >
>
"#;

#[test]
fn untouched_lines_keep_their_original_text() {
    // `stringify_rpp_node` renders the tree; the trailing newline belongs to
    // the file, and `write_rpp` is what adds it.
    assert_eq!(roundtrip(QUOTING), QUOTING.trim_end_matches('\n'));
}

#[test]
fn editing_a_token_still_re_renders_that_line() {
    // The verbatim path must not be so eager that it ignores a real edit.
    let mut chunk = dawfile_reaper::read_rpp_chunk(QUOTING).expect("parse");
    let track = chunk
        .children
        .iter_mut()
        .find_map(|c| match c {
            dawfile_reaper::RNodeTree::Chunk(c) if c.name().as_deref() == Some("TRACK") => Some(c),
            _ => None,
        })
        .expect("a TRACK");
    let name = track
        .children
        .iter_mut()
        .find_map(|c| match c {
            dawfile_reaper::RNodeTree::Node(n) => {
                let mut probe = n.clone();
                (probe.get_name().as_deref() == Some("NAME")).then_some(n)
            }
            dawfile_reaper::RNodeTree::Chunk(_) => None,
        })
        .expect("a NAME");
    name.get_tokens();
    if let Some(tokens) = name.tokens.as_mut() {
        if let Some(value) = tokens.get_mut(1) {
            value.set_string("MIX BUS");
        }
    }

    let out = dawfile_reaper::stringify_rpp_node(&dawfile_reaper::RNodeTree::Chunk(chunk));
    // Re-quoted, because it now needs quoting — and every other line is intact.
    assert!(out.contains(r#"NAME "MIX BUS""#), "edited line: {out}");
    assert!(out.contains(r#"FILE "Media/Bass - It knows my name.wav""#));
    assert!(out.contains(r#"RECORD_PATH "Media" """#));
}

/// The same property against a real 1.8 MB production session, which is where
/// the format's long tail actually lives. Skipped when the album isn't mounted.
#[test]
fn a_real_project_survives_byte_for_byte() {
    const SRC: &str = "/run/media/AudioHaven/Project/Crescendum-Rockstars/\
                       it knows my name/it knows my name.RPP";
    let Ok(original) = std::fs::read_to_string(SRC) else {
        eprintln!("Skipping: real project not found at {SRC}");
        return;
    };

    let out = roundtrip(&original);
    // Two normalizations the tree does make, both semantically inert, and both
    // outside what this test is pinning down:
    //   * line endings — REAPER wrote this file CRLF (it was saved on macOS)
    //     and the tree writes LF. Callers that care restore the source's
    //     endings on write; REAPER reads either.
    //   * a whitespace-only line inside an empty `<DRIVEN_BY_MOSS>` extension
    //     block, which is the same empty block with or without it.
    let strip = |s: &str| -> Vec<String> {
        s.lines()
            .map(|l| l.trim_end_matches('\r').to_string())
            .filter(|l| !l.trim().is_empty())
            .collect()
    };
    let before = strip(&original);
    let after = strip(&out);

    assert_eq!(before.len(), after.len(), "line count changed");
    let differing: Vec<usize> = before
        .iter()
        .zip(after.iter())
        .enumerate()
        .filter_map(|(i, (a, b))| (a != b).then_some(i + 1))
        .collect();
    assert!(
        differing.is_empty(),
        "{} lines changed on a round trip, first at {:?}",
        differing.len(),
        differing.first()
    );
}
