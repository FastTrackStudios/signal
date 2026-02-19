//! Cable resolution, SVG path generation, and CableLayer component.

use dioxus::prelude::*;

use super::interaction::GridConnection;
use super::layout::{input_port_pos, output_port_pos, CELL_GAP, CELL_SIZE, GROUP_PAD};
use super::types::GridSlot;

// ─────────────────────────────────────────────────────────────────────────────
// Cable struct
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
pub(crate) struct Cable {
    pub(crate) from: (f64, f64),
    pub(crate) to: (f64, f64),
    pub(crate) color: String,
    pub(crate) straight: bool,
    pub(crate) route_y: Option<f64>,
    pub(crate) bypassed: bool,
}

impl Cable {
    pub(crate) fn new(from: (f64, f64), to: (f64, f64), color: String, bypassed: bool) -> Self {
        Self {
            from,
            to,
            color,
            straight: false,
            route_y: None,
            bypassed,
        }
    }

    pub(crate) fn routed(
        from: (f64, f64),
        to: (f64, f64),
        color: String,
        route_y: f64,
        bypassed: bool,
    ) -> Self {
        Self {
            from,
            to,
            color,
            straight: false,
            route_y: Some(route_y),
            bypassed,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Module I/O ports
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
pub(crate) struct ModulePort {
    pub(crate) pos: (f64, f64),
    pub(crate) color: String,
    pub(crate) bypassed: bool,
}

pub(crate) fn compute_module_ports(chain: &[GridSlot]) -> Vec<ModulePort> {
    use std::collections::BTreeMap;

    let mut ports = Vec::new();
    if chain.is_empty() {
        return ports;
    }

    let mut group_map: Vec<(String, usize, usize, usize, usize, String)> = Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();

    for s in chain.iter() {
        let Some(ref g) = s.module_group else {
            continue;
        };
        let color = s.block_type.color().bg.to_string();
        if let Some(&idx) = seen.get(g) {
            let entry = &mut group_map[idx];
            entry.1 = entry.1.min(s.col);
            entry.2 = entry.2.max(s.col);
            entry.3 = entry.3.min(s.row);
            entry.4 = entry.4.max(s.row);
        } else {
            seen.insert(g.clone(), group_map.len());
            group_map.push((g.clone(), s.col, s.col, s.row, s.row, color));
        }
    }

    let step = (CELL_SIZE + CELL_GAP) as f64;
    for (name, min_c, max_c, min_r, max_r, color) in &group_map {
        let all_bypassed = {
            let slots: Vec<&GridSlot> = chain
                .iter()
                .filter(|s| s.module_group.as_deref() == Some(name) && !s.is_phantom)
                .collect();
            !slots.is_empty() && slots.iter().all(|s| s.bypassed)
        };

        let in_x = *min_c as f64 * step - GROUP_PAD;
        let top = *min_r as f64 * step;
        let bottom = *max_r as f64 * step + CELL_SIZE as f64;
        let center_y = (top + bottom) / 2.0;
        ports.push(ModulePort {
            pos: (in_x, center_y),
            color: color.clone(),
            bypassed: all_bypassed,
        });

        let out_x = *max_c as f64 * step + CELL_SIZE as f64 + GROUP_PAD;
        ports.push(ModulePort {
            pos: (out_x, center_y),
            color: color.clone(),
            bypassed: all_bypassed,
        });
    }

    ports
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal module boundary info
// ─────────────────────────────────────────────────────────────────────────────

struct ModuleIO {
    name: String,
    layer_group: Option<String>,
    #[allow(dead_code)]
    left_edge: Vec<(usize, usize)>,
    right_edge: Vec<(usize, usize)>,
    min_col: usize,
    max_col: usize,
    min_row: usize,
    max_row: usize,
    color: String,
}

impl ModuleIO {
    fn input_point(&self) -> (f64, f64) {
        let step = (CELL_SIZE + CELL_GAP) as f64;
        let x = self.min_col as f64 * step - GROUP_PAD;
        let top = self.min_row as f64 * step;
        let bottom = self.max_row as f64 * step + CELL_SIZE as f64;
        (x, (top + bottom) / 2.0)
    }

    fn output_point(&self) -> (f64, f64) {
        let step = (CELL_SIZE + CELL_GAP) as f64;
        let x = self.max_col as f64 * step + CELL_SIZE as f64 + GROUP_PAD;
        let top = self.min_row as f64 * step;
        let bottom = self.max_row as f64 * step + CELL_SIZE as f64;
        (x, (top + bottom) / 2.0)
    }

    fn is_multi_row(&self) -> bool {
        self.min_row != self.max_row
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Cable resolution
// ─────────────────────────────────────────────────────────────────────────────

pub(crate) fn resolve_cables(chain: &[GridSlot]) -> Vec<Cable> {
    use std::collections::BTreeMap;

    let mut cables = Vec::new();
    if chain.is_empty() {
        return cables;
    }

    // (name, min_col, max_col, min_row, max_row, color, layer_group)
    let mut group_map: Vec<(String, usize, usize, usize, usize, String, Option<String>)> =
        Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();

    for s in chain.iter() {
        let Some(ref g) = s.module_group else {
            continue;
        };
        let color = s.block_type.color().bg.to_string();
        if let Some(&idx) = seen.get(g) {
            let entry = &mut group_map[idx];
            entry.1 = entry.1.min(s.col);
            entry.2 = entry.2.max(s.col);
            entry.3 = entry.3.min(s.row);
            entry.4 = entry.4.max(s.row);
        } else {
            seen.insert(g.clone(), group_map.len());
            group_map.push((
                g.clone(),
                s.col,
                s.col,
                s.row,
                s.row,
                color,
                s.layer_group.clone(),
            ));
        }
    }

    let modules: Vec<ModuleIO> = group_map
        .iter()
        .map(|(name, min_c, max_c, min_r, max_r, color, layer_group)| {
            let mut left_edge: Vec<(usize, usize)> = chain
                .iter()
                .filter(|s| {
                    s.module_group.as_deref() == Some(name) && s.col == *min_c && !s.is_phantom
                })
                .map(|s| (s.col, s.row))
                .collect();
            left_edge.sort_by_key(|&(_, r)| r);

            let mut right_edge: Vec<(usize, usize)> = chain
                .iter()
                .filter(|s| {
                    s.module_group.as_deref() == Some(name) && s.col == *max_c && !s.is_phantom
                })
                .map(|s| (s.col, s.row))
                .collect();
            right_edge.sort_by_key(|&(_, r)| r);

            ModuleIO {
                name: name.clone(),
                layer_group: layer_group.clone(),
                left_edge,
                right_edge,
                min_col: *min_c,
                max_col: *max_c,
                min_row: *min_r,
                max_row: *max_r,
                color: color.clone(),
            }
        })
        .collect();

    // 1. Intra-module horizontal adjacency
    for a in chain.iter() {
        if a.is_phantom {
            continue;
        }
        for b in chain.iter() {
            if b.is_phantom {
                continue;
            }
            let same_group = match (&a.module_group, &b.module_group) {
                (Some(ga), Some(gb)) => ga == gb,
                _ => false,
            };
            if !same_group {
                continue;
            }
            if a.row == b.row && b.col == a.col + 1 {
                let color = a.block_type.color().bg.to_string();
                cables.push(Cable::new(
                    output_port_pos(a.col, a.row),
                    input_port_pos(b.col, b.row),
                    color,
                    a.bypassed && b.bypassed,
                ));
            }
        }
    }

    // 2. Fan-out / fan-in for multi-row modules
    let module_all_bypassed = |name: &str| -> bool {
        let slots: Vec<&GridSlot> = chain
            .iter()
            .filter(|s| s.module_group.as_deref() == Some(name))
            .collect();
        !slots.is_empty() && slots.iter().all(|s| s.bypassed)
    };
    let block_bypassed_at = |col: usize, row: usize| -> bool {
        chain
            .iter()
            .find(|s| s.col == col && s.row == row)
            .map_or(false, |s| s.bypassed)
    };

    for m in &modules {
        if m.is_multi_row() {
            let mod_bypassed = module_all_bypassed(&m.name);
            let mod_in = m.input_point();
            for &(col, row) in &m.left_edge {
                cables.push(Cable::new(
                    mod_in,
                    input_port_pos(col, row),
                    m.color.clone(),
                    mod_bypassed || block_bypassed_at(col, row),
                ));
            }

            let mod_out = m.output_point();
            for &(col, row) in &m.right_edge {
                cables.push(Cable::new(
                    output_port_pos(col, row),
                    mod_out,
                    m.color.clone(),
                    mod_bypassed || block_bypassed_at(col, row),
                ));
            }

            // Dry pass-through: straight cable from module input → output.
            // Represents the unprocessed signal path through a wet/dry split.
            cables.push(Cable::new(mod_in, mod_out, m.color.clone(), mod_bypassed));
        }
    }

    // 3. Inter-module cables — only within the same layer.
    //    Layers are parallel signal paths within an engine, and engines are
    //    parallel to each other, so we must NOT draw cables across layers or
    //    engines. Group modules by layer_group and connect sequentially
    //    within each group.
    {
        let step = (CELL_SIZE + CELL_GAP) as f64;

        // Group module indices by layer_group, preserving insertion order.
        let mut layer_groups: Vec<(Option<String>, Vec<usize>)> = Vec::new();
        let mut layer_idx_map: BTreeMap<Option<String>, usize> = BTreeMap::new();
        for (i, m) in modules.iter().enumerate() {
            if let Some(&idx) = layer_idx_map.get(&m.layer_group) {
                layer_groups[idx].1.push(i);
            } else {
                layer_idx_map.insert(m.layer_group.clone(), layer_groups.len());
                layer_groups.push((m.layer_group.clone(), vec![i]));
            }
        }

        for (_layer, mod_indices) in &layer_groups {
            for pair in mod_indices.windows(2) {
                let from_mod = &modules[pair[0]];
                let to_mod = &modules[pair[1]];
                let from_pt = from_mod.output_point();
                let to_pt = to_mod.input_point();
                let color = from_mod.color.clone();
                let both_bypassed =
                    module_all_bypassed(&from_mod.name) && module_all_bypassed(&to_mod.name);

                let rows_overlap =
                    from_mod.min_row <= to_mod.max_row && to_mod.min_row <= from_mod.max_row;

                if rows_overlap {
                    cables.push(Cable::new(from_pt, to_pt, color, both_bypassed));
                } else {
                    let is_wrap = to_pt.0 < from_pt.0 && to_pt.1 > from_pt.1;

                    if is_wrap {
                        let channel_y = from_mod.max_row as f64 * step
                            + CELL_SIZE as f64
                            + CELL_GAP as f64 * 0.12;
                        let mut c = Cable::new(from_pt, to_pt, color, both_bypassed);
                        c.route_y = Some(channel_y);
                        cables.push(c);
                    } else {
                        let upper_bottom_row = from_mod.max_row.min(to_mod.max_row);
                        let channel_y = upper_bottom_row as f64 * step
                            + CELL_SIZE as f64
                            + CELL_GAP as f64 * 0.25;

                        cables.push(Cable::routed(
                            from_pt,
                            to_pt,
                            color,
                            channel_y,
                            both_bypassed,
                        ));
                    }
                }
            }
        }
    }

    cables
}

pub(crate) fn resolve_cables_or_connections(
    chain: &[GridSlot],
    connections: &[GridConnection],
) -> Vec<Cable> {
    if connections.is_empty() {
        return resolve_cables(chain);
    }
    connections
        .iter()
        .filter_map(|conn| {
            let from = chain.iter().find(|s| s.id == conn.from_slot_id)?;
            let to = chain.iter().find(|s| s.id == conn.to_slot_id)?;
            let color = from.block_type.color().bg.to_string();
            Some(Cable::new(
                output_port_pos(from.col, from.row),
                input_port_pos(to.col, to.row),
                color,
                from.bypassed && to.bypassed,
            ))
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// SVG path generation
// ─────────────────────────────────────────────────────────────────────────────

pub(crate) fn routed_cable_path(from: (f64, f64), to: (f64, f64), channel_y: f64) -> String {
    let r = 10.0f64;
    let (fx, fy) = from;
    let (tx, ty) = to;
    let dy1 = channel_y - fy;
    let dy2 = ty - channel_y;
    let dx = tx - fx;

    if dy1.abs() < r * 2.0 || dy2.abs() < r * 2.0 {
        return cable_path_d(from, to);
    }

    let going_down_first = dy1 > 0.0;
    let going_right = dx > 0.0;
    let going_up_last = dy2 < 0.0;

    let (c1_vy_end, _c1_hy_start, sweep1) = if going_down_first && going_right {
        (channel_y - r, channel_y, 0)
    } else if going_down_first && !going_right {
        (channel_y - r, channel_y, 1)
    } else if !going_down_first && going_right {
        (channel_y + r, channel_y, 1)
    } else {
        (channel_y + r, channel_y, 0)
    };

    let c1_hx = if going_right { fx + r } else { fx - r };

    let (c2_hx_end, c2_vy_start, sweep2) = if going_right && going_up_last {
        (tx - r, channel_y - r, 0)
    } else if going_right && !going_up_last {
        (tx - r, channel_y + r, 1)
    } else if !going_right && going_up_last {
        (tx + r, channel_y - r, 1)
    } else {
        (tx + r, channel_y + r, 0)
    };

    format!(
        "M {fx},{fy} \
         L {fx},{c1_vy_end} \
         A {r},{r} 0 0 {sweep1} {c1_hx},{channel_y} \
         L {c2_hx_end},{channel_y} \
         A {r},{r} 0 0 {sweep2} {tx},{c2_vy_start} \
         L {tx},{ty}",
    )
}

pub(crate) fn cable_path_d(from: (f64, f64), to: (f64, f64)) -> String {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let abs_dx = dx.abs();
    let abs_dy = dy.abs();

    let is_row_wrap = dy > 40.0 && dx < -40.0;

    if is_row_wrap {
        let r = 12.0;
        let channel_y = (from.1 + to.1) * 0.5;
        let right_x = from.0 + (CELL_GAP as f64) * 0.5;
        let left_x = to.0 - (CELL_GAP as f64) * 0.5;

        format!(
            "M {fx},{fy} \
             L {c1sx},{fy} \
             A {r},{r} 0 0 1 {c1ex},{c1ey} \
             L {right_x},{c2sy} \
             A {r},{r} 0 0 1 {c2ex},{channel_y} \
             L {c3sx},{channel_y} \
             A {r},{r} 0 0 0 {c3ex},{c3ey} \
             L {left_x},{c4sy} \
             A {r},{r} 0 0 0 {c4ex},{ty} \
             L {tx},{ty}",
            fx = from.0,
            fy = from.1,
            r = r,
            c1sx = right_x - r,
            c1ex = right_x,
            c1ey = from.1 + r,
            right_x = right_x,
            c2sy = channel_y - r,
            c2ex = right_x - r,
            channel_y = channel_y,
            c3sx = left_x + r,
            c3ex = left_x,
            c3ey = channel_y + r,
            left_x = left_x,
            c4sy = to.1 - r,
            c4ex = left_x + r,
            ty = to.1,
            tx = to.0,
        )
    } else if abs_dx >= abs_dy {
        let offset = abs_dx.max(60.0) * 0.4;
        format!(
            "M {},{} C {},{} {},{} {},{}",
            from.0,
            from.1,
            from.0 + offset,
            from.1,
            to.0 - offset,
            to.1,
            to.0,
            to.1,
        )
    } else {
        let offset = abs_dy.max(60.0) * 0.4;
        format!(
            "M {},{} C {},{} {},{} {},{}",
            from.0,
            from.1,
            from.0,
            from.1 + offset,
            to.0,
            to.1 - offset,
            to.0,
            to.1,
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CableLayer component
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub(super) struct CableLayerProps {
    pub cables: Vec<Cable>,
    pub module_ports: Vec<ModulePort>,
    pub nat_w: f64,
    pub nat_h: f64,
}

#[component]
pub(super) fn CableLayer(props: CableLayerProps) -> Element {
    let nat_w = props.nat_w;
    let nat_h = props.nat_h;

    rsx! {
        div {
            style: "position: absolute; left: 0; top: 0; width: {nat_w}px; height: {nat_h}px; \
                    z-index: 0; pointer-events: none; overflow: visible;",
            svg {
                style: "overflow: visible;",
                width: "{nat_w}",
                height: "{nat_h}",
                view_box: "0 0 {nat_w} {nat_h}",

                for cable in props.cables.iter() {
                    {
                        let d = if let Some(ry) = cable.route_y {
                            routed_cable_path(cable.from, cable.to, ry)
                        } else if cable.straight {
                            format!("M {},{} L {},{}", cable.from.0, cable.from.1, cable.to.0, cable.to.1)
                        } else {
                            cable_path_d(cable.from, cable.to)
                        };
                        let stroke = cable.color.clone();
                        let opacity = if cable.bypassed { "0.15" } else { "0.7" };
                        rsx! {
                            path {
                                d: "{d}",
                                fill: "none",
                                stroke: "{stroke}",
                                stroke_width: "2.5",
                                stroke_opacity: "{opacity}",
                                stroke_linecap: "round",
                            }
                        }
                    }
                }

                for port in props.module_ports.iter() {
                    {
                        let cx = port.pos.0;
                        let cy = port.pos.1;
                        let fill = port.color.clone();
                        let fill_op = if port.bypassed { "0.15" } else { "0.8" };
                        let stroke_op = if port.bypassed { "0.08" } else { "0.4" };
                        rsx! {
                            circle {
                                cx: "{cx}",
                                cy: "{cy}",
                                r: "4",
                                fill: "{fill}",
                                fill_opacity: "{fill_op}",
                                stroke: "{fill}",
                                stroke_width: "1.5",
                                stroke_opacity: "{stroke_op}",
                            }
                        }
                    }
                }
            }
        }
    }
}
