//! **Pack normalization** — every soundpack at the same working level, so the
//! mixer means something.
//!
//! Sampled libraries are mastered to wildly different levels. Keyscape's C7
//! Grand and Rhodes sit far below the Omnisphere soundsources: with every
//! fader at unity the pad buries the piano, and the only fix a player has is
//! to ride faders permanently at +12 to make the mixer usable. That is a
//! calibration problem masquerading as a mix decision.
//!
//! So a pack carries a **trim**: the dB it needs to land on a common target,
//! applied under the module fader. Unity on the fader then means "this
//! sound's normal level" for every pack, and the faders are back to being
//! mix decisions.
//!
//! The trim comes from a measured integrated loudness (ITU-R BS.1770 — the
//! same measure [`signal_sampler::loudness`] uses to level-match amp models)
//! written into `~/.config/signal/keys/pack-levels.styx`:
//!
//! ```styx
//! target_lufs -18
//!
//! packs ({name "Keyscape LA Custom C7 Grand", lufs -31.4} {name "OB-8 PWM Big Strings", trim_db -2})
//! ```
//!
//! Omit the field you are not using — writing `@` for an absent `Option` is
//! what the serializer emits but not what the parser takes.
//!
//! Either field works: `lufs` is a measurement and the trim is derived from
//! it, `trim_db` is a hand-set override for when a measurement reads right
//! and still sounds wrong. Unlisted packs get 0 dB — an unmeasured library is
//! left exactly as its author mastered it rather than guessed at.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use facet::Facet;

/// Where every pack should land. −18 LUFS leaves headroom for a stacked patch
/// (four modules summing) without the master clipping.
const DEFAULT_TARGET_LUFS: f64 = -18.0;

/// No pack is trimmed further than this. A pack needing more than 18 dB is
/// mismatched enough that it wants re-packing, not a bigger number — and a
/// runaway trim on a bad measurement should not be able to blow up a service.
const MAX_TRIM_DB: f32 = 18.0;

/// One pack's level.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
pub struct PackLevel {
    /// Pack / library name, as the browser shows it.
    pub name: String,
    /// Measured integrated loudness. The trim is `target − lufs`.
    #[facet(default)]
    pub lufs: Option<f64>,
    /// Hand-set trim, overriding any measurement.
    #[facet(default)]
    pub trim_db: Option<f32>,
}

/// The level book.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct PackLevels {
    #[facet(default)]
    pub target_lufs: f64,
    #[facet(default)]
    pub packs: Vec<PackLevel>,
}

impl Default for PackLevels {
    fn default() -> Self {
        Self { target_lufs: DEFAULT_TARGET_LUFS, packs: Vec::new() }
    }
}

impl PackLevels {
    fn path() -> Option<std::path::PathBuf> {
        if let Ok(p) = std::env::var("FTS_KEYS_PACK_LEVELS") {
            return Some(std::path::PathBuf::from(p));
        }
        let base = std::env::var("XDG_CONFIG_HOME").map(std::path::PathBuf::from).ok().or_else(
            || std::env::var("HOME").ok().map(|h| std::path::PathBuf::from(h).join(".config")),
        )?;
        Some(base.join("signal/keys/pack-levels.styx"))
    }

    fn load() -> Self {
        let Some(path) = Self::path() else { return Self::default() };
        let Ok(text) = std::fs::read_to_string(&path) else { return Self::default() };
        match facet_styx::from_str::<Self>(&text) {
            Ok(mut levels) => {
                if levels.target_lufs == 0.0 {
                    levels.target_lufs = DEFAULT_TARGET_LUFS;
                }
                tracing::info!(?path, packs = levels.packs.len(), "keys: pack levels loaded");
                levels
            }
            Err(e) => {
                tracing::error!(?path, "keys: pack levels unreadable ({e}); no trims applied");
                Self::default()
            }
        }
    }

    /// Trim for one pack, in dB. Unknown packs are left alone.
    fn trim_for(&self, name: &str) -> f32 {
        let Some(entry) = self.packs.iter().find(|p| p.name == name) else { return 0.0 };
        let trim = match (entry.trim_db, entry.lufs) {
            (Some(t), _) => t,
            (None, Some(lufs)) => (self.target_lufs - lufs) as f32,
            (None, None) => 0.0,
        };
        trim.clamp(-MAX_TRIM_DB, MAX_TRIM_DB)
    }
}

/// Read once: the book is a calibration, not live state, and re-reading it per
/// audio callback would be absurd.
fn levels() -> &'static PackLevels {
    static LEVELS: OnceLock<PackLevels> = OnceLock::new();
    LEVELS.get_or_init(PackLevels::load)
}

/// The trim a pack needs to sit at the common level, in dB. `0.0` when the
/// pack has no entry — an unmeasured library plays as its author mastered it.
pub fn trim_db(pack: &str) -> f32 {
    if pack.is_empty() {
        return 0.0;
    }
    static SEEN: OnceLock<std::sync::Mutex<BTreeMap<String, f32>>> = OnceLock::new();
    let cache = SEEN.get_or_init(|| std::sync::Mutex::new(BTreeMap::new()));
    if let Ok(mut c) = cache.lock() {
        if let Some(v) = c.get(pack) {
            return *v;
        }
        let trim = levels().trim_for(pack);
        if trim == 0.0 {
            tracing::debug!(pack, "keys: no measured level for pack — playing it untrimmed");
        } else {
            tracing::info!(pack, trim_db = trim, "keys: pack level trim");
        }
        c.insert(pack.to_string(), trim);
        return trim;
    }
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_is_derived_from_loudness_and_clamped() {
        let levels = PackLevels {
            target_lufs: -18.0,
            packs: vec![
                PackLevel { name: "Quiet".into(), lufs: Some(-31.0), trim_db: None },
                PackLevel { name: "Loud".into(), lufs: Some(-12.0), trim_db: None },
                // A measurement so far off it would swamp everything.
                PackLevel { name: "Broken".into(), lufs: Some(-90.0), trim_db: None },
                // A hand-set override wins over any measurement.
                PackLevel { name: "Judged".into(), lufs: Some(-31.0), trim_db: Some(-2.0) },
            ],
        };
        assert!((levels.trim_for("Quiet") - 13.0).abs() < 0.001);
        assert!((levels.trim_for("Loud") + 6.0).abs() < 0.001);
        assert_eq!(levels.trim_for("Broken"), MAX_TRIM_DB);
        assert!((levels.trim_for("Judged") + 2.0).abs() < 0.001);
        // An unlisted pack is left exactly as it was mastered.
        assert_eq!(levels.trim_for("Unmeasured"), 0.0);
    }
}

#[cfg(test)]
mod styx_shape {
    use super::*;

    /// Prints the canonical file the loader accepts — run with `--nocapture`
    /// when hand-writing `pack-levels.styx`.
    #[test]
    fn round_trips() {
        let levels = PackLevels {
            target_lufs: -18.0,
            packs: vec![
                PackLevel { name: "Keyscape LA Custom C7 Grand".into(), lufs: None, trim_db: Some(10.0) },
                PackLevel { name: "OB-8 PWM Big Strings".into(), lufs: Some(-16.2), trim_db: None },
            ],
        };
        let text = facet_styx::to_string(&levels).expect("serialize");
        println!("---\n{text}\n---");
        let back: PackLevels = facet_styx::from_str(&text).expect("parse");
        assert_eq!(back, levels);
    }
}
