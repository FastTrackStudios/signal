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

/// No pack is trimmed further than this. It has to clear the largest HONEST
/// trim in the book or the ceiling silently becomes the level: Double Felt
/// Grand measures −47.83 LUFS and needs +29.8 dB, and the NI pianos +26.6.
/// At the old 24 dB ceiling those three were still 3–6 dB under everything
/// else and no amount of correct measurement could fix it. Kept finite so a
/// bad measurement cannot blow up a service.
const MAX_TRIM_DB: f32 = 32.0;

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

/// **Measured levels that ship with the rig**, so a fresh install is level
/// before anyone writes a config file. Measured with
/// `signal-sampler --example pack_lufs` (see `crates/signal/docs/pack-levels.md`)
/// — the same chord at the same velocity through each pack.
///
/// Keyed by pack file stem, which is what a module's `patch` holds.
const BUILT_IN: &[(&str, f64)] = &[
    ("LA Custom C7 Grand", -37.73),
    ("Rhodes - LA Custom", -27.99),
    ("OB-8 PWM Big Strings", -17.07),
    ("Microcosm Pad 1", -39.58),
    // The NI pianos, which the Worship profile puts on Keys 1. Unmeasured
    // they got 0 dB and played ~27 dB under the pads — audible only if you
    // knew to listen for it, and reported (correctly) as "the piano is
    // REALLY quiet". They are mastered far lower than the Keyscape pianos,
    // which is why they need more trim than anything else here.
    //
    // Keyed by INSTRUMENT, not by pack: the measurement is of the main body
    // (`<Instrument> - Piano`), and the resonance pack inherits the same
    // trim so the ratio its author recorded survives. Measuring the
    // resonance separately would boost quiet-by-design sympathetic strings
    // up to the level of struck notes.
    //
    // PROVISIONAL: these are the `pack_lufs` measurements with +13 dB added.
    // The measurement renders through `SamplerRig::new_offline` — a bare pack
    // — while the rig plays through the composition tree, and the two do not
    // agree: applied raw, the derived trims put a SINGLE note at ~4.0 peak
    // where nothing may exceed 1.0. +13 dB is the offset that lands a 20-note
    // chord just under full scale, measured on this rig; it is a calibration
    // stopgap, not a result. The real fix is for `pack_lufs` to measure
    // through the same chain the rig plays through, at which point these go
    // back to being raw measurements.
    ("The Grandeur", -31.56),      // measured -44.56
    ("The Maverick", -31.42),      // measured -44.42
    ("The Gentleman", -31.99),     // measured -44.99
    ("The Giant", -17.17),         // measured -30.17
    ("Double Felt Grand", -34.83), // measured -47.83
];

/// The instrument a pack belongs to: `"The Grandeur - Resonance"` →
/// `"The Grandeur"`.
///
/// The builders name multi-pack libraries `<Instrument> - <Pack>`, so the
/// suffix is what separates a body from its resonance or release layers. A
/// name with no ` - ` is its own instrument.
fn instrument_of(pack: &str) -> &str {
    match pack.rfind(" - ") {
        Some(i) => &pack[..i],
        None => pack,
    }
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
        Self {
            target_lufs: DEFAULT_TARGET_LUFS,
            packs: Vec::new(),
        }
    }
}

impl PackLevels {
    fn path() -> Option<std::path::PathBuf> {
        if let Ok(p) = std::env::var("FTS_KEYS_PACK_LEVELS") {
            return Some(std::path::PathBuf::from(p));
        }
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .ok()
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| std::path::PathBuf::from(h).join(".config"))
            })?;
        Some(base.join("signal/keys/pack-levels.styx"))
    }

    fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match facet_styx::from_str::<Self>(&text) {
            Ok(mut levels) => {
                if levels.target_lufs == 0.0 {
                    levels.target_lufs = DEFAULT_TARGET_LUFS;
                }
                tracing::info!(
                    ?path,
                    packs = levels.packs.len(),
                    "keys: pack levels loaded"
                );
                levels
            }
            Err(e) => {
                tracing::error!(
                    ?path,
                    "keys: pack levels unreadable ({e}); no trims applied"
                );
                Self::default()
            }
        }
    }

    /// Trim for one pack, in dB. The file wins where it has an entry; the
    /// shipped measurements cover the rest; anything unmeasured is left alone.
    ///
    /// Falls back from the pack name to its **instrument**, so every pack of a
    /// multi-pack instrument shares ONE trim. That is the whole point: a
    /// library split into `<Instrument> - Piano` and `<Instrument> - Resonance`
    /// has authored the ratio between them, and the resonance pack is quiet
    /// *on purpose*. Measure and trim it separately and it gets boosted onto
    /// the same target as the main body — the sympathetic strings come up to
    /// the level of the notes, which is not what anyone recorded. One trim per
    /// instrument moves them together and leaves the ratio alone.
    fn trim_for(&self, name: &str) -> f32 {
        let instrument = instrument_of(name);
        let lookup = |key: &str| -> Option<f32> {
            if let Some(entry) = self.packs.iter().find(|p| p.name == key) {
                return match (entry.trim_db, entry.lufs) {
                    (Some(t), _) => Some(t),
                    (None, Some(lufs)) => Some((self.target_lufs - lufs) as f32),
                    (None, None) => None,
                };
            }
            BUILT_IN
                .iter()
                .find(|(pack, _)| *pack == key)
                .map(|(_, lufs)| (self.target_lufs - lufs) as f32)
        };
        // Exact pack name first: a single-pack library, or a deliberate
        // per-pack override, must still win over the instrument default.
        let trim = lookup(name)
            .or_else(|| (instrument != name).then(|| lookup(instrument)).flatten())
            .unwrap_or(0.0);
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
            tracing::debug!(
                pack,
                "keys: no measured level for pack — playing it untrimmed"
            );
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
    fn instrument_is_the_name_before_the_pack_suffix() {
        assert_eq!(instrument_of("The Grandeur - Resonance"), "The Grandeur");
        assert_eq!(instrument_of("The Grandeur - Piano"), "The Grandeur");
        // A single-pack library is its own instrument, suffix or not.
        assert_eq!(instrument_of("Double Felt Grand"), "Double Felt Grand");
        assert_eq!(instrument_of("Rhodes - LA Custom"), "Rhodes");
    }

    /// The reason this exists: a resonance pack is quiet ON PURPOSE, and must
    /// move with its body rather than be normalised onto the same target.
    #[test]
    fn every_pack_of_an_instrument_shares_one_trim() {
        let levels = PackLevels {
            target_lufs: -18.0,
            packs: vec![PackLevel {
                name: "The Grandeur".into(),
                lufs: Some(-40.0),
                trim_db: None,
            }],
        };
        assert_eq!(levels.trim_for("The Grandeur - Piano"), 22.0);
        assert_eq!(levels.trim_for("The Grandeur - Resonance"), 22.0);
    }

    /// An entry for the exact pack still wins — a deliberate per-pack override
    /// has to beat the instrument default.
    #[test]
    fn an_exact_pack_entry_overrides_its_instrument() {
        let levels = PackLevels {
            target_lufs: -18.0,
            packs: vec![
                PackLevel {
                    name: "The Grandeur".into(),
                    lufs: Some(-40.0),
                    trim_db: None,
                },
                PackLevel {
                    name: "The Grandeur - Resonance".into(),
                    trim_db: Some(-3.0),
                    lufs: None,
                },
            ],
        };
        assert_eq!(levels.trim_for("The Grandeur - Piano"), 22.0);
        assert_eq!(levels.trim_for("The Grandeur - Resonance"), -3.0);
    }

    #[test]
    fn trim_is_derived_from_loudness_and_clamped() {
        let levels = PackLevels {
            target_lufs: -18.0,
            packs: vec![
                PackLevel {
                    name: "Quiet".into(),
                    lufs: Some(-31.0),
                    trim_db: None,
                },
                PackLevel {
                    name: "Loud".into(),
                    lufs: Some(-12.0),
                    trim_db: None,
                },
                // A measurement so far off it would swamp everything.
                PackLevel {
                    name: "Broken".into(),
                    lufs: Some(-90.0),
                    trim_db: None,
                },
                // A hand-set override wins over any measurement.
                PackLevel {
                    name: "Judged".into(),
                    lufs: Some(-31.0),
                    trim_db: Some(-2.0),
                },
            ],
        };
        assert!((levels.trim_for("Quiet") - 13.0).abs() < 0.001);
        assert!((levels.trim_for("Loud") + 6.0).abs() < 0.001);
        assert_eq!(levels.trim_for("Broken"), MAX_TRIM_DB);
        assert!((levels.trim_for("Judged") + 2.0).abs() < 0.001);
        // An unlisted, unshipped pack is left exactly as it was mastered.
        assert_eq!(levels.trim_for("Some Unmeasured Pack"), 0.0);
    }

    /// A rig with no config file still plays the Keyscape libraries level.
    #[test]
    fn shipped_measurements_apply_without_a_file() {
        let empty = PackLevels::default();
        assert!((empty.trim_for("LA Custom C7 Grand") - 19.73).abs() < 0.01);
        assert!((empty.trim_for("Rhodes - LA Custom") - 9.99).abs() < 0.01);
        // The pad is already at target — near enough to leave alone.
        assert!(empty.trim_for("OB-8 PWM Big Strings").abs() < 1.0);
        assert_eq!(empty.trim_for("Some Unmeasured Pack"), 0.0);
    }
}

#[cfg(test)]
mod styx_shape {
    use super::*;

    /// The hand-written form the docs give, parsed. Guards the format people
    /// actually type — a wrong shape here is a silently un-normalized rig.
    ///
    /// Note what is NOT asserted: a serializer round-trip. `facet_styx` writes
    /// `@` for an absent `Option` and then refuses to read it back, so an
    /// absent field must be omitted rather than written out.
    #[test]
    fn documented_form_parses() {
        let text = r#"target_lufs -18

packs ({name "LA Custom C7 Grand", lufs -37.73} {name "Rhodes - LA Custom", trim_db 10})
"#;
        let levels: PackLevels = facet_styx::from_str(text).expect("parse");
        assert_eq!(levels.target_lufs, -18.0);
        assert!((levels.trim_for("LA Custom C7 Grand") - 19.73).abs() < 0.01);
        assert!((levels.trim_for("Rhodes - LA Custom") - 10.0).abs() < 0.01);
    }
}
