//! **Multi** (`.mlt_omn`) parsing and mapping — 8 Parts + the master mixer.

use std::path::Path;

use super::model::{OmniPatch, parse_patch_node};
use super::tree::patch_to_container;
use super::{SoundsourceIndex, parse_xml};
use crate::rig_node::Container;

// ── Multis ───────────────────────────────────────────────────────────────────

/// A parsed `.mlt_omn` Multi: up to 8 Parts + their mixer strip.
#[derive(Debug, Clone, Default)]
pub struct OmniMulti {
    pub name: String,
    /// `(patch, level 0..1, muted)` per part.
    pub parts: Vec<(OmniPatch, f32, bool)>,
}

/// Parse a `.mlt_omn` document: `SynthMaster` wraps 8 `SynthEngine` parts
/// plus `MasterEngineBaseParamBlock` mixer attrs (`pLevel0..7`, `pMute0..7`).
pub fn parse_multi(xml: &str) -> Result<OmniMulti, String> {
    let root = parse_xml(xml)?;
    let mut multi = OmniMulti::default();
    if let Some(descr) = root.child("ENTRYDESCR") {
        multi.name = descr.attr("name").unwrap_or("").to_string();
    }
    let mixer = root.find("MasterEngineBaseParamBlock");
    for (i, engine) in root.children_tagged("SynthSubEngine").enumerate() {
        let patch = parse_patch_node(engine)?;
        let level = mixer
            .and_then(|m| m.num(&format!("pLevel{i}")))
            .unwrap_or(0.75)
            .clamp(0.0, 1.0);
        let muted = mixer
            .and_then(|m| m.num(&format!("pMute{i}")))
            .unwrap_or(0.0)
            != 0.0;
        multi.parts.push((patch, level, muted));
    }
    if multi.parts.is_empty() {
        return Err("no SynthEngine parts (not an Omnisphere multi?)".into());
    }
    Ok(multi)
}

/// Map a Multi onto one composition tree: Parts sum in parallel, each with
/// its mixer level (0.75 ≈ unity — CALIBRATE) and mute (bypass).
pub fn multi_to_container(multi: &OmniMulti, index: &SoundsourceIndex) -> Container {
    let title = if multi.name.is_empty() {
        "Omnisphere Multi".to_string()
    } else {
        multi.name.clone()
    };
    let mut parts = Container::parallel("Parts");
    for (i, (patch, level, muted)) in multi.parts.iter().enumerate() {
        // Skip empty default parts (no soundsource, no layers of note).
        let named = !patch.name.is_empty();
        let has_content = named || patch.layers.iter().any(|l| !l.soundsource.is_empty());
        if !has_content {
            continue;
        }
        let mut part = patch_to_container(patch, index);
        part.role = crate::rig_node::Role::Engine;
        part.name = format!("Part {}: {}", i + 1, patch.name);
        part.output_db = if *level <= 0.0 {
            -60.0
        } else {
            (20.0 * (level / 0.75).log10()).max(-60.0)
        };
        part.bypassed = *muted;
        parts = parts.add(part);
    }
    Container::preset(title).add(parts)
}

/// Convenience: read + parse + map a `.mlt_omn` file.
pub fn load_multi_file(path: &Path, index: &SoundsourceIndex) -> Result<Container, String> {
    let xml = std::fs::read_to_string(path).map_err(|e| format!("read {path:?}: {e}"))?;
    let multi = parse_multi(&xml)?;
    Ok(multi_to_container(&multi, index))
}
