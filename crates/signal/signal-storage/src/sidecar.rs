//! Sidecar metadata for REAPER-native signal presets (`.signal.styx` files).
//!
//! Each `.RfxChain` or `.RTrackTemplate` file can have an optional sidecar
//! `Foo.signal.styx` alongside it, providing signal-specific metadata (tags,
//! description, block type, parameters) that REAPER's native format doesn't carry.
//!
//! Files without sidecars are auto-indexed from their path: folder name → category,
//! file stem → display name, path hash → stable ID.

use std::path::Path;

use facet::Facet;

// ─── Sidecar types ────────────────────────────────────────────

/// The type of signal preset this sidecar describes.
#[derive(Debug, Clone, PartialEq, Facet)]
#[repr(C)]
pub enum PresetKind {
    /// A single FX plugin with state (lives in FXChains/01-Blocks/).
    Block { block_type: String },
    /// An ordered chain of blocks (lives in FXChains/02-Modules/).
    Module,
    /// A module + track config, instrument-scoped (TrackTemplates/{instrument}/01-Layers/).
    Layer,
    /// Layer selections, instrument-scoped (TrackTemplates/{instrument}/02-Engines/).
    Engine,
    /// Engine selections, instrument-scoped (TrackTemplates/{instrument}/03-Rigs/).
    Rig,
    /// Named configurations, instrument-scoped (TrackTemplates/{instrument}/04-Profiles/).
    Profile,
    /// Section-based performance structure (TrackTemplates/{instrument}/05-Songs/).
    Song,
    /// Multi-rig setups with nested profiles/songs (TrackTemplates/Racks/).
    Rack,
}

/// A parameter entry in the sidecar (for display/search, not for loading state).
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct SidecarParam {
    pub id: String,
    pub name: String,
    pub value: f64,
}

/// The `.signal.styx` sidecar file content.
///
/// Deserialized via `facet-styx` from files like:
/// ```styx
/// version 1
/// id "550e8400-e29b-41d4-a716-446655440000"
/// kind @block{block_type amp}
/// tags (neural-dsp clean funk)
/// description "Clean funk tone"
/// ```
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct SignalSidecar {
    /// Schema version (always 1 for now).
    pub version: u32,
    /// Stable UUID for this preset.
    pub id: String,
    /// What kind of signal preset this is.
    pub kind: PresetKind,
    /// Freeform tags for search and filtering.
    #[facet(default)]
    pub tags: Vec<String>,
    /// Human-readable description.
    #[facet(default)]
    pub description: Option<String>,
    /// Curated parameter values for display/search.
    #[facet(default)]
    pub parameters: Vec<SidecarParam>,
}

// ─── Read/write helpers ───────────────────────────────────────

/// Compute the `.signal.styx` sidecar path for a given REAPER preset file.
///
/// `Foo.RfxChain` → `Foo.signal.styx`
/// `Foo.RTrackTemplate` → `Foo.signal.styx`
pub fn sidecar_path(preset_path: &Path) -> std::path::PathBuf {
    let stem = preset_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    preset_path.with_file_name(format!("{stem}.signal.styx"))
}

/// Read and parse a `.signal.styx` sidecar file, returning `None` if missing or malformed.
pub fn read_sidecar(preset_path: &Path) -> Option<SignalSidecar> {
    let path = sidecar_path(preset_path);
    let content = std::fs::read_to_string(&path).ok()?;
    match facet_styx::from_str::<SignalSidecar>(&content) {
        Ok(sidecar) => Some(sidecar),
        Err(e) => {
            eprintln!(
                "[signal-storage] Failed to parse sidecar {}: {e}",
                path.display()
            );
            None
        }
    }
}

/// Write a `.signal.styx` sidecar file alongside a REAPER preset file.
pub fn write_sidecar(preset_path: &Path, sidecar: &SignalSidecar) -> std::io::Result<()> {
    let path = sidecar_path(preset_path);
    let content = render_sidecar_styx(sidecar);
    std::fs::write(&path, content)
}

/// Render a `SignalSidecar` to styx format string.
///
/// We hand-render rather than using a serializer since facet-styx is
/// deserialization-focused and the output is simple enough to template.
pub fn render_sidecar_styx(s: &SignalSidecar) -> String {
    let mut out = String::new();

    out.push_str(&format!("version {}\n", s.version));
    out.push_str(&format!("id \"{}\"\n", s.id));

    // Render kind as a styx tag — PascalCase to match Facet enum variant names
    match &s.kind {
        PresetKind::Block { block_type } => {
            out.push_str(&format!("kind @Block{{block_type {block_type}}}\n"));
        }
        PresetKind::Module => out.push_str("kind @Module@\n"),
        PresetKind::Layer => out.push_str("kind @Layer@\n"),
        PresetKind::Engine => out.push_str("kind @Engine@\n"),
        PresetKind::Rig => out.push_str("kind @Rig@\n"),
        PresetKind::Profile => out.push_str("kind @Profile@\n"),
        PresetKind::Song => out.push_str("kind @Song@\n"),
        PresetKind::Rack => out.push_str("kind @Rack@\n"),
    }

    if !s.tags.is_empty() {
        let tags: Vec<_> = s.tags.iter().map(|t| quote_if_needed(t)).collect();
        out.push_str(&format!("tags ({})\n", tags.join(" ")));
    }

    if let Some(desc) = &s.description {
        out.push_str(&format!("description \"{}\"\n", desc.replace('"', "\\\"")));
    }

    if !s.parameters.is_empty() {
        out.push_str("parameters (\n");
        for p in &s.parameters {
            out.push_str(&format!(
                "  {{id {}, name {}, value {}}}\n",
                quote_if_needed(&p.id),
                quote_if_needed(&p.name),
                p.value
            ));
        }
        out.push_str(")\n");
    }

    out
}

/// Quote a string value if it contains spaces or special characters.
fn quote_if_needed(s: &str) -> String {
    if s.contains(char::is_whitespace) || s.contains('"') || s.is_empty() {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn sidecar_path_for_rfxchain() {
        let p = PathBuf::from("/foo/bar/Clean Funk.RfxChain");
        assert_eq!(
            sidecar_path(&p),
            PathBuf::from("/foo/bar/Clean Funk.signal.styx")
        );
    }

    #[test]
    fn sidecar_path_for_track_template() {
        let p = PathBuf::from("/foo/bar/Drive Layer.RTrackTemplate");
        assert_eq!(
            sidecar_path(&p),
            PathBuf::from("/foo/bar/Drive Layer.signal.styx")
        );
    }

    #[test]
    fn round_trip_block_sidecar() {
        let sidecar = SignalSidecar {
            version: 1,
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            kind: PresetKind::Block {
                block_type: "amp".to_string(),
            },
            tags: vec!["neural-dsp".to_string(), "clean".to_string()],
            description: Some("Clean funk tone".to_string()),
            parameters: vec![SidecarParam {
                id: "gain".to_string(),
                name: "Gain".to_string(),
                value: 0.5,
            }],
        };

        let rendered = render_sidecar_styx(&sidecar);
        let parsed: SignalSidecar = facet_styx::from_str(&rendered)
            .expect("should parse rendered styx");

        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.id, sidecar.id);
        assert_eq!(parsed.kind, sidecar.kind);
        assert_eq!(parsed.tags, sidecar.tags);
        assert_eq!(parsed.description, sidecar.description);
        assert_eq!(parsed.parameters.len(), 1);
        assert_eq!(parsed.parameters[0].id, "gain");
    }

    #[test]
    fn round_trip_module_sidecar() {
        let sidecar = SignalSidecar {
            version: 1,
            id: "test-module-id".to_string(),
            kind: PresetKind::Module,
            tags: vec!["worship".to_string(), "clean".to_string()],
            description: None,
            parameters: vec![],
        };

        let rendered = render_sidecar_styx(&sidecar);
        let parsed: SignalSidecar = facet_styx::from_str(&rendered)
            .expect("should parse rendered styx");

        assert_eq!(parsed.kind, PresetKind::Module);
        assert_eq!(parsed.tags, sidecar.tags);
        assert!(parsed.description.is_none());
        assert!(parsed.parameters.is_empty());
    }

    #[test]
    fn parse_minimal_sidecar() {
        let styx = r#"
version 1
id test-id
kind @Module@
"#;
        let parsed: SignalSidecar = facet_styx::from_str(styx)
            .expect("should parse minimal sidecar");

        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.id, "test-id");
        assert_eq!(parsed.kind, PresetKind::Module);
        assert!(parsed.tags.is_empty());
        assert!(parsed.description.is_none());
    }
}
