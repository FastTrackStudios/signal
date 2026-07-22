//! Rig configuration for the FTS Signal shell (v0).
//!
//! The plugin loads ONE rig at startup. Which rig is chosen:
//!
//! 1. `$FTS_SIGNAL_RIG` — path to either a `.signalpack` (loaded directly
//!    with defaults) or a `.styx` file describing a [`RigConfig`];
//! 2. otherwise `~/.config/signal/plugin/rig.styx` (the plugin's default
//!    config location, sibling to the live engine's `~/.config/signal/rig/`)
//!    if it exists;
//! 3. otherwise no rig: the plugin runs as a gain-staged audio passthrough
//!    and ignores MIDI.
//!
//! Example rig styx:
//!
//! ```text
//! # the sampler pack to load (required)
//! pack "/path/to/Keyscape Grand.signalpack"
//! attack_ms 5
//! release_ms 200
//! cache_budget_mb 8192
//! ```
//!
//! This is a per-machine setting, exactly like `signal-sampler-clap`'s
//! `$SIGNAL_SAMPLER_CLAP_PATCH`: real rig browsing/management (and
//! session-persisted rig state via the plugin state chunk) arrives with the
//! GUI. Guitar/vocal FX-chain rig configs are not accepted yet — see the
//! crate docs for the facade gap.

use facet::Facet;
use std::path::PathBuf;

/// Env var pointing at a `.signalpack` or a [`RigConfig`] styx file.
pub const RIG_ENV: &str = "FTS_SIGNAL_RIG";

/// Instrument id the plugin loads and drives inside the [`SamplerBank`].
///
/// [`SamplerBank`]: signal_sampler::SamplerBank
pub const INSTRUMENT_ID: &str = "rig";

#[derive(Debug, Clone, Facet)]
pub struct RigConfig {
    /// Path to the `.signalpack` to load (required).
    pub pack: String,
    #[facet(default)]
    pub attack_ms: Option<u32>,
    #[facet(default)]
    pub release_ms: Option<u32>,
    /// Decoded-sample cache budget (MiB). Default 8192.
    #[facet(default)]
    pub cache_budget_mb: Option<u64>,
}

impl RigConfig {
    /// A config that loads `pack` with all defaults (the bare-`.signalpack`
    /// path of [`RigConfig::resolve`]).
    pub fn from_pack_path(pack: impl Into<String>) -> Self {
        Self {
            pack: pack.into(),
            attack_ms: None,
            release_ms: None,
            cache_budget_mb: None,
        }
    }

    pub fn from_styx(s: &str) -> eyre::Result<Self> {
        facet_styx::from_str(s).map_err(|e| eyre::eyre!("rig config parse: {e}"))
    }

    /// Default config location: `~/.config/signal/plugin/rig.styx`.
    pub fn default_path() -> Option<PathBuf> {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join(".config/signal/plugin/rig.styx"))
    }

    /// Resolve per the priority in the module docs. `Ok(None)` means "no rig
    /// configured" (passthrough mode) — only real errors (unreadable file,
    /// bad styx) are `Err`.
    pub fn resolve() -> eyre::Result<Option<Self>> {
        if let Ok(path) = std::env::var(RIG_ENV) {
            if !path.is_empty() {
                return Self::load(&PathBuf::from(&path))
                    .map(Some)
                    .map_err(|e| eyre::eyre!("{RIG_ENV}={path}: {e}"));
            }
        }
        match Self::default_path() {
            Some(p) if p.exists() => Self::load(&p)
                .map(Some)
                .map_err(|e| eyre::eyre!("{}: {e}", p.display())),
            _ => Ok(None),
        }
    }

    /// Load from a path: `.signalpack` → direct pack config; anything else is
    /// parsed as a [`RigConfig`] styx file.
    pub fn load(path: &PathBuf) -> eyre::Result<Self> {
        let is_pack = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("signalpack"));
        if is_pack {
            return Ok(Self::from_pack_path(path.to_string_lossy().into_owned()));
        }
        let text =
            std::fs::read_to_string(path).map_err(|e| eyre::eyre!("read rig config: {e}"))?;
        Self::from_styx(&text)
    }

    pub fn cache_budget_bytes(&self) -> Option<usize> {
        Some((self.cache_budget_mb.unwrap_or(8192) as usize) * 1024 * 1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rig_config_parses_minimal_and_full_styx() {
        let min = RigConfig::from_styx(r#"pack "/tmp/rig.signalpack""#).expect("minimal");
        assert_eq!(min.pack, "/tmp/rig.signalpack");
        assert!(min.attack_ms.is_none());

        let full = RigConfig::from_styx(
            r#"
pack "/tmp/rig.signalpack"
attack_ms 5
release_ms 200
cache_budget_mb 1024
"#,
        )
        .expect("full");
        assert_eq!(full.attack_ms, Some(5));
        assert_eq!(
            full.cache_budget_bytes(),
            Some(1024 * 1024 * 1024),
            "budget in MiB"
        );
    }

    #[test]
    fn signalpack_paths_load_directly() {
        let cfg = RigConfig::load(&PathBuf::from("/tmp/x.SignalPack")).expect("pack path");
        assert_eq!(cfg.pack, "/tmp/x.SignalPack");
    }
}
