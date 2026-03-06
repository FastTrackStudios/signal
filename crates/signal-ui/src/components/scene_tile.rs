//! Scene tile grid -- a Quad Cortex-style tile grid.
//!
//! Provides:
//! - [`SceneTileGrid`] -- renders a 4x2 grid of tiles from data
//! - [`SceneTileCell`] -- a single colored tile with name and active state
//!
//! This is a domain-agnostic presentation component. The caller provides
//! tile data (name, active, empty) and click handlers.

use dioxus::prelude::*;

/// Default background colors per grid position.
const TILE_COLORS: [&str; 8] = [
    "#b45309", // Amber/warm brown (brighter)
    "#059669", // Emerald/teal (brighter)
    "#0891b2", // Cyan/teal (brighter)
    "#7c3aed", // Violet/purple (brighter)
    "#c2410c", // Orange/rust (brighter)
    "#2563eb", // Blue (brighter)
    "#9333ea", // Purple (brighter)
    "#6b7280", // Gray (brighter)
];

/// Data for a single tile in the grid.
#[derive(Clone, PartialEq)]
pub struct TileData {
    /// Display name for the tile.
    pub name: String,
    /// Optional subtitle shown below the name (e.g. profile/patch source).
    pub subtitle: Option<String>,
    /// Whether this tile is the active/selected one.
    pub active: bool,
    /// Whether this tile is empty (no content assigned).
    pub empty: bool,
    /// Whether this tile's patch has been preloaded and is ready for instant switching.
    /// Non-preloaded tiles are rendered darker to indicate they aren't ready yet.
    pub preloaded: bool,
    /// Whether this tile is in a loading/switching state (optimistic feedback).
    /// Loading tiles pulse to indicate a switch is in progress.
    pub loading: bool,
    /// Whether this tile exists but can't be activated (unresolvable target).
    pub disabled: bool,
}

/// A 4x2 scene tile grid.
///
/// Renders up to 8 tiles in a 4-column, 2-row CSS grid layout.
/// Empty slots beyond the provided data are rendered as empty tiles.
#[component]
pub fn SceneTileGrid(
    /// Tile data. Up to 8 tiles are rendered; extras are ignored.
    tiles: Vec<TileData>,
    /// Callback when a non-empty tile is clicked. Receives the tile index (0-7).
    on_tile_click: EventHandler<usize>,
    /// Number of tile slots to render. Default: 8.
    #[props(default = 8)]
    slot_count: usize,
) -> Element {
    rsx! {
        div { class: "h-full w-full bg-zinc-950 p-2",
            div { class: "grid grid-cols-4 grid-rows-2 gap-2 h-full",
                for i in 0..slot_count {
                    {
                        let tile = tiles.get(i).cloned().unwrap_or(TileData {
                            name: String::new(),
                            subtitle: None,
                            active: false,
                            empty: true,
                            preloaded: false,
                            loading: false,
                            disabled: false,
                        });
                        let color = TILE_COLORS[i % TILE_COLORS.len()].to_string();
                        rsx! {
                            SceneTileCell {
                                key: "{i}",
                                name: tile.name,
                                subtitle: tile.subtitle,
                                active: tile.active,
                                empty: tile.empty,
                                preloaded: tile.preloaded,
                                loading: tile.loading,
                                disabled: tile.disabled,
                                color,
                                on_click: move |_| {
                                    on_tile_click.call(i);
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A single scene tile in the grid.
///
/// Renders as a colored rectangle with the tile name centered.
/// Active tiles get a green ring highlight. Empty tiles are dimmed.
#[component]
pub fn SceneTileCell(
    /// Display name.
    name: String,
    /// Optional subtitle shown below the name.
    #[props(default)]
    subtitle: Option<String>,
    /// Whether this tile is active/selected.
    #[props(default)]
    active: bool,
    /// Whether this tile is empty.
    #[props(default)]
    empty: bool,
    /// Whether this tile's patch is preloaded and ready for instant switching.
    #[props(default = true)]
    preloaded: bool,
    /// Whether this tile is in a loading/switching state (pulse animation).
    #[props(default)]
    loading: bool,
    /// Whether this tile exists but can't be activated (blacked out).
    #[props(default)]
    disabled: bool,
    /// Background color (hex, e.g. `"#92400e"`).
    color: String,
    /// Click handler.
    on_click: EventHandler<()>,
) -> Element {
    // Always set all style properties explicitly so Wry's WebView repaints
    // on changes. Removing a property (e.g. dropping `filter`) doesn't always
    // trigger a repaint — but changing its value does.
    let bg_style = if empty {
        "background-color: #18181b; filter: brightness(1.0); animation: none;".to_string()
    } else if disabled {
        format!("background-color: {color}; filter: brightness(0.08); animation: none;")
    } else if loading {
        format!("background-color: {color}; filter: brightness(1.0); animation: tile-pulse 0.8s ease-in-out infinite;")
    } else if active {
        format!("background-color: {color}; filter: brightness(1.0); animation: none;")
    } else if preloaded {
        format!("background-color: {color}; filter: brightness(0.55); animation: none;")
    } else {
        format!("background-color: {color}; filter: brightness(0.2); animation: none;")
    };

    let active_class = if active {
        "ring-2 ring-green-400 shadow-lg shadow-green-500/30"
    } else {
        ""
    };

    let empty_class = if empty { "opacity-50" } else { "" };

    let cursor_class = if empty || disabled {
        "cursor-default"
    } else {
        "cursor-pointer hover:brightness-110"
    };

    rsx! {
        // Inject the keyframe animation (only emitted once per page due to <style>)
        style { "@keyframes tile-pulse {{ 0%, 100% {{ filter: brightness(0.5); }} 50% {{ filter: brightness(1.0); }} }}" }

        div {
            class: "relative rounded-lg overflow-hidden transition-all duration-150 {active_class} {empty_class} {cursor_class}",
            style: "{bg_style}",
            onclick: move |_| {
                if !empty && !disabled {
                    on_click.call(());
                }
            },

            if !empty {
                div {
                    class: "absolute inset-0 flex flex-col items-center justify-center text-center px-2",
                    span {
                        class: if disabled {
                            "text-sm font-bold text-white/20 uppercase tracking-wide leading-tight"
                        } else {
                            "text-sm font-bold text-white uppercase tracking-wide leading-tight"
                        },
                        style: "text-shadow: 0 1px 3px rgba(0,0,0,0.5);",
                        "{name}"
                    }
                    if let Some(ref sub) = subtitle {
                        span {
                            class: if disabled {
                                "text-[10px] text-white/10 leading-tight mt-0.5 truncate max-w-full"
                            } else {
                                "text-[10px] text-white/60 leading-tight mt-0.5 truncate max-w-full"
                            },
                            style: "text-shadow: 0 1px 2px rgba(0,0,0,0.5);",
                            "{sub}"
                        }
                    }
                }
            }
        }
    }
}
