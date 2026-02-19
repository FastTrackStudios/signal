//! Pure grid geometry functions — cell positions, module bounds, collision detection.

use signal::block::BlockColor;
use signal::ModuleType;

use super::types::GridSlot;

// ─────────────────────────────────────────────────────────────────────────────
// Grid sizing constants
// ─────────────────────────────────────────────────────────────────────────────

pub(crate) const CELL_SIZE: usize = 76;
pub(crate) const CELL_GAP: usize = 20;
pub(crate) const PORT_SIZE: f64 = 10.0;

const MIN_COLS: usize = 14;
const MIN_ROWS: usize = 1;

// Module-level container padding + title
pub(crate) const GROUP_PAD: f64 = CELL_GAP as f64 * 0.25;
pub(crate) const GROUP_TITLE_H: f64 = 12.0;

// Layer-level: left-side label, no top title bar needed.
// Extra left padding to fit the rotated label.
pub(crate) const LAYER_PAD: f64 = GROUP_PAD + 8.0;
pub(crate) const LAYER_LEFT_PAD: f64 = 36.0;
pub(crate) const LAYER_TITLE_H: f64 = 0.0;

// Engine-level: top label with enough height for clear separation.
pub(crate) const ENGINE_PAD: f64 = LAYER_PAD + 8.0;
pub(crate) const ENGINE_TITLE_H: f64 = 14.0;

// ─────────────────────────────────────────────────────────────────────────────
// Grid bounds
// ─────────────────────────────────────────────────────────────────────────────

pub(crate) fn compute_grid_bounds(chain: &[GridSlot]) -> (usize, usize) {
    if chain.is_empty() {
        return (MIN_COLS, MIN_ROWS);
    }
    let max_col = chain.iter().map(|s| s.col).max().unwrap_or(0);
    let max_row = chain.iter().map(|s| s.row).max().unwrap_or(0);
    let cols = (max_col + 2).max(MIN_COLS);
    let rows = (max_row + 2).max(MIN_ROWS);
    (cols, rows)
}

pub(crate) fn grid_natural_width(cols: usize) -> usize {
    cols * CELL_SIZE + cols.saturating_sub(1) * CELL_GAP
}

pub(crate) fn grid_natural_height(rows: usize) -> usize {
    rows * CELL_SIZE + rows.saturating_sub(1) * CELL_GAP
}

// ─────────────────────────────────────────────────────────────────────────────
// Port positions
// ─────────────────────────────────────────────────────────────────────────────

pub(crate) fn output_port_pos(col: usize, row: usize) -> (f64, f64) {
    let x = (col * (CELL_SIZE + CELL_GAP) + CELL_SIZE) as f64;
    let y = (row * (CELL_SIZE + CELL_GAP)) as f64 + CELL_SIZE as f64 / 2.0;
    (x, y)
}

pub(crate) fn input_port_pos(col: usize, row: usize) -> (f64, f64) {
    let x = (col * (CELL_SIZE + CELL_GAP)) as f64;
    let y = (row * (CELL_SIZE + CELL_GAP)) as f64 + CELL_SIZE as f64 / 2.0;
    (x, y)
}

// ─────────────────────────────────────────────────────────────────────────────
// Module group bounding boxes
// ─────────────────────────────────────────────────────────────────────────────

pub(crate) struct ModuleGroupRect {
    /// Full group key (e.g. "Engine/Layer/Module") — used for matching.
    pub(crate) name: String,
    /// Short label for display (last path segment).
    pub(crate) display_name: String,
    pub(crate) color: BlockColor,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
}

pub(crate) fn compute_module_groups(chain: &[GridSlot]) -> Vec<ModuleGroupRect> {
    use std::collections::BTreeMap;

    struct GroupInfo {
        min_col: usize,
        max_col: usize,
        min_row: usize,
        max_row: usize,
        module_type: ModuleType,
        name: String,
    }

    let mut groups: BTreeMap<String, GroupInfo> = BTreeMap::new();

    for slot in chain {
        let Some(ref key) = slot.module_group else {
            continue;
        };
        let mt = slot.module_type.unwrap_or(ModuleType::Custom);
        groups
            .entry(key.clone())
            .and_modify(|g| {
                g.min_col = g.min_col.min(slot.col);
                g.max_col = g.max_col.max(slot.col);
                g.min_row = g.min_row.min(slot.row);
                g.max_row = g.max_row.max(slot.row);
            })
            .or_insert(GroupInfo {
                min_col: slot.col,
                max_col: slot.col,
                min_row: slot.row,
                max_row: slot.row,
                module_type: mt,
                name: key.clone(),
            });
    }

    groups
        .into_values()
        .map(|g| {
            let step = (CELL_SIZE + CELL_GAP) as f64;
            let cell_x = g.min_col as f64 * step;
            let cell_y = g.min_row as f64 * step;
            let cell_x2 = g.max_col as f64 * step + CELL_SIZE as f64;
            let cell_y2 = g.max_row as f64 * step + CELL_SIZE as f64;

            let color = module_type_color(g.module_type);
            let display_name = g.name.rsplit('/').next().unwrap_or(&g.name).to_string();

            ModuleGroupRect {
                name: g.name,
                display_name,
                color,
                x: cell_x - GROUP_PAD,
                y: cell_y - GROUP_PAD - GROUP_TITLE_H,
                w: (cell_x2 - cell_x) + GROUP_PAD * 2.0,
                h: (cell_y2 - cell_y) + GROUP_PAD * 2.0 + GROUP_TITLE_H,
            }
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// Collision detection
// ─────────────────────────────────────────────────────────────────────────────

pub(crate) fn group_move_is_valid(
    chain: &[GridSlot],
    group_name: &str,
    col_delta: isize,
    row_delta: isize,
) -> bool {
    use std::collections::HashSet;

    let occupied: HashSet<(usize, usize)> = chain
        .iter()
        .filter(|s| s.module_group.as_deref() != Some(group_name))
        .map(|s| (s.col, s.row))
        .collect();

    for s in chain.iter() {
        if s.module_group.as_deref() != Some(group_name) {
            continue;
        }
        let new_col = s.col as isize + col_delta;
        let new_row = s.row as isize + row_delta;
        if new_col < 0 || new_row < 0 {
            continue;
        }
        if occupied.contains(&(new_col as usize, new_row as usize)) {
            return false;
        }
    }
    true
}

// ─────────────────────────────────────────────────────────────────────────────
// Module type → block color mapping
// ─────────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────────
// Hierarchical container groups (Engine → Layer → Module)
// ─────────────────────────────────────────────────────────────────────────────

/// Visual nesting level for container backgrounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ContainerLevel {
    Engine,
    Layer,
    Module,
}

/// A computed container bounding box at any nesting level.
pub(crate) struct ContainerGroupRect {
    /// Full group key (e.g. "Engine/Layer/Module").
    #[allow(dead_code)]
    pub(crate) key: String,
    /// Short display label (last path segment).
    pub(crate) display_name: String,
    pub(crate) level: ContainerLevel,
    pub(crate) color: BlockColor,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
}

/// Compute hierarchical container bounding boxes at all three levels.
///
/// Returns containers sorted outermost-first (Engine, then Layer, then Module)
/// so they render in correct z-order.
pub(crate) fn compute_container_groups(chain: &[GridSlot]) -> Vec<ContainerGroupRect> {
    use std::collections::BTreeMap;

    let step = (CELL_SIZE + CELL_GAP) as f64;

    // Helper: gather bounds for a group field
    struct BoundsInfo {
        min_col: usize,
        max_col: usize,
        min_row: usize,
        max_row: usize,
        module_type: ModuleType,
    }

    // Gather bounds per group key at each level
    fn gather_bounds(
        chain: &[GridSlot],
        key_fn: impl Fn(&GridSlot) -> Option<&String>,
    ) -> BTreeMap<String, BoundsInfo> {
        let mut map = BTreeMap::new();
        for s in chain {
            let Some(key) = key_fn(s) else { continue };
            let mt = s.module_type.unwrap_or(ModuleType::Custom);
            map.entry(key.clone())
                .and_modify(|b: &mut BoundsInfo| {
                    b.min_col = b.min_col.min(s.col);
                    b.max_col = b.max_col.max(s.col);
                    b.min_row = b.min_row.min(s.row);
                    b.max_row = b.max_row.max(s.row);
                })
                .or_insert(BoundsInfo {
                    min_col: s.col,
                    max_col: s.col,
                    min_row: s.row,
                    max_row: s.row,
                    module_type: mt,
                });
        }
        map
    }

    fn make_rects(
        bounds: BTreeMap<String, BoundsInfo>,
        level: ContainerLevel,
        pad: f64,
        left_extra: f64,
        title_h: f64,
        step: f64,
        cell_size: f64,
    ) -> Vec<ContainerGroupRect> {
        bounds
            .into_iter()
            .map(|(key, b)| {
                let cell_x = b.min_col as f64 * step;
                let cell_y = b.min_row as f64 * step;
                let cell_x2 = b.max_col as f64 * step + cell_size;
                let cell_y2 = b.max_row as f64 * step + cell_size;

                let color = module_type_color(b.module_type);
                let display_name = key.rsplit('/').next().unwrap_or(&key).to_string();

                ContainerGroupRect {
                    key,
                    display_name,
                    level,
                    color,
                    x: cell_x - pad - left_extra,
                    y: cell_y - pad - title_h,
                    w: (cell_x2 - cell_x) + pad * 2.0 + left_extra,
                    h: (cell_y2 - cell_y) + pad * 2.0 + title_h,
                }
            })
            .collect()
    }

    let cell_size = CELL_SIZE as f64;
    let mut result = Vec::new();

    // Gather bounds at each level
    let engine_bounds = gather_bounds(chain, |s| s.engine_group.as_ref());
    let layer_bounds = gather_bounds(chain, |s| s.layer_group.as_ref());
    let module_bounds = gather_bounds(chain, |s| s.module_group.as_ref());

    // Bottom-up visibility: module containers always render.
    // Higher levels only render when they group multiple children.
    use std::collections::HashMap;

    let num_engines = engine_bounds.len();

    let mut layers_per_engine: HashMap<String, usize> = HashMap::new();
    for lk in layer_bounds.keys() {
        let engine_key = lk.split('/').next().unwrap_or(lk);
        *layers_per_engine.entry(engine_key.to_string()).or_insert(0) += 1;
    }

    // Layer level — only if parent engine has > 1 layer
    let visible_layers: BTreeMap<String, BoundsInfo> = layer_bounds
        .into_iter()
        .filter(|(key, _)| {
            let engine_key = key.split('/').next().unwrap_or(key);
            layers_per_engine.get(engine_key).copied().unwrap_or(0) > 1
        })
        .collect();
    let layer_rects = make_rects(
        visible_layers,
        ContainerLevel::Layer,
        LAYER_PAD,
        LAYER_LEFT_PAD,
        LAYER_TITLE_H,
        step,
        cell_size,
    );

    // Engine level — built from child layer/cell bounds so it wraps correctly.
    // Uses the actual layer rects when available, falls back to cell bounds.
    if num_engines > 1 {
        for (engine_key, eb) in &engine_bounds {
            // Start with the engine's own cell bounds
            let cell_x = eb.min_col as f64 * step;
            let cell_y = eb.min_row as f64 * step;
            let cell_x2 = eb.max_col as f64 * step + cell_size;
            let cell_y2 = eb.max_row as f64 * step + cell_size;
            let mut min_x = cell_x;
            let mut min_y = cell_y;
            let mut max_x = cell_x2;
            let mut max_y = cell_y2;

            // Expand to encompass child layer rects
            for lr in &layer_rects {
                if lr.key.starts_with(engine_key) {
                    min_x = min_x.min(lr.x);
                    min_y = min_y.min(lr.y);
                    max_x = max_x.max(lr.x + lr.w);
                    max_y = max_y.max(lr.y + lr.h);
                }
            }

            let color = module_type_color(eb.module_type);
            let display_name = engine_key
                .rsplit('/')
                .next()
                .unwrap_or(engine_key)
                .to_string();

            result.push(ContainerGroupRect {
                key: engine_key.clone(),
                display_name,
                level: ContainerLevel::Engine,
                color,
                x: min_x - ENGINE_PAD,
                y: min_y - ENGINE_PAD - ENGINE_TITLE_H,
                w: (max_x - min_x) + ENGINE_PAD * 2.0,
                h: (max_y - min_y) + ENGINE_PAD * 2.0 + ENGINE_TITLE_H,
            });
        }
    }

    result.extend(layer_rects);

    // Module level — always rendered (lowest common denominator)
    result.extend(make_rects(
        module_bounds,
        ContainerLevel::Module,
        GROUP_PAD,
        0.0,
        GROUP_TITLE_H,
        step,
        cell_size,
    ));

    result
}

/// Map a ModuleType to its display color using the ModuleType's own color palette.
pub(crate) fn module_type_color(mt: ModuleType) -> BlockColor {
    let mc = mt.color();
    BlockColor {
        bg: mc.bg,
        fg: mc.fg,
        border: mc.border,
    }
}
