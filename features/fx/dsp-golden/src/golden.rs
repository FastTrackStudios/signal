//! Record and compare reference vectors.
//!
//! The contract is bit-exactness. A refactor that turns an indexed loop into an
//! iterator, or a raw `as` into a checked conversion, does not change a single
//! float — so the reference is stored as IEEE-754 bit patterns and compared for
//! equality, not "close enough". A tolerance would let exactly the drift this
//! harness exists to catch slip through.
//!
//! Reference files are text and live in `<crate>/tests/golden/`. They hold a
//! hash of the whole output plus a spread of individual probe samples: the hash
//! is the verdict, the probes are the diagnosis, so a failure says *where* the
//! signal diverged instead of only *that* it did.
//!
//! Regenerate with `UPDATE_GOLDEN=1 cargo nextest run -p <crate>`, and read the
//! resulting diff — a reference file changing in a commit that claimed to be a
//! pure refactor is the alarm, not a chore.

use core::fmt::{self, Write as _};
use std::fs;
use std::path::{Path, PathBuf};

/// How many individual samples a reference file pins alongside the hash.
const PROBES: usize = 64;
/// Differing probes listed in a failure before the report is truncated.
const REPORTED: usize = 8;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a over the raw bit patterns. Spelled out rather than pulled from a
/// crate so that no dependency bump can silently invalidate every reference
/// file in the tree.
#[must_use]
fn digest(samples: &[f32]) -> u64 {
    samples.iter().fold(FNV_OFFSET, |acc, sample| {
        sample
            .to_bits()
            .to_le_bytes()
            .iter()
            .fold(acc, |h, byte| (h ^ u64::from(*byte)).wrapping_mul(FNV_PRIME))
    })
}

/// Evenly spaced probe indices, always including the first sample.
fn probe_indices(len: usize) -> impl Iterator<Item = usize> {
    let stride = len.div_ceil(PROBES).max(1);
    (0..len).step_by(stride).take(PROBES)
}

/// The measured shape of one output buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Record {
    len: usize,
    hash: u64,
    probes: Vec<(usize, u32)>,
}

impl Record {
    fn measure(samples: &[f32]) -> Self {
        Self {
            len: samples.len(),
            hash: digest(samples),
            probes: probe_indices(samples.len())
                .filter_map(|i| samples.get(i).map(|s| (i, s.to_bits())))
                .collect(),
        }
    }

    fn render(&self, name: &str) -> String {
        let mut out = String::new();
        out.push_str("# dsp-golden v1 — bit-exact reference vector.\n");
        out.push_str("# Regenerate: UPDATE_GOLDEN=1 cargo nextest run -p <crate>\n");
        out.push_str("# A change here in a refactor commit means the audio changed. Investigate.\n");
        let _ = writeln!(out, "name {name}");
        let _ = writeln!(out, "len {}", self.len);
        let _ = writeln!(out, "hash {:016x}", self.hash);
        for (index, bits) in &self.probes {
            let _ = writeln!(out, "probe {index} {bits:08x} {}", f32::from_bits(*bits));
        }
        out
    }

    fn parse(text: &str) -> Option<Self> {
        let mut len = None;
        let mut hash = None;
        let mut probes = Vec::new();
        for line in text.lines() {
            let mut field = line.split_whitespace();
            match (field.next(), field.next(), field.next()) {
                (Some("len"), Some(value), _) => len = value.parse().ok(),
                (Some("hash"), Some(value), _) => hash = u64::from_str_radix(value, 16).ok(),
                (Some("probe"), Some(index), Some(bits)) => {
                    probes.push((index.parse().ok()?, u32::from_str_radix(bits, 16).ok()?));
                }
                _ => {}
            }
        }
        Some(Self { len: len?, hash: hash?, probes })
    }
}

/// Why a comparison failed, phrased so the message alone is enough to act on.
#[derive(Debug)]
pub enum Mismatch {
    /// No reference file yet. Not a silent pass: an unrecorded vector proves
    /// nothing, so it fails until someone records it deliberately.
    Missing { name: String, path: PathBuf },
    /// The file exists but could not be read or written.
    Io { path: PathBuf, error: std::io::Error },
    /// The file is present but malformed — treated as a hard failure rather
    /// than re-recorded, because overwriting a corrupt reference destroys the
    /// only evidence of what the output used to be.
    Corrupt { path: PathBuf },
    /// The output changed.
    Drift {
        name: String,
        report: String,
    },
    /// The output is not a finite signal, which no reference should ever pin.
    NotFinite { name: String, index: usize, value: f32 },
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { name, path } => write!(
                f,
                "no golden reference for `{name}`.\n  \
                 expected at: {}\n  \
                 record it with: UPDATE_GOLDEN=1 cargo nextest run\n  \
                 then read the new file before committing it — you are declaring this output correct.",
                path.display()
            ),
            Self::Io { path, error } => write!(f, "golden file {}: {error}", path.display()),
            Self::Corrupt { path } => write!(
                f,
                "golden file {} is malformed. Restore it from git rather than re-recording — \
                 re-recording would destroy the reference you are trying to compare against.",
                path.display()
            ),
            Self::Drift { name, report } => write!(f, "`{name}` no longer produces the reference output.\n{report}"),
            Self::NotFinite { name, index, value } => {
                write!(f, "`{name}` produced a non-finite sample at index {index}: {value}")
            }
        }
    }
}

impl std::error::Error for Mismatch {}

/// A crate's reference-vector directory.
#[derive(Debug, Clone)]
pub struct Golden {
    dir: PathBuf,
    update: bool,
}

impl Golden {
    /// Point at `<manifest_dir>/tests/golden`. Use the [`crate::golden!`] macro
    /// rather than passing the path by hand.
    #[must_use]
    pub fn new(manifest_dir: &str) -> Self {
        Self {
            dir: Path::new(manifest_dir).join("tests").join("golden"),
            update: std::env::var_os("UPDATE_GOLDEN").is_some_and(|v| v != "0"),
        }
    }

    fn path_for(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.golden", name.replace('/', "__")))
    }

    /// Compare `samples` against the stored reference for `name`, or record it
    /// when `UPDATE_GOLDEN` is set.
    ///
    /// # Errors
    ///
    /// Returns [`Mismatch`] when the reference is absent, unreadable, malformed,
    /// or no longer matches the output.
    pub fn check(&self, name: &str, samples: &[f32]) -> Result<(), Mismatch> {
        if let Some((index, value)) = samples.iter().enumerate().find(|(_, s)| !s.is_finite()) {
            return Err(Mismatch::NotFinite { name: name.to_owned(), index, value: *value });
        }

        let measured = Record::measure(samples);
        let path = self.path_for(name);

        if self.update {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| Mismatch::Io { path: path.clone(), error })?;
            }
            return fs::write(&path, measured.render(name))
                .map_err(|error| Mismatch::Io { path, error });
        }

        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(Mismatch::Missing { name: name.to_owned(), path });
            }
            Err(error) => return Err(Mismatch::Io { path, error }),
        };
        let stored = Record::parse(&text).ok_or(Mismatch::Corrupt { path })?;

        if stored == measured {
            return Ok(());
        }
        Err(Mismatch::Drift { name: name.to_owned(), report: report(&stored, &measured, samples) })
    }
}

fn report(stored: &Record, measured: &Record, samples: &[f32]) -> String {
    let mut out = String::new();
    if stored.len != measured.len {
        let _ = writeln!(
            out,
            "  length changed: {} -> {} samples (the reference cannot be compared further)",
            stored.len, measured.len
        );
        return out;
    }
    let _ = writeln!(out, "  hash: {:016x} -> {:016x}", stored.hash, measured.hash);

    let drifted: Vec<_> = stored
        .probes
        .iter()
        .filter_map(|(index, was)| {
            samples.get(*index).and_then(|now| {
                (now.to_bits() != *was).then(|| (*index, f32::from_bits(*was), *now))
            })
        })
        .collect();

    if drifted.is_empty() {
        out.push_str(
            "  every probed sample still matches, so the change is between probes — \
             the whole-buffer hash is the authority here.\n",
        );
        return out;
    }

    let _ = writeln!(out, "  {} of {} probes drifted:", drifted.len(), stored.probes.len());
    for (index, was, now) in drifted.iter().take(REPORTED) {
        let delta = now - was;
        let relative = if was.abs() > f32::MIN_POSITIVE { delta / was } else { f32::NAN };
        let _ = writeln!(
            out,
            "    [{index}] {was:+.9e} -> {now:+.9e}  (delta {delta:+.3e}, {:.3} ppm)",
            relative * 1e6
        );
    }
    if drifted.len() > REPORTED {
        let _ = writeln!(out, "    ... and {} more", drifted.len().saturating_sub(REPORTED));
    }
    out.push_str(
        "  A drift of a few ULP still fails: a refactor that reorders float math is not \
         a pure refactor, and this is the only place that will tell you.\n",
    );
    out
}

/// The reference-vector directory of the calling crate.
#[macro_export]
macro_rules! golden {
    () => {
        $crate::Golden::new(env!("CARGO_MANIFEST_DIR"))
    };
}

/// Compare a buffer against its reference, panicking with the full diagnosis on
/// any mismatch. Intended for `#[test]` bodies.
#[macro_export]
macro_rules! assert_golden {
    ($golden:expr, $name:expr, $samples:expr) => {
        if let Err(mismatch) = $golden.check($name, $samples) {
            panic!("{mismatch}");
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_round_trips_through_its_text_form() {
        let samples = crate::signal::noise(1024, 3);
        let record = Record::measure(&samples);
        let parsed = Record::parse(&record.render("round/trip"));
        assert_eq!(parsed, Some(record));
    }

    #[test]
    fn one_flipped_bit_changes_the_hash() {
        let mut samples = crate::signal::sine(512, 440.0, 48_000.0);
        let before = digest(&samples);
        if let Some(last) = samples.last_mut() {
            *last = f32::from_bits(last.to_bits() ^ 1);
        }
        assert_ne!(before, digest(&samples));
    }

    #[test]
    fn probes_cover_the_buffer_without_overrunning_it() {
        for len in [1_usize, 7, 64, 1000, 48_000] {
            let indices: Vec<_> = probe_indices(len).collect();
            assert!(!indices.is_empty());
            assert!(indices.iter().all(|i| *i < len));
            assert_eq!(indices.first(), Some(&0));
        }
    }
}
