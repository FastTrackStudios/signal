//! REAPER-specific InputProcessor wrapper.
//!
//! Bridges the gap between REAPER's raw VK-code keyboard API and the
//! unified `input::InputProcessor` state machine.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{OnceLock, RwLock};

use enumflags2::BitFlags;
use input::command::InputCommand;
use input::config::KeymapConfig;
use input::context::ActionContext;
use input::key::KeyChord;
use input::processor::InputProcessor;
use input::trie::KeyTrie;
use reaper_medium::{AcceleratorBehavior, AcceleratorKeyCode};
use tracing::{debug, info};

use super::keybinds::bridge;
use super::keybinds::defaults::{ALL_OVERRIDES, ALL_PRESETS};
use super::keybinds::{KeybindContext, KeybindOverride, KeybindPreset};

// ---------------------------------------------------------------------------
// ReaperInputProcessor
// ---------------------------------------------------------------------------

/// Wraps `InputProcessor` for REAPER-specific usage.
///
/// Handles VK code → InputEvent conversion, context management,
/// override layers, and preset switching.
pub struct ReaperInputProcessor {
    processor: InputProcessor,
    context: ActionContext,

    /// The base KeymapConfig (from preset + which-key trees).
    base_config: KeymapConfig,

    /// Active override configs, keyed by name.
    active_overrides: Vec<(String, KeymapConfig)>,

    /// All available presets.
    available_presets: HashMap<String, (KeybindPreset, Vec<super::keybinds::WhichKeyTree>)>,

    /// All available override layers.
    available_overrides: HashMap<String, KeybindOverride>,

    /// Name of the currently active preset.
    active_preset_name: String,

    /// Which-key trees for the current preset (kept for introspection).
    current_trees: Vec<super::keybinds::WhichKeyTree>,

    /// Mirror of the last `set_reaper_context` value, for introspection
    /// (which-key continuation filtering by focused window).
    reaper_context: KeybindContext,

    /// Canonical key-sequence (chord display joined by spaces, e.g.
    /// "S-t 2 4") → binding `description`. Plain `bindings(...)` descs don't
    /// survive into the keytrie, so the which-key overlay resolves labels
    /// for sequence bindings through this map.
    binding_descs: HashMap<String, String>,
}

impl ReaperInputProcessor {
    /// Create from a preset and its which-key trees.
    pub fn new(preset: &KeybindPreset, trees: &[super::keybinds::WhichKeyTree]) -> Self {
        let config = bridge::preset_to_keymap_config(preset, trees);
        let processor = InputProcessor::from_config(config.clone()).unwrap_or_else(|e| {
            tracing::error!("Failed to build InputProcessor from config: {}", e);
            InputProcessor::new()
        });

        let mut this = Self {
            processor,
            context: ActionContext::new(),
            base_config: config,
            active_overrides: Vec::new(),
            available_presets: HashMap::new(),
            available_overrides: HashMap::new(),
            active_preset_name: preset.name.clone(),
            current_trees: trees.to_vec(),
            reaper_context: KeybindContext::Global,
            binding_descs: HashMap::new(),
        };
        this.collect_binding_descs(&preset.bindings);
        this
    }

    /// Load all default presets and overrides.
    pub fn load_defaults(&mut self) {
        // Load presets
        for preset_fn in ALL_PRESETS.iter() {
            let preset = preset_fn();
            let name = preset.name.clone();
            let trees = preset.which_key_trees.clone();

            self.available_presets.insert(name, (preset, trees));
        }

        // Load overrides
        for override_fn in ALL_OVERRIDES.iter() {
            let overlay = override_fn();
            self.available_overrides
                .insert(overlay.name.clone(), overlay);
        }

        info!(
            preset_count = self.available_presets.len(),
            override_count = self.available_overrides.len(),
            "ReaperInputProcessor defaults loaded"
        );
    }

    /// Process a REAPER key event, returning commands to execute.
    pub fn process_key(
        &mut self,
        key: AcceleratorKeyCode,
        behavior: &BitFlags<AcceleratorBehavior>,
        raw_flags: u8,
    ) -> Vec<InputCommand> {
        let Some(event) = bridge::vk_to_input_event_with_flags(key, behavior, raw_flags) else {
            return Vec::new(); // Pure modifier key, skip
        };

        self.processor.process(event, &self.context)
    }

    /// Process an InputEvent directly (for testing — bypasses VK code conversion).
    pub fn process_event(&mut self, event: input::InputEvent) -> Vec<InputCommand> {
        self.processor.process(event, &self.context)
    }

    /// Forward a key-release event to the underlying processor so it can drop
    /// the matching chord from `held_keys` and tear down sticky which-key
    /// state when a prefix anchor is released.
    ///
    /// Returns `true` when the host should hide the which-key overlay
    /// (sticky-prefix sequence ended after firing one or more actions).
    pub fn notify_key_release(
        &mut self,
        key: AcceleratorKeyCode,
        behavior: &BitFlags<AcceleratorBehavior>,
        raw_flags: u8,
    ) -> bool {
        let Some(input::InputEvent::Key(event)) =
            bridge::vk_to_input_event_with_flags(key, behavior, raw_flags)
        else {
            return false;
        };
        let chord = input::KeyChord::new(event.key, event.modifiers);
        self.processor.notify_key_release(chord)
    }

    /// Clear any pending key sequence and reset the processor to idle state.
    pub fn clear_pending(&mut self) {
        // Process an Escape key to cancel any pending sequence
        let esc = input::InputEvent::Key(input::KeyEvent {
            key: input::KeyCode::Escape,
            modifiers: input::Modifiers::default(),
        });
        let _ = self.processor.process(esc, &self.context);
    }

    /// The last context handed to [`set_reaper_context`] — i.e. where the
    /// most recent key event was headed.
    pub fn reaper_context(&self) -> KeybindContext {
        self.reaper_context.clone()
    }

    /// Update the REAPER context (e.g., when focus changes to MIDI editor).
    pub fn set_reaper_context(&mut self, ctx: KeybindContext) {
        self.reaper_context = ctx.clone();
        // Remove all context tags first
        self.context.remove_tag("context:main");
        self.context.remove_tag("context:midi");
        self.context.remove_tag("context:midi_inline");
        self.context.remove_tag("context:media_explorer");
        self.context.remove_tag("context:global");

        // Set the active context tag
        match ctx {
            KeybindContext::Global => self.context.set_tag("context:global"),
            KeybindContext::Main => self.context.set_tag("context:main"),
            KeybindContext::Midi => self.context.set_tag("context:midi"),
            KeybindContext::MidiInline => self.context.set_tag("context:midi_inline"),
            KeybindContext::MediaExplorer => self.context.set_tag("context:media_explorer"),
            KeybindContext::Custom(name) => {
                self.context.set_tag(format!("context:{name}"));
            }
        }
    }

    /// Enable an override layer by name.
    pub fn enable_override(&mut self, name: &str) {
        if self.active_overrides.iter().any(|(n, _)| n == name) {
            return; // Already active
        }

        let Some(overlay) = self.available_overrides.get(name) else {
            tracing::warn!(name = %name, "Attempted to enable unknown override layer");
            return;
        };

        let config = bridge::override_to_keymap_config(overlay);
        self.active_overrides.push((name.to_string(), config));

        // Sort by priority (highest last so they override)
        self.active_overrides.sort_by(|a, b| {
            let a_pri = self
                .available_overrides
                .get(&a.0)
                .map(|o| o.priority)
                .unwrap_or(0);
            let b_pri = self
                .available_overrides
                .get(&b.0)
                .map(|o| o.priority)
                .unwrap_or(0);
            a_pri.cmp(&b_pri)
        });

        self.rebuild_processor();
        info!(name = %name, "Override layer enabled");
    }

    /// Disable an override layer by name.
    pub fn disable_override(&mut self, name: &str) {
        let before = self.active_overrides.len();
        self.active_overrides.retain(|(n, _)| n != name);

        if self.active_overrides.len() != before {
            self.rebuild_processor();
            info!(name = %name, "Override layer disabled");
        }
    }

    /// Toggle an override layer, returns new active state.
    pub fn toggle_override(&mut self, name: &str) -> bool {
        if self.is_override_active(name) {
            self.disable_override(name);
            false
        } else {
            self.enable_override(name);
            true
        }
    }

    /// Check if an override is active.
    pub fn is_override_active(&self, name: &str) -> bool {
        self.active_overrides.iter().any(|(n, _)| n == name)
    }

    /// Disable all active overrides.
    pub fn disable_all_overrides(&mut self) {
        if !self.active_overrides.is_empty() {
            let count = self.active_overrides.len();
            self.active_overrides.clear();
            self.rebuild_processor();
            info!(count = count, "Disabled all overrides");
        }
    }

    /// Set the active preset by name.
    pub fn set_preset(&mut self, name: &str) -> bool {
        let Some((preset, trees)) = self.available_presets.get(name).cloned() else {
            tracing::warn!(name = %name, "Attempted to set unknown preset");
            return false;
        };

        self.base_config = bridge::preset_to_keymap_config(&preset, &trees);
        self.active_preset_name = name.to_string();
        self.current_trees = trees;
        self.rebuild_processor();
        info!(name = %name, "Preset changed");
        true
    }

    /// Get the display string for pending keys (for which-key overlay).
    pub fn pending_display(&self) -> Option<String> {
        self.processor.pending_display()
    }

    /// Check if the processor is waiting for more keys.
    pub fn needs_timeout(&self) -> bool {
        self.processor.needs_timeout()
    }

    /// Returns `true` while the anchor key of the pending sequence is still
    /// physically held. Used to skip timeout-based sequence clears so a
    /// long-held prefix anchor doesn't flicker the overlay.
    pub fn is_anchor_held(&self) -> bool {
        self.processor.is_anchor_held()
    }

    /// Handle timeout expiration for pending key sequences.
    pub fn timeout_expired(&mut self) -> Vec<InputCommand> {
        self.processor.timeout_expired()
    }

    /// Access the underlying processor for introspection (visualizer).
    pub fn processor(&self) -> &InputProcessor {
        &self.processor
    }

    /// Get the currently active preset name.
    pub fn active_preset_name(&self) -> &str {
        &self.active_preset_name
    }

    /// Get all available presets as (id, display_name) pairs.
    pub fn available_presets(&self) -> Vec<(String, String)> {
        self.available_presets
            .values()
            .map(|(preset, _)| (preset.name.clone(), preset.display_name.clone()))
            .collect()
    }

    /// Get names of all available override layers.
    pub fn available_overrides(&self) -> Vec<String> {
        self.available_overrides.keys().cloned().collect()
    }

    /// Get the names of currently active override layers.
    pub fn active_override_names(&self) -> Vec<&str> {
        self.active_overrides
            .iter()
            .map(|(n, _)| n.as_str())
            .collect()
    }

    /// Get the keytrie for the normal mode (for which-key introspection).
    pub fn normal_keytrie(&self) -> Option<&KeyTrie> {
        self.processor.keymaps().get(&input::mode::ModeId::normal())
    }

    /// Get continuations at a given prefix path (for which-key overlay).
    pub fn continuations_at(&self, prefix: &[KeyChord]) -> Vec<(String, String, bool)> {
        if let Some(trie) = self.normal_keytrie() {
            bridge::trie_continuations_at(trie, prefix)
        } else {
            Vec::new()
        }
    }

    /// Get all root-level prefix keys with labels (for which-key cheat sheet).
    pub fn all_root_prefixes(&self) -> Vec<(String, String, bool)> {
        self.continuations_at(&[])
    }

    /// Get the which-key trees for the current preset.
    pub fn current_trees(&self) -> &[super::keybinds::WhichKeyTree] {
        &self.current_trees
    }

    /// Resolve a mouse wheel binding using active overrides first, then the base preset.
    pub fn resolve_wheel(
        &self,
        context: KeybindContext,
        modifiers: &str,
        horizontal: bool,
    ) -> Option<String> {
        for (name, _) in self.active_overrides.iter().rev() {
            let Some(overlay) = self.available_overrides.get(name) else {
                continue;
            };
            if let Some(action) = resolve_wheel_in_bindings(
                &overlay.wheel_bindings,
                context.clone(),
                modifiers,
                horizontal,
            ) {
                return Some(action);
            }
        }

        let (preset, _) = self.available_presets.get(&self.active_preset_name)?;
        resolve_wheel_in_bindings(&preset.wheel_bindings, context, modifiers, horizontal)
    }

    /// Insert or replace a preset loaded from a config file.
    pub fn add_preset(&mut self, preset: KeybindPreset) {
        let name = preset.name.clone();
        let trees = preset.which_key_trees.clone();
        let is_active = self.active_preset_name == name;
        self.available_presets.insert(name, (preset, trees));

        if is_active {
            let active = self.active_preset_name.clone();
            let _ = self.set_preset(&active);
        }
    }

    /// Insert or replace an override layer loaded from a config file.
    pub fn add_override(&mut self, overlay: KeybindOverride) {
        self.available_overrides
            .insert(overlay.name.clone(), overlay);
    }

    /// Rebuild the InputProcessor from base config + active overrides.
    fn rebuild_processor(&mut self) {
        let mut merged = self.base_config.clone();

        for (_, override_config) in &self.active_overrides {
            merged = KeymapConfig::merge(merged, override_config.clone());
        }

        match InputProcessor::from_config(merged) {
            Ok(processor) => {
                self.processor = processor;
                debug!("InputProcessor rebuilt");
            }
            Err(e) => {
                tracing::error!("Failed to rebuild InputProcessor: {}", e);
            }
        }

        self.rebuild_binding_descs();
    }

    /// Rebuild the sequence → description map from the active preset and the
    /// active override layers (later overrides win, matching merge order).
    fn rebuild_binding_descs(&mut self) {
        self.binding_descs.clear();
        if let Some((preset, _)) = self
            .available_presets
            .get(&self.active_preset_name)
            .cloned()
        {
            self.collect_binding_descs(&preset.bindings);
        }
        let active_names: Vec<String> = self
            .active_overrides
            .iter()
            .map(|(n, _)| n.clone())
            .collect();
        for name in active_names {
            if let Some(overlay) = self.available_overrides.get(&name).cloned() {
                self.collect_binding_descs(&overlay.bindings);
            }
        }
    }

    fn collect_binding_descs(&mut self, bindings: &[super::keybinds::Keybind]) {
        for binding in bindings {
            if let Some(desc) = &binding.description
                && !desc.is_empty()
            {
                let canonical = bridge::canonical_sequence(&binding.keys);
                if !canonical.is_empty() {
                    self.binding_descs.insert(canonical, desc.clone());
                }
            }
        }
    }

    /// Description for a canonical key sequence (see `bridge::canonical_sequence`).
    pub fn binding_desc(&self, canonical_seq: &str) -> Option<&str> {
        self.binding_descs.get(canonical_seq).map(String::as_str)
    }

    /// Key tries that can fire in the current context, highest priority
    /// first: matching context layers (e.g. `@Midi` bindings while the MIDI
    /// editor is focused), then the global keymap. The `bool` flags whether
    /// the trie came from a context layer.
    pub fn active_tries(&self) -> Vec<(&KeyTrie, bool)> {
        let normal = input::mode::ModeId::normal();
        let mut out = Vec::new();
        if let Some(layers) = self.processor.context_keymaps().get(&normal) {
            for (when, trie) in layers {
                if when.evaluate(&self.context) {
                    out.push((trie, true));
                }
            }
        }
        if let Some(trie) = self.processor.keymaps().get(&normal) {
            out.push((trie, false));
        }
        out
    }
}

fn resolve_wheel_in_bindings(
    bindings: &[super::keybinds::WheelBind],
    context: KeybindContext,
    modifiers: &str,
    horizontal: bool,
) -> Option<String> {
    let requested_mods = normalize_wheel_modifiers(modifiers);

    let mods_dir_match = |binding: &super::keybinds::WheelBind| {
        binding.horizontal == horizontal
            && normalize_wheel_modifiers(&binding.modifiers) == requested_mods
    };

    // Prefer a binding declared for this exact context over a context-agnostic
    // (Global) one, independent of definition order — otherwise a Global
    // arrange binding can shadow the MIDI-editor binding for the same
    // modifier+direction (they carry different section action IDs).
    bindings
        .iter()
        .rev()
        .find(|b| mods_dir_match(b) && b.context == Some(context.clone()))
        .or_else(|| {
            bindings.iter().rev().find(|b| {
                mods_dir_match(b)
                    && b.context
                        .clone()
                        .unwrap_or(KeybindContext::Global)
                        .matches(&context)
            })
        })
        .map(|binding| binding.action.clone())
}

fn normalize_wheel_modifiers(modifiers: &str) -> Vec<&str> {
    let trimmed = modifiers.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let inner = trimmed
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(trimmed)
        .trim_end_matches('-');

    let mut parts: Vec<&str> = inner.split('-').filter(|part| !part.is_empty()).collect();
    parts.sort_unstable();
    parts
}

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

/// Get the global ReaperInputProcessor instance.
pub fn get_processor() -> &'static RwLock<ReaperInputProcessor> {
    static PROCESSOR: OnceLock<RwLock<ReaperInputProcessor>> = OnceLock::new();
    PROCESSOR.get_or_init(|| {
        let mut proc = init_processor();
        proc.load_defaults();
        // FTS preset is config-driven; set_preset("fasttrackstudio") is called
        // by apply_config() after load_from_dir() loads the styx files.

        info!("ReaperInputProcessor lazily initialized with defaults");
        RwLock::new(proc)
    })
}

/// Create the initial processor with an empty placeholder.
///
/// The real FTS preset is config-driven and will be loaded from
/// `config/fasttrackstudio/` via `load_from_dir`, then activated by
/// `set_preset("fasttrackstudio")` when `apply_config` runs.
fn init_processor() -> ReaperInputProcessor {
    use super::keybinds::KeybindPreset;

    let placeholder =
        KeybindPreset::new("fasttrackstudio", "FastTrackStudio (config not yet loaded)");
    ReaperInputProcessor::new(&placeholder, &[])
}

/// Initialize the processor (explicit trigger for lazy init).
pub fn init() {
    let _ = get_processor();
    info!("ReaperInputProcessor initialized");
}

/// Resolve a key event through the global processor, returning commands.
pub fn process_key(
    key: AcceleratorKeyCode,
    behavior: &BitFlags<AcceleratorBehavior>,
    raw_flags: u8,
) -> Vec<InputCommand> {
    let mut proc = get_processor().write().unwrap();
    proc.process_key(key, behavior, raw_flags)
}

/// Forward a key-release event to the global processor. Returns `true`
/// when the caller should hide the which-key overlay because a sticky
/// sequence that fired one or more actions just ended.
pub fn notify_key_release(
    key: AcceleratorKeyCode,
    behavior: &BitFlags<AcceleratorBehavior>,
    raw_flags: u8,
) -> bool {
    let mut proc = get_processor().write().unwrap();
    proc.notify_key_release(key, behavior, raw_flags)
}

/// Update the global processor's REAPER context.
pub fn set_context(ctx: KeybindContext) {
    let mut proc = get_processor().write().unwrap();
    proc.set_reaper_context(ctx);
}

/// Enable an override layer globally.
pub fn enable_override(name: &str) {
    let mut proc = get_processor().write().unwrap();
    proc.enable_override(name);
}

/// Disable an override layer globally.
pub fn disable_override(name: &str) {
    let mut proc = get_processor().write().unwrap();
    proc.disable_override(name);
}

/// Toggle an override layer globally, returns new state.
pub fn toggle_override(name: &str) -> bool {
    let mut proc = get_processor().write().unwrap();
    proc.toggle_override(name)
}

/// Check if an override is active.
pub fn is_override_active(name: &str) -> bool {
    let proc = get_processor().read().unwrap();
    proc.is_override_active(name)
}

/// Disable all overrides.
pub fn disable_all_overrides() {
    let mut proc = get_processor().write().unwrap();
    proc.disable_all_overrides();
}

/// Load presets and overlays from a keybinds config directory.
///
/// Scans `keybinds_dir` for subdirectories containing `profile.styx` (presets)
/// and for `overlays/*.styx` files (overlay layers). Loaded items are merged
/// into the global processor alongside the hardcoded defaults.
///
/// Safe to call more than once — re-loading a name replaces the previous entry.
pub fn load_from_dir(keybinds_dir: &std::path::Path) {
    let mut proc = get_processor().write().unwrap();
    load_from_dir_into(&mut proc, keybinds_dir);
}

/// Rebuild the processor's available presets and overrides from disk.
///
/// This clears previously loaded config-file presets/overrides first so file
/// deletions and renames are reflected in the live processor state.
pub fn reload_from_dir(keybinds_dir: &std::path::Path) {
    let mut proc = get_processor().write().unwrap();
    let active_preset = proc.active_preset_name.clone();

    let mut fresh = init_processor();
    fresh.load_defaults();
    load_from_dir_into(&mut fresh, keybinds_dir);

    if !fresh.set_preset(&active_preset) {
        tracing::warn!(
            preset = %active_preset,
            "Active preset missing after reload; falling back to fasttrackstudio if available"
        );
        if !fresh.set_preset("fasttrackstudio")
            && let Some((fallback, _)) = fresh.available_presets.values().next().cloned()
        {
            let fallback_name = fallback.name.clone();
            let _ = fresh.set_preset(&fallback_name);
        }
    }

    *proc = fresh;
}

fn load_from_dir_into(proc: &mut ReaperInputProcessor, keybinds_dir: &Path) {
    use crate::keybind_config::{load_overlay, load_profile_preset};

    if !keybinds_dir.exists() {
        info!(path = %keybinds_dir.display(), "Keybinds directory does not exist — skipping config load");
        return;
    }

    // Load profile subdirectories
    let read_dir = match std::fs::read_dir(keybinds_dir) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(path = %keybinds_dir.display(), "Cannot read keybinds dir: {e}");
            return;
        }
    };

    for entry in read_dir.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let dir_name = entry.file_name();
        if dir_name == "overlays" {
            continue;
        }
        if let Some(preset) = load_profile_preset(&path) {
            info!(name = %preset.name, "Loaded config-file preset '{}'", preset.name);
            proc.add_preset(preset);
        }
    }

    // Load overlay files from the `overlays/` subdirectory
    let overlays_dir = keybinds_dir.join("overlays");
    if overlays_dir.is_dir() {
        let overlay_entries = match std::fs::read_dir(&overlays_dir) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(path = %overlays_dir.display(), "Cannot read overlays dir: {e}");
                return;
            }
        };
        for entry in overlay_entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("styx") {
                continue;
            }
            if let Some(overlay) = load_overlay(&path) {
                info!(name = %overlay.name, "Loaded config-file overlay '{}'", overlay.name);
                proc.add_override(overlay);
            }
        }
    }

    info!(
        preset_count = proc.available_presets.len(),
        override_count = proc.available_overrides.len(),
        "Config-file presets loaded from {:?}",
        keybinds_dir
    );
}

/// Set the active preset.
pub fn set_preset(name: &str) -> bool {
    let mut proc = get_processor().write().unwrap();
    proc.set_preset(name)
}

/// Get the active preset name.
pub fn active_preset_name() -> String {
    let proc = get_processor().read().unwrap();
    proc.active_preset_name().to_string()
}

/// Get all available presets as (id, display_name) pairs.
pub fn available_presets() -> Vec<(String, String)> {
    let proc = get_processor().read().unwrap();
    proc.available_presets()
}

/// Get all available override names.
pub fn available_overrides() -> Vec<String> {
    let proc = get_processor().read().unwrap();
    proc.available_overrides()
}

/// Resolve a mouse wheel binding through the active preset and overlays.
/// Resolve a single-chord binding declared for *exactly* `ctx` in the
/// active preset — no Global-layer fallback.
///
/// The routing primitive for app-defined contexts (the expression
/// editor): a binding fires only when its config named this context,
/// and everything else is the caller's to pass through to the focused
/// window. Later bindings win, matching the profile's compose order.
pub fn resolve_exact_context(ctx: &KeybindContext, key_str: &str) -> Option<String> {
    let proc = get_processor().read().unwrap();
    let (preset, _) = proc.available_presets.get(&proc.active_preset_name)?;
    preset
        .bindings
        .iter()
        .filter(|b| b.context.as_ref() == Some(ctx) && b.keys == key_str)
        .next_back()
        .map(|b| b.action.clone())
}

pub fn resolve_wheel(context: KeybindContext, modifiers: &str, horizontal: bool) -> Option<String> {
    let proc = get_processor().read().unwrap();
    proc.resolve_wheel(context, modifiers, horizontal)
}

/// Check if the processor has a pending sequence.
pub fn needs_timeout() -> bool {
    let proc = get_processor().read().unwrap();
    proc.needs_timeout()
}

/// Check if the prefix anchor of the pending sequence is currently held.
pub fn is_anchor_held() -> bool {
    let proc = get_processor().read().unwrap();
    proc.is_anchor_held()
}

/// Get the pending display string.
pub fn pending_display() -> Option<String> {
    let proc = get_processor().read().unwrap();
    proc.pending_display()
}

/// Clear any pending key sequence.
pub fn clear_pending() {
    let mut proc = get_processor().write().unwrap();
    proc.clear_pending();
}

/// Handle timeout expiration.
pub fn timeout_expired() -> Vec<InputCommand> {
    let mut proc = get_processor().write().unwrap();
    proc.timeout_expired()
}

#[cfg(test)]
mod wheel_resolution_tests {
    use super::resolve_wheel_in_bindings;
    use crate::input::keybinds::{KeybindContext, WheelBind, WheelDirection};

    fn wb(mods: &str, action: &str, ctx: Option<KeybindContext>) -> WheelBind {
        WheelBind {
            modifiers: mods.into(),
            action: action.into(),
            direction: WheelDirection::Both,
            horizontal: false,
            description: None,
            context: ctx,
        }
    }

    /// Regression: the same modifier+direction is bound to different actions
    /// per context (Option+wheel = vertical zoom in both arrange and the MIDI
    /// editor, but via different section action IDs). Each context must
    /// resolve to its own action — a Global (arrange) binding must not shadow
    /// the MIDI-editor one.
    #[test]
    fn wheel_resolves_per_context() {
        let bindings = vec![
            wb("<A->", "1001", None),                        // arrange (Global)
            wb("<A->", "40663", Some(KeybindContext::Midi)), // MIDI editor
        ];
        assert_eq!(
            resolve_wheel_in_bindings(&bindings, KeybindContext::Midi, "<A->", false),
            Some("40663".to_string()),
            "MIDI context must use the MIDI-editor action, not the Global one"
        );
        assert_eq!(
            resolve_wheel_in_bindings(&bindings, KeybindContext::Main, "<A->", false),
            Some("1001".to_string()),
            "Arrange context uses the Global action"
        );
    }

    /// The exact-context binding wins regardless of definition order.
    #[test]
    fn wheel_context_specific_wins_regardless_of_order() {
        let bindings = vec![
            wb("<A->", "40663", Some(KeybindContext::Midi)),
            wb("<A->", "1001", None),
        ];
        assert_eq!(
            resolve_wheel_in_bindings(&bindings, KeybindContext::Midi, "<A->", false),
            Some("40663".to_string())
        );
    }
}
