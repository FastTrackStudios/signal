//! The rig as a browser plays it — what `signal rig web-bundle` writes.
//!
//! Every profile's patches, resolved exactly as the live rig resolves them
//! ([`crate::nodes::profile_from_library`]), with each chain's models and
//! IRs copied beside it under content-addressed keys:
//!
//! ```text
//! <out>/rig.json                 WebBundle
//! <out>/assets/<blake3>.nam      every model a chain uses, once
//! <out>/assets/<blake3>.wav      every IR
//! ```
//!
//! Content addressing does three jobs: a capture shared by twenty patches is
//! shipped once, a changed capture gets a new URL (so browsers can cache
//! assets forever), and no local path — a home directory, a Downloads
//! folder — ends up in a bundle that is served to the public.

use std::collections::BTreeMap;
use std::path::Path;

use facet::Facet;
use signal_sampler::RigBlock;

/// Bumped when the shape changes incompatibly.
pub const BUNDLE_VERSION: u32 = 1;

#[derive(Facet, Clone, Debug)]
pub struct WebBundle {
    pub version: u32,
    pub profiles: Vec<WebProfile>,
    pub assets: Vec<WebAsset>,
}

#[derive(Facet, Clone, Debug)]
pub struct WebProfile {
    pub name: String,
    pub default_patch: usize,
    pub patches: Vec<WebPatch>,
}

#[derive(Facet, Clone, Debug)]
pub struct WebPatch {
    pub name: String,
    pub preset: String,
    pub scene: String,
    pub input_trim_db: f32,
    pub output_trim_db: f32,
    /// The chain, asset paths replaced by bundle keys.
    pub blocks: Vec<RigBlock>,
}

#[derive(Facet, Clone, Debug)]
pub struct WebAsset {
    /// Relative to the bundle root; also the key the rig installs it under.
    pub key: String,
    pub bytes: u64,
}

/// What an export skipped, so the CLI can say so.
#[derive(Debug, Default)]
pub struct Skipped {
    /// Blocks hosting a native plugin — there is no plugin host in a browser.
    pub plugin_blocks: usize,
    /// Blocks with nothing to run yet (see [`RigBlock::has_backend`]).
    pub placeholders: usize,
    /// Assets a chain names that are not on disk (the block is dropped).
    pub missing: Vec<String>,
}

/// Write the bundle for `profiles` (all when empty) into `out`.
///
/// # Errors
///
/// When the bundle cannot be written.
pub fn export(out: &Path, profiles: &[String]) -> Result<(WebBundle, Skipped), String> {
    let lib = crate::library::RigLibrary::load_or_bootstrap();
    let assets_dir = out.join("assets");
    std::fs::create_dir_all(&assets_dir).map_err(|e| format!("{}: {e}", assets_dir.display()))?;

    let mut keys: BTreeMap<String, Option<WebAsset>> = BTreeMap::new();
    let mut skipped = Skipped::default();
    let mut web_profiles = Vec::new();

    for def in &lib.profiles {
        if !profiles.is_empty() && !profiles.iter().any(|p| p.eq_ignore_ascii_case(&def.name)) {
            continue;
        }
        let resolved = crate::nodes::profile_from_library(def, &lib.drive_presets);
        let mut patches = Vec::new();
        for patch in resolved.patches {
            let mut blocks = Vec::with_capacity(patch.chain.len());
            for mut block in patch.chain {
                if block.is_plugin() {
                    skipped.plugin_blocks += 1;
                    continue;
                }
                // A placeholder (Pitch, until it has DSP): the live rig skips
                // it at install, so the bundle leaves it out — keeping every
                // slot index the page, the planner and the worklet use equal.
                if !block.has_backend() {
                    skipped.placeholders += 1;
                    continue;
                }
                let mut ok = true;
                for path in [&mut block.nam, &mut block.ir] {
                    if path.is_empty() {
                        continue;
                    }
                    match bundle_asset(path, &assets_dir, &mut keys) {
                        Some(key) => *path = key,
                        None => {
                            skipped.missing.push(path.clone());
                            ok = false;
                        }
                    }
                }
                if ok {
                    blocks.push(block);
                }
            }
            patches.push(WebPatch {
                name: patch.name,
                preset: patch.preset,
                scene: patch.scene,
                input_trim_db: patch.input_trim_db,
                output_trim_db: patch.output_trim_db,
                blocks,
            });
        }
        web_profiles.push(WebProfile {
            name: resolved.name,
            default_patch: resolved.default_patch,
            patches,
        });
    }

    skipped.missing.sort();
    skipped.missing.dedup();
    let bundle = WebBundle {
        version: BUNDLE_VERSION,
        profiles: web_profiles,
        // Two source paths with the same bytes are one asset.
        assets: keys
            .into_values()
            .flatten()
            .map(|a| (a.key.clone(), a))
            .collect::<BTreeMap<_, _>>()
            .into_values()
            .collect(),
    };
    let json = facet_json::to_string(&bundle).map_err(|e| format!("rig.json: {e}"))?;
    std::fs::write(out.join("rig.json"), json).map_err(|e| format!("rig.json: {e}"))?;
    Ok((bundle, skipped))
}

/// Copy the asset at `path` into the bundle once; its bundle key.
/// `keys` maps source paths to what they became (`None`: missing).
fn bundle_asset(
    path: &str,
    dir: &Path,
    keys: &mut BTreeMap<String, Option<WebAsset>>,
) -> Option<String> {
    if let Some(done) = keys.get(path) {
        return done.as_ref().map(|a| a.key.clone());
    }
    let asset = std::fs::read(path).ok().and_then(|bytes| {
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin")
            .to_ascii_lowercase();
        let hash = blake3::hash(&bytes).to_hex();
        let key = format!("assets/{}.{ext}", &hash[..16]);
        let file = dir.join(format!("{}.{ext}", &hash[..16]));
        if !file.exists() {
            std::fs::write(&file, &bytes).ok()?;
        }
        Some(WebAsset {
            key,
            bytes: bytes.len() as u64,
        })
    });
    let key = asset.as_ref().map(|a| a.key.clone());
    keys.insert(path.to_string(), asset);
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two paths with the same bytes are one asset; a missing path is
    /// reported, not bundled.
    #[test]
    fn assets_are_content_addressed_and_deduped() {
        let tmp = std::env::temp_dir().join(format!("signal-web-bundle-{}", std::process::id()));
        let src = tmp.join("src");
        std::fs::create_dir_all(&src).unwrap();
        let (a, b) = (src.join("a.nam"), src.join("b.NAM"));
        std::fs::write(&a, b"same").unwrap();
        std::fs::write(&b, b"same").unwrap();
        let dir = tmp.join("assets");
        std::fs::create_dir_all(&dir).unwrap();
        let mut keys = BTreeMap::new();
        let ka = bundle_asset(a.to_str().unwrap(), &dir, &mut keys).unwrap();
        let kb = bundle_asset(b.to_str().unwrap(), &dir, &mut keys).unwrap();
        assert_eq!(ka, kb);
        assert!(ka.starts_with("assets/") && ka.ends_with(".nam"));
        assert!(!ka.contains("src"), "no source path leaks into the key");
        assert!(bundle_asset("/nope/missing.nam", &dir, &mut keys).is_none());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
