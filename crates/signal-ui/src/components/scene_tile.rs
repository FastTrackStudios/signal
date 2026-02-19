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
    "#92400e", // Amber/warm brown
    "#065f46", // Emerald/teal
    "#0e7490", // Cyan/teal
    "#5b21b6", // Violet/purple
    "#9a3412", // Orange/rust
    "#1e40af", // Blue
    "#7c3aed", // Purple
    "#374151", // Gray (neutral)
];

/// Data for a single tile in the grid.
#[derive(Clone, PartialEq)]
pub struct TileData {
    /// Display name for the tile.
    pub name: String,
    /// Whether this tile is the active/selected one.
    pub active: bool,
    /// Whether this tile is empty (no content assigned).
    pub empty: bool,
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
        div { class: "h-full w-full bg-card p-2",
            div { class: "grid grid-cols-4 grid-rows-2 gap-2 h-full",
                for i in 0..slot_count {
                    {
                        let tile = tiles.get(i).cloned().unwrap_or(TileData {
                            name: String::new(),
                            active: false,
                            empty: true,
                        });
                        let color = TILE_COLORS[i % TILE_COLORS.len()].to_string();
                        rsx! {
                            SceneTileCell {
                                key: "{i}",
                                name: tile.name,
                                active: tile.active,
                                empty: tile.empty,
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
    /// Whether this tile is active/selected.
    #[props(default)]
    active: bool,
    /// Whether this tile is empty.
    #[props(default)]
    empty: bool,
    /// Background color (hex, e.g. `"#92400e"`).
    color: String,
    /// Click handler.
    on_click: EventHandler<()>,
) -> Element {
    let bg_style = if empty {
        "background-color: #374151;".to_string()
    } else {
        format!("background-color: {color};")
    };

    let active_class = if active {
        "ring-2 ring-green-400 shadow-lg shadow-green-500/30"
    } else {
        ""
    };

    let empty_class = if empty { "opacity-50" } else { "" };

    let cursor_class = if empty {
        "cursor-default"
    } else {
        "cursor-pointer hover:brightness-110"
    };

    rsx! {
        div {
            class: "relative rounded-lg overflow-hidden transition-all duration-150 {active_class} {empty_class} {cursor_class}",
            style: "{bg_style}",
            onclick: move |_| {
                if !empty {
                    on_click.call(());
                }
            },

            if !empty {
                div {
                    class: "absolute inset-0 flex items-center justify-center text-center px-2",
                    span {
                        class: "text-sm font-bold text-white uppercase tracking-wide leading-tight",
                        style: "text-shadow: 0 1px 3px rgba(0,0,0,0.5);",
                        "{name}"
                    }
                }
            }
        }
    }
}
