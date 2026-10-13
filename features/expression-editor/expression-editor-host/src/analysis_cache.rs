//! Detected drum hits, kept on disk so a reopen is not a re-analysis.
//!
//! Opening a kit costs about forty-four seconds before the window can
//! appear, and almost none of it is the UI: `track_timeline` decodes and
//! composes the whole song for every mic — eighteen of them, three
//! minutes each, serially, because the daw facade is not `Send` — and
//! then onset detection runs over the result.
//!
//! What comes out the other side is small. A track's analysis is a peak
//! envelope at one value per 512 samples (~67 KB for three minutes) and a
//! list of hit times. Everything expensive is in producing them, and
//! nothing about them changes unless the project or its media does. So
//! they are written beside the project and read back on the next open,
//! which is the same trade `.reapeaks` makes for waveform peaks one layer
//! down.
//!
//! **Validity is deliberately coarse.** The composed timeline depends on
//! the media, on where the items sit, on their lengths and takes and
//! volumes — everything the `.rpp` records. Rather than track which of
//! those a given track's audio actually depends on, the whole cache is
//! keyed on the project file's own mtime: edit the project at all and
//! every entry is stale. That will re-analyse after edits that could not
//! have changed the audio, and it will never serve a stale hit, which is
//! the error worth avoiding — a wrong hit list looks like a detector bug
//! and costs an afternoon.
//!
//! Written as plain bytes rather than JSON: the peaks are the bulk and
//! they are `f32`s, which text would quadruple for no benefit, since
//! nothing but this module ever reads the file.

use std::path::{Path, PathBuf};

/// Bump when the detector changes what it produces from the same audio.
/// An entry written by a different version is ignored, not migrated.
const FORMAT: u32 = 1;
const MAGIC: &[u8; 8] = b"FTSDRUM\x01";

/// One track's analysis: everything `percussion_doc` needs that the audio
/// was decoded to find.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Analysis {
    pub sample_rate: f64,
    /// Length in samples, for the frame count the document is built on.
    pub samples: u64,
    /// One peak per 512 samples — the backdrop the lanes draw.
    pub peaks: Vec<f32>,
    /// `(seconds, loudness)` per detected hit.
    pub hits: Vec<(f64, f64)>,
}

/// Where a project's analyses live: one directory beside the `.rpp`, so
/// deleting it is obvious and never touches the recording.
fn dir(project: &Path) -> PathBuf {
    let mut name = project.file_name().unwrap_or_default().to_os_string();
    name.push(".fts-analysis");
    project.with_file_name(name)
}

/// The stamp an entry must match to be used: the project's own mtime.
fn stamp(project: &Path) -> Option<u64> {
    std::fs::metadata(project)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// A track's file. Keyed by guid, which is stable across reopens in a way
/// a name or an index is not.
fn entry(project: &Path, track_guid: &str) -> PathBuf {
    // Guids come from the project file; keep only characters that cannot
    // walk out of the directory.
    let safe: String = track_guid
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    dir(project).join(format!("{safe}.bin"))
}

/// Read a track's analysis, if one was written for this exact project.
pub(crate) fn load(project: &Path, track_guid: &str) -> Option<Analysis> {
    let want = stamp(project)?;
    let bytes = std::fs::read(entry(project, track_guid)).ok()?;
    decode(&bytes, want)
}

/// Write a track's analysis. Best-effort: a read-only or full disk costs
/// the next open its speed, never this one its correctness.
pub(crate) fn store(project: &Path, track_guid: &str, analysis: &Analysis) {
    let Some(stamp) = stamp(project) else { return };
    let path = entry(project, track_guid);
    let Some(parent) = path.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    // Written to a neighbour and renamed, so a kill mid-write leaves the
    // previous entry rather than a truncated one that would decode to
    // fewer hits than the take has.
    let temp = path.with_extension("bin.part");
    if std::fs::write(&temp, encode(analysis, stamp)).is_ok() {
        let _ = std::fs::rename(&temp, &path);
    }
}

fn encode(a: &Analysis, stamp: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(64 + a.peaks.len() * 4 + a.hits.len() * 16);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT.to_le_bytes());
    out.extend_from_slice(&stamp.to_le_bytes());
    out.extend_from_slice(&a.sample_rate.to_le_bytes());
    out.extend_from_slice(&a.samples.to_le_bytes());
    out.extend_from_slice(&(a.peaks.len() as u64).to_le_bytes());
    out.extend_from_slice(&(a.hits.len() as u64).to_le_bytes());
    for p in &a.peaks {
        out.extend_from_slice(&p.to_le_bytes());
    }
    for (at, loudness) in &a.hits {
        out.extend_from_slice(&at.to_le_bytes());
        out.extend_from_slice(&loudness.to_le_bytes());
    }
    out
}

/// Decode, returning `None` for anything that is not exactly what this
/// version wrote for a project with this mtime. Every length is checked
/// against the bytes actually present: a truncated file must read as a
/// miss, not as a take with no hits in its second half.
fn decode(bytes: &[u8], want_stamp: u64) -> Option<Analysis> {
    const HEADER: usize = 8 + 4 + 8 + 8 + 8 + 8 + 8;
    if bytes.len() < HEADER || &bytes[0..8] != MAGIC {
        return None;
    }
    if u32::from_le_bytes(bytes[8..12].try_into().ok()?) != FORMAT {
        return None;
    }
    if u64::from_le_bytes(bytes[12..20].try_into().ok()?) != want_stamp {
        return None;
    }
    let sample_rate = f64::from_le_bytes(bytes[20..28].try_into().ok()?);
    let samples = u64::from_le_bytes(bytes[28..36].try_into().ok()?);
    let peak_count = u64::from_le_bytes(bytes[36..44].try_into().ok()?) as usize;
    let hit_count = u64::from_le_bytes(bytes[44..52].try_into().ok()?) as usize;
    if bytes.len() != HEADER + peak_count * 4 + hit_count * 16 {
        return None;
    }
    let mut at = HEADER;
    let mut peaks = Vec::with_capacity(peak_count);
    for _ in 0..peak_count {
        peaks.push(f32::from_le_bytes(bytes[at..at + 4].try_into().ok()?));
        at += 4;
    }
    let mut hits = Vec::with_capacity(hit_count);
    for _ in 0..hit_count {
        let t = f64::from_le_bytes(bytes[at..at + 8].try_into().ok()?);
        let l = f64::from_le_bytes(bytes[at + 8..at + 16].try_into().ok()?);
        hits.push((t, l));
        at += 16;
    }
    Some(Analysis {
        sample_rate,
        samples,
        peaks,
        hits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Analysis {
        Analysis {
            sample_rate: 48_000.0,
            samples: 96_000,
            peaks: vec![0.0, 0.5, 1.0, 0.25],
            hits: vec![(0.5, 0.9), (1.25, 0.4)],
        }
    }

    #[test]
    fn a_written_analysis_reads_back_exactly() {
        let a = sample();
        assert_eq!(decode(&encode(&a, 42), 42), Some(a));
    }

    #[test]
    fn a_different_project_mtime_is_a_miss() {
        // The whole point: an edited project must never serve old hits.
        assert_eq!(decode(&encode(&sample(), 42), 43), None);
    }

    #[test]
    fn a_truncated_file_is_a_miss_not_a_short_take() {
        let bytes = encode(&sample(), 42);
        for cut in [0, 1, 8, 20, bytes.len() - 1] {
            assert_eq!(
                decode(&bytes[..cut], 42),
                None,
                "{cut} bytes decoded as valid"
            );
        }
    }

    #[test]
    fn foreign_bytes_are_a_miss() {
        assert_eq!(decode(b"not ours at all, really", 42), None);
        // Right magic, wrong version.
        let mut wrong = encode(&sample(), 42);
        wrong[8] = FORMAT as u8 + 9;
        assert_eq!(decode(&wrong, 42), None);
    }

    #[test]
    fn a_guid_cannot_escape_the_analysis_directory() {
        let path = entry(Path::new("/tmp/song.rpp"), "../../etc/passwd");
        assert!(path.starts_with("/tmp/song.rpp.fts-analysis"));
        assert!(!path.to_string_lossy().contains(".."));
    }
}
