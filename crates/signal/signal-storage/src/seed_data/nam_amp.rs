//! NAM amp block presets — ML Sound Labs full-rig captures.
//!
//! Each preset corresponds to a real amp captured by ML Sound Labs using Neural Amp
//! Modeler. Variations are FULL-rig captures (amp + cabinet + mic) at different gain
//! stages (Clean, Drive, Lead/Overdrive).
//!
//! Unlike the parameter-only presets in `amp.rs`, these include `state_data` — the
//! absolute path to the `.nam` model file. At load time, the DAW bridge gets the
//! plugin's default REAPER chunk, rewrites the model path via `nam-manager`, and
//! applies the updated state.

use nam_manager::{full_rig_models_by_pack, FullRigModel, PackDefinition};
use signal_proto::metadata::Metadata;
use signal_proto::{seed_id, Block, BlockParameter, BlockType, Preset, Snapshot};
use std::path::PathBuf;

/// NAM plugin parameter defaults (normalized 0.0–1.0).
const INPUT_LEVEL: f32 = 0.5;
const OUTPUT_LEVEL: f32 = 0.5;
const NOISE_GATE_THRESHOLD: f32 = 0.0;
const NOISE_GATE_ACTIVE: f32 = 0.0;

/// Generate NAM amp block presets from ML Sound Labs packs.
///
/// Returns an empty vec if the signal-library NAM directory is not found.
pub fn presets() -> Vec<Preset> {
    let nam_root = match find_nam_root() {
        Some(r) => r,
        None => return vec![],
    };

    let packs_dir = nam_root.join("packs");
    let search_roots: Vec<&std::path::Path> = vec![nam_root.as_path()];

    let packs_with_models =
        match full_rig_models_by_pack(&packs_dir, &search_roots, "ML Sound Labs") {
            Ok(v) => v,
            Err(_) => return vec![],
        };

    let mut out = Vec::new();
    for (pack, models) in &packs_with_models {
        if let Some(preset) = build_preset(pack, models) {
            out.push(preset);
        }
    }

    out
}

/// Build a single Preset from a pack + its FULL-rig models.
fn build_preset(pack: &PackDefinition, models: &[FullRigModel]) -> Option<Preset> {
    if models.is_empty() {
        return None;
    }

    // Preset name: strip "ML Sound Labs — " prefix, add [NAM] suffix
    let amp_name = pack
        .label
        .strip_prefix("ML Sound Labs — ")
        .or_else(|| pack.label.strip_prefix("ML Sound Labs — "))
        .unwrap_or(&pack.label);
    let preset_name = format!("{} [NAM]", amp_name);

    // Seed ID: "nam-amp-{pack_id}" e.g. "nam-amp-ml-sound-labs-peavey-5150"
    let preset_seed = format!("nam-amp-{}", pack.id);

    // First model becomes the default snapshot, rest are additional
    let default_snapshot = build_snapshot(&preset_seed, &models[0], true);
    let additional: Vec<Snapshot> = models[1..]
        .iter()
        .map(|m| build_snapshot(&preset_seed, m, false))
        .collect();

    let metadata = Metadata::new()
        .with_tag("vendor:ml-sound-labs")
        .with_tag("plugin:nam")
        .with_tag("source:VST3: NeuralAmpModeler (Steven Atkinson)");

    Some(
        Preset::new(
            seed_id(&preset_seed),
            preset_name,
            BlockType::Amp,
            default_snapshot,
            additional,
        )
        .with_metadata(metadata),
    )
}

/// Build a Snapshot for a single FULL-rig model.
fn build_snapshot(preset_seed: &str, model: &FullRigModel, is_default: bool) -> Snapshot {
    // Snapshot name from tone: "Clean", "Drive", "Lead", etc.
    let snapshot_name = model
        .tone
        .as_deref()
        .map(capitalize_tone)
        .unwrap_or_else(|| snapshot_name_from_filename(&model.filename));

    // Seed ID: "nam-amp-{pack_id}-{tone}" or "-default"
    let snapshot_seed = if is_default {
        format!("{}-default", preset_seed)
    } else {
        let tone_slug = model
            .tone
            .as_deref()
            .unwrap_or("variation")
            .to_lowercase()
            .replace(' ', "-");
        format!("{}-{}", preset_seed, tone_slug)
    };

    let block = nam_block();
    let state_data = build_state_data(&model.absolute_path);

    let mut snapshot = Snapshot::new(seed_id(&snapshot_seed), &snapshot_name, block);

    if let Some(data) = state_data {
        snapshot = snapshot.with_state_data(data);
    }

    // Add tone tag to snapshot metadata
    if let Some(ref tone) = model.tone {
        let metadata = Metadata::new().with_tag(format!("tone:{}", tone));
        snapshot = snapshot.with_metadata(metadata);
    }

    snapshot
}

/// Create a Block with NAM plugin parameters at sensible defaults.
fn nam_block() -> Block {
    Block::from_parameters(vec![
        BlockParameter::new("input_level", "Input Level", INPUT_LEVEL),
        BlockParameter::new("output_level", "Output Level", OUTPUT_LEVEL),
        BlockParameter::new(
            "noise_gate_threshold",
            "Noise Gate Threshold",
            NOISE_GATE_THRESHOLD,
        ),
        BlockParameter::new("noise_gate", "Noise Gate", NOISE_GATE_ACTIVE),
    ])
}

/// Build state data for a NAM snapshot: just the model path as UTF-8 bytes.
///
/// At load time, `execute_fx_load` detects NAM presets and performs the full
/// REAPER chunk rewrite: get default chunk → decode → rewrite model path →
/// encode → set back. This matches the proven approach in `reaper_nam_load.rs`.
fn build_state_data(model_path: &str) -> Option<Vec<u8>> {
    if model_path.is_empty() {
        return None;
    }
    Some(model_path.as_bytes().to_vec())
}

/// Capitalize a tone string for display: "clean" → "Clean", "overdrive" → "Overdrive"
fn capitalize_tone(tone: &str) -> String {
    let mut chars = tone.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

/// Extract a human-readable snapshot name from a filename when tone is unknown.
/// E.g. "ML FMAN BE Plexi FULL.nam" → "BE Plexi"
fn snapshot_name_from_filename(filename: &str) -> String {
    filename
        .trim_end_matches(".nam")
        .replace(" FULL", "")
        .split_whitespace()
        .skip(2) // Skip "ML XXXX" prefix
        .collect::<Vec<_>>()
        .join(" ")
}

/// Resolve the signal-library/nam/ root directory.
///
/// Uses `CARGO_MANIFEST_DIR` (compile-time) to navigate from this crate
/// up to the workspace root, then into `signal-library/nam/`.
fn find_nam_root() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../signal-library/nam");
    if root.exists() {
        Some(root)
    } else {
        None
    }
}

// ─── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capitalize_tone_works() {
        assert_eq!(capitalize_tone("clean"), "Clean");
        assert_eq!(capitalize_tone("drive"), "Drive");
        assert_eq!(capitalize_tone("overdrive"), "Overdrive");
        assert_eq!(capitalize_tone("lead"), "Lead");
        assert_eq!(capitalize_tone(""), "");
    }

    #[test]
    fn snapshot_name_from_filename_works() {
        assert_eq!(
            snapshot_name_from_filename("ML PEAV Block Clean FULL.nam"),
            "Block Clean"
        );
        assert_eq!(
            snapshot_name_from_filename("ML FMAN BE Plexi FULL.nam"),
            "BE Plexi"
        );
    }

    #[test]
    fn nam_block_has_4_params() {
        let block = nam_block();
        assert_eq!(block.parameters().len(), 4);
    }

    #[test]
    fn build_state_data_stores_model_path() {
        let data = build_state_data("/test/model.nam");
        assert!(data.is_some());
        let bytes = data.unwrap();
        // state_data is just the model path as UTF-8 bytes
        let path = std::str::from_utf8(&bytes).expect("should be valid UTF-8");
        assert_eq!(path, "/test/model.nam");
    }

    #[test]
    fn build_state_data_empty_returns_none() {
        assert!(build_state_data("").is_none());
    }

    #[test]
    fn presets_generates_nam_amps() {
        let p = presets();
        // On machines with the NAM library, we should get 9 presets
        // On CI/machines without it, gracefully returns empty
        if p.is_empty() {
            eprintln!("NAM library not found — skipping preset content checks");
            return;
        }

        assert_eq!(p.len(), 9, "expected 9 ML Sound Labs NAM amp presets");

        for preset in &p {
            assert_eq!(preset.block_type(), BlockType::Amp);
            assert!(preset.name().ends_with("[NAM]"), "name should end with [NAM]: {}", preset.name());
            assert!(preset.snapshots().len() >= 3, "each preset needs at least 3 snapshots (clean/drive/lead)");

            // Every snapshot should have state_data
            for snap in preset.snapshots() {
                assert!(
                    snap.state_data().is_some(),
                    "snapshot '{}' in '{}' missing state_data",
                    snap.name(),
                    preset.name()
                );
            }
        }
    }

    #[test]
    fn preset_ids_are_unique() {
        let presets = presets();
        let mut ids = std::collections::HashSet::new();
        for preset in &presets {
            assert!(ids.insert(preset.id().to_string()), "duplicate preset id: {}", preset.id());
        }
    }

    #[test]
    fn snapshot_ids_globally_unique() {
        let presets = presets();
        let mut ids = std::collections::HashSet::new();
        for preset in &presets {
            for snapshot in preset.snapshots() {
                assert!(
                    ids.insert(snapshot.id().to_string()),
                    "duplicate snapshot id: {}",
                    snapshot.id()
                );
            }
        }
    }

    #[test]
    fn parameter_values_in_range() {
        for preset in presets() {
            for snapshot in preset.snapshots() {
                for param in snapshot.block().parameters() {
                    let v = param.value().get();
                    assert!(
                        (0.0..=1.0).contains(&v),
                        "preset '{}' snapshot '{}' param '{}' = {} out of range",
                        preset.name(),
                        snapshot.name(),
                        param.id(),
                        v,
                    );
                }
            }
        }
    }
}
