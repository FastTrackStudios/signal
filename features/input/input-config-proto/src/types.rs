//! Styx-parseable types for keybind config files.

use facet::Facet;

use crate::context::{KeybindContext, MouseModifierContext};

// ── Profile ──────────────────────────────────────────────────────────────────

/// Parsed from `<profile-dir>/profile.styx`.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ProfileConfig {
    /// Human-readable display name shown in the UI (e.g., "FastTrackStudio", "Logic Pro").
    /// The internal ID is derived from the directory name, not this field.
    pub name: String,

    /// Long description for the profile.
    pub description: String,

    /// Semver version string.
    pub version: String,

    /// Section filenames to load from the same directory, in order.
    /// Later files win on key conflicts within the same priority.
    pub sections: Vec<String>,
}

// ── Section ───────────────────────────────────────────────────────────────────

/// Parsed from each section file (e.g. `transport.styx`).
#[derive(Facet, Debug, Clone, Default, PartialEq)]
pub struct SectionConfig {
    /// Keyboard bindings.
    #[facet(skip_serializing_if = Option::is_none)]
    pub bindings: Option<Vec<KeybindDef>>,

    /// Which-key prefix trees.
    #[facet(skip_serializing_if = Option::is_none)]
    pub which_key: Option<Vec<WhichKeyTreeDef>>,

    /// Mouse wheel bindings.
    #[facet(skip_serializing_if = Option::is_none)]
    pub wheel: Option<Vec<WheelBindDef>>,

    /// Mouse click/drag modifier bindings.
    #[facet(skip_serializing_if = Option::is_none)]
    pub mouse: Option<Vec<MouseBindDef>>,
}

impl SectionConfig {
    pub fn bindings(&self) -> &[KeybindDef] {
        self.bindings.as_deref().unwrap_or(&[])
    }

    pub fn wheel(&self) -> &[WheelBindDef] {
        self.wheel.as_deref().unwrap_or(&[])
    }

    pub fn which_key(&self) -> &[WhichKeyTreeDef] {
        self.which_key.as_deref().unwrap_or(&[])
    }

    pub fn mouse(&self) -> &[MouseBindDef] {
        self.mouse.as_deref().unwrap_or(&[])
    }
}

// ── Per-binding types ─────────────────────────────────────────────────────────

/// A single keyboard binding.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct KeybindDef {
    /// Key sequence, e.g. `h`, `<C-s>`, `"<space>"`.
    pub keys: String,

    /// REAPER action ID (numeric string) or named action ID.
    pub action: String,

    /// Human-readable description for which-key hints.
    #[facet(skip_serializing_if = Option::is_none)]
    pub desc: Option<String>,

    /// Context where active. `None` = Global.
    #[facet(skip_serializing_if = Option::is_none)]
    pub context: Option<KeybindContext>,

    /// When a special editor (e.g. the MIDI editor) is focused, global
    /// bindings are by default passed through to that editor rather than run
    /// in the main section. Set `passthrough true` to override that and let
    /// this binding run in the main section even from an editor.
    #[facet(skip_serializing_if = Option::is_none)]
    pub passthrough: Option<bool>,

    /// Short memory hook for the mapping, e.g. `"S for Save"`,
    /// `"R like Record"`. Rendered inline in references/tutorials.
    #[facet(skip_serializing_if = Option::is_none)]
    pub mnemonic: Option<String>,

    /// The reasoning behind this mapping — why THIS key: design intent,
    /// what it mirrors from other DAWs, ergonomics. Longer-form than
    /// `mnemonic`; shown as expandable detail in references/tutorials.
    #[facet(skip_serializing_if = Option::is_none)]
    pub why: Option<String>,
}

/// A which-key prefix tree loaded from a section file.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct WhichKeyTreeDef {
    /// Prefix key that opens the tree, e.g. `z`.
    pub prefix: String,

    /// Human-readable label shown in the overlay.
    pub label: String,

    /// Children under the prefix.
    pub entries: Vec<WhichKeyEntryDef>,

    /// When `true`, children match regardless of Shift state. Useful for
    /// trees whose prefix is itself a shifted chord (e.g. `<S-m>`) and the
    /// user expects to keep Shift held while picking leaves. Defaults to
    /// `false`. Optional in styx — omit to inherit the default.
    #[facet(skip_serializing_if = Option::is_none)]
    pub case_insensitive: Option<bool>,
}

/// A which-key entry. If `action` is present, the entry is a leaf. If
/// `children` is present, the entry is a branch.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct WhichKeyEntryDef {
    /// Key under the current prefix/branch.
    pub key: String,

    /// Human-readable label shown in the overlay.
    pub label: String,

    /// Action command ID for leaf entries.
    #[facet(skip_serializing_if = Option::is_none)]
    pub action: Option<String>,

    /// Nested entries for branch entries.
    #[facet(skip_serializing_if = Option::is_none)]
    pub children: Option<Vec<WhichKeyEntryDef>>,
}

/// A single mouse wheel binding.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct WheelBindDef {
    /// Modifier string, e.g. `""`, `<S->`, `<C->`, `<A->`.
    pub modifiers: String,

    /// REAPER action ID.
    pub action: String,

    /// `true` = horizontal wheel event. Absent = vertical.
    #[facet(skip_serializing_if = Option::is_none)]
    pub horizontal: Option<bool>,

    /// Human-readable description.
    #[facet(skip_serializing_if = Option::is_none)]
    pub desc: Option<String>,

    /// Context where active. `None` = Global.
    #[facet(skip_serializing_if = Option::is_none)]
    pub context: Option<KeybindContext>,

    /// Short memory hook for the mapping.
    #[facet(skip_serializing_if = Option::is_none)]
    pub mnemonic: Option<String>,

    /// The reasoning behind this mapping.
    #[facet(skip_serializing_if = Option::is_none)]
    pub why: Option<String>,
}

/// A single mouse click/drag modifier binding.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct MouseBindDef {
    /// Mouse context, e.g. `@MediaItemLeftEdge`, `@MediaItemFade`.
    pub ctx: MouseModifierContext,

    /// Modifier string, e.g. `""`, `<S->`, `<A->`, `<C->`.
    pub modifiers: String,

    /// Action name, e.g. `edge_resize`, `slip_edit`, `copy_item`, `fade_adjust`.
    pub action: String,

    /// Human-readable description.
    #[facet(skip_serializing_if = Option::is_none)]
    pub desc: Option<String>,

    /// Short memory hook for the mapping.
    #[facet(skip_serializing_if = Option::is_none)]
    pub mnemonic: Option<String>,

    /// The reasoning behind this mapping.
    #[facet(skip_serializing_if = Option::is_none)]
    pub why: Option<String>,
}

/// A single REAPER mouse modifier setting (for the MouseModifierManager).
/// Uses raw REAPER context strings and behavior IDs.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct MouseModifierSettingDef {
    /// REAPER context string (e.g., `MM_CTX_ITEMEDGE`, `MM_CTX_ITEM`).
    pub ctx: String,

    /// Modifier flags: `""`, `<S->`, `<C->`, `<A->`, `<S-C->`, `<S-A->`, `<C-A->`, `<S-C-A->`.
    pub mods: String,

    /// REAPER behavior ID string (e.g., `"1 m"`, `"3 m"`).
    pub behavior: String,

    /// Human-readable description.
    #[facet(skip_serializing_if = Option::is_none)]
    pub desc: Option<String>,
}

/// Parsed from a `mouse-profile.styx` file inside a profile directory.
///
/// Defines REAPER mouse modifier settings for the `MouseModifierManager`.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct MouseProfileConfig {
    /// Human-readable display name (e.g., `"FastTrackStudio"`, `"Logic Pro"`).
    /// The internal ID is derived from the directory name, not this field.
    pub name: String,

    /// Human-readable description.
    pub description: String,

    /// Mouse modifier settings.
    #[facet(skip_serializing_if = Option::is_none)]
    pub settings: Option<Vec<MouseModifierSettingDef>>,
}

impl MouseProfileConfig {
    pub fn settings(&self) -> &[MouseModifierSettingDef] {
        self.settings.as_deref().unwrap_or(&[])
    }
}

// ── Overlay ───────────────────────────────────────────────────────────────────

/// Parsed from an overlay file (e.g. `overlays/tempo-map.styx`).
///
/// Overlays stack on top of the active base preset.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct OverlayConfig {
    /// Internal name — used as the overlay key in the processor.
    pub name: String,

    /// Human-readable description.
    pub description: String,

    /// Stacking priority (higher = overrides lower priority overlays).
    pub priority: i32,

    /// Keyboard bindings.
    #[facet(skip_serializing_if = Option::is_none)]
    pub bindings: Option<Vec<KeybindDef>>,

    /// Mouse-wheel bindings.
    #[facet(skip_serializing_if = Option::is_none)]
    pub wheel: Option<Vec<WheelBindDef>>,

    /// Mouse click/drag modifier bindings.
    #[facet(skip_serializing_if = Option::is_none)]
    pub mouse: Option<Vec<MouseBindDef>>,

    /// REAPER mouse modifier settings to apply when this overlay is active.
    /// These configure REAPER's built-in mouse modifier preferences.
    #[facet(skip_serializing_if = Option::is_none)]
    pub mouse_settings: Option<Vec<MouseModifierSettingDef>>,
}

impl OverlayConfig {
    pub fn bindings(&self) -> &[KeybindDef] {
        self.bindings.as_deref().unwrap_or(&[])
    }

    pub fn wheel(&self) -> &[WheelBindDef] {
        self.wheel.as_deref().unwrap_or(&[])
    }

    pub fn mouse(&self) -> &[MouseBindDef] {
        self.mouse.as_deref().unwrap_or(&[])
    }

    pub fn mouse_settings(&self) -> &[MouseModifierSettingDef] {
        self.mouse_settings.as_deref().unwrap_or(&[])
    }
}

// ── Workflow ──────────────────────────────────────────────────────────────────

/// A REAPER setting toggled when a workflow activates.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ReaperSettingDef {
    /// REAPER command ID (numeric string) or named action ID.
    pub command: String,

    /// Desired toggle state when the workflow is active.
    pub enabled: bool,

    /// Human-readable description.
    #[facet(skip_serializing_if = Option::is_none)]
    pub desc: Option<String>,
}

/// An action armed (via REAPER's `ArmCommand`) while a workflow is active, and
/// disarmed when it deactivates. Fires on the next click — e.g. arm a
/// "split under mouse" action so a click-drag splits then slips.
#[derive(Facet, Debug, Clone, PartialEq)]
pub struct ArmedActionDef {
    /// REAPER command ID (numeric string) or named action ID to arm.
    pub command: String,

    /// Human-readable name (defaults to the command).
    #[facet(skip_serializing_if = Option::is_none)]
    pub name: Option<String>,

    /// Section name; empty/absent = main section.
    #[facet(skip_serializing_if = Option::is_none)]
    pub section: Option<String>,

    /// Intercept left-clicks via FTS instead of REAPER's native arm. Default false.
    #[facet(skip_serializing_if = Option::is_none)]
    pub intercept_clicks: Option<bool>,

    /// After the intercepted click runs the action, FTS drives a slip-edit drag
    /// on the item under the mouse itself (instead of passing the click through
    /// to REAPER). Lets "split then slip the right piece" work as one gesture
    /// without REAPER's edge-detection grabbing the fresh boundary. Requires
    /// `intercept_clicks true`. Default false.
    #[facet(skip_serializing_if = Option::is_none)]
    pub slip_drag: Option<bool>,
}

/// Parsed from a workflow file (e.g. `workflows/tempo-mapping.styx`).
///
/// Identity (id, name, description) is derived from the **filename** by the
/// loader, so none of those fields are required in the file itself:
/// - `id` = filename stem (`tempo-mapping`)
/// - `name` = kebab → Title Case (`Tempo Mapping`)
/// - `description` = optional override; empty string if absent
///
/// The file only needs to describe *what changes* when the workflow is active.
#[derive(Facet, Debug, Clone, Default, PartialEq)]
pub struct WorkflowConfig {
    /// Optional display-name override. Defaults to kebab→Title Case of filename.
    #[facet(skip_serializing_if = Option::is_none)]
    pub name: Option<String>,

    /// Optional description override.
    #[facet(skip_serializing_if = Option::is_none)]
    pub description: Option<String>,

    /// Named keybind overlays to enable when this workflow is active.
    #[facet(skip_serializing_if = Option::is_none)]
    pub keybind_overlays: Option<Vec<String>>,

    /// Named mouse-modifier overlays to enable.
    #[facet(skip_serializing_if = Option::is_none)]
    pub mouse_overlays: Option<Vec<String>>,

    /// REAPER settings to apply (and restore on deactivation).
    #[facet(skip_serializing_if = Option::is_none)]
    pub settings: Option<Vec<ReaperSettingDef>>,

    /// Inline keyboard bindings — auto-creates a keybind overlay named
    /// `workflow-<id>` and activates it alongside any explicit overlays.
    #[facet(skip_serializing_if = Option::is_none)]
    pub bindings: Option<Vec<KeybindDef>>,

    /// Inline wheel bindings (part of the auto-created inline keybind overlay).
    #[facet(skip_serializing_if = Option::is_none)]
    pub wheel: Option<Vec<WheelBindDef>>,

    /// Inline mouse modifier settings — auto-creates a mouse override named
    /// `workflow-<id>` and activates it alongside any explicit mouse overlays.
    #[facet(skip_serializing_if = Option::is_none)]
    pub mouse_settings: Option<Vec<MouseModifierSettingDef>>,

    /// An action to arm while the workflow is active (disarmed on deactivate).
    #[facet(skip_serializing_if = Option::is_none)]
    pub armed_action: Option<ArmedActionDef>,
}

impl WorkflowConfig {
    pub fn keybind_overlays(&self) -> &[String] {
        self.keybind_overlays.as_deref().unwrap_or(&[])
    }

    pub fn mouse_overlays(&self) -> &[String] {
        self.mouse_overlays.as_deref().unwrap_or(&[])
    }

    pub fn settings(&self) -> &[ReaperSettingDef] {
        self.settings.as_deref().unwrap_or(&[])
    }

    pub fn bindings(&self) -> &[KeybindDef] {
        self.bindings.as_deref().unwrap_or(&[])
    }

    pub fn wheel(&self) -> &[WheelBindDef] {
        self.wheel.as_deref().unwrap_or(&[])
    }

    pub fn mouse_settings(&self) -> &[MouseModifierSettingDef] {
        self.mouse_settings.as_deref().unwrap_or(&[])
    }

    pub fn armed_action(&self) -> Option<&ArmedActionDef> {
        self.armed_action.as_ref()
    }

    pub fn has_inline_bindings(&self) -> bool {
        !self.bindings().is_empty() || !self.wheel().is_empty()
    }

    pub fn has_inline_mouse_settings(&self) -> bool {
        !self.mouse_settings().is_empty()
    }

    pub fn inline_overlay_name(id: &str) -> String {
        format!("workflow-{}", id)
    }
}

/// Convert a kebab-case id to Title Case display name.
/// `tempo-mapping` → `Tempo Mapping`
pub fn kebab_to_title(s: &str) -> String {
    s.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
