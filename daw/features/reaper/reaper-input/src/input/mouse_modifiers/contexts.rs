//! Mouse modifier contexts
//!
//! Complete list of all REAPER mouse modifier contexts, derived from
//! `reaper_config::mouse::Context::ALL` as the authoritative source.
//! Uses internal MM_CTX_* names for the string representations.

use super::behaviors::media_item::{MediaItemEdgeLeftDragBehavior, MediaItemLeftDragBehavior};
use super::behaviors::shared::traits::{BehaviorDisplay, BehaviorId};
use once_cell::sync::Lazy;
use reaper_config::mouse::Context;

/// All mouse modifier contexts in REAPER, as MM_CTX_* strings.
///
/// Derived from [`reaper_config::mouse::Context::ALL`] — add new contexts there
/// and they will appear here automatically.
pub static ALL_CONTEXTS: Lazy<Vec<&'static str>> = Lazy::new(|| {
    Context::ALL
        .iter()
        .map(|c| c.to_reaper_string_static())
        .collect()
});

/// Get display name for a behavior ID in a specific context
///
/// Takes a behavior string like "2 m" or "13 m" and returns a human-readable name
/// based on the context (e.g., "Stretch item" for "2 m" in MM_CTX_ITEMEDGE)
pub fn get_behavior_display_name(context: &str, behavior_str: &str) -> String {
    // Parse the behavior ID from strings like "2 m", "13 m", etc.
    let behavior_id: Option<u32> = behavior_str
        .trim()
        .strip_suffix(" m")
        .or_else(|| behavior_str.trim().strip_suffix("m"))
        .or(Some(behavior_str.trim()))
        .and_then(|s| s.trim().parse().ok());

    let Some(id) = behavior_id else {
        return behavior_str.to_string();
    };

    // Map context to behavior enum and get display name
    match context {
        "MM_CTX_ITEM" => {
            let behavior = MediaItemLeftDragBehavior::from_behavior_id(id);
            behavior.display_name().to_string()
        }
        "MM_CTX_ITEMEDGE" => {
            let behavior = MediaItemEdgeLeftDragBehavior::from_behavior_id(id);
            behavior.display_name().to_string()
        }
        // For contexts we don't have behavior enums for, return the raw string
        _ => behavior_str.to_string(),
    }
}

/// Get display name for a context (for logging/UI).
///
/// Delegates to [`reaper_config::mouse::Context::display_name_static`] and falls
/// back to the raw context string for unrecognised names.
pub fn get_display_name(context: &str) -> &str {
    match Context::from_reaper_str(context).display_name_static() {
        Some(name) => name,
        None => context,
    }
}

/// Categorize contexts for organization
pub mod categories {
    use super::ALL_CONTEXTS;

    pub fn arrange_view() -> Vec<&'static str> {
        ALL_CONTEXTS
            .iter()
            .filter(|ctx| ctx.starts_with("MM_CTX_ARRANGE"))
            .copied()
            .collect()
    }

    pub fn automation() -> Vec<&'static str> {
        ALL_CONTEXTS
            .iter()
            .filter(|ctx| ctx.starts_with("MM_CTX_POOLEDENV"))
            .copied()
            .collect()
    }

    pub fn envelope() -> Vec<&'static str> {
        ALL_CONTEXTS
            .iter()
            .filter(|ctx| ctx.starts_with("MM_CTX_ENV"))
            .copied()
            .collect()
    }

    pub fn fixed_lane() -> Vec<&'static str> {
        ALL_CONTEXTS
            .iter()
            .filter(|ctx| {
                ctx.starts_with("MM_CTX_FIXEDLANE") || ctx.starts_with("MM_CTX_LINKEDLANE")
            })
            .copied()
            .collect()
    }

    pub fn midi() -> Vec<&'static str> {
        ALL_CONTEXTS
            .iter()
            .filter(|ctx| ctx.starts_with("MM_CTX_MIDI"))
            .copied()
            .collect()
    }

    pub fn media_item() -> Vec<&'static str> {
        ALL_CONTEXTS
            .iter()
            .filter(|ctx| ctx.starts_with("MM_CTX_ITEM") || ctx.starts_with("MM_CTX_CROSSFADE"))
            .copied()
            .collect()
    }

    pub fn mixer() -> Vec<&'static str> {
        ALL_CONTEXTS
            .iter()
            .filter(|ctx| ctx.starts_with("MM_CTX_MCP"))
            .copied()
            .collect()
    }

    pub fn project() -> Vec<&'static str> {
        ALL_CONTEXTS
            .iter()
            .filter(|ctx| {
                ctx.starts_with("MM_CTX_MARKER")
                    || ctx.starts_with("MM_CTX_REGION")
                    || ctx.starts_with("MM_CTX_TEMPOMARKER")
            })
            .copied()
            .collect()
    }

    pub fn razor_edit() -> Vec<&'static str> {
        ALL_CONTEXTS
            .iter()
            .filter(|ctx| ctx.starts_with("MM_CTX_AREASEL"))
            .copied()
            .collect()
    }

    pub fn ruler() -> Vec<&'static str> {
        ALL_CONTEXTS
            .iter()
            .filter(|ctx| ctx.starts_with("MM_CTX_RULER") && !ctx.starts_with("MM_CTX_MIDI_RULER"))
            .copied()
            .collect()
    }

    pub fn track() -> Vec<&'static str> {
        ALL_CONTEXTS
            .iter()
            .filter(|ctx| ctx.starts_with("MM_CTX_TRACK") || ctx.starts_with("MM_CTX_TCP"))
            .copied()
            .collect()
    }

    pub fn other() -> Vec<&'static str> {
        ALL_CONTEXTS
            .iter()
            .filter(|ctx| {
                !ctx.starts_with("MM_CTX_ARRANGE")
                    && !ctx.starts_with("MM_CTX_POOLEDENV")
                    && !ctx.starts_with("MM_CTX_ENV")
                    && !ctx.starts_with("MM_CTX_FIXEDLANE")
                    && !ctx.starts_with("MM_CTX_LINKEDLANE")
                    && !ctx.starts_with("MM_CTX_MIDI")
                    && !ctx.starts_with("MM_CTX_ITEM")
                    && !ctx.starts_with("MM_CTX_CROSSFADE")
                    && !ctx.starts_with("MM_CTX_MCP")
                    && !ctx.starts_with("MM_CTX_MARKER")
                    && !ctx.starts_with("MM_CTX_REGION")
                    && !ctx.starts_with("MM_CTX_TEMPOMARKER")
                    && !ctx.starts_with("MM_CTX_AREASEL")
                    && !ctx.starts_with("MM_CTX_RULER")
                    && !ctx.starts_with("MM_CTX_TRACK")
                    && !ctx.starts_with("MM_CTX_TCP")
            })
            .copied()
            .collect()
    }
}
