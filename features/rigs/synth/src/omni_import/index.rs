//! **Soundsource index** — name → spec-path lookup over the local extraction.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Root of the built `.signalpack` soundsource library (loops baked in, disk
/// streaming, tags). Mirrors the Keyscape packs root; preferred over the raw
/// extraction. Override with `FTS_OMNISPHERE_PACKS`.
pub(crate) const OMNISPHERE_PACKS_ROOT: &str =
    "/run/media/AudioHaven/Signal/Libraries/Keys/Omnisphere/Packs";

/// Root of the built Keyscape packs. Indexed alongside the Omnisphere ones
/// because Keyscape is an Omnisphere library — its soundsources are nameable
/// from an ordinary Omnisphere patch. Override with `FTS_KEYSCAPE_PACKS`.
pub(crate) const KEYSCAPE_PACKS_ROOT: &str =
    "/run/media/AudioHaven/Signal/Libraries/Keys/Keyscape/Packs";

/// Root of the built NI Essential Piano packs. Not an Omnisphere library, but
/// indexed here because this is the rig's one name→source lookup and a keys
/// profile names all three families through it. Override with
/// `FTS_NI_PIANO_PACKS`.
pub(crate) const NI_PIANO_PACKS_ROOT: &str = "/run/media/AudioHaven/Signal/Libraries/Full/Keys";

/// Root of the authored `.prt_omn` patches. A synthesis-mode patch realizes a
/// source block as an oscillator the same way a pack realizes one as a
/// sampler, so patches are soundsources too. Override with
/// `FTS_OMNISPHERE_PATCHES`.
pub(crate) const PATCH_ROOT: &str =
    "/run/media/AudioHaven/Sampled/Synth/Spectrasonics-Patches/Omnisphere/Settings Library/Patches";

// ── Soundsource index ────────────────────────────────────────────────────────

/// Name → spec-path index over the local soundsource extraction. A built
/// `<Name>.signalpack` (preferred) wins over a multisample `<Name>/library.styx`
/// dir or a flat one-shot `<Name>.styx`.
#[derive(Debug, Default)]
pub struct SoundsourceIndex {
    by_name: HashMap<String, PathBuf>,
}

impl SoundsourceIndex {
    /// Walk `root` (e.g. `…/Omnisphere`) up to a few levels, collecting every
    /// soundsource spec keyed by lower-cased name.
    pub fn scan(root: &Path) -> Self {
        let mut idx = Self::default();
        idx.scan_dir(root, 0);
        idx
    }

    /// Scan the default extraction root (`FTS_OMNISPHERE_ROOT` override), then
    /// overlay the built `.signalpack` library (`FTS_OMNISPHERE_PACKS`) so a
    /// pack always wins over the raw styx for the same name.
    pub fn scan_default() -> Self {
        let root = std::env::var("FTS_OMNISPHERE_ROOT")
            .unwrap_or_else(|_| crate::omni::OMNISPHERE_ROOT.into());
        let mut idx = Self::default();
        idx.scan_dir(Path::new(&root), 0);
        let packs =
            std::env::var("FTS_OMNISPHERE_PACKS").unwrap_or_else(|_| OMNISPHERE_PACKS_ROOT.into());
        // Packs overwrite raw entries.
        idx.scan_dir(Path::new(&packs), 0);

        // Keyscape runs *inside* Omnisphere, so an Omnisphere patch can name a
        // Keyscape soundsource — the gig's "Hammered Dolceola" and "MK-80
        // Rhodes" both do. Those packs live in their own tree, so without this
        // the patch resolves half its layers and quietly plays thin.
        let keyscape =
            std::env::var("FTS_KEYSCAPE_PACKS").unwrap_or_else(|_| KEYSCAPE_PACKS_ROOT.into());
        idx.scan_dir(Path::new(&keyscape), 0);
        // …and the NI pianos, so one index answers for every family a keys
        // profile can name.
        let ni = std::env::var("FTS_NI_PIANO_PACKS").unwrap_or_else(|_| NI_PIANO_PACKS_ROOT.into());
        idx.scan_dir(Path::new(&ni), 0);
        // Finally the authored patches. Last so a built pack of the same name
        // wins: a pack is cheaper to play than re-realizing a patch tree.
        let patches = std::env::var("FTS_OMNISPHERE_PATCHES").unwrap_or_else(|_| PATCH_ROOT.into());
        idx.scan_dir(Path::new(&patches), 0);
        idx
    }

    fn scan_dir(&mut self, dir: &Path, depth: usize) {
        if depth > 4 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // A multisample soundsource dir: <Name>/library.styx.
                let lib = path.join("library.styx");
                if lib.exists() {
                    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                        self.by_name.insert(name.to_lowercase(), lib);
                    }
                } else {
                    self.scan_dir(&path, depth + 1);
                }
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("signalpack"))
            {
                // A built pack (preferred): <Name>.signalpack.
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    self.by_name.insert(stem.to_lowercase(), path.clone());
                }
            } else if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("prt_omn"))
            {
                // An Omnisphere patch. A soundsource is *what realizes a
                // source block*, and a synthesis-mode patch realizes one as an
                // oscillator exactly as a pack realizes one as a sampler — so
                // it belongs in the same index. The rig's "Worship PHAT Bass"
                // is only reachable this way: it names no soundsource and was
                // never saved to the Spectrasonics library, so the copy
                // exported from the gig is the only one there is.
                //
                // Keyed by the patch's own name, taken from the filename after
                // any `<instance>.partN.` prefix that `gig_extract dump` adds.
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let name = stem.rsplit_once('.').map_or(stem, |(_, n)| n);
                    self.by_name
                        .entry(name.replace('_', " ").to_lowercase())
                        .or_insert_with(|| path.clone());
                }
            } else if path.extension().is_some_and(|e| e == "styx")
                && path.file_name().is_some_and(|f| f != "library.styx")
            {
                // A flat one-shot: <Name>.styx beside its FLAC.
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    self.by_name.insert(stem.to_lowercase(), path.clone());
                }
            }
        }
    }

    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }

    /// Look a soundsource up by its patch name (case-insensitive).
    ///
    /// Falls back to a normalized match when the exact name misses, because
    /// patch names and extracted folder names disagree in two systematic ways:
    ///
    /// - Omnisphere marks a multi-dynamic soundsource with a trailing `^`
    ///   (`Choir Men Ohs  ^`, sometimes double-spaced) and a patch selects one
    ///   of its layers with a ` - <dyn>` suffix (`Choir Men Ohs - mf`). Our
    ///   extraction flattens the dynamics into the one folder, so the suffix
    ///   has nothing left to select and the base name is the right target.
    /// - Whitespace runs differ between the two.
    ///
    /// Exact matches always win, so this can only rescue a lookup that would
    /// otherwise have failed outright.
    pub fn find(&self, name: &str) -> Option<&Path> {
        if let Some(p) = self.by_name.get(&name.to_lowercase()) {
            return Some(p.as_path());
        }
        let want = normalize_soundsource_name(name);
        if want.is_empty() {
            return None;
        }
        let keys: Vec<&str> = self.by_name.keys().map(String::as_str).collect();
        let hit = resolve_name(name, keys.iter().copied())?.to_string();
        self.by_name.get(&hit).map(|p| p.as_path())
    }
}

/// Match `want` against a set of known names, in three tiers.
///
/// Shared so every lookup agrees: the index resolves a patch to a file, the
/// keys backend resolves the same name against its own scanned library, and
/// the two disagreeing is precisely how a lane ends up silently empty. It has
/// happened once already.
///
/// 1. Exact (case-insensitive).
/// 2. [`normalize_soundsource_name`] on both sides — drops the `^`
///    multi-dynamic marker and anything after it, plus a trailing ` - <dyn>`.
/// 3. A trailing single-letter word, which is a Keyscape capture variant the
///    extraction flattened (`Clavichord a ^ RR` against `Clavichord`).
///
/// Tier 3 is deliberately narrow. A general prefix match would let
/// "Choir Men Ahs" find a "Choir Men", and playing the wrong instrument is
/// worse than failing to find one.
pub fn resolve_name<'a>(
    want: &str,
    known: impl Iterator<Item = &'a str> + Clone,
) -> Option<&'a str> {
    let lower = want.to_lowercase();
    if let Some(k) = known.clone().find(|k| k.to_lowercase() == lower) {
        return Some(k);
    }
    let norm = normalize_soundsource_name(want);
    if norm.is_empty() {
        return None;
    }
    if let Some(k) = known
        .clone()
        .find(|k| normalize_soundsource_name(k) == norm)
    {
        return Some(k);
    }
    let (head, last) = norm.rsplit_once(' ')?;
    if last.chars().count() == 1 && last.chars().all(|c| c.is_ascii_alphabetic()) {
        return known
            .into_iter()
            .find(|k| normalize_soundsource_name(k) == head);
    }
    None
}

/// Reduce a soundsource name to the part that identifies the *source* rather
/// than which of its dynamic layers a patch wanted.
///
/// Public because the keys backend does its own library scan and needs to
/// match names the same way — two lookups disagreeing is how a lane ends up
/// silently empty.
pub fn normalize_soundsource_name(name: &str) -> String {
    let mut s = name.to_lowercase();
    // Everything from the `^` marker onward describes *which capture* of the
    // source a patch wanted — the marker itself, and any round-robin variant
    // after it (`Dolceola ^ RR Lite`, `Clavichord a ^ RR`). The extraction
    // ships one folder per source, so all of that resolves to the same pack.
    if let Some(caret) = s.find('^') {
        s.truncate(caret);
    }
    s = s.trim_end().to_string();
    // Drop a trailing dynamic selector: " - mf", " - ff", " - p" …
    if let Some((head, tail)) = s.rsplit_once(" - ") {
        const DYNAMICS: [&str; 8] = ["ppp", "pp", "p", "mp", "mf", "f", "ff", "fff"];
        if DYNAMICS.contains(&tail.trim()) {
            s = head.to_string();
        }
    }
    // Collapse whitespace runs — folder names carry stray doubles.
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::normalize_soundsource_name;

    #[test]
    fn dynamic_suffix_and_multidynamic_marker_normalize_together() {
        // The real mismatch: the gig's Gentle Gothics asks for these names,
        // the extraction wrote those folders.
        assert_eq!(
            normalize_soundsource_name("Choir Men Ohs - mf"),
            normalize_soundsource_name("Choir Men Ohs  ^")
        );
        assert_eq!(
            normalize_soundsource_name("Choir Women Oos - mf"),
            normalize_soundsource_name("Choir Women Oos  ^")
        );
        // Every dynamic marker, not just mf.
        for dyn_ in ["ppp", "pp", "p", "mp", "mf", "f", "ff", "fff"] {
            assert_eq!(normalize_soundsource_name(&format!("Pad - {dyn_}")), "pad");
        }
    }

    /// Everything from `^` onward is which-capture, not which-source.
    #[test]
    fn round_robin_variants_after_the_caret_normalize_to_the_base_source() {
        assert_eq!(normalize_soundsource_name("Dolceola ^ RR Lite"), "dolceola");
        assert_eq!(
            normalize_soundsource_name("MK-80 Contemporary Rhodes ^"),
            "mk-80 contemporary rhodes"
        );
        assert_eq!(
            normalize_soundsource_name("Clavichord a ^ RR"),
            "clavichord a"
        );
    }

    /// The trailing-letter tier is narrow on purpose. A general prefix match
    /// would let a query find a strictly shorter source and play the wrong
    /// instrument, which is worse than not finding one.
    #[test]
    fn only_a_single_trailing_letter_is_treated_as_a_capture_variant() {
        let strip = |q: &str| -> Option<String> {
            let want = normalize_soundsource_name(q);
            let (head, last) = want.rsplit_once(' ')?;
            (last.chars().count() == 1 && last.chars().all(|c| c.is_ascii_alphabetic()))
                .then(|| head.to_string())
        };
        assert_eq!(strip("Clavichord a ^ RR").as_deref(), Some("clavichord"));
        // A real name whose last word is one letter would also strip — but only
        // AFTER exact and normalized lookups miss, and "Hohner Clavinet" is not
        // a source, so nothing wrong can be reached.
        assert_eq!(
            strip("Hohner Clavinet C").as_deref(),
            Some("hohner clavinet")
        );
        // Multi-letter tails are never stripped.
        assert_eq!(strip("Choir Men Ahs"), None);
        assert_eq!(strip("Big Berthas Lead"), None);
    }

    #[test]
    fn a_hyphen_that_is_not_a_dynamic_is_left_alone() {
        // Real soundsource names contain hyphens; only a trailing dynamic
        // token may be stripped, or distinct sources would collide.
        assert_eq!(
            normalize_soundsource_name("OB-8 PWM Big Strings"),
            "ob-8 pwm big strings"
        );
        assert_eq!(
            normalize_soundsource_name("Rhodes - LA Custom"),
            "rhodes - la custom"
        );
        assert_ne!(
            normalize_soundsource_name("Choir Men Ohs - mf"),
            normalize_soundsource_name("Choir Men Ahs - mf")
        );
    }
}
