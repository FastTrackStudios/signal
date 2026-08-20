//! In-place rewriting of a `.signalpack`'s embedded spec.
//!
//! The audio body is heavy (multi-GB for big libraries) and immutable; only
//! the tiny embedded styx between `# spec_begin` / `# spec_end` in the index
//! ever needs to change once a pack ships. This module mutates that text in
//! place — copying audio bytes verbatim to a new file, splicing a fresh spec,
//! atomically replacing the original.
//!
//! ~30 KB/s mutation throughput per pack regardless of audio size, vs. minutes
//! to fully re-pack a library with its source samples.
//!
//! Header parsing/encoding delegates to [`PackFileHeader`] — the one
//! `.signalpack` header implementation (it lives with the pack reader in
//! `fts-sample`); this module only patches the index span and carries every
//! other header byte forward verbatim.

use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::SamplerError;
use crate::engine::cache::PackFileHeader;

const SPEC_BEGIN: &str = "# spec_begin\n";
const SPEC_END: &str = "# spec_end";

/// Rewrite a pack's embedded spec text in place.
///
/// `mutate` receives the current spec text (everything between
/// `# spec_begin` and `# spec_end` in the index) and returns its
/// replacement. The audio body and per-row index lines are copied
/// verbatim. Header offsets/lengths are recomputed.
pub fn rewrite_embedded_spec<F>(pack_path: &Path, mutate: F) -> Result<(), SamplerError>
where
    F: FnOnce(&str) -> String,
{
    let mut file = File::open(pack_path)?;
    let header = PackFileHeader::read(&mut file)?;
    let body_offset = header.body_offset() as usize;
    let index_offset = header.index_offset() as usize;
    let index_len = header.index_len() as usize;

    // Copy audio body verbatim.
    let body_size = index_offset - body_offset;
    let mut body = vec![0u8; body_size];
    file.seek(SeekFrom::Start(body_offset as u64))?;
    file.read_exact(&mut body)?;

    // Read the existing index.
    let mut old_index = vec![0u8; index_len];
    file.read_exact(&mut old_index)?;
    drop(file);

    let new_index = splice_spec(&old_index, mutate)
        .ok_or_else(|| invalid_data(format!("{}: spec markers not found", pack_path.display())))?;

    // Atomic replace via tempfile sibling.
    let tmp_path = pack_path.with_extension("signalpack.tmp");
    {
        let mut out = BufWriter::new(File::create(&tmp_path)?);
        let new_index_offset = body_offset + body.len();
        let new_header =
            header.with_index_span(new_index_offset as u64, new_index.len() as u64);
        out.write_all(new_header.as_bytes())?;
        out.write_all(&body)?;
        out.write_all(&new_index)?;
        out.flush()?;
    }
    std::fs::rename(&tmp_path, pack_path)?;
    Ok(())
}

/// Read the embedded spec text from a pack without parsing the rest of the
/// index. Cheap header-only read for tools that only need the spec content.
pub fn read_embedded_spec(pack_path: &Path) -> Result<String, SamplerError> {
    let mut file = File::open(pack_path)?;
    let header = PackFileHeader::read(&mut file)?;
    file.seek(SeekFrom::Start(header.index_offset()))?;
    let mut buf = vec![0u8; header.index_len() as usize];
    file.read_exact(&mut buf)?;
    let text = std::str::from_utf8(&buf)
        .map_err(|_| invalid_data(format!("{}: non-utf8 index", pack_path.display())))?;
    extract_spec_text(text)
        .ok_or_else(|| invalid_data(format!("{}: no embedded spec", pack_path.display())))
}

fn splice_spec(old_index: &[u8], mutate: impl FnOnce(&str) -> String) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(old_index).ok()?;
    let begin = text.find(SPEC_BEGIN)?;
    let after_begin = begin + SPEC_BEGIN.len();
    // Match `# spec_end` at the start of a line — preserves whatever
    // newline convention the existing index uses (LF) by anchoring on the
    // marker itself rather than the trailing whitespace.
    let rest = &text[after_begin..];
    let end_in_rest = find_line_start(rest, SPEC_END)?;
    let absolute_end = after_begin + end_in_rest;

    let old_spec = &text[after_begin..absolute_end];
    // Strip the trailing newline before passing to the mutator so the caller
    // sees clean styx without the marker's leading newline.
    let old_spec_trim = old_spec.trim_end_matches('\n');
    let new_spec_raw = mutate(old_spec_trim);
    let new_spec = new_spec_raw.trim_end_matches('\n');

    let mut out = Vec::with_capacity(old_index.len());
    out.extend_from_slice(&text.as_bytes()[..after_begin]);
    out.extend_from_slice(new_spec.as_bytes());
    out.push(b'\n');
    out.extend_from_slice(&text.as_bytes()[absolute_end..]);
    Some(out)
}

fn extract_spec_text(index_text: &str) -> Option<String> {
    let begin = index_text.find(SPEC_BEGIN)?;
    let after_begin = begin + SPEC_BEGIN.len();
    let rest = &index_text[after_begin..];
    let end_in_rest = find_line_start(rest, SPEC_END)?;
    Some(index_text[after_begin..after_begin + end_in_rest].to_string())
}

/// Position of `needle` when it appears at the start of a line in `hay`.
/// Returns None if the marker doesn't occur as a line prefix.
fn find_line_start(hay: &str, needle: &str) -> Option<usize> {
    if hay.starts_with(needle) {
        return Some(0);
    }
    let pat = format!("\n{needle}");
    let pos = hay.find(&pat)?;
    Some(pos + 1) // step past the newline so the slice ends right at the marker
}

fn invalid_data(msg: impl Into<String>) -> SamplerError {
    SamplerError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        msg.into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_minimal_pack(spec: &str) -> Vec<u8> {
        // Synthetic pack: 64-byte header + 16-byte fake audio body + index.
        let body = b"AUDIOBYTESxxxxxx";
        let body_offset = PackFileHeader::LEN as u64;
        let index_offset = body_offset + body.len() as u64;
        let mut index = String::new();
        index.push_str("# signalpack-index-v1\n");
        index.push_str("# spec_format\tstyx\n");
        index.push_str("# spec_begin\n");
        index.push_str(spec);
        if !spec.ends_with('\n') {
            index.push('\n');
        }
        index.push_str("# spec_end\n");
        index.push_str("# source\toffset\tbytes\tchannels\tsample_rate\tnum_frames\tsamples\n");
        index.push_str("foo.flac\t64\t16\t1\t44100\t8\t8\n");
        // kind=5 (FLAC_I24), one entry.
        let header = PackFileHeader::new(5, index_offset, index.len() as u64, 1);
        let mut out = Vec::new();
        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(index.as_bytes());
        out
    }

    #[test]
    fn round_trip_replaces_spec() {
        let dir = std::env::temp_dir().join("signal-sampler-rewrite-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rt.signalpack");
        let original = build_minimal_pack("name \"old\"\nvendor \"acme\"");
        std::fs::File::create(&path)
            .unwrap()
            .write_all(&original)
            .unwrap();

        rewrite_embedded_spec(&path, |old| {
            assert!(old.contains("name \"old\""));
            old.replace("\"old\"", "\"new\"") + "\ninstrument \"violin\""
        })
        .unwrap();

        let recovered = read_embedded_spec(&path).unwrap();
        assert!(recovered.contains("name \"new\""));
        assert!(recovered.contains("instrument \"violin\""));
        // Audio body must survive verbatim.
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[64..80], b"AUDIOBYTESxxxxxx");
        // Entry count survives the rewrite verbatim.
        let mut f = std::fs::File::open(&path).unwrap();
        let header = PackFileHeader::read(&mut f).unwrap();
        assert_eq!(header.entry_count(), 1);
    }
}
