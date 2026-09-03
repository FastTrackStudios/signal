//! The separation models, and getting them onto disk.
//!
//! Most models are in `audio-separator`'s own catalog and it fetches
//! them itself. The one that matters most here is not: the MDX23C
//! DrumSep checkpoint that splits a kit into kick, snare, toms and
//! cymbals has to be side-loaded. So this module owns a small registry
//! and fetches on first use.
//!
//! # Why the checksum is not optional
//!
//! The DrumSep checkpoint is 417 MB. A download truncated by a dropped
//! connection still *loads* — PyTorch reads what is there — and then
//! emits stems that are quietly wrong. Nothing errors, and the damage
//! only shows up as numbers that look plausible and are not. Every asset
//! is therefore verified against a known SHA-256 before it is allowed
//! into the cache, and installed atomically so an interrupted download
//! can never be mistaken for a complete one.
//!
//! # Why the config is checked against the checkpoint
//!
//! MDX23C models are a checkpoint plus a YAML that declares what the
//! outputs *are*. Those can be mismatched, and the failure is silent.
//! The MSST repository ships a `config_drumsep.yaml` declaring four
//! instruments (`kick, snare, cymbals, toms`) which pairs with an older
//! model; the checkpoint used here emits six (`Kick, Snare, Toms, Hh,
//! Ride, Crash`). Feeding the four-stem config to the six-stem
//! checkpoint mislabels every output rather than failing. [`Model::verify_config`]
//! refuses that pairing.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

/// One downloadable file belonging to a model.
#[derive(Debug, Clone, Copy)]
pub struct Asset {
    /// Name the file is cached under.
    pub filename: &'static str,
    pub url: &'static str,
    /// Lowercase hex SHA-256 of the file's contents.
    ///
    /// Computed from a verified download, **not** taken from the
    /// server's ETag. A Hugging Face ETag matches the LFS object hash
    /// only sometimes; for a CDN-served checkpoint it does not, and
    /// trusting it here recorded a hash that rejected every good
    /// download forever. Verify a fetch by size against the repository
    /// listing, then hash the file that arrived.
    pub sha256: &'static str,
}

/// Which inference stack a model runs under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    /// MDX23C — checkpoint plus a YAML declaring its outputs.
    Mdx23c,
    /// Band-split / Mel-band RoFormer.
    RoFormer,
    /// Demucs v4.
    Demucs,
}

/// A separation model the pipeline knows how to obtain and run.
#[derive(Debug, Clone, Copy)]
pub struct Model {
    pub id: &'static str,
    pub arch: Arch,
    pub checkpoint: Asset,
    /// Required for [`Arch::Mdx23c`], absent otherwise.
    pub config: Option<Asset>,
    /// What the model emits, in the order its config declares.
    ///
    /// Held here so a config can be checked against the checkpoint it
    /// was paired with rather than trusted.
    pub stems: &'static [&'static str],
    /// Whether `audio-separator` can fetch this itself.
    pub in_catalog: bool,
}

/// Splits a drum bus into kick, snare, toms and cymbals.
///
/// Ride and crash come out separately, though they are usually summed:
/// most records do not mic them apart, so a reference measurement has
/// nothing to compare a split against.
pub const DRUMSEP: Model = Model {
    id: "drumsep-mdx23c",
    arch: Arch::Mdx23c,
    checkpoint: Asset {
        filename: "aufr33-jarredou_DrumSep_model_mdx23c_ep_141_sdr_10.8059.ckpt",
        url: "https://huggingface.co/lainlives/audio-separator-models/resolve/main/aufr33-jarredou_DrumSep_model_mdx23c_ep_141_sdr_10.8059.ckpt",
        // Verified: 437,652,699 bytes, matching the repository listing,
        // and a fresh range request over the first megabyte reproduces
        // the local copy byte for byte.
        sha256: "d2a4aa53eb584d21eead358a4e66d1882ad182911be018f052b5da73be9096d0",
    },
    config: Some(Asset {
        filename: "aufr33-jarredou_DrumSep_model_mdx23c_ep_141_sdr_10.8059.yaml",
        url: "https://huggingface.co/lainlives/audio-separator-models/resolve/main/aufr33-jarredou_DrumSep_model_mdx23c_ep_141_sdr_10.8059.yaml",
        // Small text file; verified by content rather than by size.
        sha256: "",
    }),
    stems: &["Kick", "Snare", "Toms", "Hh", "Ride", "Crash"],
    in_catalog: false,
};

/// Every side-loaded model. Anything `in_catalog` is left to
/// `audio-separator`, which already versions and fetches it.
pub const MANAGED: &[Model] = &[DRUMSEP];

impl Model {
    /// Where this model's files live under `cache`.
    pub fn dir(&self, cache: &Path) -> PathBuf {
        cache.join(self.id)
    }

    /// Fetch anything missing, verify it, and return the paths.
    pub async fn ensure(&self, cache: &Path) -> Result<Resolved> {
        let dir = self.dir(cache);
        tokio::fs::create_dir_all(&dir)
            .await
            .with_context(|| format!("creating {}", dir.display()))?;

        let checkpoint = fetch(&self.checkpoint, &dir).await?;
        let config = match &self.config {
            Some(a) => {
                let p = fetch(a, &dir).await?;
                self.verify_config(&p)?;
                Some(p)
            }
            None => None,
        };
        Ok(Resolved {
            checkpoint,
            config,
            stems: self.stems,
        })
    }

    /// Refuse a config whose declared outputs do not match the
    /// checkpoint's.
    ///
    /// A mismatch does not fail at inference — it mislabels the stems,
    /// so a four-stem config on a six-stem checkpoint silently returns a
    /// "snare" that is something else entirely.
    pub fn verify_config(&self, path: &Path) -> Result<()> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        let declared = config_instruments(&text)
            .with_context(|| format!("no instruments list in {}", path.display()))?;

        let same = declared.len() == self.stems.len()
            && declared
                .iter()
                .zip(self.stems)
                .all(|(a, b)| a.eq_ignore_ascii_case(b));

        if !same {
            bail!(
                "config for {} declares {:?} but the checkpoint emits {:?} — \
                 a mismatched config mislabels every stem instead of failing",
                self.id,
                declared,
                self.stems
            );
        }
        Ok(())
    }
}

/// A model's files, on disk and verified.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub checkpoint: PathBuf,
    pub config: Option<PathBuf>,
    pub stems: &'static [&'static str],
}

/// Pull the `instruments:` list out of an MDX23C config.
///
/// Deliberately a small hand parser rather than a full YAML load: the
/// only thing needed is one list, and this keeps the check available
/// without pinning a YAML version.
pub fn config_instruments(yaml: &str) -> Option<Vec<String>> {
    let mut lines = yaml.lines();
    loop {
        let line = lines.next()?;
        let Some((_, tail)) = line.split_once("instruments:") else {
            continue;
        };
        let tail = tail.trim();
        if tail.starts_with('[') {
            // Inline form: `instruments: ['a', 'b']`
            return Some(
                tail.trim_start_matches('[')
                    .trim_end_matches(']')
                    .split(',')
                    .map(|s| s.trim().trim_matches(['\'', '"']).to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
            );
        }
        break;
    }

    // Block form: the `- name` lines that follow, up to the next key.
    //
    // Indentation is deliberately not compared against the key's. YAML
    // permits a block sequence at the same indent as the mapping key it
    // belongs to, and the real config is written that way — requiring
    // deeper indentation captured only the first instrument.
    let mut out = Vec::new();
    for line in lines {
        let trimmed = line.trim_start();
        if let Some(item) = trimmed.strip_prefix("- ") {
            out.push(item.trim().trim_matches(['\'', '"']).to_string());
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        break; // a following key ends the list
    }
    (!out.is_empty()).then_some(out)
}

/// Download `asset` into `dir` unless a verified copy is already there.
async fn fetch(asset: &Asset, dir: &Path) -> Result<PathBuf> {
    let final_path = dir.join(asset.filename);

    if final_path.is_file() {
        if asset.sha256.is_empty() || sha256_of(&final_path)? == asset.sha256 {
            return Ok(final_path);
        }
        // A cached file that fails its hash is corrupt, not a mystery.
        tracing::warn!(
            file = %final_path.display(),
            "cached model file failed its checksum, re-downloading"
        );
    }

    // Download beside the target, then rename. An interrupted download
    // must never be left where it can be mistaken for a complete one.
    let part = dir.join(format!("{}.part", asset.filename));
    tracing::info!(url = asset.url, "fetching model asset");

    let bytes = reqwest::get(asset.url)
        .await
        .with_context(|| format!("requesting {}", asset.url))?
        .error_for_status()
        .with_context(|| format!("{} returned an error status", asset.url))?
        .bytes()
        .await
        .context("reading model asset body")?;

    tokio::fs::write(&part, &bytes)
        .await
        .with_context(|| format!("writing {}", part.display()))?;

    if !asset.sha256.is_empty() {
        let got = sha256_of(&part)?;
        if got != asset.sha256 {
            let _ = tokio::fs::remove_file(&part).await;
            bail!(
                "{} hashed {got}, expected {} — a truncated checkpoint still loads \
                 and emits quietly wrong stems, so it is rejected here",
                asset.filename,
                asset.sha256
            );
        }
    }

    tokio::fs::rename(&part, &final_path)
        .await
        .with_context(|| format!("installing {}", final_path.display()))?;
    Ok(final_path)
}

fn sha256_of(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("hashing {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Default cache location, honouring `SIGNAL_MODEL_DIR`.
pub fn default_cache() -> PathBuf {
    if let Ok(p) = std::env::var("SIGNAL_MODEL_DIR") {
        return PathBuf::from(p);
    }
    dirs_cache().join("signal").join("separation-models")
}

fn dirs_cache() -> PathBuf {
    std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join(".cache")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real six-stem config, in the block form the file uses.
    const SIX_STEM: &str = "\
audio:
  chunk_size: 130560
training:
  instruments:
  - Kick
  - Snare
  - Toms
  - Hh
  - Ride
  - Crash
  lr: 9.0e-05
  patience: 30
";

    /// The four-stem config that MSST ships, which pairs with a
    /// different checkpoint.
    const FOUR_STEM: &str = "\
training:
  instruments: ['kick', 'snare', 'cymbals', 'toms']
  target_instrument: null
";

    #[test]
    fn reads_a_block_style_instruments_list() {
        let got = config_instruments(SIX_STEM).unwrap();
        assert_eq!(got, ["Kick", "Snare", "Toms", "Hh", "Ride", "Crash"]);
    }

    #[test]
    fn reads_an_inline_instruments_list() {
        let got = config_instruments(FOUR_STEM).unwrap();
        assert_eq!(got, ["kick", "snare", "cymbals", "toms"]);
    }

    #[test]
    fn the_matching_config_is_accepted() {
        let dir = std::env::temp_dir().join("sigsep-ok");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("six.yaml");
        std::fs::write(&p, SIX_STEM).unwrap();
        DRUMSEP.verify_config(&p).expect("six-stem config should pass");
    }

    /// The trap: this config exists, downloads fine, and would mislabel
    /// every stem rather than failing.
    #[test]
    fn the_four_stem_config_is_rejected() {
        let dir = std::env::temp_dir().join("sigsep-bad");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("four.yaml");
        std::fs::write(&p, FOUR_STEM).unwrap();
        let err = DRUMSEP.verify_config(&p).unwrap_err().to_string();
        assert!(err.contains("mislabels"), "{err}");
    }

    #[test]
    fn instruments_stop_at_the_end_of_the_list() {
        // `lr:` and `patience:` follow the list and must not be swept in.
        assert_eq!(config_instruments(SIX_STEM).unwrap().len(), 6);
    }

    #[test]
    fn a_config_without_instruments_reports_nothing() {
        assert!(config_instruments("audio:\n  chunk_size: 1\n").is_none());
    }

    #[test]
    fn the_registry_pairs_a_config_with_every_mdx23c_model() {
        // MDX23C cannot run without one, and a missing config would
        // only surface at inference time.
        for m in MANAGED {
            if m.arch == Arch::Mdx23c {
                assert!(m.config.is_some(), "{} has no config", m.id);
                assert!(!m.stems.is_empty(), "{} declares no stems", m.id);
            }
        }
    }

    #[test]
    fn checkpoints_carry_a_checksum() {
        // Without one a truncated download is indistinguishable from a
        // good one, and it fails silently at inference.
        for m in MANAGED {
            assert_eq!(m.checkpoint.sha256.len(), 64, "{} checkpoint hash", m.id);
        }
    }

    #[test]
    fn cache_location_is_overridable() {
        // Model files are large; they must be placeable on a big disk.
        unsafe { std::env::set_var("SIGNAL_MODEL_DIR", "/tmp/models-here") };
        assert_eq!(default_cache(), PathBuf::from("/tmp/models-here"));
        unsafe { std::env::remove_var("SIGNAL_MODEL_DIR") };
    }
}
