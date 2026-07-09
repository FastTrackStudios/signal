//! Patch configuration for the CLAP shell (phase 2).
//!
//! The plugin loads ONE bank instrument at startup. Which patch is chosen:
//!
//! 1. `$SIGNAL_SAMPLER_CLAP_PATCH` — path to a `.styx` file describing the
//!    patch (see [`PatchConfig`] for the schema);
//! 2. otherwise the dev default: the Cinematic Studio Strings **1st
//!    Violins** patch used by `examples/render_document_going_home.rs`
//!    (local sample paths — this default only resolves on the dev machine).
//!
//! Example patch styx:
//!
//! ```text
//! # engine config (articulations / keyswitch / legato tables) — optional
//! config "/path/to/cinematic-strings.styx"
//! # zone library (required)
//! zones  "/path/to/_patches/1st Violins/library.styx"
//! # samples root — required when `config` is set
//! root   "/path/to/Cinematic Studio Strings"
//! section "1st Violins"
//! mic "Mix"
//! solo_mic "Mix"
//! attack_ms 20
//! release_ms 400
//! cache_budget_mb 8192
//! ```
//!
//! Proper session-persisted patch state (CLAP state chunk) is deferred to a
//! later phase — for now the patch is a per-machine setting, which is what a
//! dev/test loop inside REAPER needs.

use facet::Facet;
use std::path::PathBuf;

/// Env var pointing at a [`PatchConfig`] styx file.
pub const PATCH_ENV: &str = "SIGNAL_SAMPLER_CLAP_PATCH";

/// Dev defaults — the CSS Violin 1 patch from
/// `examples/render_document_going_home.rs`.
pub const DEV_CSS_ROOT: &str =
    "/run/media/AudioHaven/Sampled/Orchestral/Cinematic Series/Cinematic Studio Strings";
pub const DEV_CSS_CONFIG: &str =
    "/run/media/Development/FastTrackStudio/sample-collector/specs/cinematic-strings.styx";

/// Bank instrument id the plugin loads and drives.
pub const INSTRUMENT_ID: &str = "sampler";

#[derive(Debug, Clone, Facet)]
pub struct PatchConfig {
    /// Engine config styx (articulations / keyswitch / CC58 / legato) merged
    /// over the zone library — the way Cinematic Studio libraries ship.
    #[facet(default)]
    pub config: Option<String>,
    /// Zone library styx (required).
    pub zones: String,
    /// Samples root. Required when `config` is set; without `config`, zones
    /// resolve relative to this (or the spec's own layout when omitted).
    #[facet(default)]
    pub root: Option<String>,
    #[facet(default)]
    pub section: Option<String>,
    #[facet(default)]
    pub mic: Option<String>,
    /// Solo one mic position (e.g. "Mix").
    #[facet(default)]
    pub solo_mic: Option<String>,
    #[facet(default)]
    pub attack_ms: Option<u32>,
    #[facet(default)]
    pub release_ms: Option<u32>,
    /// Decoded-sample cache budget (MiB). Default 8192.
    #[facet(default)]
    pub cache_budget_mb: Option<u64>,
}

impl PatchConfig {
    /// Dev default: CSS 1st Violins, Mix mic, CSS-default envelope.
    pub fn dev_default() -> Self {
        Self {
            config: Some(DEV_CSS_CONFIG.to_string()),
            zones: PathBuf::from(DEV_CSS_ROOT)
                .join("_patches/1st Violins/library.styx")
                .to_string_lossy()
                .into_owned(),
            root: Some(DEV_CSS_ROOT.to_string()),
            section: Some("1st Violins".to_string()),
            mic: Some("Mix".to_string()),
            solo_mic: Some("Mix".to_string()),
            attack_ms: Some(20),
            release_ms: Some(400),
            cache_budget_mb: None,
        }
    }

    pub fn from_styx(s: &str) -> eyre::Result<Self> {
        facet_styx::from_str(s).map_err(|e| eyre::eyre!("patch config parse: {e}"))
    }

    /// Resolve per the priority in the module docs.
    pub fn resolve() -> eyre::Result<Self> {
        match std::env::var(PATCH_ENV) {
            Ok(path) if !path.is_empty() => {
                let text = std::fs::read_to_string(&path)
                    .map_err(|e| eyre::eyre!("read {PATCH_ENV}={path}: {e}"))?;
                Self::from_styx(&text)
            }
            _ => Ok(Self::dev_default()),
        }
    }

    pub fn cache_budget_bytes(&self) -> Option<usize> {
        Some((self.cache_budget_mb.unwrap_or(8192) as usize) * 1024 * 1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_config_parses_minimal_and_full_styx() {
        let min = PatchConfig::from_styx(r#"zones "/tmp/lib.styx""#).expect("minimal");
        assert_eq!(min.zones, "/tmp/lib.styx");
        assert!(min.config.is_none());

        let full = PatchConfig::from_styx(
            r#"
config "/tmp/engine.styx"
zones "/tmp/lib.styx"
root "/tmp/samples"
section Violins
mic Mix
solo_mic Mix
attack_ms 20
release_ms 400
cache_budget_mb 1024
"#,
        )
        .expect("full");
        assert_eq!(full.root.as_deref(), Some("/tmp/samples"));
        assert_eq!(full.attack_ms, Some(20));
        assert_eq!(
            full.cache_budget_bytes(),
            Some(1024 * 1024 * 1024),
            "budget in MiB"
        );
    }
}
