//! Load and compose keybind profiles from `.styx` config files on disk.

use std::path::Path;

use tracing::{info, warn};

use super::types::{
    KeybindDef, MouseBindDef, OverlayConfig, ProfileConfig, SectionConfig, WheelBindDef,
    WhichKeyEntryDef, WhichKeyTreeDef, WorkflowConfig,
};
use crate::input::keybinds::which_key::WhichKeyEntry;
use crate::input::keybinds::{Keybind, KeybindOverride, KeybindPreset, MouseModifier, WheelBind};
use crate::input::mouse_modifiers::core::parse_modifier_flags;
use crate::input::mouse_modifiers::manager::MouseModifierOverride;

// ── Public entry point ────────────────────────────────────────────────────────

/// Load a [`KeybindPreset`] from a profile directory.
///
/// `dir` should be e.g. `~/.config/REAPER/FTS/keybinds/fasttrackstudio`.
/// Returns `None` if `profile.styx` cannot be read or parsed.
pub fn load_profile_preset(dir: &Path) -> Option<KeybindPreset> {
    let profile = load_profile(dir)?;

    let mut all_bindings: Vec<Keybind> = Vec::new();
    let mut all_which_key: Vec<crate::input::keybinds::WhichKeyTree> = Vec::new();
    let mut all_wheel: Vec<WheelBind> = Vec::new();
    let mut all_mouse: Vec<MouseModifier> = Vec::new();

    for filename in &profile.sections {
        let section_path = dir.join(filename);
        match load_section(&section_path) {
            Some(section) => {
                all_bindings.extend(section.bindings().iter().map(convert_keybind));
                all_which_key.extend(section.which_key().iter().map(convert_which_key_tree));
                all_wheel.extend(section.wheel().iter().map(convert_wheel));
                all_mouse.extend(section.mouse().iter().map(convert_mouse));
            }
            None => warn!("Failed to load section {:?} — skipping", section_path),
        }
    }

    // Internal ID comes from the directory name; display name from the `name` field.
    let id = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&profile.name)
        .to_string();

    info!(
        "Loaded profile '{}' (display: '{}'): {} bindings, {} wheel, {} mouse",
        id,
        profile.name,
        all_bindings.len(),
        all_wheel.len(),
        all_mouse.len()
    );

    Some(KeybindPreset {
        name: id,
        display_name: profile.name,
        description: profile.description,
        version: profile.version,
        bindings: all_bindings,
        which_key_trees: all_which_key,
        wheel_bindings: all_wheel,
        mouse_modifiers: all_mouse,
    })
}

/// Load a [`KeybindOverride`] from a single overlay `.styx` file.
///
/// The file must contain `name`, `description`, `priority` fields plus optional
/// `bindings`/`wheel`/`mouse` sections (see `overlay.schema.styx`).
pub fn load_overlay(path: &Path) -> Option<KeybindOverride> {
    let contents = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            warn!("Cannot read overlay {:?}: {e}", path);
            return None;
        }
    };
    let cfg: OverlayConfig = match facet_styx::from_str(&contents) {
        Ok(c) => c,
        Err(e) => {
            warn!("Cannot parse overlay {:?}: {e}", path);
            return None;
        }
    };

    let overlay = KeybindOverride::new(&cfg.name, &cfg.description)
        .with_priority(cfg.priority)
        .with_bindings(cfg.bindings().iter().map(convert_keybind))
        .with_wheel_bindings(cfg.wheel().iter().map(convert_wheel));

    info!(
        "Loaded overlay '{}' (priority {}) from {:?}: {} bindings, {} wheel",
        cfg.name,
        cfg.priority,
        path,
        cfg.bindings().len(),
        cfg.wheel().len(),
    );

    Some(overlay)
}

/// Load a [`MouseModifierOverride`] from a single overlay `.styx` file.
///
/// Returns `None` if the file has no `mouse_settings` section, or if it
/// cannot be read/parsed.
pub fn load_mouse_overlay(path: &Path) -> Option<MouseModifierOverride> {
    let contents = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            warn!("Cannot read overlay {:?}: {e}", path);
            return None;
        }
    };
    let cfg: OverlayConfig = match facet_styx::from_str(&contents) {
        Ok(c) => c,
        Err(e) => {
            warn!("Cannot parse overlay {:?}: {e}", path);
            return None;
        }
    };

    if cfg.mouse_settings().is_empty() {
        return None;
    }

    let name: &'static str = Box::leak(cfg.name.clone().into_boxed_str());
    let desc: &'static str = Box::leak(cfg.description.clone().into_boxed_str());
    let priority = cfg.priority;
    let settings: Vec<_> = cfg.mouse_settings().to_vec();
    let settings_len = settings.len();
    let mut overlay = MouseModifierOverride::new(name, desc).with_priority(priority);
    for def in &settings {
        let ctx: &'static str = Box::leak(def.ctx.clone().into_boxed_str());
        let behavior: &'static str = Box::leak(def.behavior.clone().into_boxed_str());
        let flags = parse_modifier_flags(&def.mods);
        let mut setting =
            crate::input::mouse_modifiers::manager::MouseModifierSetting::new(ctx, flags, behavior);
        if let Some(ref d) = def.desc {
            let desc_str: &'static str = Box::leak(d.clone().into_boxed_str());
            setting = setting.with_description(desc_str);
        }
        overlay = overlay.with_setting(setting);
    }

    info!(
        "Loaded mouse overlay '{}' (priority {}) from {:?}: {} settings",
        overlay.name, overlay.priority, path, settings_len
    );

    Some(overlay)
}

/// Load a [`WorkflowConfig`] from a single workflow `.styx` file.
pub fn load_workflow_config(path: &Path) -> Option<WorkflowConfig> {
    let contents = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            warn!("Cannot read workflow {:?}: {e}", path);
            return None;
        }
    };
    match facet_styx::from_str::<WorkflowConfig>(&contents) {
        Ok(cfg) => {
            info!("Loaded workflow config from {:?}", path);
            Some(cfg)
        }
        Err(e) => {
            warn!("Cannot parse workflow {:?}: {e}", path);
            None
        }
    }
}

// ── File loading ──────────────────────────────────────────────────────────────

fn load_profile(dir: &Path) -> Option<ProfileConfig> {
    let path = dir.join("profile.styx");
    let contents = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            warn!("Cannot read profile {:?}: {e}", path);
            return None;
        }
    };
    match facet_styx::from_str::<ProfileConfig>(&contents) {
        Ok(p) => Some(p),
        Err(e) => {
            warn!("Cannot parse profile {:?}: {e}", path);
            None
        }
    }
}

fn load_section(path: &Path) -> Option<SectionConfig> {
    let contents = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            warn!("Cannot read section {:?}: {e}", path);
            return None;
        }
    };
    match facet_styx::from_str::<SectionConfig>(&contents) {
        Ok(s) => Some(s),
        Err(e) => {
            warn!("Cannot parse section {:?}: {e}", path);
            None
        }
    }
}

// ── Conversion ────────────────────────────────────────────────────────────────

fn convert_keybind(def: &KeybindDef) -> Keybind {
    let mut kb = Keybind::new(&def.keys, &def.action);
    if let Some(ref d) = def.desc {
        kb = kb.with_description(d.clone());
    }
    if let Some(ctx) = def.context.clone() {
        kb = kb.with_context(ctx);
    }
    if def.passthrough == Some(true) {
        kb = kb.with_passthrough(true);
    }
    kb
}

fn convert_which_key_tree(def: &WhichKeyTreeDef) -> crate::input::keybinds::WhichKeyTree {
    crate::input::keybinds::WhichKeyTree {
        prefix: def.prefix.clone(),
        label: def.label.clone(),
        entries: def.entries.iter().map(convert_which_key_entry).collect(),
        case_insensitive: def.case_insensitive.unwrap_or(false),
    }
}

fn convert_which_key_entry(def: &WhichKeyEntryDef) -> WhichKeyEntry {
    let children = def.children.as_deref().unwrap_or(&[]);
    if !children.is_empty() {
        let children = children.iter().map(convert_which_key_entry).collect();
        return match def.action.as_deref() {
            Some(action) => {
                WhichKeyEntry::branch_with_action(&def.key, &def.label, action, children)
            }
            None => WhichKeyEntry::branch(&def.key, &def.label, children),
        };
    }

    match def.action.as_deref() {
        Some(action) => WhichKeyEntry::leaf(&def.key, &def.label, action),
        None => {
            tracing::warn!(
                key = %def.key,
                label = %def.label,
                "Which-key entry has no action or children; creating empty branch"
            );
            WhichKeyEntry::branch(&def.key, &def.label, Vec::new())
        }
    }
}

fn convert_wheel(def: &WheelBindDef) -> WheelBind {
    let mut wb = WheelBind::new(&def.modifiers, &def.action);
    if def.horizontal.unwrap_or(false) {
        wb = wb.with_horizontal();
    }
    if let Some(ref d) = def.desc {
        wb = wb.with_description(d.clone());
    }
    if let Some(ctx) = def.context.clone() {
        wb = wb.with_context(ctx);
    }
    wb
}

fn convert_mouse(def: &MouseBindDef) -> MouseModifier {
    let mut mm = MouseModifier::new(def.ctx, &def.modifiers, &def.action);
    if let Some(ref d) = def.desc {
        mm = mm.with_description(d.clone());
    }
    mm
}

#[cfg(test)]
mod tests {
    use input_config_proto::{KeybindContext, SectionConfig};

    /// An app-defined context binding parses out of a section file —
    /// what the expression-editor section relies on. A parse failure
    /// here would otherwise surface only as a silently skipped section
    /// at REAPER startup.
    #[test]
    fn custom_context_binding_parses() {
        // Learn the canonical serialization first, then assert the
        // config file form parses back.
        let canonical = facet_styx::to_string(&SectionConfig {
            bindings: Some(vec![input_config_proto::KeybindDef {
                keys: "e".into(),
                action: "FTS_EXPRESSION_EDITOR_OPEN".into(),
                desc: Some("Close".into()),
                context: Some(KeybindContext::Custom("expression-editor".into())),
                passthrough: None,
                mnemonic: None,
                why: None,
            }]),
            ..Default::default()
        });
        eprintln!("canonical: {canonical:?}");
        let styx = r#"bindings (
            {keys e, action "FTS_EXPRESSION_EDITOR_OPEN", context @Custom"expression-editor", desc "Close"}
        )"#;
        let cfg: SectionConfig = facet_styx::from_str(styx).expect("section parses");
        let bindings = cfg.bindings();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].keys, "e");
        assert_eq!(
            bindings[0].context,
            Some(KeybindContext::Custom("expression-editor".into()))
        );
    }
}
